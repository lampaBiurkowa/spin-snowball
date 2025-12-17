use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ClientMessage {
    Input {
        left: bool,
        right: bool,
        shoot: bool,
    },
    Ping {
        ts: u64,
    },
    Command {
        cmd: Command,
    },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "cmd")]
pub enum Command {
    Start {
        score_limit: Option<u32>,
        time_limit_secs: Option<u32>,
    },
    Stop,
    Pause,
    Resume,
    LoadMap {
        data: String,
    },
    JoinAsPlayer {
        team: Team,
    },
    JoinAsSpectator,
    SetNick {
        nick: String,
    },
    SetTeamColor {
        color: TeamColor,
        team: Team,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Team {
    Team1,
    Team2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PlayerStatus {
    Spectator,
    Playing(Team),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MatchPhase {
    Lobby,
    Playing {
        score_limit: Option<u32>,
        time_limit_secs: Option<u32>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TeamColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ServerMessage {
    AssignId {
        id: String,
    },
    WorldState {
        players: Vec<PlayerState>,
        snowballs: Vec<SnowballState>,
        scores: std::collections::HashMap<Team, u32>,
        ball: Option<BallState>,
        phase: MatchPhase,
        time_elapsed: f32,
        paused: bool,
        team1_color: TeamColor,
        team2_color: TeamColor,
    },
    Pong {
        ts: u64,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BallState {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerState {
    pub id: String,
    pub nick: String,
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub rot_deg: f32,
    pub status: PlayerStatus,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SnowballState {
    pub id: u64,
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub life: f32,
}

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(PartialEq)]
pub enum CollisionMaskTag {
    Ball,
    PlayerTeam1,
    PlayerTeam2,
    Snowball,
}

pub fn matches_ball(mask: &Vec<CollisionMaskTag>) -> bool {
    mask.contains(&CollisionMaskTag::Ball)
}

pub fn matches_player(mask: &Vec<CollisionMaskTag>, team: Team) -> bool {
    match team {
        Team::Team1 => mask.contains(&CollisionMaskTag::PlayerTeam1),
        Team::Team2 => mask.contains(&CollisionMaskTag::PlayerTeam2),
    }
}

pub fn matches_snowball(mask: &Vec<CollisionMaskTag>) -> bool {
    mask.contains(&CollisionMaskTag::Snowball)
}

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
        mask: Vec<CollisionMaskTag>,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        factor: f32,
        color: ColorDef,
        is_hole: bool,
        mask: Vec<CollisionMaskTag>,
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
    pub physics: PhysicsSettings,
    pub mode: GameMode,
    pub team1: TeamDef,
    pub team2: TeamDef,
    pub football: Option<FootballSettings>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PhysicsSettings {
    pub player_radius: f32,
    pub player_mass: f32,

    pub snowball_radius: f32,
    pub snowball_mass: f32,

    pub player_bounciness: f32,
    pub snowball_bounciness: f32,
    pub ball_radius: f32,
    pub ball_mass: f32,
    pub ball_bounciness: f32,

    pub friction_per_frame: f32,
}
impl Default for PhysicsSettings {
    fn default() -> Self {
        Self {
            player_radius: 18.0,
            player_mass: 1.0,
            snowball_radius: 8.0,
            snowball_mass: 0.5,
            player_bounciness: 0.9,
            snowball_bounciness: 0.9,
            friction_per_frame: 0.98,
            ball_bounciness: 0.8,
            ball_mass: 1.0,
            ball_radius: 10.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum GameMode {
    Fight,
    Football,
}
#[derive(Debug, Clone, Deserialize)]
pub struct TeamDef {
    pub spawn_x: f32,
    pub spawn_y: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BallDef {
    pub spawn_x: f32,
    pub spawn_y: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalDef {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub team: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FootballSettings {
    pub ball: BallDef,
    pub goals: Vec<GoalDef>,
}
