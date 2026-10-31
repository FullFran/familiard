/// One animation frame: straight-alpha RGBA, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Alpha above this counts as part of the pet when deciding what can be clicked.
const SOLID_ALPHA: u8 = 16;

impl Sprite {
    /// Returns `None` when `rgba` does not hold exactly `width * height` pixels.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Self> {
        (rgba.len() == width as usize * height as usize * 4).then_some(Self { width, height, rgba })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = (y as usize * self.width as usize + x as usize) * 4;
        [self.rgba[at], self.rgba[at + 1], self.rgba[at + 2], self.rgba[at + 3]]
    }

    /// Horizontal runs of solid pixels in a row, as `(start, end)` with `end` exclusive.
    pub fn solid_spans(&self, y: u32) -> Vec<(u32, u32)> {
        let mut spans = Vec::new();
        let mut start = None;
        for x in 0..self.width {
            let solid = self.pixel(x, y)[3] > SOLID_ALPHA;
            match (solid, start) {
                (true, None) => start = Some(x),
                (false, Some(from)) => {
                    spans.push((from, x));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(from) = start {
            spans.push((from, self.width));
        }
        spans
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_buffer_of_the_wrong_size() {
        assert!(Sprite::new(2, 2, vec![0; 15]).is_none());
    }

    #[test]
    fn finds_solid_spans_ignoring_faint_pixels() {
        let alpha = [255, 0, 16, 17];
        let rgba = alpha.iter().flat_map(|a| [0, 0, 0, *a]).collect();
        let sprite = Sprite::new(4, 1, rgba).unwrap();
        assert_eq!(sprite.solid_spans(0), vec![(0, 1), (3, 4)]);
    }
}
