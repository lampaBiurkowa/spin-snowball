use ggez::input::keyboard::KeyCode;

#[derive(Default)]
pub struct InputState {
    rotating_left: bool,
    rotating_right: bool,
    spin_timer: f32,
}

#[derive(Debug, Clone)]
pub enum PlayerAction {
    RotateLeft,
    RotateRight,
    Shoot(f32), // charge ratio [0.0 - 1.0]
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spin_timer(&self) -> f32 {
        self.spin_timer
    }

    /// Called every frame by main.rs
    pub fn update(&mut self, dt: f32) {
        if self.rotating_left || self.rotating_right {
            self.spin_timer += dt;
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
        let max_charge = 1.0;
        match key {
            KeyCode::ArrowLeft if self.rotating_left => {
                self.rotating_left = false;
                let charge_ratio = (self.spin_timer / max_charge).clamp(0.0, 1.0);
                self.spin_timer = 0.0;
                Some(PlayerAction::Shoot(charge_ratio))
            }
            KeyCode::ArrowRight if self.rotating_right => {
                self.rotating_right = false;
                let charge_ratio = (self.spin_timer / max_charge).clamp(0.0, 1.0);
                self.spin_timer = 0.0;
                Some(PlayerAction::Shoot(charge_ratio))
            }
            _ => None,
        }
    }
}
