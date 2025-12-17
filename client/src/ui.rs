use ggegui::egui;
use ggez::graphics::{Canvas, DrawParam};
use std::sync::mpsc::Sender;

use crate::{
    network::{self, MatchPhase, PlayerStatus},
    state::GameState,
};

pub enum UIMessage {
    Start {
        score_limit: Option<u32>,
        time_limit_secs: Option<u32>,
    },
    Pause,
    Resume,
    Stop,
    LoadMap {
        path: String,
    },
    JoinTeam {
        player_id: String,
        status: PlayerStatus,
    },
}

pub struct UiState {
    ctx: ggegui::Gui,
    sender: Sender<UIMessage>,
    score_limit_enabled: bool,
    score_limit: u32,
    time_limit_enabled: bool,
    time_limit_secs: u32,
    map_path: String,
}

impl UiState {
    pub fn new(ctx: &mut ggez::Context, tx: Sender<UIMessage>) -> Self {
        Self {
            ctx: ggegui::Gui::new(ctx),
            sender: tx,
            score_limit_enabled: false,
            score_limit: 5,
            time_limit_enabled: false,
            time_limit_secs: 300,
            map_path: "default_map.json".to_string(),
        }
    }

    pub fn render(&mut self, ctx: &mut ggez::Context) {
        let mut canvas = Canvas::from_frame(ctx, None);
        canvas.draw(&self.ctx, DrawParam::default().dest(glam::Vec2::ZERO));
        canvas.finish(ctx).unwrap();
    }
    pub fn update(&mut self, state: &GameState, ctx: &mut ggez::Context) {
        let egui_ctx = self.ctx.ctx();
        let match_stopped = matches!(state.phase, MatchPhase::Lobby);
        let can_start = matches!(state.phase, MatchPhase::Lobby);
        let can_pause = matches!(state.phase, MatchPhase::Playing { .. }) && !state.paused;
        let can_resume = matches!(state.phase, MatchPhase::Playing { .. }) && state.paused;
        let can_stop = matches!(state.phase, MatchPhase::Playing  { .. });

        egui::TopBottomPanel::top("top_hud")
            .resizable(false)
            .show(&egui_ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("Phase: {:?}", state.phase));
                    ui.separator();

                    ui.label(format!("Time: {:.1}", state.time_elapsed));
                    ui.separator();

                    for (id, score) in &state.scores {
                        ui.label(format!("{id}: {score}"));
                    }
                });
            });
        egui::Window::new("Menu")
            .default_width(460.0)
            .show(&egui_ctx, |ui| {
                ui.heading("Players");
                ui.separator();

                let enabled = match_stopped;

                ui.columns(3, |cols| {
                    self.team_column(
                        &mut cols[0],
                        "Team 1",
                        state
                            .all_players
                            .iter()
                            .filter_map(|p| {
                                matches!(
                                    p.status,
                                    network::PlayerStatus::Playing(network::Team::Team1)
                                )
                                .then_some(p.id.clone())
                            })
                            .collect(),
                        network::PlayerStatus::Playing(network::Team::Team1),
                        enabled,
                    );

                    self.team_column(
                        &mut cols[1],
                        "Team 2",
                        state
                            .all_players
                            .iter()
                            .filter_map(|p| {
                                matches!(
                                    p.status,
                                    network::PlayerStatus::Playing(network::Team::Team2)
                                )
                                .then_some(p.id.clone())
                            })
                            .collect(),
                        network::PlayerStatus::Playing(network::Team::Team2),
                        enabled,
                    );

                    self.team_column(
                        &mut cols[2],
                        "Spectators",
                        state
                            .all_players
                            .iter()
                            .filter_map(|p| {
                                matches!(p.status, network::PlayerStatus::Spectator)
                                    .then_some(p.id.clone())
                            })
                            .collect(),
                        network::PlayerStatus::Spectator,
                        enabled,
                    );
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                ui.heading("Match Settings");

                ui.add_enabled_ui(match_stopped, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.score_limit_enabled, "Score limit");
                        ui.add_enabled(
                            self.score_limit_enabled,
                            egui::DragValue::new(&mut self.score_limit).clamp_range(1..=100),
                        );
                    });

                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.time_limit_enabled, "Time limit (sec)");
                        ui.add_enabled(
                            self.time_limit_enabled,
                            egui::DragValue::new(&mut self.time_limit_secs).clamp_range(30..=3600),
                        );
                    });

                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label("Level:");
                        ui.text_edit_singleline(&mut self.map_path);
                        if ui.button("Load").clicked() {
                            self.sender
                                .send(UIMessage::LoadMap {
                                    path: self.map_path.clone(),
                                })
                                .unwrap();
                        }
                    });
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.add_enabled_ui(can_start, |ui| {
                        if ui.button("Start").clicked() {
                            self.sender
                                .send(UIMessage::Start {
                                    score_limit: self
                                        .score_limit_enabled
                                        .then_some(self.score_limit),
                                    time_limit_secs: self
                                        .time_limit_enabled
                                        .then_some(self.time_limit_secs),
                                })
                                .unwrap();
                        }
                    });

                    ui.add_enabled_ui(can_pause, |ui| {
                        if ui.button("Pause").clicked() {
                            self.sender.send(UIMessage::Pause).unwrap();
                        }
                    });

                    ui.add_enabled_ui(can_resume, |ui| {
                        if ui.button("Resume").clicked() {
                            self.sender.send(UIMessage::Resume).unwrap();
                        }
                    });

                    ui.add_enabled_ui(can_stop, |ui| {
                        if ui.button("Stop").clicked() {
                            self.sender.send(UIMessage::Stop).unwrap();
                        }
                    });
                });
            });

        self.ctx.update(ctx);
    }
    fn team_column(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        players: Vec<String>,
        drop_status: PlayerStatus,
        enabled: bool,
    ) {
        ui.heading(title);
        ui.add_space(4.0);

        egui::Frame::group(ui.style())
            .fill(ui.visuals().extreme_bg_color)
            .show(ui, |ui| {
                // Allocate a minimum rect for the drop area
                let min_size = if players.is_empty() {
                    egui::vec2(100.0, 20.0)
                } else {
                    Default::default()
                };
                let drop_rect = ui.allocate_rect(
                    egui::Rect::from_min_size(ui.min_rect().min, min_size),
                    egui::Sense::hover(),
                );

                // Handle drop
                if enabled {
                    if let Some(player_id) = egui::DragAndDrop::payload::<String>(ui.ctx()) {
                        if drop_rect.hovered() && ui.input(|i| i.pointer.any_released()) {
                            self.sender
                                .send(UIMessage::JoinTeam {
                                    player_id: player_id.to_string(),
                                    status: drop_status,
                                })
                                .unwrap();
                            egui::DragAndDrop::clear_payload(ui.ctx());
                        }
                    }
                }

                // Draw players
                for id in players {
                    let response = ui.add(egui::Label::new(&id).sense(if enabled {
                        egui::Sense::drag()
                    } else {
                        egui::Sense::hover()
                    }));

                    if enabled && response.drag_started() {
                        egui::DragAndDrop::set_payload(ui.ctx(), id.clone());
                    }
                }
            });
    }

    pub(crate) fn text_input_event(&mut self, ctx: &mut ggez::Context, character: char) {
        self.ctx.input.text_input_event(character, ctx);
    }
}
