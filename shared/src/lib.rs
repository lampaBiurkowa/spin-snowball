#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
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
        score_limit: Option<u8>,
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
    SetColorDef {
        color: ColorDef,
        team: Team,
    },
    SetPhysicsSettings {
        settings: PhysicsSettings,
    },
    SetGameMode {
        game_mode: GameMode,
        action_target_time: Option<f32>,
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
        score_limit: Option<u8>,
        time_limit_secs: Option<u32>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ServerMessage {
    AssignId {
        id: String,
    },
    WorldState {
        world: WorldState,
    },
    PhysicsSettings {
        settings: PhysicsSettings,
    },
    Map {
        map: GameMap,
    },
    Pong {
        ts: u64,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorldState {
    pub players: Vec<PlayerState>,
    pub snowballs: Vec<SnowballState>,
    pub scores_team1: u8,
    pub scores_team2: u8,
    pub ball: Option<BallState>,
    pub phase: MatchPhase,
    pub time_elapsed: f32,
    pub paused: bool,
    pub team1_color: ColorDef,
    pub team2_color: ColorDef,
    pub player_with_active_action: Option<(String, f32)>,
    pub game_mode: GameMode,
    pub action_target_time: Option<f32>,
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

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(PartialEq)]
pub enum CollisionMaskTag {
    Ball,
    Team1,
    Team2,
    Snowball,
}

pub fn matches_ball(mask: &Vec<CollisionMaskTag>) -> bool {
    mask.contains(&CollisionMaskTag::Ball)
}

pub fn matches_player(mask: &Vec<CollisionMaskTag>, team: Team) -> bool {
    match team {
        Team::Team1 => mask.contains(&CollisionMaskTag::Team1),
        Team::Team2 => mask.contains(&CollisionMaskTag::Team2),
    }
}

pub fn matches_snowball(mask: &Vec<CollisionMaskTag>) -> bool {
    mask.contains(&CollisionMaskTag::Snowball)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    Line {
        ax: f32,
        ay: f32,
        bx: f32,
        by: f32,
        factor: f32,
        color: ColorDef,
        is_hole: bool,
        mask: Vec<CollisionMaskTag>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorDef {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameMap {
    pub name: String,
    pub width: f32,
    pub height: f32,
    pub objects: Vec<MapObject>,
    pub physics: PhysicsSettings,
    pub team1: TeamDef,
    pub team2: TeamDef,
    pub ball: Option<BallDef>,
    pub goals: Vec<GoalDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhysicsSettings {
    pub player_radius: f32,
    pub player_mass: f32,

    pub snowball_radius: f32,
    pub snowball_mass: f32,
    pub snowball_lifetime_sec: f32,

    pub player_bounciness: f32,
    pub snowball_bounciness: f32,
    pub ball_radius: f32,
    pub ball_mass: f32,
    pub ball_bounciness: f32,

    pub friction_per_frame: f32,
    pub recoil_power: f32,
    pub shoot_cooldown_sec: f32,
}
impl Default for PhysicsSettings {
    fn default() -> Self {
        Self {
            player_radius: 18.0,
            player_mass: 1.0,
            snowball_radius: 8.0,
            snowball_mass: 0.5,
            snowball_lifetime_sec: 3.0,
            player_bounciness: 0.9,
            snowball_bounciness: 0.9,
            friction_per_frame: 0.98,
            ball_bounciness: 0.8,
            ball_mass: 1.0,
            ball_radius: 10.0,
            recoil_power: 1.2,
            shoot_cooldown_sec: 0.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Copy)]
#[serde(rename_all = "camelCase")]
pub enum GameMode {
    Fight,
    Football,
    Ctf,
    Htf,
    KingOfTheHill,
    Race,
    HotPotato,
    Shooter,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamDef {
    pub spawn_x: f32,
    pub spawn_y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BallDef {
    pub spawn_x: f32,
    pub spawn_y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalDef {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub team: Team,
}
