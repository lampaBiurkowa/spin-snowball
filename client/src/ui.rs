use ggegui::egui;
use ggez::graphics::{Canvas, DrawParam};
use spin_snowball_shared::*;
use std::sync::mpsc::Sender;

use crate::state::GameState;

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
    SetNick {
        nick: String,
    },
    SetTeamColor {
        color: TeamColor,
        team: Team,
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
    nick_edit: String,
    team1_color: egui::Color32,
    team2_color: egui::Color32,
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
            nick_edit: String::new(),
            team1_color: egui::Color32::from_rgb(200, 0, 0),
            team2_color: egui::Color32::from_rgb(0, 0, 200),
        }
    }

    pub fn render(&mut self, ctx: &mut ggez::Context) {
        let mut canvas = Canvas::from_frame(ctx, None);
        canvas.draw(&self.ctx, DrawParam::default().dest(glam::Vec2::ZERO));
        canvas.finish(ctx).unwrap();
    }

    pub fn update(&mut self, state: &GameState, ctx: &mut ggez::Context) {
        let egui_ctx = self.ctx.ctx();

        self.draw_top_hud(&egui_ctx, state);

        egui::Window::new("Menu")
            .default_width(460.0)
            .show(&egui_ctx, |ui| {
                self.draw_players_section(ui, state);
                ui.separator();

                egui::CollapsingHeader::new("Player")
                    .default_open(true)
                    .show(ui, |ui| {
                        self.draw_player_section(ui);
                    });

                egui::CollapsingHeader::new("Team Colors")
                    .default_open(true)
                    .show(ui, |ui| {
                        self.draw_team_colors_section(ui);
                    });

                ui.separator();
                self.draw_match_settings(ui, state);
                ui.separator();
                self.draw_match_controls(ui, state);
            });

        self.ctx.update(ctx);
    }

    fn draw_top_hud(&self, egui_ctx: &egui::Context, state: &GameState) {
        egui::TopBottomPanel::top("top_hud")
            .resizable(false)
            .show(egui_ctx, |ui| {
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
    }

    fn draw_players_section(&mut self, ui: &mut egui::Ui, state: &GameState) {
        ui.heading("Players");
        ui.separator();

        let enabled = matches!(state.phase, MatchPhase::Lobby);

        ui.columns(3, |cols| {
            self.team_column(
                &mut cols[0],
                "Team 1",
                state
                    .all_players
                    .iter()
                    .filter(|p| matches!(p.status, PlayerStatus::Playing(Team::Team1)))
                    .cloned()
                    .collect(),
                PlayerStatus::Playing(Team::Team1),
                enabled,
            );

            self.team_column(
                &mut cols[1],
                "Team 2",
                state
                    .all_players
                    .iter()
                    .filter(|p| matches!(p.status, PlayerStatus::Playing(Team::Team2)))
                    .cloned()
                    .collect(),
                PlayerStatus::Playing(Team::Team2),
                enabled,
            );

            self.team_column(
                &mut cols[2],
                "Spectators",
                state
                    .all_players
                    .iter()
                    .filter(|p| matches!(p.status, PlayerStatus::Spectator))
                    .cloned()
                    .collect(),
                PlayerStatus::Spectator,
                enabled,
            );
        });
    }

    fn draw_player_section(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Nick:");
            let resp = ui.text_edit_singleline(&mut self.nick_edit);

            if (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                || ui.button("Set").clicked()
            {
                if !self.nick_edit.trim().is_empty() {
                    self.sender
                        .send(UIMessage::SetNick {
                            nick: self.nick_edit.trim().to_string(),
                        })
                        .unwrap();
                }
            }
        });
    }

    fn draw_team_colors_section(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Team 1:");
            if ui.color_edit_button_srgba(&mut self.team1_color).changed() {
                self.sender
                    .send(UIMessage::SetTeamColor {
                        team: Team::Team1,
                        color: egui_to_server_color(self.team1_color),
                    })
                    .unwrap();
            }
        });

        ui.horizontal(|ui| {
            ui.label("Team 2:");
            if ui.color_edit_button_srgba(&mut self.team2_color).changed() {
                self.sender
                    .send(UIMessage::SetTeamColor {
                        team: Team::Team2,
                        color: egui_to_server_color(self.team2_color),
                    })
                    .unwrap();
            }
        });
    }

    fn draw_match_settings(&mut self, ui: &mut egui::Ui, state: &GameState) {
        let match_stopped = matches!(state.phase, MatchPhase::Lobby);

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
    }

    fn draw_match_controls(&mut self, ui: &mut egui::Ui, state: &GameState) {
        let can_start = matches!(state.phase, MatchPhase::Lobby);
        let can_pause = matches!(state.phase, MatchPhase::Playing { .. }) && !state.paused;
        let can_resume = matches!(state.phase, MatchPhase::Playing { .. }) && state.paused;
        let can_stop = matches!(state.phase, MatchPhase::Playing { .. });

        ui.horizontal(|ui| {
            ui.add_enabled_ui(can_start, |ui| {
                if ui.button("Start").clicked() {
                    self.sender
                        .send(UIMessage::Start {
                            score_limit: self.score_limit_enabled.then_some(self.score_limit),
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
    }

    fn team_column(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        players: Vec<PlayerState>,
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
                for p in players {
                    let response = ui.add(egui::Label::new(&p.nick).sense(if enabled {
                        egui::Sense::drag()
                    } else {
                        egui::Sense::hover()
                    }));

                    if enabled && response.drag_started() {
                        egui::DragAndDrop::set_payload(ui.ctx(), p.id.clone());
                    }
                }
            });
    }

    pub(crate) fn text_input_event(&mut self, ctx: &mut ggez::Context, character: char) {
        self.ctx.input.text_input_event(character, ctx);
    }
}

fn egui_to_server_color(c: egui::Color32) -> TeamColor {
    TeamColor {
        r: c.r(),
        g: c.g(),
        b: c.b(),
        a: c.a(),
    }
}
