use ggez::event::{self, EventHandler};
use ggez::input::keyboard::{KeyCode, KeyInput};
use ggez::{Context, ContextBuilder, GameError, GameResult};

mod map;
mod network;

pub mod input;
pub mod physics;
pub mod rendering;
pub mod state;

use input::InputState;
use network::NetworkClient;
use physics::update_physics;
use rendering::Renderer;
use state::GameState;

use crate::input::PlayerAction;
use crate::network::ClientMessage;

struct MainState {
    game: GameState,
    input: InputState,
    network: NetworkClient,
    renderer: Renderer,
}

impl MainState {
    fn new(server_addr: &str) -> GameResult<Self> {
        // Load map
        let map_data = std::fs::read_to_string("default_map.json")?;
        let map: map::GameMap = serde_json::from_str(&map_data).unwrap();

        Ok(Self {
            game: GameState::new(map),
            input: InputState::default(),
            network: NetworkClient::new(server_addr),
            renderer: Renderer::new(),
        })
    }
}

impl EventHandler for MainState {
    fn update(&mut self, ctx: &mut Context) -> GameResult {
        let dt = ctx.time.delta().as_secs_f32();

        // Process input
        self.input.update(dt);
        if let Some(actions) = self.input.consume_actions() {
            for action in actions {
                match action {
                    PlayerAction::RotateLeft => {
                        self.network.send(ClientMessage::Input {
                            left: true,
                            right: false,
                            shoot: false,
                        });
                    }
                    PlayerAction::RotateRight => {
                        self.network.send(ClientMessage::Input {
                            left: false,
                            right: true,
                            shoot: false,
                        });
                    }
                    _ => {}
                }
            }
        }

        // Handle incoming network state
        if let Some(msg) = self.network.poll() {
            match msg {
                network::ServerMessage::AssignId { id } => {
                    self.game.player.id = Some(id);
                }
                network::ServerMessage::WorldState {
                    players,
                    snowballs,
                    ball,
                    scores,
                } => {
                    self.game
                        .apply_world_state(players, snowballs, ball, scores);
                }
                network::ServerMessage::Pong { .. } => {}
            }
        }

        // Update physics
        update_physics(&mut self.game, dt);

        // Optional: ping server for latency measurements
        if ctx.time.ticks() % 300 == 0 {
            self.network.send(network::ClientMessage::Ping { ts: 0 });
        }

        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        self.renderer.draw(ctx, &self.game, self.input.spin_timer())
    }

    fn key_down_event(
        &mut self,
        _ctx: &mut Context,
        input: KeyInput,
        _repeat: bool,
    ) -> Result<(), GameError> {
        if let Some(keycode) = input.keycode {
            self.input.process_key_down(keycode);
        }
        Ok(())
    }

    fn key_up_event(&mut self, _ctx: &mut Context, input: KeyInput) -> Result<(), GameError> {
        if let Some(keycode) = input.keycode {
            if let Some(action) = self.input.process_key_up(keycode) {
                if let PlayerAction::Shoot(charge) = action {
                    self.network.send(ClientMessage::Input {
                        left: false,
                        right: false,
                        shoot: true,
                    });
                }
            }
        }
        Ok(())
    }
}

pub fn main() -> GameResult {
    let (ctx, event_loop) = ContextBuilder::new("snowball_spin_net", "you")
        .window_setup(ggez::conf::WindowSetup::default().title("Snowball Spin - Client"))
        .window_mode(ggez::conf::WindowMode::default().dimensions(800.0, 600.0))
        .build()?;

    let client = MainState::new("127.0.0.1:9001")?;
    event::run(ctx, event_loop, client)
}
