use ggez::{
    Context, GameResult,
    glam::Vec2,
    graphics::{self, Color, DrawMode, MeshBuilder, Text, TextFragment},
};
use spin_snowball_shared::*;

use crate::state::GameState;

pub struct Renderer;

impl Renderer {
    pub fn new() -> Self {
        Self
    }

    pub fn draw(&self, ctx: &mut Context, state: &GameState, spin_timer: f32) -> GameResult {
        let mut canvas = graphics::Canvas::from_frame(ctx, Color::from_rgb(20, 20, 30));
        let mut mb = MeshBuilder::new();

        // Draw map
        for obj in &state.map.objects {
            match obj {
                MapObject::Circle {
                    x,
                    y,
                    radius,
                    factor: _,
                    color,
                    is_hole: _,
                    mask: _,
                } => {
                    let c = Color::from_rgba(
                        color.r,
                        color.g,
                        color.b,
                        color.a,
                    );

                    mb.circle(DrawMode::fill(), Vec2::new(*x, *y), *radius, 0.5, c)?;
                }

                MapObject::Rect {
                    x,
                    y,
                    w,
                    h,
                    factor: _,
                    color,
                    is_hole: _,
                    mask: _,
                } => {
                    let c = Color::from_rgba(
                        color.r,
                        color.g,
                        color.b,
                        color.a,
                    );
                    mb.rectangle(DrawMode::fill(), graphics::Rect::new(*x, *y, *w, *h), c)?;
                }

                MapObject::Line {
                    ax,
                    ay,
                    bx,
                    by,
                    color,
                    is_hole,
                    ..
                } => {
                    let mut c = Color::from_rgba(
                        color.r,
                        color.g,
                        color.b,
                        color.a,
                    );

                    if *is_hole {
                        c.a *= 0.6;
                    }

                    mb.line(&[Vec2::new(*ax, *ay), Vec2::new(*bx, *by)], 3.0, c)?;
                }
            }
        }

        // Draw goals
        for goal in &state.map.goals {
            let c = player_color(state, goal.team);

            mb.rectangle(
                DrawMode::stroke(2.0),
                graphics::Rect::new(goal.x, goal.y, goal.w, goal.h),
                c,
            )?;
            let border = c;
            let fill = Color {
                r: (border.r * 0.8).clamp(0.0, 1.0),
                g: (border.g * 0.8).clamp(0.0, 1.0),
                b: (border.b * 0.8).clamp(0.0, 1.0),
                a: (border.a * 0.6).clamp(0.0, 1.0),
            };
            let stroke_width = 2.0;
            let inset = stroke_width / 2.0;
            mb.rectangle(
                DrawMode::fill(),
                graphics::Rect::new(
                    goal.x + inset,
                    goal.y + inset,
                    goal.w - stroke_width,
                    goal.h - stroke_width,
                ),
                fill,
            )?;
        }

        // Draw players
        for p in &state.other_players {
            if Some(&p.id) == state.player.id.as_ref() {
                continue;
            }
            if let PlayerStatus::Playing(team) = p.status {
                let color = player_color(state, team);

                mb.circle(
                    DrawMode::fill(),
                    Vec2::new(p.pos[0], p.pos[1]),
                    state.map.physics.player_radius,
                    0.5,
                    color,
                )?;

                let text = Text::new(
                    TextFragment::new(p.nick.clone())
                        .color(Color::WHITE)
                        .scale(14.0),
                );

                let dims = text.measure(ctx)?;
                let text_pos = Vec2::new(
                    p.pos[0] - dims.x / 2.0,
                    p.pos[1] + state.map.physics.player_radius + 4.0,
                );
                canvas.draw(&text, graphics::DrawParam::default().dest(text_pos).z(100));
            }
        }

        if let PlayerStatus::Playing(team) = state.player_status {
            let color = player_color(state, team);
            // Local player
            mb.circle(
                DrawMode::fill(),
                state.player.pos,
                state.map.physics.player_radius,
                0.5,
                color,
            )?;
        }

        // direction indicator triangle for local player
        let dir = state.forward_vector();
        let tip = Vec2::new(
            state.player.pos.x + dir.x * (state.map.physics.player_radius + 8.0),
            state.player.pos.y + dir.y * (state.map.physics.player_radius + 8.0),
        );
        let left = Vec2::new(
            state.player.pos.x + (-dir.y) * 8.0,
            state.player.pos.y + (dir.x) * 8.0,
        );
        let right = Vec2::new(
            state.player.pos.x + (dir.y) * 8.0,
            state.player.pos.y + (-dir.x) * 8.0,
        );
        mb.polygon(
            DrawMode::fill(),
            &[tip, left, right],
            Color::from_rgb(255, 100, 100),
        )?;

        // snowballs
        for sb in &state.snowballs {
            let c = { Color::WHITE };
            mb.circle(
                DrawMode::fill(),
                Vec2::new(sb.pos.x, sb.pos.y),
                state.map.physics.snowball_radius,
                0.5,
                c,
            )?;
        }

        if let Some(ball) = &state.ball {
            let c = Color::from_rgb(250, 230, 120);
            mb.circle(DrawMode::fill(), ball.pos, ball.radius, 0.5, c)?;
        }

        let mesh = mb.build();
        let mesh = graphics::Mesh::from_data(&ctx.gfx, mesh);
        canvas.draw(&mesh, ggez::graphics::DrawParam::default());

        let mesh = graphics::Mesh::from_data(&ctx.gfx, mb.build());
        canvas.draw(&mesh, graphics::DrawParam::default());

        // HUD: charge bar
        let bar_w = 200.0;
        let bar_h = 12.0;
        let x = 20.0;
        let y = state.map.height - 30.0;
        let charge = (spin_timer / state.player.max_charge).clamp(0.0, 1.0);
        let bar_back = graphics::Mesh::new_rectangle(
            ctx,
            DrawMode::fill(),
            graphics::Rect::new(x, y, bar_w, bar_h),
            Color::from_rgba(40, 40, 40, 200),
        )?;
        let bar_front = graphics::Mesh::new_rectangle(
            ctx,
            DrawMode::fill(),
            graphics::Rect::new(x, y, bar_w * charge, bar_h),
            Color::from_rgba(120, 200, 255, 200),
        )?;
        canvas.draw(&bar_back, graphics::DrawParam::default());
        canvas.draw(&bar_front, graphics::DrawParam::default());

        if state.game_mode == GameMode::Htf {
            let text = if let Some(carrier_id) = &state.action_player {
                let nick = state
                    .other_players
                    .iter()
                    .find(|p| &p.id == carrier_id)
                    .map(|p| p.nick.clone())
                    .or_else(|| {
                        if state.player.id.as_ref() == Some(carrier_id) {
                            Some(carrier_id.to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| carrier_id.clone());

                format!("FLAG: {} - {:.1}s", nick, state.action_time)
            } else {
                "FLAG: free".to_string()
            };

            let hud_text = Text::new(TextFragment::new(text).color(Color::WHITE).scale(18.0));

            canvas.draw(
                &hud_text,
                graphics::DrawParam::default()
                    .dest(Vec2::new(20.0, state.map.height - 60.0))
                    .z(200),
            );
        }

        if state.game_mode == GameMode::KingOfTheHill {
            let text = if let Some(king_id) = &state.action_player {
                let nick = state
                    .other_players
                    .iter()
                    .find(|p| &p.id == king_id)
                    .map(|p| p.nick.clone())
                    .or_else(|| {
                        if state.player.id.as_ref() == Some(king_id) {
                            Some(king_id.to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| king_id.clone());

                format!("HILL: {} - {:.1}s", nick, state.action_time)
            } else {
                "HILL: contested".to_string()
            };

            let hud_text = Text::new(TextFragment::new(text).color(Color::WHITE).scale(18.0));

            canvas.draw(
                &hud_text,
                graphics::DrawParam::default()
                    .dest(Vec2::new(20.0, state.map.height - 60.0))
                    .z(200),
            );
        }

        if state.game_mode == GameMode::HotPotato {
            if let Some(action_target_time) = state.action_target_time {
                let text = if state.action_time < action_target_time {
                    format!("TERRITORY: {:.1}s", state.action_time)
                } else {
                    "TERRITORY: deciding…".to_string()
                };

                let hud_text = Text::new(TextFragment::new(text).color(Color::WHITE).scale(18.0));

                canvas.draw(
                    &hud_text,
                    graphics::DrawParam::default()
                        .dest(Vec2::new(20.0, state.map.height - 60.0))
                        .z(200),
                );
            }
        }

        canvas.finish(ctx)
    }
}

fn player_color(state: &GameState, team: Team) -> Color {
    match team {
        Team::Team1 => Color {
            r: state.team1_color.r as f32 / 255.0,
            g: state.team1_color.g as f32 / 255.0,
            b: state.team1_color.b as f32 / 255.0,
            a: state.team1_color.a as f32 / 255.0,
        },
        Team::Team2 => Color {
            r: state.team2_color.r as f32 / 255.0,
            g: state.team2_color.g as f32 / 255.0,
            b: state.team2_color.b as f32 / 255.0,
            a: state.team2_color.a as f32 / 255.0,
        },
    }
}
