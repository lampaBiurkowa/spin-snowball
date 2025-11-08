use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use glam::Vec2;
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MapObject {
    Circle {
        x: f32,
        y: f32,
        radius: f32,
        factor: f32,
        color: ColorDef,
        is_hole: bool,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        factor: f32,
        color: ColorDef,
        is_hole: bool,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorDef {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GameMap {
    pub name: String,
    pub width: f32,
    pub height: f32,
    pub objects: Vec<MapObject>,
}

const TICK_HZ: f32 = 60.0;
const DT: f32 = 1.0 / TICK_HZ;
const WORLD_W: f32 = 800.0;
const WORLD_H: f32 = 600.0;
const PLAYER_RADIUS: f32 = 18.0;
const PLAYER_MASS: f32 = 1.0;

const SNOWBALL_RADIUS: f32 = 8.0;
const SNOWBALL_MASS: f32 = 0.5;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
enum ClientMessage {
    Input {
        left: bool,
        right: bool,
        shoot: bool,
    },
    Ping {
        ts: u64,
    },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
enum ServerMessage {
    AssignId {
        id: String,
    },
    WorldState {
        players: Vec<PlayerState>,
        snowballs: Vec<SnowballState>,
    },
    Pong {
        ts: u64,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct PlayerState {
    id: String,
    pos: [f32; 2],
    vel: [f32; 2],
    rot_deg: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SnowballState {
    id: u64,
    pos: [f32; 2],
    vel: [f32; 2],
    life: f32,
}

// Server side runtime structs
struct Player {
    id: String,
    pos: Vec2,
    vel: Vec2,
    rot_deg: f32,
    rotating_left: bool,
    rotating_right: bool,
    spin_timer: f32,
    last_shoot_pressed: bool,
}

struct Snowball {
    id: u64,
    pos: Vec2,
    vel: Vec2,
    life: f32,
}

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<String, Tx>>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:9001".to_string());
    println!("Starting server on {}", addr);

    let listener = TcpListener::bind(&addr).await?;
    let peers: PeerMap = Arc::new(Mutex::new(HashMap::new()));

    // Shared game state (players and snowballs)
    let map: GameMap = {
        let data = std::fs::read_to_string("default_map.json")?;
        serde_json::from_str(&data)?
    };

    let game_state = Arc::new(Mutex::new(GameState::new(map)));

    // Spawn physics loop
    {
        let peers = peers.clone();
        let game_state = game_state.clone();
        tokio::spawn(async move {
            physics_loop(game_state, peers).await;
        });
    }

    // Accept loop
    while let Ok((stream, _)) = listener.accept().await {
        let peers = peers.clone();
        let game_state = game_state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, peers, game_state).await {
                println!("Connection error: {e:?}");
            }
        });
    }

    Ok(())
}

async fn handle_connection(
    stream: TcpStream,
    peers: PeerMap,
    game_state: Arc<Mutex<GameState>>,
) -> anyhow::Result<()> {
    let ws = accept_async(stream).await?;
    let (mut ws_sender, mut ws_receiver) = ws.split();

    // Each connection gets an ID
    let client_id = Uuid::new_v4().to_string();
    println!("New client {}", client_id);

    // channel for sending outbound messages to this client
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    // Insert tx into peers for broadcast
    peers.lock().unwrap().insert(client_id.clone(), tx.clone());

    // Create player in game state
    {
        let mut gs = game_state.lock().unwrap();
        gs.add_player(client_id.clone());
    }

    // Send assign id message
    let assign = ServerMessage::AssignId {
        id: client_id.clone(),
    };
    ws_sender
        .send(Message::Text(serde_json::to_string(&assign)?.into()))
        .await?;

    // task: forward rx -> ws_sender
    let forward_out = async {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
        // closing
        Ok::<(), anyhow::Error>(())
    };

    // task: inbound from ws -> process client messages
    let peers_clone = peers.clone();
    let game_state_clone = game_state.clone();
    let client_id_clone = client_id.clone();
    let inbound = async {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            if let Message::Text(txt) = msg {
                match serde_json::from_str::<ClientMessage>(&txt) {
                    Ok(ClientMessage::Input { left, right, shoot }) => {
                        // update player's input snapshot in game state
                        let mut gs = game_state_clone.lock().unwrap();
                        gs.apply_input(&client_id_clone, left, right, shoot);
                    }
                    Ok(ClientMessage::Ping { ts }) => {
                        // reply Pong
                        if let Some(tx) = peers_clone.lock().unwrap().get(&client_id_clone) {
                            let _ = tx.send(Message::Text(
                                serde_json::to_string(&ServerMessage::Pong { ts })
                                    .unwrap()
                                    .into(),
                            ));
                        }
                    }
                    Err(e) => {
                        println!("Malformed client msg: {e}");
                    }
                }
            }
        }
        Ok::<(), anyhow::Error>(())
    };

    // run both tasks until one finishes
    tokio::select! {
        res = forward_out => { let _ = res; },
        res = inbound => { let _ = res; },
    }

    println!("Client {} disconnected", client_id);
    // cleanup
    peers.lock().unwrap().remove(&client_id);
    {
        let mut gs = game_state.lock().unwrap();
        gs.remove_player(&client_id);
    }

    Ok(())
}

// GameState holds authoritative players, snowballs and input tags
struct GameState {
    players: HashMap<String, Player>,
    snowballs: HashMap<u64, Snowball>,
    next_snowball_id: u64,
    map: GameMap,
}

impl GameState {
    fn new(map: GameMap) -> Self {
        Self {
            players: HashMap::new(),
            snowballs: HashMap::new(),
            next_snowball_id: 1,
            map,
        }
    }

    fn add_player(&mut self, id: String) {
        // random spawn
        let pos = Vec2::new(
            rand::random::<f32>() * (WORLD_W - 40.0) + 20.0,
            rand::random::<f32>() * (WORLD_H - 40.0) + 20.0,
        );
        self.players.insert(
            id.clone(),
            Player {
                id,
                pos,
                vel: Vec2::ZERO,
                rot_deg: -90.0,
                rotating_left: false,
                rotating_right: false,
                spin_timer: 0.0,
                last_shoot_pressed: false,
            },
        );
    }

    fn remove_player(&mut self, id: &str) {
        self.players.remove(id);
    }

    fn apply_input(&mut self, id: &str, left: bool, right: bool, shoot: bool) {
        if let Some(p) = self.players.get_mut(id) {
            // Edge-detect the shoot button on server side:
            // only spawn a snowball when shoot transitions from false -> true
            if shoot && !p.last_shoot_pressed {
                // spawn based on current rotation & spin_timer
                let charge = p.spin_timer.min(1.5);
                let charge_t = charge / 1.5;
                let base_speed = 280.0;
                let snowball_speed = base_speed + 700.0 * charge_t;

                let r = p.rot_deg.to_radians();
                let dir = Vec2::new(r.cos(), r.sin());
                let spawn_pos = p.pos + dir * (18.0 + 8.0);

                let id = self.next_snowball_id;
                self.next_snowball_id += 1;
                self.snowballs.insert(
                    id,
                    Snowball {
                        id,
                        pos: spawn_pos,
                        vel: dir * snowball_speed,
                        life: 2.0,
                    },
                );

                let recoil_strength = 0.45 + 1.0 * charge_t;
                p.vel -= dir * (snowball_speed * recoil_strength / 3.0);

                p.spin_timer = 0.0;
                p.last_shoot_pressed = true; // remember that we have seen the press
            } else {
                // If shoot is not pressed, clear the previous flag so we can detect next rising edge.
                if !shoot {
                    p.last_shoot_pressed = false;
                }
                // Normal rotate state handling
                p.rotating_left = left;
                p.rotating_right = right;
            }
        }
    }

    // physics step
    fn step(&mut self, dt: f32) {
        // update players rotation/positions
        for (_id, p) in self.players.iter_mut() {
            if p.rotating_left {
                p.rot_deg -= 180.0 * dt;
                p.spin_timer += dt;
            }
            if p.rotating_right {
                p.rot_deg += 180.0 * dt;
                p.spin_timer += dt;
            }
            // wrap
            if p.rot_deg > 360.0 || p.rot_deg < -360.0 {
                p.rot_deg = p.rot_deg % 360.0;
            }
            // integrate
            p.pos += p.vel * dt;
            // friction
            p.vel *= 0.98f32.powf(dt * 60.0);

            // clamp to world
            p.pos.x = p.pos.x.clamp(0.0, WORLD_W);
            p.pos.y = p.pos.y.clamp(0.0, WORLD_H);
        }

        // update snowballs
        let mut dead = Vec::new();
        for (&id, sb) in self.snowballs.iter_mut() {
            sb.pos += sb.vel * dt;
            sb.vel *= 0.995;
            sb.life -= dt;
            if sb.life <= 0.0 {
                dead.push(id);
            }
        }
        for id in dead {
            self.snowballs.remove(&id);
        }

        // ------------------------------------------------------------
        // PLAYER <-> PLAYER COLLISIONS (elastic bounce)
        // ------------------------------------------------------------
        let player_ids: Vec<String> = self.players.keys().cloned().collect();
        for i in 0..player_ids.len() {
            for j in i + 1..player_ids.len() {
                let (id_a, id_b) = (&player_ids[i], &player_ids[j]);
                let (pa, pb) = {
                    let pa = self.players.get(id_a).unwrap();
                    let pb = self.players.get(id_b).unwrap();
                    (pa.pos, pb.pos)
                };

                let delta = pb - pa;
                let dist = delta.length();
                let min_dist = PLAYER_RADIUS * 2.0;

                if dist < min_dist && dist > 0.0 {
                    // normalize
                    let n = delta / dist;
                    let penetration = min_dist - dist;

                    // separate the players
                    {
                        let p = self.players.get_mut(id_a).unwrap();
                        p.pos -= n * (penetration * 0.5);
                    }
                    {
                        let p = self.players.get_mut(id_b).unwrap();
                        p.pos += n * (penetration * 0.5);
                    }

                    // bounce (simple elastic)
                    let va = self.players.get(id_a).unwrap().vel;
                    let vb = self.players.get(id_b).unwrap().vel;

                    let rel = vb - va;
                    let sep_vel = rel.dot(n);

                    if sep_vel < 0.0 {
                        let impulse = -(1.0 + 0.9) * sep_vel / (PLAYER_MASS + PLAYER_MASS);

                        {
                            let p = self.players.get_mut(id_a).unwrap();
                            p.vel -= n * (impulse * PLAYER_MASS);
                        }
                        {
                            let p = self.players.get_mut(id_b).unwrap();
                            p.vel += n * (impulse * PLAYER_MASS);
                        }
                    }
                }
            }
        }

        // ------------------------------------------------------------
        // PLAYER <-> SNOWBALL COLLISIONS
        // ------------------------------------------------------------
        let snow_ids: Vec<u64> = self.snowballs.keys().cloned().collect();
        for pid in player_ids.iter() {
            for sid in snow_ids.iter() {
                let (pp, sp) = {
                    let p = self.players.get(pid).unwrap();
                    let s = self.snowballs.get(sid).unwrap();
                    (p.pos, s.pos)
                };

                let delta = sp - pp;
                let dist = delta.length();
                let min_dist = PLAYER_RADIUS + SNOWBALL_RADIUS;

                if dist < min_dist && dist > 0.0 {
                    let n = delta / dist;
                    let penetration = min_dist - dist;

                    // push apart
                    {
                        let p = self.players.get_mut(pid).unwrap();
                        p.pos -=
                            n * (penetration * (SNOWBALL_MASS / (PLAYER_MASS + SNOWBALL_MASS)));
                    }
                    {
                        let s = self.snowballs.get_mut(sid).unwrap();
                        s.pos += n * (penetration * (PLAYER_MASS / (PLAYER_MASS + SNOWBALL_MASS)));
                    }

                    // bounce velocities
                    let va = self.players.get(pid).unwrap().vel;
                    let vb = self.snowballs.get(sid).unwrap().vel;

                    let rel = vb - va;
                    let sep_vel = rel.dot(n);

                    if sep_vel < 0.0 {
                        let impulse = -(1.0 + 0.9) * sep_vel / (PLAYER_MASS + SNOWBALL_MASS);

                        {
                            let p = self.players.get_mut(pid).unwrap();
                            p.vel -= n * (impulse * SNOWBALL_MASS);
                        }
                        {
                            let s = self.snowballs.get_mut(sid).unwrap();
                            s.vel += n * (impulse * PLAYER_MASS);
                        }
                    }
                }
            }
        }

        // MAP COLLISIONS - for players and snowballs
        // We'll handle players first:
        for p in self.players.values_mut() {
            let pos = p.pos;
            for obj in &self.map.objects {
                match obj {
                    MapObject::Circle {
                        x,
                        y,
                        radius,
                        factor,
                        color,
                        is_hole,
                    } => {
                        // treat object circle radius, test intersection with player radius
                        if circle_intersects_circle(pos.x, pos.y, PLAYER_RADIUS, *x, *y, *radius) {
                            if *is_hole {
                                // fell into hole -> respawn
                                p.pos = Vec2::new(100.0, 100.0);
                                p.vel = Vec2::ZERO;
                            } else {
                                let delta = pos - Vec2::new(*x, *y);
                                let dist = delta.length().max(0.0001);
                                let n = delta / dist;
                                p.pos = Vec2::new(*x, *y) + n * (*radius + PLAYER_RADIUS);
                                p.vel = p.vel - 2.0 * p.vel.dot(n) * n * (*factor);
                            }
                        }
                    }

                    MapObject::Rect {
                        x,
                        y,
                        w,
                        h,
                        factor,
                        color,
                        is_hole,
                    } => {
                        // test circle (player) vs rect by nearest point
                        if circle_intersects_rect(pos.x, pos.y, PLAYER_RADIUS, *x, *y, *w, *h) {
                            if *is_hole {
                                p.pos = Vec2::new(100.0, 100.0);
                                p.vel = Vec2::ZERO;
                            } else {
                                // compute nearest point on rect and robust normal
                                let cx = pos.x.clamp(*x, x + w);
                                let cy = pos.y.clamp(*y, y + h);
                                let mut n = (pos - Vec2::new(cx, cy));

                                // If exactly inside center (very rare), choose outward axis with max penetration
                                if n.length_squared() < 1e-6 {
                                    // compute penetration distances to each side
                                    let left_pen = (pos.x - *x).abs();
                                    let right_pen = (pos.x - (x + w)).abs();
                                    let top_pen = (pos.y - *y).abs();
                                    let bottom_pen = (pos.y - (y + h)).abs();

                                    // pick largest distance to determine normal direction
                                    if left_pen < right_pen
                                        && left_pen < top_pen
                                        && left_pen < bottom_pen
                                    {
                                        n = Vec2::new(-1.0, 0.0);
                                    } else if right_pen < left_pen
                                        && right_pen < top_pen
                                        && right_pen < bottom_pen
                                    {
                                        n = Vec2::new(1.0, 0.0);
                                    } else if top_pen < bottom_pen {
                                        n = Vec2::new(0.0, -1.0);
                                    } else {
                                        n = Vec2::new(0.0, 1.0);
                                    }
                                }

                                let n = n.normalize_or_zero();
                                // move player out along normal until touching (approx)
                                let overlap = PLAYER_RADIUS - (pos - Vec2::new(cx, cy)).length();
                                if overlap > 0.0 {
                                    p.pos += n * overlap;
                                } else {
                                    // fallback small n push
                                    p.pos += n * 1.0;
                                }
                                p.vel = p.vel - 2.0 * p.vel.dot(n) * n * factor;
                            }
                        }
                    }
                }
            }
        }

        // Now handle snowballs similarly (so they bounce on map)
        let snow_ids: Vec<u64> = self.snowballs.keys().cloned().collect();
        for sid in snow_ids.iter() {
            // we will mutably borrow inside, so take a copy of position first
            if let Some(sb) = self.snowballs.get(sid) {
                let sb_pos = sb.pos;
                for obj in &self.map.objects {
                    match obj {
                        MapObject::Circle {
                            x,
                            y,
                            radius,
                            factor,
                            color,
                            is_hole,
                        } => {
                            if circle_intersects_circle(
                                sb_pos.x,
                                sb_pos.y,
                                SNOWBALL_RADIUS,
                                *x,
                                *y,
                                *radius,
                            ) {
                                if *is_hole {
                                    // snowball falls into hole -> remove immediately
                                    self.snowballs.remove(sid);
                                    break;
                                } else {
                                    if let Some(sbm) = self.snowballs.get_mut(sid) {
                                        let delta = sb_pos - Vec2::new(*x, *y);
                                        let dist = delta.length().max(0.0001);
                                        let n = delta / dist;
                                        sbm.pos =
                                            Vec2::new(*x, *y) + n * (*radius + SNOWBALL_RADIUS);
                                        sbm.vel = sbm.vel - 2.0 * sbm.vel.dot(n) * n * (*factor);
                                    }
                                }
                            }
                        }

                        MapObject::Rect {
                            x,
                            y,
                            w,
                            h,
                            factor,
                            color,
                            is_hole,
                        } => {
                            if circle_intersects_rect(
                                sb_pos.x,
                                sb_pos.y,
                                SNOWBALL_RADIUS,
                                *x,
                                *y,
                                *w,
                                *h,
                            ) {
                                if *is_hole {
                                    self.snowballs.remove(sid);
                                    break;
                                } else {
                                    if let Some(sbm) = self.snowballs.get_mut(sid) {
                                        let cx = sb_pos.x.clamp(*x, x + w);
                                        let cy = sb_pos.y.clamp(*y, y + h);
                                        let mut n = (sb_pos - Vec2::new(cx, cy));
                                        if n.length_squared() < 1e-6 {
                                            // choose axis direction
                                            n = Vec2::new(
                                                (sb_pos.x - (x + w / 2.0)).signum(),
                                                (sb_pos.y - (y + h / 2.0)).signum(),
                                            );
                                        }
                                        let n = n.normalize_or_zero();
                                        sbm.pos += n * (SNOWBALL_RADIUS * 0.5 + 0.5);
                                        sbm.vel = sbm.vel - 2.0 * sbm.vel.dot(n) * n * factor;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn snapshot(&self) -> (Vec<PlayerState>, Vec<SnowballState>) {
        let players = self
            .players
            .values()
            .map(|p| PlayerState {
                id: p.id.clone(),
                pos: [p.pos.x, p.pos.y],
                vel: [p.vel.x, p.vel.y],
                rot_deg: p.rot_deg,
            })
            .collect();

        let snowballs = self
            .snowballs
            .values()
            .map(|s| SnowballState {
                id: s.id,
                pos: [s.pos.x, s.pos.y],
                vel: [s.vel.x, s.vel.y],
                life: s.life,
            })
            .collect();

        (players, snowballs)
    }
}

async fn physics_loop(game_state: Arc<Mutex<GameState>>, peers: PeerMap) {
    let tick = Duration::from_secs_f32(DT);
    let mut last = Instant::now();

    loop {
        let now = Instant::now();
        let elapsed = now.duration_since(last);
        if elapsed >= tick {
            // step physics once (we can step multiple times if behind; keep simple)
            {
                let mut gs = game_state.lock().unwrap();
                gs.step(DT);
                let (players, snowballs) = gs.snapshot();
                let msg = ServerMessage::WorldState { players, snowballs };
                let txt = serde_json::to_string(&msg).unwrap();

                // broadcast to all peers
                let peers_guard = peers.lock().unwrap();
                for (_id, tx) in peers_guard.iter() {
                    let _ = tx.send(Message::Text(txt.clone().into()));
                }
            }
            last = now;
        } else {
            tokio::time::sleep(tick - elapsed).await;
        }
    }
}

/// Returns squared distance between two points (avoid sqrt unless needed)
#[inline]
fn dist2(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = ax - bx;
    let dy = ay - by;
    dx * dx + dy * dy
}

/// Returns true if a circle at (px,py) with radius `r_entity` intersects
/// a circle at (x,y) with radius `r_obj`.
#[inline]
fn circle_intersects_circle(px: f32, py: f32, r_entity: f32, x: f32, y: f32, r_obj: f32) -> bool {
    dist2(px, py, x, y) < (r_entity + r_obj) * (r_entity + r_obj)
}

/// Returns true if a circle at (px,py) with radius `r_entity` intersects
/// an axis-aligned rectangle at (x,y) size (w,h).
#[inline]
fn circle_intersects_rect(px: f32, py: f32, r_entity: f32, x: f32, y: f32, w: f32, h: f32) -> bool {
    // Find closest point on rect to circle center
    let closest_x = px.clamp(x, x + w);
    let closest_y = py.clamp(y, y + h);
    dist2(px, py, closest_x, closest_y) < r_entity * r_entity
}
