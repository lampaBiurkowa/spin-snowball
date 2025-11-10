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
}

#[derive(Debug, Clone, Deserialize)]
pub struct PhysicsSettings {
    pub player_radius: f32,
    pub player_mass: f32,

    pub snowball_radius: f32,
    pub snowball_mass: f32,

    pub player_bounciness: f32,
    pub snowball_bounciness: f32,

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
        }
    }
}
