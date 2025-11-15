use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use glam::Vec2;
use tokio::net::TcpListener;
use tokio::sync::mpsc::UnboundedSender;
use tokio_tungstenite::tungstenite::Message;

use crate::map::{GameMap, GameMode};
use crate::network::{BallState, PlayerState, ServerMessage, SnowballState, handle_connection};
use crate::physics::{simulate_collisions, simulate_movement};

mod map;
mod network;
mod physics;

const TICK_HZ: f32 = 60.0;
const DT: f32 = 1.0 / TICK_HZ;

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

    let map: GameMap = {
        let data = std::fs::read_to_string("default_map.json")?;
        serde_json::from_str(&data)?
    };

    let game_state = Arc::new(Mutex::new(GameState::new(map)));

    {
        let peers = peers.clone();
        let game_state = game_state.clone();
        tokio::spawn(async move {
            physics_loop(game_state, peers).await;
        });
    }

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

#[derive(Clone)]
struct Ball {
    pos: Vec2,
    vel: Vec2,
}

struct GameState {
    players: HashMap<String, Player>,
    snowballs: HashMap<u64, Snowball>,
    next_snowball_id: u64,
    map: GameMap,
    scores: HashMap<String, u32>,
    ball: Option<Ball>,
}

impl GameState {
    fn new(map: GameMap) -> Self {
        let ball = match map.mode {
            GameMode::Football => {
                let b = &map.football.as_ref().unwrap().ball;
                Some(Ball {
                    pos: Vec2::new(b.spawn_x, b.spawn_y),
                    vel: Vec2::ZERO,
                })
            }
            _ => None,
        };
        println!("HEJA {:?}" , ball.is_some() );

        Self {
            players: HashMap::new(),
            snowballs: HashMap::new(),
            next_snowball_id: 1,
            scores: HashMap::new(),
            ball,
            map,
        }
    }

    fn add_player(&mut self, id: String) {
        let pos = Vec2::new(
            rand::random::<f32>() * (self.map.width - 40.0) + 20.0,
            rand::random::<f32>() * (self.map.height - 40.0) + 20.0,
        );
        self.scores.insert(id.clone(), 0);
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

    fn logic_step(&mut self, dt: f32) {
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
                gs.logic_step(DT);
                simulate_movement(&mut gs, DT);
                let response = simulate_collisions(&mut gs);

                for id in response.players_in_holes.into_iter() {
                    if gs.map.mode == GameMode::Fight {
                        if let Some(p) = gs.players.get_mut(&id) {
                            p.pos = Vec2::new(100.0, 100.0); //respawn pos
                            p.vel = Vec2::ZERO;
                        }
                        for (other_id, score) in gs.scores.iter_mut() {
                            if *other_id != id {
                                *score += 1;
                            }
                        }
                    }
                }

                for sid in response.snowballs_in_holes.into_iter() {
                    gs.snowballs.remove(&sid);
                }

                if let Some(scoring_team) = response.goal_for_team {
                    gs.scores
                        .entry(format!("team_{}", scoring_team))
                        .and_modify(|x| *x += 1)
                        .or_insert(1);

                    let b = gs.map.football.clone().unwrap().ball;
                    if let Some(ball) = &mut gs.ball {
                        ball.pos = Vec2::new(b.spawn_x, b.spawn_y);
                        ball.vel = Vec2::ZERO;
                    }
                }

                let (players, snowballs) = gs.snapshot();
                let msg = ServerMessage::WorldState { players, snowballs, ball: gs.ball.clone().map(|x| BallState { pos: x.pos.into(), vel: x.vel.into() }), scores: gs.scores.clone() };
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
