//! Spotting the moment the player earns one of the declared achievements.
//!
//! Every key here has to appear in `.ndib/achievements.json`; the launcher
//! refuses one the product never declared. The three counting achievements
//! (`snowstorm`, `serial-winner`, `all-rounder`) are not unlocked here — they
//! watch `stats.xml`, which this keeps up to date.

use ggez::glam::Vec2;
use spin_snowball_shared::*;

use crate::dibrysoft::{Launcher, Stats};
use crate::state::GameState;

const FIRST_THROW: &str = "first-throw";
const WOUND_UP: &str = "wound-up";
const FIRST_WIN: &str = "first-win";
const CLEAN_SHEET: &str = "clean-sheet";
const FLAG_BEARER: &str = "flag-bearer";
const LONG_LIVE_THE_KING: &str = "long-live-the-king";
const CHECKERED_FLAG: &str = "checkered-flag";
const PIPING_HOT: &str = "piping-hot";
const GRAVITY_WINS: &str = "gravity-wins";
const THE_LONG_GAME: &str = "the-long-game";

// every key this game posts itself, for the test that holds them against what
// the product declares
#[cfg(test)]
const POSTED_BY_THE_GAME: [&str; 10] = [
    FIRST_THROW,
    WOUND_UP,
    FIRST_WIN,
    CLEAN_SHEET,
    FLAG_BEARER,
    LONG_LIVE_THE_KING,
    CHECKERED_FLAG,
    PIPING_HOT,
    GRAVITY_WINS,
    THE_LONG_GAME,
];

/// The spin a shot needs to count as fully charged, as the server sees it.
const FULL_CHARGE_SECS: f32 = 1.0;
const LONG_MATCH_SECS: f32 = 600.0;
/// How recently the player has to have been at a rim for a conceded point in
/// Fight to be read as their own fall.
const FELL_IN_WITHIN_SECS: f32 = 0.5;

pub struct Achievements {
    launcher: Launcher,
    stats: Stats,

    // the match as it looked last frame, since what is earned is a change
    playing: bool,
    // kept while playing, so it is still known on the frame the match ends and
    // everybody is turned back into a spectator
    team: Option<Team>,
    scores: (u8, u8),
    holding_hill: bool,
    // the server empties the hole in the same tick it awards the point, so the
    // fall itself is never in a world state. How long ago we were at the rim is
    // what tells us the point that just went in was ours to give away
    since_near_a_hole: f32,
    // the server ignores a shot inside the cooldown, so this mirrors it rather
    // than counting every key release
    cooldown: f32,
}

impl Achievements {
    pub fn new() -> Self {
        Self {
            launcher: Launcher::detect(),
            stats: Stats::load(),
            playing: false,
            team: None,
            scores: (0, 0),
            holding_hill: false,
            since_near_a_hole: f32::MAX,
            cooldown: 0.0,
        }
    }

    /// A shot the player just released, with the spin it was charged with.
    pub fn shot_fired(&mut self, game: &GameState, charge: f32) {
        if !playing(game) || game.paused || self.team.is_none() || self.cooldown > 0.0 {
            return;
        }

        self.cooldown = game.map.physics.shoot_cooldown_sec;
        self.stats.snowball_thrown();
        self.launcher.unlock(FIRST_THROW);

        if charge >= FULL_CHARGE_SECS {
            self.launcher.unlock(WOUND_UP);
        }
    }

    /// Called once a frame, after the world state has been applied.
    pub fn observe(&mut self, game: &GameState, dt: f32) {
        self.cooldown = (self.cooldown - dt).max(0.0);

        let playing = playing(game);
        if playing {
            self.during_match(game, dt);
        }

        if self.playing && !playing {
            self.match_ended(game);
        }

        self.playing = playing;
        self.stats.maybe_flush();
    }

    fn during_match(&mut self, game: &GameState, dt: f32) {
        if let PlayerStatus::Playing(team) = game.player_status {
            self.team = Some(team);
            self.stats.mode_played(mode_index(game.game_mode));
        }

        let Some(team) = self.team else { return };
        let scores = from_our_side(game, team);
        let (mine, theirs) = scores;
        let (previously_mine, previously_theirs) = self.scores;
        self.scores = scores;

        self.since_near_a_hole = if near_a_hole(game) {
            0.0
        } else {
            self.since_near_a_hole + dt
        };

        if mine > previously_mine {
            match game.game_mode {
                // the potato only ever scores once it is ready to blow
                GameMode::HotPotato => self.launcher.unlock(PIPING_HOT),
                GameMode::KingOfTheHill if self.holding_hill => {
                    self.launcher.unlock(LONG_LIVE_THE_KING)
                }
                _ => {}
            }
        }

        let ours = game.action_player.as_deref() == game.player.id.as_deref()
            && game.action_player.is_some();

        match game.game_mode {
            GameMode::Ctf | GameMode::Htf if ours => self.launcher.unlock(FLAG_BEARER),
            // the count resets the moment the hill is left, so who was on it
            // has to be remembered from the frame before it pays out
            GameMode::KingOfTheHill => self.holding_hill = ours,
            // in Fight a hole pays the other team, and standing at the rim as
            // it happens is as close as the game gets to knowing it was us
            GameMode::Fight
                if theirs > previously_theirs && self.since_near_a_hole <= FELL_IN_WITHIN_SECS =>
            {
                self.launcher.unlock(GRAVITY_WINS)
            }
            _ => {}
        }

        if game.time_elapsed >= LONG_MATCH_SECS {
            self.launcher.unlock(THE_LONG_GAME);
        }
    }

    /// The scores are still the final ones on the frame the match ends, and the
    /// team is the one held on to from while it was being played.
    fn match_ended(&mut self, game: &GameState) {
        if let Some(team) = self.team {
            let (mine, theirs) = from_our_side(game, team);

            // a match stopped by hand counts the same as one played out; the
            // game is not told which of the two happened
            if mine > theirs {
                self.launcher.unlock(FIRST_WIN);
                self.stats.match_won();

                if theirs == 0 {
                    self.launcher.unlock(CLEAN_SHEET);
                }

                if matches!(game.game_mode, GameMode::Race) {
                    self.launcher.unlock(CHECKERED_FLAG);
                }
            }
        }

        self.team = None;
        self.scores = (0, 0);
        self.holding_hill = false;
        self.since_near_a_hole = f32::MAX;
        self.stats.flush();
    }

    /// Keeps what has been counted, for the launcher to read after we are gone.
    pub fn save(&mut self) {
        self.stats.flush();
    }
}

fn playing(game: &GameState) -> bool {
    matches!(game.phase, MatchPhase::Playing { .. })
}

fn score(game: &GameState, team: Team) -> u8 {
    game.scores.get(&team).copied().unwrap_or(0)
}

/// The scores as (ours, theirs).
fn from_our_side(game: &GameState, team: Team) -> (u8, u8) {
    match team {
        Team::Team1 => (score(game, Team::Team1), score(game, Team::Team2)),
        Team::Team2 => (score(game, Team::Team2), score(game, Team::Team1)),
    }
}

/// A stable bit per mode, so the count in `stats.xml` keeps its meaning between
/// sessions. New modes go on the end.
fn mode_index(mode: GameMode) -> u32 {
    match mode {
        GameMode::Fight => 0,
        GameMode::Football => 1,
        GameMode::Ctf => 2,
        GameMode::Htf => 3,
        GameMode::KingOfTheHill => 4,
        GameMode::Race => 5,
        GameMode::HotPotato => 6,
        GameMode::Shooter => 7,
    }
}

/// Whether the player is in a hole or close enough to its rim to be falling in.
/// The server's own test is the same one without the margin.
fn near_a_hole(game: &GameState) -> bool {
    let pos = game.player.pos;
    // a player's width of slack, so the rim counts and the rest of the map does not
    let radius = game.map.physics.player_radius * 2.0;

    game.map.objects.iter().any(|object| match object {
        MapObject::Circle {
            x,
            y,
            radius: hole,
            is_hole,
            ..
        } => *is_hole && pos.distance(Vec2::new(*x, *y)) <= radius + hole,
        MapObject::Rect {
            x,
            y,
            w,
            h,
            is_hole,
            ..
        } => {
            *is_hole
                && pos.distance(Vec2::new(pos.x.clamp(*x, x + w), pos.y.clamp(*y, y + h))) <= radius
        }
        MapObject::Line {
            ax,
            ay,
            bx,
            by,
            is_hole,
            ..
        } => {
            *is_hole && distance_to_segment(pos, Vec2::new(*ax, *ay), Vec2::new(*bx, *by)) <= radius
        }
    })
}

fn distance_to_segment(point: Vec2, a: Vec2, b: Vec2) -> f32 {
    let along = b - a;
    let length_squared = along.length_squared();
    if length_squared < 1e-6 {
        return point.distance(a);
    }

    let t = ((point - a).dot(along) / length_squared).clamp(0.0, 1.0);
    point.distance(a + along * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> GameMap {
        GameMap {
            name: "test".into(),
            width: 400.0,
            height: 400.0,
            objects: vec![
                MapObject::Circle {
                    x: 100.0,
                    y: 100.0,
                    radius: 20.0,
                    factor: 1.0,
                    color: ColorDef { r: 0, g: 0, b: 0, a: 255 },
                    is_hole: true,
                    mask: vec![],
                },
                MapObject::Rect {
                    x: 300.0,
                    y: 0.0,
                    w: 40.0,
                    h: 40.0,
                    factor: 1.0,
                    color: ColorDef { r: 0, g: 0, b: 0, a: 255 },
                    is_hole: false,
                    mask: vec![],
                },
            ],
            physics: PhysicsSettings::default(),
            team1: TeamDef { spawn_x: 0.0, spawn_y: 0.0 },
            team2: TeamDef { spawn_x: 0.0, spawn_y: 0.0 },
            ball: None,
            goals: vec![],
        }
    }

    fn state_at(x: f32, y: f32) -> GameState {
        let mut game = GameState::new(map());
        game.player.pos = Vec2::new(x, y);
        game
    }

    #[test]
    fn the_rim_of_a_hole_counts_as_falling_in() {
        // the hole in the test map is at 100,100 with a radius of 20, and the
        // margin is a player's width either side of the player's own radius
        let reach = 20.0 + PhysicsSettings::default().player_radius * 2.0;

        assert!(near_a_hole(&state_at(100.0, 100.0)));
        assert!(near_a_hole(&state_at(100.0 + reach - 0.5, 100.0)));
        assert!(!near_a_hole(&state_at(100.0 + reach + 0.5, 100.0)));
    }

    #[test]
    fn solid_objects_are_not_holes() {
        assert!(!near_a_hole(&state_at(320.0, 20.0)));
    }

    #[test]
    fn scores_are_read_from_our_own_side() {
        let mut game = state_at(0.0, 0.0);
        game.scores = [(Team::Team1, 3), (Team::Team2, 1)].into();

        assert_eq!(from_our_side(&game, Team::Team1), (3, 1));
        assert_eq!(from_our_side(&game, Team::Team2), (1, 3));
    }

    #[test]
    fn every_mode_has_its_own_bit() {
        let modes = [
            GameMode::Fight,
            GameMode::Football,
            GameMode::Ctf,
            GameMode::Htf,
            GameMode::KingOfTheHill,
            GameMode::Race,
            GameMode::HotPotato,
            GameMode::Shooter,
        ];

        let mask = modes.iter().fold(0u32, |mask, mode| mask | 1 << mode_index(*mode));
        assert_eq!(mask.count_ones(), 8, "all-rounder needs all eight to count");
    }

    /// A key the product never declared is answered with 404 and nothing
    /// unlocks, so the two lists have to agree exactly.
    #[test]
    fn the_keys_match_what_the_product_declares() {
        let declared: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../.ndib/achievements.json")).unwrap();

        let keys: Vec<&str> = declared
            .iter()
            .map(|x| x["key"].as_str().expect("every entry needs a key"))
            .collect();

        for key in POSTED_BY_THE_GAME {
            assert!(keys.contains(&key), "{key} is not declared in .ndib/achievements.json");
        }

        // anything the game does not post has to be one the launcher watches,
        // or it could never be earned at all
        for entry in &declared {
            let key = entry["key"].as_str().unwrap();
            if POSTED_BY_THE_GAME.contains(&key) {
                continue;
            }

            assert!(
                entry.get("watch").is_some() && entry.get("unlockWhen").is_some(),
                "{key} is neither posted by the game nor watched by the launcher"
            );
        }
    }

    #[test]
    fn distance_to_a_degenerate_segment_is_to_its_point() {
        let point = Vec2::new(3.0, 4.0);
        assert_eq!(distance_to_segment(point, Vec2::ZERO, Vec2::ZERO), 5.0);
    }
}
