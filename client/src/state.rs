use crate::{
    map::GameMap,
    network::{BallState, MatchPhase, PlayerState, PlayerStatus, SnowballState, TeamColor},
};
use ggez::glam::Vec2;
use std::collections::HashMap;

pub struct Player {
    pub id: Option<String>,
    pub pos: Vec2,
    pub vel: Vec2,
    pub rotation: f32,
    pub radius: f32,

    pub spin_timer: f32,
    pub max_charge: f32,
}

pub struct Snowball {
    pub id: u64,
    pub pos: Vec2,
    pub vel: Vec2,
    pub life: f32,
}

pub struct Ball {
    pub pos: Vec2,
    pub vel: Vec2,
    pub radius: f32,
}

pub struct GameState {
    pub player: Player,
    pub other_players: Vec<PlayerState>,
    pub snowballs: Vec<Snowball>,
    pub ball: Option<Ball>,
    pub scores: HashMap<String, u32>,
    pub map: GameMap,
    pub friction: f32,
    pub phase: MatchPhase,
    pub time_elapsed: f32,
    pub all_players: Vec<PlayerState>,
    pub player_status: PlayerStatus,
    pub paused: bool,
    pub team1_color: TeamColor,
    pub team2_color: TeamColor,
}

impl GameState {
    pub fn new(map: GameMap) -> Self {
        let center = Vec2::new(map.width / 2.0, map.height / 2.0);
        Self {
            player: Player {
                id: None,
                pos: center,
                vel: Vec2::ZERO,
                rotation: -90.0,
                radius: 18.0,
                spin_timer: 0.0,
                max_charge: 1.5,
            },
            other_players: vec![],
            snowballs: vec![],
            ball: None,
            scores: HashMap::new(),
            friction: map.physics.friction_per_frame,
            map,
            phase: MatchPhase::Lobby,
            time_elapsed: Default::default(),
            all_players: vec![],
            player_status: PlayerStatus::Spectator,
            paused: Default::default(),
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

    pub fn apply_world_state(
        &mut self,
        players: Vec<PlayerState>,
        snowballs: Vec<SnowballState>,
        ball: Option<BallState>,
        scores: HashMap<String, u32>,
        phase: MatchPhase,
        time_elapsed: f32,
        paused: bool,
        team1_color: TeamColor,
        team2_color: TeamColor,
    ) {
        if let Some(id) = &self.player.id {
            for p in &players {
                if &p.id == id {
                    self.player.pos = Vec2::new(p.pos[0], p.pos[1]);
                    self.player.vel = Vec2::new(p.vel[0], p.vel[1]);
                    self.player.rotation = p.rot_deg;
                }
            }
        }

        self.other_players = players
            .clone()
            .into_iter()
            .filter(|p| {
                // do not include yourself
                if Some(&p.id) == self.player.id.as_ref() {
                    return false;
                }
                // do not draw spectators
                matches!(p.status, PlayerStatus::Playing(_))
            })
            .collect();
        self.snowballs = snowballs
            .into_iter()
            .map(|sb| Snowball {
                id: sb.id,
                pos: Vec2::new(sb.pos[0], sb.pos[1]),
                vel: Vec2::new(sb.vel[0], sb.vel[1]),
                life: sb.life,
            })
            .collect();
        self.scores = scores;

        self.ball = ball.map(|b| Ball {
            pos: Vec2::new(b.pos[0], b.pos[1]),
            vel: Vec2::new(b.vel[0], b.vel[1]),
            radius: self.map.physics.ball_radius,
        });
        self.time_elapsed = time_elapsed;
        self.phase = phase;
        self.all_players = players.clone();
        if let Some(me) = players
            .iter()
            .find(|p| Some(&p.id) == self.player.id.as_ref())
        {
            self.player_status = me.status.clone();
        }
        self.paused = paused;
        self.team1_color = team1_color;
        self.team2_color = team2_color;
    }

    pub fn forward_vector(&self) -> Vec2 {
        let r = self.player.rotation.to_radians();
        Vec2::new(r.cos(), r.sin())
    }
}
