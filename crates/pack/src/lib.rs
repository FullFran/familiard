//! Loads Codex-style pet packs: a `pet.json` next to a spritesheet. Frames are found
//! through the transparent gaps between them, so sheets do not need a fixed grid.
use familiard_application::{Failure, PetSource};
use familiard_domain::{Clip, Mood, Pet, Sprite};
use image::RgbaImage;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

const SOLID_ALPHA: u8 = 16;

/// Sheet rows in the Codex layout. Rows 1 and 2 (running sideways) have no mood yet.
const ROWS: [(usize, Mood); 7] = [
    (0, Mood::Idle),
    (3, Mood::Done),
    (4, Mood::Celebrating),
    (5, Mood::Failed),
    (6, Mood::Waiting),
    (7, Mood::Working),
    (8, Mood::Reviewing),
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    display_name: String,
    spritesheet_path: PathBuf,
}

pub struct PetPack {
    directory: PathBuf,
}

impl PetPack {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self { directory: directory.into() }
    }
}

impl PetSource for PetPack {
    fn load(&self) -> Result<Pet, Failure> {
        let manifest_path = self.directory.join("pet.json");
        let manifest: Manifest = serde_json::from_str(
            &std::fs::read_to_string(&manifest_path)
                .map_err(|e| format!("{}: {e}", manifest_path.display()))?,
        )
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        let sheet_path = self.directory.join(&manifest.spritesheet_path);
        let sheet = image::open(&sheet_path)
            .map_err(|e| format!("{}: {e}", sheet_path.display()))?
            .into_rgba8();
        Ok(Pet::new(manifest.display_name, clips_from(&sheet))?)
    }
}

fn clips_from(sheet: &RgbaImage) -> HashMap<Mood, Clip> {
    let rows = runs(sheet.height(), |y| (0..sheet.width()).any(|x| solid(sheet, x, y)));
    ROWS.iter()
        .filter_map(|(row, mood)| {
            let (top, bottom) = *rows.get(*row)?;
            let columns = runs(sheet.width(), |x| (top..bottom).any(|y| solid(sheet, x, y)));
            let frames = columns.iter().map(|(left, right)| crop(sheet, *left, top, *right, bottom));
            Some((*mood, Clip::new(frames.collect())?))
        })
        .collect()
}

fn solid(sheet: &RgbaImage, x: u32, y: u32) -> bool {
    sheet.get_pixel(x, y)[3] > SOLID_ALPHA
}

/// Contiguous runs of `length` positions where `occupied` holds, end exclusive.
fn runs(length: u32, occupied: impl Fn(u32) -> bool) -> Vec<(u32, u32)> {
    let mut found = Vec::new();
    let mut start = None;
    for at in 0..length {
        match (occupied(at), start) {
            (true, None) => start = Some(at),
            (false, Some(from)) => {
                found.push((from, at));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        found.push((from, length));
    }
    found
}

fn crop(sheet: &RgbaImage, left: u32, top: u32, right: u32, bottom: u32) -> Sprite {
    let (width, height) = (right - left, bottom - top);
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in top..bottom {
        for x in left..right {
            rgba.extend_from_slice(&sheet.get_pixel(x, y).0);
        }
    }
    Sprite::new(width, height, rgba).expect("crop produces a matching buffer")
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// Draws solid blocks as `(x, y, width, height)` on a transparent sheet.
    fn sheet(width: u32, height: u32, blocks: &[(u32, u32, u32, u32)]) -> RgbaImage {
        let mut image = RgbaImage::new(width, height);
        for (x, y, w, h) in blocks {
            for py in *y..y + h {
                for px in *x..x + w {
                    image.put_pixel(px, py, Rgba([9, 9, 9, 255]));
                }
            }
        }
        image
    }

    #[test]
    fn slices_frames_by_gaps_and_tolerates_uneven_rows() {
        let image = sheet(20, 8, &[(1, 1, 2, 2), (5, 1, 3, 2), (1, 5, 2, 2)]);
        let clips = clips_from(&image);
        let idle = &clips[&Mood::Idle];
        assert_eq!(idle.len(), 2);
        assert_eq!((idle.frame(1).width(), idle.frame(1).height()), (3, 2));
        assert!(!clips.contains_key(&Mood::Working));
    }

    #[test]
    fn maps_rows_to_moods_in_the_codex_layout() {
        let blocks: Vec<_> = (0..9).map(|row| (0, row * 4, 2, 2 + (row % 2))).collect();
        let clips = clips_from(&sheet(4, 40, &blocks));
        assert_eq!(clips[&Mood::Working].frame(0).height(), 3);
        assert_eq!(clips[&Mood::Reviewing].frame(0).height(), 2);
        assert_eq!(clips.len(), 7);
    }

    #[test]
    fn the_bundled_demo_pet_has_a_clip_for_every_mood() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../pets/demo");
        let pet = PetPack::new(dir).load().unwrap();
        assert_eq!(pet.name(), "Demo");
        assert_eq!(pet.clip(Mood::Idle).len(), 9);
        assert_eq!(pet.clip(Mood::Done).len(), 4);
        assert_eq!(pet.clip(Mood::Celebrating).len(), 5);
        assert_eq!(pet.clip(Mood::Working).len(), 6);
    }

    #[test]
    fn loads_a_pack_from_disk_and_reports_missing_files() {
        let dir = std::env::temp_dir().join(format!("familiard-pack-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        sheet(4, 4, &[(0, 0, 2, 2)]).save(dir.join("sheet.png")).unwrap();
        std::fs::write(dir.join("pet.json"), r#"{"id":"a","displayName":"Ana","spritesheetPath":"sheet.png"}"#).unwrap();
        assert_eq!(PetPack::new(&dir).load().unwrap().name(), "Ana");
        std::fs::remove_file(dir.join("sheet.png")).unwrap();
        assert!(PetPack::new(&dir).load().unwrap_err().to_string().contains("sheet.png"));
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(PetPack::new(&dir).load().unwrap_err().to_string().contains("pet.json"));
    }
}
