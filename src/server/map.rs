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
pub struct GameMap {
    pub name: String,
    pub width: f32,
    pub height: f32,
    pub objects: Vec<MapObject>,
    pub physics: PhysicsSettings,
    pub mode: GameMode,
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
            ball_radius: 10.0
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
