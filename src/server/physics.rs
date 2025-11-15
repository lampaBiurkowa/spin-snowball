use glam::Vec2;

use crate::{
    Ball, GameState, Team,
    map::{MapObject, PhysicsSettings, matches_ball, matches_player},
};

pub struct SimulateCollisionResponse {
    pub players_in_holes: Vec<Team>,
    pub snowballs_in_holes: Vec<u64>,
    pub goal_for_team: Option<u32>,
}

pub trait Body {
    fn pos(&self) -> Vec2;
    fn pos_mut(&mut self) -> &mut Vec2;
    fn vel(&self) -> Vec2;
    fn vel_mut(&mut self) -> &mut Vec2;
    fn radius(&self, physics: &PhysicsSettings) -> f32;
    fn mass(&self, physics: &PhysicsSettings) -> f32;
}

impl Body for crate::Player {
    fn pos(&self) -> Vec2 {
        self.pos
    }
    fn pos_mut(&mut self) -> &mut Vec2 {
        &mut self.pos
    }
    fn vel(&self) -> Vec2 {
        self.vel
    }
    fn vel_mut(&mut self) -> &mut Vec2 {
        &mut self.vel
    }
    fn radius(&self, physics: &PhysicsSettings) -> f32 {
        physics.player_radius
    }
    fn mass(&self, physics: &PhysicsSettings) -> f32 {
        physics.player_mass
    }
}

impl Body for crate::Snowball {
    fn pos(&self) -> Vec2 {
        self.pos
    }
    fn pos_mut(&mut self) -> &mut Vec2 {
        &mut self.pos
    }
    fn vel(&self) -> Vec2 {
        self.vel
    }
    fn vel_mut(&mut self) -> &mut Vec2 {
        &mut self.vel
    }
    fn radius(&self, physics: &PhysicsSettings) -> f32 {
        physics.snowball_radius
    }
    fn mass(&self, physics: &PhysicsSettings) -> f32 {
        physics.snowball_mass
    }
}

impl Body for Ball {
    fn pos(&self) -> Vec2 {
        self.pos
    }
    fn pos_mut(&mut self) -> &mut Vec2 {
        &mut self.pos
    }
    fn vel(&self) -> Vec2 {
        self.vel
    }
    fn vel_mut(&mut self) -> &mut Vec2 {
        &mut self.vel
    }

    fn radius(&self, physics: &PhysicsSettings) -> f32 {
        physics.ball_radius
    }
    fn mass(&self, physics: &PhysicsSettings) -> f32 {
        physics.ball_mass
    }
}

pub fn simulate_movement(game_state: &mut GameState, dt: f32) {
    for (_id, p) in game_state.players.iter_mut() {
        // rotation
        if p.rotating_left {
            p.rot_deg -= 180.0 * dt;
            p.spin_timer += dt;
        }
        if p.rotating_right {
            p.rot_deg += 180.0 * dt;
            p.spin_timer += dt;
        }

        if p.rot_deg > 360.0 || p.rot_deg < -360.0 {
            p.rot_deg = p.rot_deg % 360.0;
        }

        // integrate
        p.pos += p.vel * dt;

        // friction
        p.vel *= game_state.map.physics.friction_per_frame.powf(dt * 60.0);

        // clamp to world
        p.pos.x = p.pos.x.clamp(0.0, game_state.map.width);
        p.pos.y = p.pos.y.clamp(0.0, game_state.map.height);
    }

    // integrate snowballs
    for (_id, s) in game_state.snowballs.iter_mut() {
        s.pos += s.vel * dt;
        // optional: apply friction / air drag if desired (kept constant motion here)
    }

    if let Some(ball) = &mut game_state.ball {
        ball.pos += ball.vel * dt;
        ball.vel *= game_state.map.physics.friction_per_frame.powf(dt * 60.0);
        let r = game_state.map.physics.ball_radius;
        ball.pos.x = ball.pos.x.clamp(r, game_state.map.width - r);
        ball.pos.y = ball.pos.y.clamp(r, game_state.map.height - r);
    }
}

/// Top-level collision simulation.
/// Uses helper functions below for clarity.
pub fn simulate_collisions(game_state: &mut GameState) -> SimulateCollisionResponse {
    simulate_player_player_collisions(game_state);
    simulate_player_snowball_collisions(game_state);
    simulate_ball_collisions(game_state);
    simulate_map_collisions(game_state)
}

/// Player vs Player collisions (circle-circle elastic).
fn simulate_player_player_collisions(game_state: &mut GameState) {
    // Collect ids to avoid borrowing issues when iterating map
    let player_ids: Vec<String> = game_state.players.keys().cloned().collect();

    for i in 0..player_ids.len() {
        for j in (i + 1)..player_ids.len() {
            let id_a = &player_ids[i];
            let id_b = &player_ids[j];

            if let [Some(a), Some(b)] = game_state.players.get_disjoint_mut([id_a, id_b]) {
                resolve_circle_circle(
                    a,
                    b,
                    game_state.map.physics.player_bounciness,
                    &game_state.map.physics,
                );
            }
        }
    }
}

/// Player vs Snowball collisions (circle-circle with differing masses).
fn simulate_player_snowball_collisions(game_state: &mut GameState) {
    let player_ids: Vec<String> = game_state.players.keys().cloned().collect();
    let snow_ids: Vec<u64> = game_state.snowballs.keys().cloned().collect();

    for pid in player_ids.iter() {
        for sid in snow_ids.iter() {
            // We need mutable refs to a player and a snowball. They live in different hashmaps,
            // so two mutable borrows are fine.
            // However, since we earlier collected ids, they are guaranteed distinct & stable.
            let player_exists = game_state.players.contains_key(pid);
            let snow_exists = game_state.snowballs.contains_key(sid);
            if !player_exists || !snow_exists {
                continue;
            }

            // Re-borrow concrete types (can't hold both as &mut dyn Body across scope easily)
            // Re-check existence then get mutable concrete references:
            if let (Some(p_mut), Some(s_mut)) = (
                game_state.players.get_mut(pid),
                game_state.snowballs.get_mut(sid),
            ) {
                resolve_circle_circle_custom_masses(
                    p_mut,
                    s_mut,
                    game_state.map.physics.snowball_bounciness,
                    &game_state.map.physics,
                );
            }
        }
    }
}

fn simulate_ball_collisions(game_state: &mut GameState) {
    let ball = match game_state.ball.as_mut() {
        Some(b) => b,
        None => return,
    };

    let physics = &game_state.map.physics;

    // Ball vs players
    for p in game_state.players.values_mut() {
        resolve_circle_circle(p, ball, physics.ball_bounciness, physics);
    }

    // Ball vs snowballs
    for s in game_state.snowballs.values_mut() {
        resolve_circle_circle(s, ball, physics.ball_bounciness, physics);
    }
}

fn simulate_map_collisions(game_state: &mut GameState) -> SimulateCollisionResponse {
    let mut response = SimulateCollisionResponse {
        players_in_holes: vec![],
        snowballs_in_holes: vec![],
        goal_for_team: None,
    };

    // Players
    let mut player_respawns: Vec<String> = Vec::new();
    for (id, p) in game_state.players.iter_mut() {
        handle_map_for_body_player(
            p,
            id,
            &mut player_respawns,
            &game_state.map.objects,
            &game_state.map.physics,
            &mut response,
        );
    }

    // Snowballs
    let snow_ids: Vec<u64> = game_state.snowballs.keys().cloned().collect();
    for sid in snow_ids.iter() {
        // we need the original position for collision checks to avoid mutable borrow issues
        if let Some(sb_snapshot) = game_state.snowballs.get(sid).map(|s| s.pos) {
            for obj in &game_state.map.objects {
                match obj {
                    MapObject::Circle {
                        x,
                        y,
                        radius,
                        factor,
                        color: _,
                        is_hole,
                        mask: _,
                    } => {
                        if circle_intersects_circle(
                            sb_snapshot.x,
                            sb_snapshot.y,
                            game_state.map.physics.snowball_radius,
                            *x,
                            *y,
                            *radius,
                        ) {
                            if *is_hole {
                                response.snowballs_in_holes.push(*sid);
                            } else if let Some(sbm) = game_state.snowballs.get_mut(sid) {
                                let delta = sb_snapshot - Vec2::new(*x, *y);
                                let dist = delta.length().max(0.0001);
                                let n = delta / dist;
                                sbm.pos = Vec2::new(*x, *y)
                                    + n * (*radius + game_state.map.physics.snowball_radius);
                                sbm.vel = sbm.vel - 2.0 * sbm.vel.dot(n) * n * (*factor);
                            }
                        }
                    }
                    MapObject::Rect {
                        x,
                        y,
                        w,
                        h,
                        factor,
                        color: _,
                        is_hole,
                        mask: _,
                    } => {
                        if circle_intersects_rect(
                            sb_snapshot.x,
                            sb_snapshot.y,
                            game_state.map.physics.snowball_radius,
                            *x,
                            *y,
                            *w,
                            *h,
                        ) {
                            if *is_hole {
                                response.snowballs_in_holes.push(*sid);
                            } else if let Some(sbm) = game_state.snowballs.get_mut(sid) {
                                let cx = sb_snapshot.x.clamp(*x, x + w);
                                let cy = sb_snapshot.y.clamp(*y, y + h);
                                let mut n = sb_snapshot - Vec2::new(cx, cy);
                                if n.length_squared() < 1e-6 {
                                    n = Vec2::new(
                                        (sb_snapshot.x - (x + w / 2.0)).signum(),
                                        (sb_snapshot.y - (y + h / 2.0)).signum(),
                                    );
                                }
                                let n = n.normalize_or_zero();
                                sbm.pos += n * (game_state.map.physics.snowball_radius * 0.5 + 0.5);
                                sbm.vel = sbm.vel - 2.0 * sbm.vel.dot(n) * n * factor;
                            }
                        }
                    }
                }
            }
        }
    }

    // Ball
    if let Some(ball) = &mut game_state.ball {
        for obj in &game_state.map.objects {
            let mask = match obj {
                MapObject::Circle { mask, .. } | MapObject::Rect { mask, .. } => mask,
            };
            if !matches_ball(&mask) {
                continue;
            }
            match obj {
                MapObject::Circle {
                    x,
                    y,
                    radius,
                    factor,
                    color: _,
                    is_hole,
                    mask: _,
                } => {
                    if circle_intersects_circle(
                        ball.pos.x,
                        ball.pos.y,
                        game_state.map.physics.ball_radius,
                        *x,
                        *y,
                        *radius,
                    ) {
                        if *is_hole {
                        } else {
                            let delta = ball.pos - Vec2::new(*x, *y);
                            let dist = delta.length().max(0.0001);
                            let n = delta / dist;
                            ball.pos = Vec2::new(*x, *y)
                                + n * (*radius + game_state.map.physics.ball_radius);
                            ball.vel = ball.vel - 2.0 * ball.vel.dot(n) * n * (*factor);
                        }
                    }
                }
                MapObject::Rect {
                    x,
                    y,
                    w,
                    h,
                    factor,
                    color: _,
                    is_hole,
                    mask: _,
                } => {
                    if circle_intersects_rect(
                        ball.pos.x,
                        ball.pos.y,
                        game_state.map.physics.ball_radius,
                        *x,
                        *y,
                        *w,
                        *h,
                    ) {
                        if *is_hole {
                        } else {
                            let cx = ball.pos.x.clamp(*x, x + w);
                            let cy = ball.pos.y.clamp(*y, y + h);
                            let mut n = ball.pos - Vec2::new(cx, cy);
                            if n.length_squared() < 1e-6 {
                                n = Vec2::new(
                                    (ball.pos.x - (x + w / 2.0)).signum(),
                                    (ball.pos.y - (y + h / 2.0)).signum(),
                                );
                            }
                            let n = n.normalize_or_zero();
                            ball.pos += n * (game_state.map.physics.ball_radius * 0.5 + 0.5);
                            ball.vel = ball.vel - 2.0 * ball.vel.dot(n) * n * factor;
                        }
                    }
                }
            }
        }
    }

    if let Some(ball) = &mut game_state.ball {
        if let Some(x) = &game_state.map.football {
            for goal in x.goals.iter() {
                if circle_intersects_rect(
                    ball.pos.x,
                    ball.pos.y,
                    game_state.map.physics.ball_radius,
                    goal.x,
                    goal.y,
                    goal.w,
                    goal.h,
                ) {
                    response.goal_for_team = Some(goal.team);
                }
            }
        }
    }

    response
}

/// Handle map collisions for a player body.
/// If player falls into hole, push its id onto `respawns` to be processed later.
fn handle_map_for_body_player(
    player: &mut crate::Player,
    id: &str,
    respawns: &mut Vec<String>,
    objects: &[MapObject],
    physics: &PhysicsSettings,
    response: &mut SimulateCollisionResponse,
) {
    let pos = player.pos;
    for obj in objects {
        let mask = match obj {
            MapObject::Circle { mask, .. } | MapObject::Rect { mask, .. } => mask,
        };
        if !matches_player(&mask, player.team) {
            continue;
        }
        match obj {
            MapObject::Circle {
                x,
                y,
                radius,
                factor,
                color: _,
                is_hole,
                mask: _,
            } => {
                if circle_intersects_circle(pos.x, pos.y, physics.player_radius, *x, *y, *radius) {
                    if *is_hole {
                        response.players_in_holes.push(player.team);
                    } else {
                        let delta = pos - Vec2::new(*x, *y);
                        let dist = delta.length().max(0.0001);
                        let n = delta / dist;
                        player.pos = Vec2::new(*x, *y) + n * (*radius + physics.player_radius);
                        player.vel = player.vel - 2.0 * player.vel.dot(n) * n * (*factor);
                    }
                }
            }
            MapObject::Rect {
                x,
                y,
                w,
                h,
                factor,
                color: _,
                is_hole,
                mask: _,
            } => {
                if circle_intersects_rect(pos.x, pos.y, physics.player_radius, *x, *y, *w, *h) {
                    if *is_hole {
                        response.players_in_holes.push(player.team);
                    } else {
                        let cx = pos.x.clamp(*x, x + w);
                        let cy = pos.y.clamp(*y, y + h);
                        let mut n = pos - Vec2::new(cx, cy);

                        if n.length_squared() < 1e-6 {
                            // choose outward axis
                            let left_pen = (pos.x - *x).abs();
                            let right_pen = (pos.x - (x + w)).abs();
                            let top_pen = (pos.y - *y).abs();
                            let bottom_pen = (pos.y - (y + h)).abs();

                            if left_pen <= right_pen
                                && left_pen <= top_pen
                                && left_pen <= bottom_pen
                            {
                                n = Vec2::new(-1.0, 0.0);
                            } else if right_pen <= left_pen
                                && right_pen <= top_pen
                                && right_pen <= bottom_pen
                            {
                                n = Vec2::new(1.0, 0.0);
                            } else if top_pen <= bottom_pen {
                                n = Vec2::new(0.0, -1.0);
                            } else {
                                n = Vec2::new(0.0, 1.0);
                            }
                        }

                        let n = n.normalize_or_zero();
                        let overlap = physics.player_radius - (pos - Vec2::new(cx, cy)).length();
                        if overlap > 0.0 {
                            player.pos += n * overlap;
                        } else {
                            player.pos += n * 1.0;
                        }
                        player.vel = player.vel - 2.0 * player.vel.dot(n) * n * factor;
                    }
                }
            }
        }
    }
}

/// Resolve circle-circle collision between two bodies with their own masses & radii.
/// This is generic and works for Player <-> Player or Player <-> Snowball (if you pass different bodies).
fn resolve_circle_circle<A: Body, B: Body>(
    a: &mut A,
    b: &mut B,
    bounciness: f32,
    physics: &PhysicsSettings,
) {
    let delta = b.pos() - a.pos();
    let dist = delta.length();
    let min_dist = a.radius(physics) + b.radius(physics);

    if dist <= 0.0 || dist >= min_dist {
        return;
    }

    let n = delta / dist;
    let penetration = min_dist - dist;

    // separate proportionally by mass
    let total_mass = a.mass(physics) + b.mass(physics);
    *a.pos_mut() -= n * (penetration * (b.mass(physics) / total_mass));
    *b.pos_mut() += n * (penetration * (a.mass(physics) / total_mass));

    // relative velocity along normal
    let rel_vel = b.vel() - a.vel();
    let sep_vel = rel_vel.dot(n);

    if sep_vel >= 0.0 {
        return; // moving apart already
    }

    let impulse = -(1.0 + bounciness) * sep_vel / total_mass;

    *a.vel_mut() -= n * (impulse * b.mass(physics));
    *b.vel_mut() += n * (impulse * a.mass(physics));
}

/// Convenience wrapper when you need to pass concrete Player and Snowball types (works the same).
fn resolve_circle_circle_custom_masses(
    a: &mut crate::Player,
    b: &mut crate::Snowball,
    bounciness: f32,
    physics: &PhysicsSettings,
) {
    // use the same generic resolver by bridging to trait impls (already implemented)
    resolve_circle_circle(a, b, bounciness, physics);
}

/// Basic circle-rectangle intersection test (returns true if circle intersects rect).
#[inline]
fn circle_intersects_rect(px: f32, py: f32, r_entity: f32, x: f32, y: f32, w: f32, h: f32) -> bool {
    let closest_x = px.clamp(x, x + w);
    let closest_y = py.clamp(y, y + h);
    dist2(px, py, closest_x, closest_y) < r_entity * r_entity
}

/// Basic circle-circle intersection test.
#[inline]
fn circle_intersects_circle(px: f32, py: f32, r_entity: f32, x: f32, y: f32, r_obj: f32) -> bool {
    dist2(px, py, x, y) < (r_entity + r_obj) * (r_entity + r_obj)
}

#[inline]
fn dist2(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = ax - bx;
    let dy = ay - by;
    dx * dx + dy * dy
}
