use ggez::{
    Context, GameResult,
    glam::Vec2,
    graphics::{self, Color, DrawMode, MeshBuilder, Text, TextFragment},
};

use crate::{
    map::MapObject,
    network::{PlayerStatus, Team},
    state::GameState,
};

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
                    factor,
                    color,
                    is_hole,
                } => {
                    let c = Color::from_rgba(
                        (color.r * 255.0) as u8,
                        (color.g * 255.0) as u8,
                        (color.b * 255.0) as u8,
                        (color.a * 255.0) as u8,
                    );

                    mb.circle(DrawMode::fill(), Vec2::new(*x, *y), *radius, 0.5, c)?;
                }

                MapObject::Rect {
                    x,
                    y,
                    w,
                    h,
                    factor,
                    color,
                    is_hole,
                } => {
                    let c = Color::from_rgba(
                        (color.r * 255.0) as u8,
                        (color.g * 255.0) as u8,
                        (color.b * 255.0) as u8,
                        (color.a * 255.0) as u8,
                    );
                    mb.rectangle(DrawMode::fill(), graphics::Rect::new(*x, *y, *w, *h), c)?;
                }
            }
        }

        // Draw goals (football mode)
        if let Some(fb) = &state.map.football {
            for goal in &fb.goals {
                let c = if goal.team == 1 {
                    Color::from_rgb(200, 50, 50)
                } else {
                    Color::from_rgb(50, 50, 200)
                };

                mb.rectangle(
                    DrawMode::stroke(2.0),
                    graphics::Rect::new(goal.x, goal.y, goal.w, goal.h),
                    c,
                )?;
            }
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
                    16.0,
                    0.5,
                    color,
                )?;

                let text = Text::new(
                    TextFragment::new(p.nick.clone())
                        .color(Color::WHITE)
                        .scale(14.0),
                );

                let dims = text.measure(ctx)?;
                let text_pos = Vec2::new(p.pos[0] - dims.x / 2.0, p.pos[1] + 16.0 + 4.0);
                canvas.draw(&text, graphics::DrawParam::default().dest(text_pos).z(100));
            }
        }

        if let PlayerStatus::Playing(team) = state.player_status {
            let color = player_color(state, team);
            // Local player
            mb.circle(
                DrawMode::fill(),
                state.player.pos,
                state.player.radius,
                0.5,
                color,
            )?;
        }

        // direction indicator triangle for local player
        let dir = state.forward_vector();
        let tip = Vec2::new(
            state.player.pos.x + dir.x * (state.player.radius + 8.0),
            state.player.pos.y + dir.y * (state.player.radius + 8.0),
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
            mb.circle(DrawMode::fill(), Vec2::new(sb.pos.x, sb.pos.y), 6.0, 0.5, c)?;
        }

        if let Some(ball) = &state.ball {
            let c = Color::from_rgb(250, 230, 120);
            mb.circle(DrawMode::fill(), ball.pos, ball.radius, 0.5, c)?;
        }

        let mesh = mb.build();
        let mesh = graphics::Mesh::from_data(&ctx.gfx, mesh);
        canvas.draw(&mesh, ggez::graphics::DrawParam::default());

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

        let mesh = graphics::Mesh::from_data(&ctx.gfx, mb.build());
        canvas.draw(&mesh, graphics::DrawParam::default());
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
