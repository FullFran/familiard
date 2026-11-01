//! Writes a pet's caption: light text on a dark pill, using a font embedded in the binary.
//! Noto Sans keeps its own license, in `fonts/OFL.txt`.
use familiard_application::{Captioner, Failure};
use familiard_domain::Sprite;
use fontdue::{Font, FontSettings};

const FONT: &[u8] = include_bytes!("../fonts/NotoSans-Regular.ttf");
const TEXT_SIZE: f32 = 13.0;
const PADDING_X: u32 = 9;
const PADDING_Y: u32 = 2;
const PILL: [f32; 3] = [28.0, 28.0, 34.0];
const PILL_ALPHA: f32 = 0.9;
const TEXT: [f32; 3] = [245.0, 245.0, 245.0];

pub struct FontCaptioner {
    font: Font,
    ascent: f32,
    height: u32,
}

impl FontCaptioner {
    pub fn new() -> Result<Self, Failure> {
        let settings = FontSettings { scale: TEXT_SIZE, load_substitutions: false, ..FontSettings::default() };
        let font = Font::from_bytes(FONT, settings).map_err(|error| format!("cannot read the caption font: {error}"))?;
        let line = font.horizontal_line_metrics(TEXT_SIZE).ok_or("the caption font has no line metrics")?;
        let height = (line.ascent - line.descent).ceil() as u32 + 2 * PADDING_Y;
        Ok(Self { font, ascent: line.ascent, height })
    }

    fn text_width(&self, text: &str) -> f32 {
        text.chars().map(|c| self.font.metrics(c, TEXT_SIZE).advance_width).sum()
    }

    /// How much of the text covers each pixel of a pill of this size, from 0 to 1.
    fn text_coverage(&self, text: &str, width: u32, height: u32) -> Vec<f32> {
        let mut coverage = vec![0.0; width as usize * height as usize];
        let baseline = PADDING_Y as f32 + self.ascent;
        let mut pen = PADDING_X as f32;
        for c in text.chars() {
            let (metrics, bitmap) = self.font.rasterize(c, TEXT_SIZE);
            let left = pen.round() as i32 + metrics.xmin;
            let top = baseline.round() as i32 - metrics.ymin - metrics.height as i32;
            for (at, value) in bitmap.iter().enumerate() {
                let x = left + (at % metrics.width) as i32;
                let y = top + (at / metrics.width) as i32;
                // A glyph can reach past the pill, which simply cuts it.
                if (0..width as i32).contains(&x) && (0..height as i32).contains(&y) {
                    let cell = &mut coverage[y as usize * width as usize + x as usize];
                    *cell = (*cell + *value as f32 / 255.0).min(1.0);
                }
            }
            pen += metrics.advance_width;
        }
        coverage
    }
}

impl Captioner for FontCaptioner {
    fn size_of(&self, text: &str) -> (u32, u32) {
        (self.text_width(text).ceil() as u32 + 2 * PADDING_X, self.height)
    }

    fn draw(&self, text: &str) -> Sprite {
        let (width, height) = self.size_of(text);
        let coverage = self.text_coverage(text, width, height);
        let mut rgba = Vec::with_capacity(coverage.len() * 4);
        for (at, ink) in coverage.iter().enumerate() {
            let (x, y) = ((at % width as usize) as u32, (at / width as usize) as u32);
            let below = PILL_ALPHA * pill_coverage(x, y, width, height);
            // Straight alpha, as a sprite is: text over pill, then undo the premultiplying.
            let alpha = ink + below * (1.0 - ink);
            let channel = |index: usize| {
                if alpha == 0.0 {
                    return 0;
                }
                ((TEXT[index] * ink + PILL[index] * below * (1.0 - ink)) / alpha).round() as u8
            };
            rgba.extend_from_slice(&[channel(0), channel(1), channel(2), (alpha * 255.0).round() as u8]);
        }
        Sprite::new(width, height, rgba).expect("the buffer is built for this size")
    }
}

/// How much of a pixel lies inside a pill of this size, softening the round ends.
fn pill_coverage(x: u32, y: u32, width: u32, height: u32) -> f32 {
    let radius = height as f32 / 2.0;
    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
    let nearest_center = px.clamp(radius, (width as f32 - radius).max(radius));
    let distance = (px - nearest_center).hypot(py - radius);
    (radius + 0.5 - distance).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use familiard_application::Player;
    use familiard_domain::{Clip, Mood, Pet, Report, Signal};
    use std::collections::HashMap;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    fn captioner() -> FontCaptioner {
        FontCaptioner::new().unwrap()
    }

    #[test]
    fn draws_exactly_the_size_it_promises_whatever_the_text() {
        let captioner = captioner();
        for text in ["", "Bug del login \u{b7} working", "a", "\u{f1}and\u{fa} \u{2026}"] {
            let sprite = captioner.draw(text);
            assert_eq!((sprite.width(), sprite.height()), captioner.size_of(text), "{text:?}");
        }
    }

    #[test]
    fn every_caption_has_the_same_height_so_the_box_can_be_reserved() {
        let captioner = captioner();
        assert_eq!(captioner.size_of("acemno").1, captioner.size_of("Ágpjy").1);
    }

    #[test]
    fn longer_text_is_wider() {
        let captioner = captioner();
        assert!(captioner.size_of("Bug \u{b7} needs you").0 > captioner.size_of("Bug").0);
    }

    #[test]
    fn it_is_a_dark_pill_with_round_ends_and_light_text() {
        let sprite = captioner().draw("Bug");
        let (width, height) = (sprite.width(), sprite.height());
        assert_eq!(sprite.pixel(0, 0)[3], 0, "the corners are round");
        assert_eq!(sprite.pixel(width - 1, height - 1)[3], 0);
        let edge = sprite.pixel(width / 2, 0);
        assert_eq!(edge, [28, 28, 34, 230], "the pill, slightly see through");
        let brightest = (0..width * height).map(|at| sprite.pixel(at % width, at / width)[0]).max().unwrap();
        assert!(brightest > 200, "the text is light: {brightest}");
    }

    #[test]
    fn characters_the_font_lacks_do_not_panic() {
        let captioner = captioner();
        for text in ["\u{1f980}", "\u{65e5}\u{672c}\u{8a9e}", "\u{ffff}", "\u{200b}", "\u{0}"] {
            let sprite = captioner.draw(text);
            assert_eq!(sprite.height(), captioner.size_of(text).1, "{text:?}");
        }
    }

    #[test]
    fn with_the_real_font_a_long_label_fits_and_the_box_does_not_move() {
        let frame = Sprite::new(10, 10, vec![255; 400]).unwrap();
        let pet = Pet::new("p", HashMap::from([(Mood::Idle, Clip::new(vec![frame]).unwrap())])).unwrap();
        let (outbox, inbox) = channel();
        let long = "A very long label that could never fit above the head of any reasonable pet at all";
        let mut player = Player::new(pet, inbox).captioned(long, &captioner());
        let reserved = player.bounds();
        assert!(reserved.0 <= 240, "{reserved:?}");
        for signal in [Signal::Working, Signal::Waiting, Signal::Done] {
            outbox.send(Report { session: "s".into(), signal, folder: None, pane: None }).unwrap();
            player.tick(Duration::ZERO);
            assert_eq!(player.bounds(), reserved);
            let text = player.caption_text().unwrap();
            assert!(text.starts_with("A very") && text.contains('\u{2026}'), "{text}");
        }
        let mut canvas = vec![0u8; (reserved.0 * reserved.1 * 4) as usize];
        assert_eq!(player.paint(&mut canvas).1, reserved.1 - 10);
    }
}
