use ggez::input::keyboard::KeyCode;

const SHOOT_COOLDOWN_SEC: f32 = 0.5;

#[derive(Default)]
pub struct InputState {
    rotating_left: bool,
    rotating_right: bool,
    spin_timer: f32,
    shoot_cooldown: f32,
}

#[derive(Debug, Clone)]
pub enum PlayerAction {
    RotateLeft,
    RotateRight,
    Shoot,
}

impl InputState {
    pub fn spin_timer(&self) -> f32 {
        self.spin_timer
    }

    pub fn update(&mut self, dt: f32) {
        if self.rotating_left || self.rotating_right {
            self.spin_timer += dt;
        }

        if self.shoot_cooldown > 0.0 {
            self.shoot_cooldown -= dt;
        }
    }

    /// Collect continuous rotation states and resets pending shoot
    pub fn consume_actions(&mut self) -> Option<Vec<PlayerAction>> {
        let mut actions = vec![];

        // Continuous rotation
        if self.rotating_left {
            actions.push(PlayerAction::RotateLeft);
        }
        if self.rotating_right {
            actions.push(PlayerAction::RotateRight);
        }

        if actions.is_empty() {
            None
        } else {
            Some(actions)
        }
    }

    pub fn process_key_down(&mut self, key: KeyCode) {
        match key {
            KeyCode::ArrowLeft => {
                if !self.rotating_left {
                    self.spin_timer = 0.0;
                }
                self.rotating_left = true;
            }
            KeyCode::ArrowRight => {
                if !self.rotating_right {
                    self.spin_timer = 0.0;
                }
                self.rotating_right = true;
            }
            _ => {}
        }
    }

    pub fn process_key_up(&mut self, key: KeyCode) -> Option<PlayerAction> {
        match key {
            KeyCode::ArrowLeft if self.rotating_left => {
                self.rotating_left = false;
                self.spin_timer = 0.0;

                if self.shoot_cooldown <= 0.0 {
                    self.shoot_cooldown = SHOOT_COOLDOWN_SEC;
                    Some(PlayerAction::Shoot)
                } else {
                    None
                }
            }

            KeyCode::ArrowRight if self.rotating_right => {
                self.rotating_right = false;
                self.spin_timer = 0.0;

                if self.shoot_cooldown <= 0.0 {
                    self.shoot_cooldown = SHOOT_COOLDOWN_SEC;
                    Some(PlayerAction::Shoot)
                } else {
                    None
                }
            }

            _ => None,
        }
    }
}
