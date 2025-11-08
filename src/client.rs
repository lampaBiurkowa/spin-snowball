use ggez::event::{self, EventHandler};
use ggez::glam::Vec2;
use ggez::graphics::{self, Color, DrawMode, MeshBuilder};
use ggez::input::keyboard::{KeyCode, KeyInput};
use ggez::{Context, ContextBuilder, GameError, GameResult};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::net::TcpStream;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread;
use std::time::Duration;
use tungstenite::{Message, WebSocket, connect};
use url::Url;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MapObject {
    Circle {
        x: f32,
        y: f32,
        radius: f32,
        obj_type: ObjectType,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        obj_type: ObjectType,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectType {
    Hole,
    Wall,
    Bouncy {
        factor: f32, // >1 stronger bounce, 1 normal, <1 weak, <0 inverts velocity
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct GameMap {
    pub name: String,
    pub width: f32,
    pub height: f32,
    pub objects: Vec<MapObject>,
}

const SCREEN_W: f32 = 800.0;
const SCREEN_H: f32 = 600.0;

/// --- Message types (must match server) ---
#[derive(Serialize, Deserialize, Debug, Clone)]
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

#[derive(Serialize, Deserialize, Debug, Clone)]
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

/// --- Game structs ---
struct Player {
    id: Option<String>, // assigned by server
    pos: Vec2,
    vel: Vec2,
    rotation: f32, // degrees
    radius: f32,

    rotating_left: bool,
    rotating_right: bool,
    spin_timer: f32,

    rotation_speed_deg: f32,
    max_charge: f32,
}

struct Snowball {
    id: u64,
    pos: Vec2,
    vel: Vec2,
    life: f32,
}

struct MainState {
    player: Player,
    other_players: Vec<PlayerState>, // for rendering other players (simple)
    snowballs: Vec<Snowball>,

    friction: f32,

    // networking
    net_tx: Sender<ClientMessage>,   // to network thread
    net_rx: Receiver<ServerMessage>, // from network thread

    my_id: Option<String>,
    last_sent_input: (bool, bool, bool),
    map: GameMap,
}

impl Player {
    fn new(x: f32, y: f32) -> Self {
        Self {
            id: None,
            pos: Vec2::new(x, y),
            vel: Vec2::new(0.0, 0.0),
            rotation: -90.0,
            radius: 18.0,
            rotating_left: false,
            rotating_right: false,
            spin_timer: 0.0,
            rotation_speed_deg: 180.0,
            max_charge: 1.5,
        }
    }

    fn forward_vector(&self) -> Vec2 {
        let r = self.rotation.to_radians();
        Vec2::new(r.cos(), r.sin())
    }
}

impl MainState {
    fn new(server_addr: &str) -> GameResult<MainState> {
        // spawn networking thread and get channels
        let (to_net, from_net) = spawn_network_thread(server_addr);

        let map_data =
            std::fs::read_to_string("default_map.json").expect("Failed to load default_map.json");
        let map: GameMap = serde_json::from_str(&map_data).expect("Invalid map json");
        let s = MainState {
            player: Player::new(SCREEN_W / 2.0, SCREEN_H / 2.0),
            other_players: Vec::new(),
            snowballs: Vec::new(),
            friction: 0.98,
            net_tx: to_net,
            net_rx: from_net,
            my_id: None,
            last_sent_input: (false, false, false),
            map,
        };
        Ok(s)
    }

    fn shoot_from_local(&mut self) {
        // still compute charge because recoil depends on it
        let charge = self.player.spin_timer.min(self.player.max_charge);
        let charge_t = charge / self.player.max_charge;
        let base_speed = 280.0;
        let bonus_speed = 700.0 * charge_t;
        let snowball_speed = base_speed + bonus_speed;
        let dir = self.player.forward_vector();

        // ✅ no local snowball spawn
        // ✅ only apply recoil immediately
        let recoil_strength = 0.45 + 1.0 * charge_t;
        self.player.vel -= dir * (snowball_speed * recoil_strength / 3.0);

        // reset the charge timer
        self.player.spin_timer = 0.0;
    }

    fn forward_input_if_changed(&mut self) {
        let input = (self.player.rotating_left, self.player.rotating_right, false);
        if input != self.last_sent_input {
            // only send left/right changes here; shoot will be sent on release
            let msg = ClientMessage::Input {
                left: self.player.rotating_left,
                right: self.player.rotating_right,
                shoot: false,
            };
            let _ = self.net_tx.send(msg);
            self.last_sent_input = input;
        }
    }
}

impl EventHandler for MainState {
    fn update(&mut self, ctx: &mut Context) -> GameResult<()> {
        let dt = ctx.time.delta().as_secs_f32();

        // handle incoming server messages (non-blocking)
        loop {
            match self.net_rx.try_recv() {
                Ok(sm) => match sm {
                    ServerMessage::AssignId { id } => {
                        println!("Assigned id by server: {}", id);
                        self.my_id = Some(id.clone());
                        self.player.id = Some(id);
                    }
                    ServerMessage::WorldState { players, snowballs } => {
                        // update local copy: find our player and snap to authoritative
                        if let Some(my_id) = &self.my_id {
                            for p in &players {
                                if &p.id == my_id {
                                    self.player.pos = Vec2::new(p.pos[0], p.pos[1]);
                                    self.player.vel = Vec2::new(p.vel[0], p.vel[1]);
                                    self.player.rotation = p.rot_deg;
                                }
                            }
                        }
                        // store other players for drawing (simple)
                        self.other_players = players.into_iter().collect();

                        // replace authoritative snowballs (client-local with id==0 coexist)
                        self.snowballs.clear();
                        for sb in snowballs {
                            self.snowballs.push(Snowball {
                                id: sb.id,
                                pos: Vec2::new(sb.pos[0], sb.pos[1]),
                                vel: Vec2::new(sb.vel[0], sb.vel[1]),
                                life: sb.life,
                            });
                        }
                    }
                    ServerMessage::Pong { ts: _ } => {}
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    println!("Network thread disconnected");
                    break;
                }
            }
        }

        // rotation while holding (client-side visuals)
        if self.player.rotating_left {
            self.player.rotation -= self.player.rotation_speed_deg * dt;
            self.player.spin_timer += dt;
        }
        if self.player.rotating_right {
            self.player.rotation += self.player.rotation_speed_deg * dt;
            self.player.spin_timer += dt;
        }

        // integrate
        self.player.pos += self.player.vel * dt;
        self.player.vel *= self.friction.powf(dt * 60.0);

        // clamp to screen
        self.player.pos.x = self.player.pos.x.clamp(0.0, SCREEN_W);
        self.player.pos.y = self.player.pos.y.clamp(0.0, SCREEN_H);

        // update local snowballs (visuals)
        for sb in &mut self.snowballs {
            sb.pos += sb.vel * dt;
            sb.vel *= 0.995f32;
            sb.life -= dt;
        }
        self.snowballs.retain(|s| s.life > 0.0);

        // send input changes if needed
        self.forward_input_if_changed();

        // occasional ping (optional)
        // if you want, send ping messages to measure latency
        if ggez::timer::ticks(ctx) % 300 == 0 {
            let _ = self.net_tx.send(ClientMessage::Ping { ts: 0 });
        }

        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
        let mut canvas = graphics::Canvas::from_frame(ctx, Color::from_rgb(20, 20, 30));

        // grid
        let mut mb = MeshBuilder::new();
        for x in (0..=(SCREEN_W as i32)).step_by(40) {
            mb.line(
                &[Vec2::new(x as f32, 0.0), Vec2::new(x as f32, SCREEN_H)],
                1.0,
                Color::from_rgb(40, 40, 40),
            )?;
        }
        for y in (0..=(SCREEN_H as i32)).step_by(40) {
            mb.line(
                &[Vec2::new(0.0, y as f32), Vec2::new(SCREEN_W, y as f32)],
                1.0,
                Color::from_rgb(40, 40, 40),
            )?;
        }

        // ============================================
        // ✅ Draw Map
        // ============================================
        for obj in &self.map.objects {
            match obj {
                MapObject::Circle {
                    x,
                    y,
                    radius,
                    obj_type,
                } => {
                    let col = match obj_type {
                        ObjectType::Hole => Color::from_rgb(10, 10, 10), // black
                        ObjectType::Wall => Color::from_rgb(150, 150, 170), // grey
                        ObjectType::Bouncy { .. } => Color::from_rgb(80, 200, 255), // blue
                    };
                    mb.circle(DrawMode::fill(), Vec2::new(*x, *y), *radius, 0.5, col)?;
                }

                MapObject::Rect {
                    x,
                    y,
                    w,
                    h,
                    obj_type,
                } => {
                    let col = match obj_type {
                        ObjectType::Hole => Color::from_rgb(10, 10, 10),
                        ObjectType::Wall => Color::from_rgb(150, 150, 170),
                        ObjectType::Bouncy { .. } => Color::from_rgb(80, 200, 255),
                    };
                    mb.rectangle(DrawMode::fill(), graphics::Rect::new(*x, *y, *w, *h), col)?;
                }
            }
        }
        // ============================================

        // draw other players (from authoritative snapshot)
        for p in &self.other_players {
            // avoid drawing self twice
            if let Some(my_id) = &self.my_id {
                if &p.id == my_id {
                    continue;
                }
            }
            let col = Color::from_rgb(180, 180, 220);
            mb.circle(
                DrawMode::fill(),
                Vec2::new(p.pos[0], p.pos[1]),
                16.0,
                0.5,
                col,
            )?;
        }

        // draw local player
        let player_color = Color::from_rgb(200, 200, 255);
        mb.circle(
            DrawMode::fill(),
            Vec2::new(self.player.pos.x, self.player.pos.y),
            self.player.radius,
            0.5,
            player_color,
        )?;

        // direction indicator triangle for local player
        let dir = self.player.forward_vector();
        let tip = Vec2::new(
            self.player.pos.x + dir.x * (self.player.radius + 8.0),
            self.player.pos.y + dir.y * (self.player.radius + 8.0),
        );
        let left = Vec2::new(
            self.player.pos.x + (-dir.y) * 8.0,
            self.player.pos.y + (dir.x) * 8.0,
        );
        let right = Vec2::new(
            self.player.pos.x + (dir.y) * 8.0,
            self.player.pos.y + (-dir.x) * 8.0,
        );
        mb.polygon(
            DrawMode::fill(),
            &[tip, left, right],
            Color::from_rgb(255, 100, 100),
        )?;

        // snowballs
        for sb in &self.snowballs {
            let c = { Color::WHITE };
            mb.circle(DrawMode::fill(), Vec2::new(sb.pos.x, sb.pos.y), 6.0, 0.5, c)?;
        }

        let mesh = mb.build();
        let mesh = graphics::Mesh::from_data(&ctx.gfx, mesh);
        canvas.draw(&mesh, ggez::graphics::DrawParam::default());

        // HUD: charge bar
        let bar_w = 200.0;
        let bar_h = 12.0;
        let x = 20.0;
        let y = SCREEN_H - 30.0;
        let charge = (self.player.spin_timer / self.player.max_charge).clamp(0.0, 1.0);
        let bar_back = graphics::Mesh::new_rectangle(
            ctx,
            DrawMode::fill(),
            graphics::Rect::new(x, y, bar_w, bar_h),
            Color::from_rgba(40, 40, 40, 200),
        )?;
        let bar_front = graphics::Mesh::new_rectangle(
            ctx,
            DrawMode::fill(),
            graphics::Rect::new(x, y, bar_w * charge, bar_h),
            Color::from_rgba(120, 200, 255, 200),
        )?;
        canvas.draw(&bar_back, graphics::DrawParam::default());
        canvas.draw(&bar_front, graphics::DrawParam::default());

        canvas.finish(ctx)
    }

    // key_down starts rotation (and client sends input if changed)
    fn key_down_event(
        &mut self,
        _ctx: &mut Context,
        input: KeyInput,
        _repeat: bool,
    ) -> Result<(), GameError> {
        match input.keycode {
            Some(KeyCode::Left) | Some(KeyCode::A) => {
                self.player.rotating_left = true;
                // send immediate input change
                let _ = self.net_tx.send(ClientMessage::Input {
                    left: true,
                    right: false,
                    shoot: false,
                });
                self.last_sent_input = (true, false, false);
            }
            Some(KeyCode::Right) | Some(KeyCode::D) => {
                self.player.rotating_right = true;
                let _ = self.net_tx.send(ClientMessage::Input {
                    left: false,
                    right: true,
                    shoot: false,
                });
                self.last_sent_input = (false, true, false);
            }
            Some(KeyCode::Space) => {
                // quick local shoot and tell server too
                self.shoot_from_local();
                let _ = self.net_tx.send(ClientMessage::Input {
                    left: false,
                    right: false,
                    shoot: true,
                });
                self.last_sent_input = (false, false, false);
            }
            _ => {}
        }
        Ok(())
    }

    // key_up releases rotation -> shoot (send shoot=true)
    fn key_up_event(&mut self, _ctx: &mut Context, input: KeyInput) -> Result<(), GameError> {
        match input.keycode {
            Some(KeyCode::Left) | Some(KeyCode::A) => {
                if self.player.rotating_left {
                    self.player.rotating_left = false;
                    // local immediate feedback
                    self.shoot_from_local();
                    // tell server we shoot
                    let _ = self.net_tx.send(ClientMessage::Input {
                        left: false,
                        right: false,
                        shoot: true,
                    });
                    self.last_sent_input = (false, false, false);
                }
            }
            Some(KeyCode::Right) | Some(KeyCode::D) => {
                if self.player.rotating_right {
                    self.player.rotating_right = false;
                    self.shoot_from_local();
                    let _ = self.net_tx.send(ClientMessage::Input {
                        left: false,
                        right: false,
                        shoot: true,
                    });
                    self.last_sent_input = (false, false, false);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn spawn_network_thread(server_addr: &str) -> (Sender<ClientMessage>, Receiver<ServerMessage>) {
    let (to_net_tx, to_net_rx) = channel::<ClientMessage>();
    let (from_net_tx, from_net_rx) = channel::<ServerMessage>();
    let server = server_addr.to_string();

    thread::spawn(move || {
        let url = Url::parse(&format!("ws://{}", server)).expect("Invalid WebSocket URL");
        println!("Connecting to {}", url);

        let (mut socket, _response) = match connect(url.to_string()) {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("WebSocket connect error: {}", e);
                return;
            }
        };

        // Optional: set read timeout so thread doesn’t block forever
        // if let Some(underlying) = socket.get_mut().get_mut() {
        //     let _ = underlying.set_read_timeout(Some(Duration::from_millis(10)));
        // }

        loop {
            // 1. Send all pending outbound messages
            while let Ok(msg) = to_net_rx.try_recv() {
                if let Ok(txt) = serde_json::to_string(&msg) {
                    if socket.send(Message::Text(txt.into())).is_err() {
                        eprintln!("Write error, closing network thread");
                        return;
                    }
                }
            }

            // 2. Try to read one incoming message (blocking up to 10 ms)
            match socket.read() {
                Ok(Message::Text(txt)) => {
                    if let Ok(sm) = serde_json::from_str::<ServerMessage>(&txt) {
                        let _ = from_net_tx.send(sm);
                    }
                }
                Err(tungstenite::Error::Io(ref e))
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    // just timeout, no problem
                }
                Err(tungstenite::Error::ConnectionClosed) => {
                    println!("Server closed connection");
                    return;
                }
                Err(e) => {
                    eprintln!("Read error: {}", e);
                    return;
                }
                _ => {}
            }

            // Small sleep to avoid busy loop
            thread::sleep(Duration::from_millis(2));
        }
    });

    (to_net_tx, from_net_rx)
}

pub fn main() -> GameResult {
    let (ctx, event_loop) = ContextBuilder::new("snowball_spin_net", "you")
        .window_setup(ggez::conf::WindowSetup::default().title("Snowball Spin - Networked Client"))
        .window_mode(ggez::conf::WindowMode::default().dimensions(SCREEN_W, SCREEN_H))
        .build()?;

    // change server address here if different
    let server_addr = "127.0.0.1:9001";
    let state = MainState::new(server_addr)?;
    event::run(ctx, event_loop, state)
}
