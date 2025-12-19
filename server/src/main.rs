use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use glam::Vec2;
use rand::Rng;
use spin_snowball_shared::*;
use tokio::net::TcpListener;
use tokio::sync::mpsc::UnboundedSender;
use tokio_tungstenite::tungstenite::Message;

// use crate::map::{GameMap, GameMode};
use crate::network::handle_connection;
use crate::physics::{simulate_collisions, simulate_movement};

mod network;
mod physics;

const TICK_HZ: f32 = 60.0;
const DT: f32 = 1.0 / TICK_HZ;

struct Player {
    id: String,
    nick: String,
    pos: Vec2,
    vel: Vec2,
    rot_deg: f32,
    rotating_left: bool,
    rotating_right: bool,
    spin_timer: f32,
    last_shoot_pressed: bool,
    status: PlayerStatus,
}

struct Snowball {
    id: u64,
    pos: Vec2,
    vel: Vec2,
    life: f32,
}

#[derive(Debug, Clone)]
pub struct MatchTimer {
    accumulated: Duration,
    running: bool,
    last_start: Option<Instant>,
}

impl MatchTimer {
    fn new() -> Self {
        Self {
            accumulated: Duration::ZERO,
            running: false,
            last_start: None,
        }
    }

    fn start(&mut self) {
        if !self.running {
            self.running = true;
            self.last_start = Some(Instant::now());
        }
    }

    fn pause(&mut self) {
        if self.running {
            if let Some(since) = self.last_start {
                let elapsed = Instant::now().duration_since(since);
                self.accumulated += elapsed;
            }
            self.running = false;
            self.last_start = None;
        }
    }

    fn reset(&mut self) {
        self.accumulated = Duration::ZERO;
        self.running = false;
        self.last_start = None;
    }

    fn elapsed(&self) -> Duration {
        if self.running {
            if let Some(since) = self.last_start {
                return self.accumulated + Instant::now().duration_since(since);
            }
        }
        self.accumulated
    }

    fn elapsed_secs(&self) -> f32 {
        let d = self.elapsed();
        d.as_secs_f32()
    }

    fn set_elapsed(&mut self, d: Duration) {
        self.accumulated = d;
        if self.running {
            self.last_start = Some(Instant::now());
        }
    }

    fn is_running(&self) -> bool {
        self.running
    }
}

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<String, Tx>>>;

fn load_map_form_data(data: &str) -> GameMap {
    serde_json::from_str(data).unwrap()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:9001".to_string());
    println!("Starting server on {}", addr);

    let listener = TcpListener::bind(&addr).await?;
    let peers: PeerMap = Arc::new(Mutex::new(HashMap::new()));
    let map = load_map_form_data(&std::fs::read_to_string("default_map.json").unwrap());
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
    scores: HashMap<Team, u32>,
    ball: Option<Ball>,
    phase: MatchPhase,
    timer: MatchTimer,
    paused: bool,
    team1_color: TeamColor,
    team2_color: TeamColor,
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
        println!("HEJA {:?}", ball.is_some());

        Self {
            players: HashMap::new(),
            snowballs: HashMap::new(),
            next_snowball_id: 1,
            scores: HashMap::new(),
            ball,
            map,
            phase: MatchPhase::Lobby,
            timer: MatchTimer::new(),
            paused: false,
            team1_color: TeamColor {
                r: 200,
                g: 0,
                b: 0,
                a: 255,
            },
            team2_color: TeamColor {
                r: 0,
                g: 0,
                b: 200,
                a: 255,
            },
        }
    }

    pub fn add_new_player(&mut self, id: String) {
        let pos = Vec2::new(
            rand::random::<f32>() * (self.map.width - 40.0) + 20.0,
            rand::random::<f32>() * (self.map.height - 40.0) + 20.0,
        );

        self.players.insert(
            id.clone(),
            Player {
                id,
                nick: format!("Player {}", self.players.len() + 1),
                pos,
                vel: Vec2::ZERO,
                rot_deg: -90.0,
                rotating_left: false,
                rotating_right: false,
                spin_timer: 0.0,
                last_shoot_pressed: false,
                status: PlayerStatus::Spectator,
            },
        );
    }

    fn remove_player(&mut self, id: &str) {
        self.players.remove(id);
    }

    fn apply_input(&mut self, id: &str, left: bool, right: bool, shoot: bool) {
        if self.paused {
            return;
        }

        if let Some(p) = self.players.get_mut(id) {
            if let PlayerStatus::Playing(_) = p.status {
                p.rotating_left = left;
                p.rotating_right = right;
                // Edge-detect the shoot button on server side:
                // only spawn a snowball when shoot transitions from false -> true
                if shoot && !p.last_shoot_pressed {
                    // spawn based on current rotation & spin_timer
                    let charge = p.spin_timer.min(1.5);
                    let charge_t = charge / 1.5;
                    let base_speed = 300.0;
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
                            life: self.map.physics.snowball_lifetime_sec,
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
                }
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
                nick: p.nick.clone(),
                pos: [p.pos.x, p.pos.y],
                vel: [p.vel.x, p.vel.y],
                rot_deg: p.rot_deg,
                status: p.status,
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

    fn load_map(&mut self, data: &str) {
        self.map = serde_json::from_str(&data).unwrap();
        self.reset_positions();
    }

    pub fn start_match(&mut self, score_limit: Option<u32>, time_limit_secs: Option<u32>) {
        println!("match started: {:?} {:?}", score_limit, time_limit_secs);

        self.scores.clear();
        self.scores.insert(Team::Team1, 0);
        self.scores.insert(Team::Team2, 0);
        self.reset_positions();
        if let GameMode::Football = self.map.mode {
            if let Some(b) = &self.map.football {
                let ball_info = &b.ball;
                self.ball = Some(Ball {
                    pos: Vec2::new(ball_info.spawn_x, ball_info.spawn_y),
                    vel: Vec2::ZERO,
                });
            }
        }

        self.phase = MatchPhase::Playing {
            score_limit,
            time_limit_secs,
        };
        self.timer.reset();
        self.timer.start();
    }

    pub fn stop_match(&mut self) {
        self.phase = MatchPhase::Lobby;
        self.timer.pause();
        for p in self.players.values_mut() {
            p.status = PlayerStatus::Spectator;
        }
    }

    pub fn pause_match(&mut self) {
        if let MatchPhase::Playing { .. } = &self.phase {
            self.paused = true;
            self.timer.pause();
        }
    }

    pub fn resume_match(&mut self) {
        if let MatchPhase::Playing { .. } = &self.phase {
            self.paused = false;
            self.timer.start();
        }
    }

    pub fn reset_positions(&mut self) {
        let mut rng = rand::rng();
        for p in self.players.values_mut() {
            match p.status {
                PlayerStatus::Playing(Team::Team1) => {
                    let x = self.map.team1.spawn_x;
                    let y = self.map.team1.spawn_y;
                    p.pos = Vec2::new(x, y);
                    p.vel = Vec2::ZERO;
                    p.rot_deg = -90.0;
                }
                PlayerStatus::Playing(Team::Team2) => {
                    let x = self.map.team2.spawn_x;
                    let y = self.map.team2.spawn_y;
                    p.pos = Vec2::new(x, y);
                    p.vel = Vec2::ZERO;
                    p.rot_deg = -90.0;
                }
                PlayerStatus::Spectator => ()
            }
        }
        
        if let Some(x) = self.map.football.clone() {
            if let Some(ball) = &mut self.ball {
                ball.pos = Vec2::new(x.ball.spawn_x, x.ball.spawn_y);
                ball.vel = Vec2::ZERO;
            }
        }
    }

    pub fn check_end_conditions(&mut self) -> bool {
        if let MatchPhase::Playing {
            score_limit,
            time_limit_secs,
        } = &self.phase
        {
            // Score limit checks (unchanged)
            if let Some(limit) = score_limit {
                if let Some(&s1) = self.scores.get(&Team::Team1) {
                    if s1 >= *limit {
                        self.phase = MatchPhase::Lobby;
                        self.timer.pause();
                        for p in self.players.values_mut() {
                            p.status = PlayerStatus::Spectator;
                        }
                        return true;
                    }
                }
                if let Some(&s2) = self.scores.get(&Team::Team2) {
                    if s2 >= *limit {
                        self.phase = MatchPhase::Lobby;
                        self.timer.pause();
                        for p in self.players.values_mut() {
                            p.status = PlayerStatus::Spectator;
                        }
                        return true;
                    }
                }
            }

            if let Some(secs) = time_limit_secs {
                let elapsed_secs = self.timer.elapsed_secs();
                if elapsed_secs >= *secs as f32 {
                    self.phase = MatchPhase::Lobby;
                    self.timer.pause();
                    for p in self.players.values_mut() {
                        p.status = PlayerStatus::Spectator;
                    }
                    return true;
                }
            }
        }
        false
    }
}

async fn physics_loop(game_state: Arc<Mutex<GameState>>, peers: PeerMap) {
    let tick = Duration::from_secs_f32(DT);
    let mut last = Instant::now();

    loop {
        let now = Instant::now();
        let elapsed = now.duration_since(last);
        if elapsed >= tick {
            {
                let mut gs = game_state.lock().unwrap();
                if gs.paused {
                    let (players, snowballs) = gs.snapshot();
                    let msg = ServerMessage::WorldState {
                        players,
                        snowballs,
                        ball: gs.ball.clone().map(|x| BallState {
                            pos: x.pos.into(),
                            vel: x.vel.into(),
                        }),
                        scores: gs.scores.clone(),
                        phase: gs.phase.clone(),
                        time_elapsed: gs.timer.elapsed_secs(),
                        paused: gs.paused,
                        team1_color: gs.team1_color.clone(),
                        team2_color: gs.team2_color.clone(),
                    };
                    let txt = serde_json::to_string(&msg).unwrap();

                    let peers_guard = peers.lock().unwrap();
                    for (_, tx) in peers_guard.iter() {
                        let _ = tx.send(Message::Text(txt.clone().into()));
                    }

                    last = now;
                    continue;
                }

                let phase = gs.phase.clone();

                if let MatchPhase::Playing { .. } = phase {
                    gs.logic_step(DT);
                    simulate_movement(&mut gs, DT);
                    let response = simulate_collisions(&mut gs);

                    for id in response.players_in_holes.into_iter() {
                        if gs.map.mode == GameMode::Fight {
                            if let Some(p) = gs
                                .players
                                .values_mut()
                                .find(|x| matches!(x.status, PlayerStatus::Playing(x) if x == id))
                            {
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
                        let team = if scoring_team == 1 {
                            Team::Team1
                        } else {
                            Team::Team2
                        }; // Convert raw number from collision detection
                        *gs.scores.entry(team).or_insert(0) += 1;

                        gs.reset_positions();
                    }

                    if gs.check_end_conditions() {
                        gs.stop_match();
                    }
                }

                let (players, snowballs) = gs.snapshot();
                let msg = ServerMessage::WorldState {
                    players,
                    snowballs,
                    ball: gs.ball.clone().map(|x| BallState {
                        pos: x.pos.into(),
                        vel: x.vel.into(),
                    }),
                    scores: gs.scores.clone(),
                    phase: gs.phase.clone(),
                    time_elapsed: gs.timer.elapsed_secs(),
                    paused: gs.paused,
                    team1_color: gs.team1_color.clone(),
                    team2_color: gs.team2_color.clone(),
                };
                let txt = serde_json::to_string(&msg).unwrap();

                let peers_guard = peers.lock().unwrap();
                for (_id, tx) in peers_guard.iter() {
                    // println!("sending: {txt}");
                    let _ = tx.send(Message::Text(txt.clone().into()));
                }
            }
            last = now;
        } else {
            tokio::time::sleep(tick - elapsed).await;
        }
    }
}
