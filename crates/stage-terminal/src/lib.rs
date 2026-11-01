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
    let scale = bounds.1 as f32 / (rows * 2) as f32;
    let columns = columns_for(bounds, rows);
    let average = |column: u32, half: u32| dot_color(sprite, bounds, scale, column, half);
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

/// The color of one dot of a grid laid over `bounds` at `scale` pixels per dot, averaging what it
/// covers, or nothing when it is mostly transparent.
fn dot_color(sprite: &Sprite, bounds: (u32, u32), scale: f32, column: u32, row: u32) -> Option<[u8; 3]> {
    let (box_width, box_height) = bounds;
    let left = box_width.saturating_sub(sprite.width()) / 2;
    let top = box_height.saturating_sub(sprite.height());
    let span = |at: u32| ((at as f32 * scale) as u32, (((at + 1) as f32 * scale).ceil() as u32).max(1));
    let (from_x, to_x) = span(column);
    let (from_y, to_y) = span(row);
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
}

/// Like [`render_smooth`] in the same room, but each cell holds 2x4 dots instead of 1x2. A cell can
/// still show only two colors, and only one where it has gaps, because the gaps must stay see-through.
/// Terminals need to know these characters from Unicode 16, so this is for those known to.
pub fn render_octants(sprite: &Sprite, bounds: (u32, u32), rows: u32) -> String {
    let scale = bounds.1 as f32 / (rows * 4) as f32;
    let columns = columns_for(bounds, rows);
    let mut art = String::new();
    for row in 0..rows {
        if row > 0 {
            art.push('\n');
        }
        for column in 0..columns {
            let dots: [Option<[u8; 3]>; 8] = std::array::from_fn(|dot| {
                let dot = dot as u32;
                dot_color(sprite, bounds, scale, column * 2 + dot % 2, row * 4 + dot / 2)
            });
            push_octant_cell(&mut art, dots);
        }
    }
    art
}

fn push_octant_cell(art: &mut String, dots: [Option<[u8; 3]>; 8]) {
    let solid: Vec<[u8; 3]> = dots.iter().flatten().copied().collect();
    if solid.is_empty() {
        art.push(' ');
    } else if solid.len() < dots.len() {
        let inked = dots.iter().enumerate().filter(|(_, dot)| dot.is_some()).fold(0, |all, (at, _)| all | 1 << at);
        let [r, g, b] = mean(&solid);
        let _ = write!(art, "\x1b[38;2;{r};{g};{b}m{}\x1b[0m", octant_glyph(inked));
    } else {
        let (inked, ink, paper) = two_colors(&solid);
        let [r, g, b] = ink;
        if inked == u8::MAX {
            let _ = write!(art, "\x1b[38;2;{r};{g};{b}m{}\x1b[0m", octant_glyph(inked));
        } else {
            let [pr, pg, pb] = paper;
            let _ = write!(art, "\x1b[38;2;{r};{g};{b};48;2;{pr};{pg};{pb}m{}\x1b[0m", octant_glyph(inked));
        }
    }
}

fn mean(colors: &[[u8; 3]]) -> [u8; 3] {
    let count = colors.len().max(1) as u32;
    std::array::from_fn(|channel| (colors.iter().map(|color| u32::from(color[channel])).sum::<u32>() / count) as u8)
}

fn distance(a: [u8; 3], b: [u8; 3]) -> u32 {
    a.iter().zip(b).map(|(a, b)| u32::from(a.abs_diff(b)).pow(2)).sum()
}

/// Splits a cell's dots into the two colors that lose the least, as the dots of the first dot's color
/// and that color and the other. The two most different dots start the groups.
fn two_colors(dots: &[[u8; 3]]) -> (u8, [u8; 3], [u8; 3]) {
    let mut centers = (dots[0], dots[0]);
    for (at, a) in dots.iter().enumerate() {
        for b in &dots[at + 1..] {
            if distance(*a, *b) > distance(centers.0, centers.1) {
                centers = (*a, *b);
            }
        }
    }
    if centers.0 == centers.1 {
        return (u8::MAX, centers.0, centers.1);
    }
    let mut inked = 0u8;
    for _ in 0..4 {
        inked = dots.iter().enumerate().fold(0, |all, (at, dot)| {
            if distance(*dot, centers.0) <= distance(*dot, centers.1) { all | 1 << at } else { all }
        });
        let group = |want: bool| -> Vec<[u8; 3]> {
            dots.iter().enumerate().filter(|(at, _)| (inked >> at & 1 == 1) == want).map(|(_, dot)| *dot).collect()
        };
        centers = (mean(&group(true)), mean(&group(false)));
    }
    if inked & 1 == 1 {
        (inked, centers.0, centers.1)
    } else {
        (!inked, centers.1, centers.0)
    }
}

/// Patterns Unicode already had a character for when it added octants, so they have no octant of
/// their own. Bit 0 is the top left dot, bit 1 the top right, and so on down to bit 7.
const OLDER_THAN_OCTANTS: [(u8, char); 26] = [
    (0, ' '),
    (1, '\u{1cea8}'),
    (2, '\u{1ceab}'),
    (3, '\u{1fb82}'),
    (5, '\u{2598}'),
    (10, '\u{259d}'),
    (15, '\u{2580}'),
    (20, '\u{1fbe6}'),
    (40, '\u{1fbe7}'),
    (63, '\u{1fb85}'),
    (64, '\u{1cea3}'),
    (80, '\u{2596}'),
    (85, '\u{258c}'),
    (90, '\u{259e}'),
    (95, '\u{259b}'),
    (128, '\u{1cea0}'),
    (160, '\u{2597}'),
    (165, '\u{259a}'),
    (170, '\u{2590}'),
    (175, '\u{259c}'),
    (192, '\u{2582}'),
    (240, '\u{2584}'),
    (245, '\u{2599}'),
    (250, '\u{259f}'),
    (252, '\u{2586}'),
    (255, '\u{2588}'),
];

/// The character showing this pattern of dots. The octants run in the order of their patterns,
/// skipping the ones above.
fn octant_glyph(dots: u8) -> char {
    match OLDER_THAN_OCTANTS.binary_search_by_key(&dots, |(pattern, _)| *pattern) {
        Ok(at) => OLDER_THAN_OCTANTS[at].1,
        Err(before) => char::from_u32(0x1cd00 + u32::from(dots) - before as u32).unwrap_or(' '),
    }
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

    /// A 2x4 box, which octants draw in one cell, one pixel per dot.
    fn one_cell(dots: [[u8; 4]; 8]) -> String {
        render_octants(&Sprite::new(2, 4, dots.concat()).unwrap(), (2, 4), 1)
    }

    #[test]
    fn octants_show_each_dot_of_a_cell_with_the_transparent_ones_left_empty() {
        let clear = [200, 200, 200, 0];
        let ink = [9, 9, 9, 255];
        assert_eq!(one_cell([clear; 8]), " ");
        assert_eq!(one_cell([ink; 8]), "\x1b[38;2;9;9;9m\u{2588}\x1b[0m");
        assert_eq!(
            one_cell([ink, clear, ink, clear, ink, clear, ink, clear]),
            "\x1b[38;2;9;9;9m\u{258c}\x1b[0m",
            "the left column is the left half block"
        );
        assert_eq!(
            one_cell([clear, clear, ink, clear, clear, clear, clear, clear]),
            "\x1b[38;2;9;9;9m\u{1cd00}\x1b[0m",
            "the third dot alone is the first octant in Unicode"
        );
        assert_eq!(
            one_cell([clear, ink, ink, ink, ink, ink, ink, ink]),
            "\x1b[38;2;9;9;9m\u{1cde5}\x1b[0m",
            "and every dot but the first is the last"
        );
    }

    #[test]
    fn octants_split_a_solid_cell_into_its_two_colors() {
        let (red, blue) = ([200, 0, 0, 255], [0, 0, 200, 255]);
        assert_eq!(
            one_cell([red, red, red, red, blue, blue, blue, blue]),
            "\x1b[38;2;200;0;0;48;2;0;0;200m\u{2580}\x1b[0m"
        );
        assert_eq!(
            one_cell([blue, blue, red, red, red, red, red, red]),
            "\x1b[38;2;0;0;200;48;2;200;0;0m\u{1fb82}\x1b[0m",
            "the first dot's color goes on top"
        );
    }

    #[test]
    fn octants_paint_a_cell_with_gaps_in_one_color_so_that_the_gaps_stay_see_through() {
        let (clear, red, blue) = ([0, 0, 0, 0], [200, 0, 0, 255], [0, 0, 100, 255]);
        assert_eq!(
            one_cell([red, blue, clear, clear, clear, clear, clear, clear]),
            "\x1b[38;2;100;0;50m\u{1fb82}\x1b[0m"
        );
    }

    #[test]
    fn octants_take_the_room_of_half_blocks() {
        let small = Sprite::new(1, 1, vec![9, 9, 9, 255]).unwrap();
        let art = render_octants(&small, (4, 4), 2);
        let lines: Vec<&str> = art.split('\n').collect();
        assert_eq!(lines.len(), 2);
        for line in lines {
            assert_eq!(line.replace(|c: char| c != ' ' && !is_octant_cell(c), "").chars().count(), 4, "{line:?}");
        }
    }

    fn is_octant_cell(c: char) -> bool {
        (0..=255).any(|dots: u8| octant_glyph(dots) == c)
    }

    #[test]
    fn every_pattern_of_dots_has_its_own_glyph() {
        let glyphs: std::collections::HashSet<char> = (0..=255).map(octant_glyph).collect();
        assert_eq!(glyphs.len(), 256);
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
