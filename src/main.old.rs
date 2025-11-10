// use ggez::event::{self, EventHandler};
// use ggez::glam::Vec2;
// use ggez::graphics::{self, Color, DrawMode, MeshBuilder};
// use ggez::input::keyboard::{KeyCode, KeyInput};
// use ggez::{Context, ContextBuilder, GameError, GameResult};
// use rand::Rng;

// const SCREEN_W: f32 = 800.0;
// const SCREEN_H: f32 = 600.0;

// struct Player {
//     pos: Vec2,
//     vel: Vec2,
//     rotation: f32, // degrees
//     radius: f32,

//     // rotation (held) state
//     rotating_left: bool,
//     rotating_right: bool,
//     spin_timer: f32,

//     // tunables
//     rotation_speed_deg: f32, // degrees per second while held
//     max_charge: f32,
// }

// struct Snowball {
//     pos: Vec2,
//     vel: Vec2,
//     life: f32,
// }

// struct MainState {
//     player: Player,
//     snowballs: Vec<Snowball>,

//     friction: f32,
// }

// impl Player {
//     fn new(x: f32, y: f32) -> Self {
//         Self {
//             pos: Vec2::new(x, y),
//             vel: Vec2::new(0.0, 0.0),
//             rotation: -90.0, // pointing up
//             radius: 18.0,
//             rotating_left: false,
//             rotating_right: false,
//             spin_timer: 0.0,
//             rotation_speed_deg: 180.0, // degrees per second
//             max_charge: 1.5,           // seconds
//         }
//     }

//     fn forward_vector(&self) -> Vec2 {
//         let r = self.rotation.to_radians();
//         Vec2::new(r.cos(), r.sin())
//     }
// }

// impl MainState {
//     fn new() -> GameResult<MainState> {
//         let s = MainState {
//             player: Player::new(SCREEN_W / 2.0, SCREEN_H / 2.0),
//             snowballs: Vec::new(),
//             friction: 0.98,
//         };
//         Ok(s)
//     }

//     fn shoot_from_player(&mut self) {
//         let charge = self.player.spin_timer.min(self.player.max_charge);
//         let charge_t = charge / self.player.max_charge; // 0..1

//         let base_speed = 280.0;
//         let bonus_speed = 700.0 * charge_t;
//         let snowball_speed = base_speed + bonus_speed;

//         let dir = self.player.forward_vector();
//         let spawn_pos = self.player.pos + dir * (self.player.radius + 8.0);

//         self.snowballs.push(Snowball {
//             pos: spawn_pos,
//             vel: dir * snowball_speed,
//             life: 2.0,
//         });

//         let recoil_strength = 0.45 + 1.0 * charge_t;
//         self.player.vel -= dir * (snowball_speed * recoil_strength / 3.0);

//         self.player.spin_timer = 0.0;
//     }
// }

// impl EventHandler for MainState {
//     fn update(&mut self, ctx: &mut Context) -> GameResult<()> {
//         let dt = ctx.time.delta().as_secs_f32();

//         if self.player.rotating_left {
//             self.player.rotation -= self.player.rotation_speed_deg * dt;
//             self.player.spin_timer += dt;
//         }
//         if self.player.rotating_right {
//             self.player.rotation += self.player.rotation_speed_deg * dt;
//             self.player.spin_timer += dt;
//         }

//         if self.player.rotation > 360.0 || self.player.rotation < -360.0 {
//             self.player.rotation = self.player.rotation % 360.0;
//         }

//         self.player.pos += self.player.vel * dt;

//         self.player.vel *= self.friction.powf(dt * 60.0);

//         if self.player.pos.x < 0.0 {
//             self.player.pos.x = 0.0;
//             self.player.vel.x = 0.0;
//         }
//         if self.player.pos.x > SCREEN_W {
//             self.player.pos.x = SCREEN_W;
//             self.player.vel.x = 0.0;
//         }
//         if self.player.pos.y < 0.0 {
//             self.player.pos.y = 0.0;
//             self.player.vel.y = 0.0;
//         }
//         if self.player.pos.y > SCREEN_H {
//             self.player.pos.y = SCREEN_H;
//             self.player.vel.y = 0.0;
//         }

//         for sb in &mut self.snowballs {
//             sb.pos += sb.vel * dt;
//             sb.vel *= 0.995f32;
//             sb.life -= dt;
//         }
//         self.snowballs.retain(|s| {
//             s.life > 0.0
//                 && s.pos.x >= -50.0
//                 && s.pos.x <= SCREEN_W + 50.0
//                 && s.pos.y >= -50.0
//                 && s.pos.y <= SCREEN_H + 50.0
//         });

//         if ctx.time.ticks() % 120 == 0 {
//             let mut rng = rand::rng();
//             self.player.vel +=
//                 Vec2::new(rng.random_range(-10.0..10.0), rng.random_range(-10.0..10.0));
//         }

//         Ok(())
//     }

//     fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
//         let mut canvas = ggez::graphics::Canvas::from_frame(ctx, Color::from_rgb(20, 20, 30));

//         let mut mb = MeshBuilder::new();
//         for x in (0..=(SCREEN_W as i32)).step_by(40) {
//             mb.line(
//                 &[Vec2::new(x as f32, 0.0), Vec2::new(x as f32, SCREEN_H)],
//                 1.0,
//                 Color::BLACK,
//             )?;
//         }
//         for y in (0..=(SCREEN_H as i32)).step_by(40) {
//             mb.line(
//                 &[Vec2::new(0.0, y as f32), Vec2::new(SCREEN_W, y as f32)],
//                 1.0,
//                 Color::BLACK,
//             )?;
//         }

//         let player_color = Color::from_rgb(200, 200, 255);
//         mb.circle(
//             DrawMode::fill(),
//             Vec2::new(self.player.pos.x, self.player.pos.y),
//             self.player.radius,
//             0.5,
//             Color::BLUE,
//         )?;

//         let dir = self.player.forward_vector();
//         let tip = Vec2::new(
//             self.player.pos.x + dir.x * (self.player.radius + 8.0),
//             self.player.pos.y + dir.y * (self.player.radius + 8.0),
//         );
//         let left = Vec2::new(
//             self.player.pos.x + (-dir.y) * 8.0,
//             self.player.pos.y + (dir.x) * 8.0,
//         );
//         let right = Vec2::new(
//             self.player.pos.x + (dir.y) * 8.0,
//             self.player.pos.y + (-dir.x) * 8.0,
//         );
//         mb.polygon(DrawMode::fill(), &[tip, left, right], Color::RED)?;

//         for sb in &self.snowballs {
//             mb.circle(
//                 DrawMode::fill(),
//                 Vec2::new(sb.pos.x, sb.pos.y),
//                 6.0,
//                 0.5,
//                 Color::WHITE,
//             )?;
//         }

//         let mesh = mb.build();
//         let mesh = graphics::Mesh::from_data(&ctx.gfx, mesh);
//         canvas.draw(
//             &mesh,
//             ggez::graphics::DrawParam::default().color(player_color),
//         );

//         let bar_w = 200.0;
//         let bar_h = 12.0;
//         let x = 20.0;
//         let y = SCREEN_H - 30.0;
//         let charge = (self.player.spin_timer / self.player.max_charge).clamp(0.0, 1.0);
//         let bar_back = graphics::Mesh::new_rectangle(
//             ctx,
//             DrawMode::fill(),
//             graphics::Rect::new(x, y, bar_w, bar_h),
//             Color::from_rgba(40, 40, 40, 200),
//         )?;
//         let bar_front = graphics::Mesh::new_rectangle(
//             ctx,
//             DrawMode::fill(),
//             graphics::Rect::new(x, y, bar_w * charge, bar_h),
//             Color::from_rgba(120, 200, 255, 200),
//         )?;
//         canvas.draw(&bar_back, graphics::DrawParam::default());
//         canvas.draw(&bar_front, graphics::DrawParam::default());

//         canvas.finish(ctx)
//     }

//     fn key_down_event(
//         &mut self,
//         _ctx: &mut Context,
//         input: KeyInput,
//         _repeat: bool,
//     ) -> Result<(), GameError> {
//         match input.keycode {
//             Some(KeyCode::Left) => {
//                 self.player.rotating_left = true;
//             }
//             Some(KeyCode::Right) => {
//                 self.player.rotating_right = true;
//             }
//             _ => {}
//         }
//         Ok(())
//     }

//     fn key_up_event(&mut self, _ctx: &mut Context, input: KeyInput) -> Result<(), GameError> {
//         match input.keycode {
//             Some(KeyCode::Left) => {
//                 if self.player.rotating_left {
//                     self.player.rotating_left = false;
//                     self.shoot_from_player();
//                 }
//             }
//             Some(KeyCode::Right) => {
//                 if self.player.rotating_right {
//                     self.player.rotating_right = false;
//                     self.shoot_from_player();
//                 }
//             }
//             _ => {}
//         }
//         Ok(())
//     }
// }

// pub fn main() -> GameResult {
//     let (ctx, event_loop) = ContextBuilder::new("snowball_spin", "you")
//         .window_setup(ggez::conf::WindowSetup::default().title("Snowball Spin"))
//         .window_mode(ggez::conf::WindowMode::default().dimensions(SCREEN_W, SCREEN_H))
//         .build()?;

//     let state = MainState::new()?;
//     event::run(ctx, event_loop, state)
// }
