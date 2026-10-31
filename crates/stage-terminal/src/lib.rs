//! Shows the pet in any truecolor terminal using half blocks. It works on every
//! platform and is the base for embedding the pet in text interfaces.
use familiard_application::{Failure, Player, Stage};
use familiard_domain::Sprite;
use std::fmt::Write as _;
use std::io::Write;
use std::time::Duration;

const SOLID_ALPHA: u8 = 128;

pub struct TerminalStage {
    rows: u32,
    frames: Option<u32>,
}

impl TerminalStage {
    /// `frames` limits how many frames are drawn; `None` runs until interrupted.
    pub fn new(rows: u32, frames: Option<u32>) -> Self {
        Self { rows, frames }
    }
}

impl Stage for TerminalStage {
    fn perform(self: Box<Self>, mut player: Player) -> Result<(), Failure> {
        let mut out = std::io::stdout();
        let bounds = player.bounds();
        let mut art = render(player.sprite(), bounds, self.rows);
        write!(out, "{art}")?;
        out.flush()?;
        let mut drawn = 1;
        while self.frames.is_none_or(|limit| drawn < limit) {
            let wait: Duration = player.next_frame_in();
            std::thread::sleep(wait);
            if let Some(sprite) = player.tick(wait) {
                art = render(sprite, bounds, self.rows);
                write!(out, "\x1b[{}A{art}", self.rows)?;
                out.flush()?;
                drawn += 1;
            }
        }
        Ok(())
    }
}

/// Draws a sprite `rows` lines tall, bottom-centered inside `bounds`.
pub fn render(sprite: &Sprite, bounds: (u32, u32), rows: u32) -> String {
    let (box_width, box_height) = bounds;
    let scale = box_height as f32 / (rows * 2) as f32;
    let columns = (box_width as f32 / scale).round() as u32;
    let left = box_width.saturating_sub(sprite.width()) / 2;
    let top = box_height.saturating_sub(sprite.height());
    let pixel = |column: u32, half: u32| -> Option<[u8; 4]> {
        let x = ((column as f32 + 0.5) * scale) as u32;
        let y = ((half as f32 + 0.5) * scale) as u32;
        if x < left || y < top {
            return None;
        }
        let (x, y) = (x - left, y - top);
        if x >= sprite.width() || y >= sprite.height() {
            return None;
        }
        Some(sprite.pixel(x, y)).filter(|p| p[3] >= SOLID_ALPHA)
    };
    let mut art = String::new();
    for row in 0..rows {
        for column in 0..columns {
            match (pixel(column, row * 2), pixel(column, row * 2 + 1)) {
                (None, None) => art.push(' '),
                (Some(up), None) => {
                    let _ = write!(art, "\x1b[38;2;{};{};{}m\u{2580}\x1b[0m", up[0], up[1], up[2]);
                }
                (None, Some(down)) => {
                    let _ = write!(art, "\x1b[38;2;{};{};{}m\u{2584}\x1b[0m", down[0], down[1], down[2]);
                }
                (Some(up), Some(down)) => {
                    let _ = write!(
                        art,
                        "\x1b[38;2;{};{};{};48;2;{};{};{}m\u{2580}\x1b[0m",
                        up[0], up[1], up[2], down[0], down[1], down[2]
                    );
                }
            }
        }
        art.push('\n');
    }
    art
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_half_blocks_and_leaves_transparency_blank() {
        // Two pixels wide, two tall: left column red over blue, right column empty.
        let rgba = vec![255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0];
        let sprite = Sprite::new(2, 2, rgba).unwrap();
        assert_eq!(
            render(&sprite, (2, 2), 1),
            "\x1b[38;2;255;0;0;48;2;0;0;255m\u{2580}\x1b[0m \n"
        );
    }

    #[test]
    fn keeps_small_frames_on_the_floor_of_the_box() {
        let sprite = Sprite::new(1, 1, vec![9, 9, 9, 255]).unwrap();
        assert_eq!(render(&sprite, (1, 2), 1), "\x1b[38;2;9;9;9m\u{2584}\x1b[0m\n");
    }
}
