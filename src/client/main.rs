use ggez::event::{self, EventHandler};
use ggez::input::keyboard::KeyInput;
use ggez::winit::keyboard::PhysicalKey;
use ggez::{Context, ContextBuilder, GameError, GameResult};
use std::sync::mpsc::Receiver;
use std::sync::mpsc::channel;

mod input;
mod map;
mod network;
mod physics;
mod rendering;
mod state;
mod text_input_workaround;
mod ui;

use input::InputState;
use network::NetworkClient;
use physics::update_physics;
use rendering::Renderer;
use state::GameState;

use crate::input::PlayerAction;
use crate::network::{ClientCommand, ClientMessage};
use crate::text_input_workaround::CharInput;
use crate::ui::{UIMessage, UiState};

struct MainState {
    game: GameState,
    input: InputState,
    network: NetworkClient,
    renderer: Renderer,
    ui: UiState,
    ui_events_rx: Receiver<UIMessage>,
    char_input: CharInput,
}

impl MainState {
    fn new(server_addr: &str, mut ctx: &mut Context) -> GameResult<Self> {
        // Load map
        let map_data = std::fs::read_to_string("default_map.json")?;
        let map: map::GameMap = serde_json::from_str(&map_data).unwrap();
        let network = NetworkClient::new(server_addr);
        network.send(ClientMessage::Command {
            cmd: ClientCommand::JoinAsPlayer {
                team: network::Team::Team1,
            },
        });
        let (tx, rx) = channel();
        Ok(Self {
            game: GameState::new(map),
            input: InputState::default(),
            network,
            renderer: Renderer::new(),
            ui: UiState::new(&mut ctx, tx),
            ui_events_rx: rx,
            char_input: CharInput::new(),
        })
    }

    fn process_ui_events(&mut self) {
        while let Ok(x) = self.ui_events_rx.try_recv() {
            match x {
                UIMessage::Pause => self.network.send(ClientMessage::Command {
                    cmd: ClientCommand::Pause,
                }),
                UIMessage::Start {
                    score_limit,
                    time_limit_secs,
                } => self.network.send(ClientMessage::Command {
                    cmd: ClientCommand::Start {
                        score_limit,
                        time_limit_secs,
                    },
                }),
                UIMessage::Stop => self.network.send(ClientMessage::Command {
                    cmd: ClientCommand::Stop,
                }),
                UIMessage::Resume => self.network.send(ClientMessage::Command {
                    cmd: ClientCommand::Resume,
                }),
                UIMessage::LoadMap { path } => {
                    let data = std::fs::read_to_string(path).unwrap();
                    self.game.map = serde_json::from_str(&data).unwrap();
                    self.network.send(ClientMessage::Command {
                        cmd: ClientCommand::LoadMap { data },
                    });
                }
                UIMessage::JoinTeam { player_id, status } => {
                    if let Some(own_id) = &self.game.player.id {
                        if player_id == *own_id {
                            let cmd = match status {
                                network::PlayerStatus::Spectator => ClientCommand::JoinAsSpectator,
                                network::PlayerStatus::Playing(team) => {
                                    ClientCommand::JoinAsPlayer { team }
                                }
                            };
                            self.network.send(ClientMessage::Command { cmd });
                        }
                    }
                }
            }
        }
    }
}

impl EventHandler for MainState {
    fn update(&mut self, ctx: &mut Context) -> GameResult {
        let dt = ctx.time.delta().as_secs_f32();

        self.input.update(dt);
        self.ui.update(&self.game, ctx);
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
                    phase,
                    time_elapsed,
                    paused
                } => {
                    self.game.apply_world_state(
                        players,
                        snowballs,
                        ball,
                        scores,
                        phase,
                        time_elapsed,
                        paused
                    );
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

        for c in self.char_input.collect(ctx) {
            self.ui.text_input_event(ctx, c);
        }
        self.process_ui_events();
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        self.renderer
            .draw(ctx, &self.game, self.input.spin_timer())?;
        self.ui.render(ctx);
        Ok(())
    }

    fn key_down_event(
        &mut self,
        ctx: &mut Context,
        input: KeyInput,
        _repeat: bool,
    ) -> Result<(), GameError> {
        if let PhysicalKey::Code(keycode) = input.event.physical_key {
            self.input.process_key_down(keycode);
        }
        Ok(())
    }

    fn key_up_event(&mut self, _ctx: &mut Context, input: KeyInput) -> Result<(), GameError> {
        if let PhysicalKey::Code(keycode) = input.event.physical_key {
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

    //doesnt trigger for some reason - maybe it will be fixed one day
    // fn text_input_event(&mut self, ctx: &mut Context, character: char) -> Result<(), GameError> {
    //     self.ui.text_input_event(ctx, character);
    //     Ok(())
    // }
}

pub fn main() -> GameResult {
    let (mut ctx, event_loop) = ContextBuilder::new("snowball_spin_net", "you")
        .window_setup(ggez::conf::WindowSetup::default().title("Snowball Spin - Client"))
        .window_mode(ggez::conf::WindowMode::default().dimensions(800.0, 600.0))
        .build()?;

    let client = MainState::new("127.0.0.1:9001", &mut ctx)?;
    event::run(ctx, event_loop, client)
}
