use serde::Deserialize;

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
#[serde(rename_all = "camelCase")]
pub enum GameMode {
    Fight,
    Football,
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

#[derive(Debug, Clone, Deserialize)]
pub struct GameMap {
    pub name: String,
    pub width: f32,
    pub height: f32,
    pub objects: Vec<MapObject>,
    pub mode: GameMode,
    pub football: Option<FootballSettings>,
    pub physics: PhysicsSettings,
}
