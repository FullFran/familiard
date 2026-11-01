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
    fn perform(self: Box<Self>, player: Player) -> Result<(), Failure> {
        // A terminal has no window to walk and no pointer to feel, but it can still show
        // the pet looking around and dozing.
        let mut player = player.still();
        let mut out = std::io::stdout();
        // The caption is a line of text here, so only the frames need room.
        let bounds = player.pet_bounds();
        let lines = self.rows + u32::from(player.caption_text().is_some());
        write!(out, "{}", scene(player.caption_text(), player.sprite(), bounds, self.rows))?;
        out.flush()?;
        let mut drawn = 1;
        while self.frames.is_none_or(|limit| drawn < limit) && !player.leaving() {
            let wait: Duration = player.next_frame_in();
            std::thread::sleep(wait);
            let changed = player.tick(wait).is_some();
            if changed && !player.leaving() {
                let art = scene(player.caption_text(), player.sprite(), bounds, self.rows);
                write!(out, "\x1b[{lines}A{art}")?;
                out.flush()?;
                drawn += 1;
            }
        }
        Ok(())
    }
}

/// The caption on its own line, wiping what a longer one left behind, then the sprite.
fn scene(caption: Option<&str>, sprite: &Sprite, bounds: (u32, u32), rows: u32) -> String {
    let art = render(sprite, bounds, rows);
    match caption {
        Some(caption) => format!("\x1b[2K{caption}\n{art}"),
        None => art,
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

/// How wide [`render_smooth`] draws a box of `bounds` in `rows` lines.
pub fn columns_for(bounds: (u32, u32), rows: u32) -> u32 {
    let scale = bounds.1 as f32 / (rows * 2) as f32;
    (bounds.0 as f32 / scale).round() as u32
}

/// Like [`render`], but each half block averages the pixels it covers, because picking one pixel
/// turns a pet into noise at the few lines it gets inside another program's interface.
pub fn render_smooth(sprite: &Sprite, bounds: (u32, u32), rows: u32) -> String {
    let (box_width, box_height) = bounds;
    let scale = box_height as f32 / (rows * 2) as f32;
    let columns = columns_for(bounds, rows);
    let left = box_width.saturating_sub(sprite.width()) / 2;
    let top = box_height.saturating_sub(sprite.height());
    let average = |column: u32, half: u32| -> Option<[u8; 3]> {
        let span = |at: u32| ((at as f32 * scale) as u32, (((at + 1) as f32 * scale).ceil() as u32).max(1));
        let (from_x, to_x) = span(column);
        let (from_y, to_y) = span(half);
        let (mut color, mut alpha, mut cells) = ([0u64; 3], 0u64, 0u64);
        for y in from_y..to_y {
            for x in from_x..to_x {
                cells += 1;
                let inside = x >= left && y >= top && x - left < sprite.width() && y - top < sprite.height();
                if !inside {
                    continue;
                }
                let pixel = sprite.pixel(x - left, y - top);
                let weight = u64::from(pixel[3]);
                alpha += weight;
                for (total, channel) in color.iter_mut().zip(pixel) {
                    *total += u64::from(channel) * weight;
                }
            }
        }
        (alpha * 2 >= cells * 255 && alpha > 0).then(|| color.map(|total| (total / alpha) as u8))
    };
    let mut art = String::new();
    for row in 0..rows {
        if row > 0 {
            art.push('\n');
        }
        for column in 0..columns {
            match (average(column, row * 2), average(column, row * 2 + 1)) {
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
    }
    art
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth_drawing_averages_what_a_half_block_covers() {
        // Two pixels squeezed into one half block: half red, half blue, all solid.
        let sprite = Sprite::new(2, 4, [[200, 0, 0, 255], [0, 0, 100, 255]].repeat(4).concat()).unwrap();
        let art = render_smooth(&sprite, (2, 4), 1);
        assert_eq!(art, "\x1b[38;2;100;0;50;48;2;100;0;50m\u{2580}\x1b[0m");
    }

    #[test]
    fn smooth_drawing_leaves_mostly_empty_cells_empty_and_ignores_their_color() {
        // One line of a 2x4 box is one column, and each half block covers four pixels.
        let clear = [200, 200, 200, 0];
        let solid = [9, 9, 9, 255];
        let quarter = [solid, clear, clear, clear, clear, clear, clear, clear].concat();
        assert_eq!(render_smooth(&Sprite::new(2, 4, quarter).unwrap(), (2, 4), 1), " ", "a quarter is not enough");
        let half = [solid, clear, solid, clear, clear, clear, clear, clear].concat();
        assert_eq!(
            render_smooth(&Sprite::new(2, 4, half).unwrap(), (2, 4), 1),
            "\x1b[38;2;9;9;9m\u{2580}\x1b[0m",
            "half is, keeping only the solid color"
        );
    }

    #[test]
    fn smooth_drawing_keeps_every_frame_the_same_size() {
        let small = Sprite::new(1, 1, vec![9, 9, 9, 255]).unwrap();
        let art = render_smooth(&small, (4, 4), 2);
        assert_eq!(columns_for((4, 4), 2), 4);
        let plain: Vec<String> = art.lines().map(|line| {
            let mut text = String::new();
            let mut escaped = false;
            for c in line.chars() {
                match (escaped, c) {
                    (true, 'm') => escaped = false,
                    (true, _) => {}
                    (false, '\x1b') => escaped = true,
                    (false, c) => text.push(c),
                }
            }
            text
        }).collect();
        assert_eq!(plain.len(), 2);
        assert!(plain.iter().all(|line| line.chars().count() == 4), "{plain:?}");
    }

    #[test]
    fn the_caption_gets_its_own_line_above_the_art() {
        let sprite = Sprite::new(1, 2, vec![9, 9, 9, 255, 9, 9, 9, 255]).unwrap();
        let art = render(&sprite, (1, 2), 1);
        assert_eq!(scene(Some("Bug \u{b7} working"), &sprite, (1, 2), 1), format!("\x1b[2KBug \u{b7} working\n{art}"));
        assert_eq!(scene(None, &sprite, (1, 2), 1), art);
    }

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
