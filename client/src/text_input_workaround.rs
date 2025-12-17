use ggez::Context;
use ggez::winit::keyboard::{Key, SmolStr};
use std::collections::HashSet;

pub struct CharInput {
    pressed: HashSet<char>,
}

impl CharInput {
    pub fn new() -> Self {
        Self {
            pressed: HashSet::new(),
        }
    }

    pub fn collect(&mut self, ctx: &Context) -> Vec<char> {
        let mut out = Vec::new();

        let mut currently_down = HashSet::new();

        for key in ctx.keyboard.pressed_logical_keys.iter() {
            if let Key::Character(s) = key {
                if let Some(c) = smolstr_to_char(s) {
                    currently_down.insert(c);

                    if !self.pressed.contains(&c) {
                        out.push(c);
                    }
                }
            }
        }

        self.pressed = currently_down;
        out
    }
}

fn smolstr_to_char(s: &SmolStr) -> Option<char> {
    let mut chars = s.chars();
    let c = chars.next()?;
    if chars.next().is_none() {
        Some(c)
    } else {
        None
    }
}
