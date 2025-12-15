use crate::state::GameState;

pub fn update_physics(state: &mut GameState, dt: f32) {
    // Update player motion
    state.player.pos += state.player.vel * dt;
    state.player.vel *= state.friction.powf(dt * 60.0);

    // Clamp to map boundaries
    state.player.pos.x = state.player.pos.x.clamp(0.0, state.map.width);
    state.player.pos.y = state.player.pos.y.clamp(0.0, state.map.height);

    // Update snowballs
    for sb in &mut state.snowballs {
        sb.pos += sb.vel * dt;
        sb.vel *= 0.995;
        sb.life -= dt;
    }
    state.snowballs.retain(|s| s.life > 0.0);

    // Ball physics (basic)
    if let Some(ball) = &mut state.ball {
        ball.pos += ball.vel * dt;
        ball.vel *= 0.995;
    }
}
