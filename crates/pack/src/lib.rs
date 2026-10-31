//! Loads Codex-style pet packs: a `pet.json` next to a spritesheet. Frames are found
//! through the transparent gaps between them, so sheets do not need a fixed grid.
use familiard_application::{Failure, PetSource};
use familiard_domain::{Clip, Mood, Moves, Pet, Sprite};
use image::RgbaImage;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

const SOLID_ALPHA: u8 = 16;

/// Sheet rows in the Codex layout that belong to a mood.
const ROWS: [(usize, Mood); 7] = [
    (0, Mood::Idle),
    (3, Mood::Done),
    (4, Mood::Celebrating),
    (5, Mood::Failed),
    (6, Mood::Waiting),
    (7, Mood::Working),
    (8, Mood::Reviewing),
];

/// Running right and running left, in every version of the layout.
const RUNNING_ROWS: (usize, usize) = (1, 2);
/// Head turns, which only version 2 of the layout has.
const LOOK_ROWS: [usize; 2] = [9, 10];

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
        if manifest.spritesheet_path.components().count() != 1
            || manifest.spritesheet_path.file_name().is_none()
        {
            return Err(format!("{}: the spritesheet must be a file in the same folder", manifest_path.display()).into());
        }
        let sheet_path = self.directory.join(&manifest.spritesheet_path);
        let sheet = image::open(&sheet_path)
            .map_err(|e| format!("{}: {e}", sheet_path.display()))?
            .into_rgba8();
        Ok(Pet::new(manifest.display_name, clips_from(&sheet))?.with_moves(moves_from(&sheet)))
    }
}

fn clips_from(sheet: &RgbaImage) -> HashMap<Mood, Clip> {
    let rows = rows_of(sheet);
    ROWS.iter().filter_map(|(row, mood)| Some((*mood, row_clip(sheet, &rows, *row)?))).collect()
}

/// What the sheet has rows for: a sheet that stops earlier simply has less.
fn moves_from(sheet: &RgbaImage) -> Moves {
    let rows = rows_of(sheet);
    let (right, left) = RUNNING_ROWS;
    let running = row_clip(sheet, &rows, right).zip(row_clip(sheet, &rows, left));
    let looks = LOOK_ROWS.iter().filter_map(|row| row_clip(sheet, &rows, *row)).collect();
    Moves { running, looks }
}

fn rows_of(sheet: &RgbaImage) -> Vec<(u32, u32)> {
    runs(sheet.height(), |y| (0..sheet.width()).any(|x| solid(sheet, x, y)))
}

fn row_clip(sheet: &RgbaImage, rows: &[(u32, u32)], row: usize) -> Option<Clip> {
    let (top, bottom) = *rows.get(row)?;
    let columns = runs(sheet.width(), |x| (top..bottom).any(|y| solid(sheet, x, y)));
    Clip::new(columns.iter().map(|(left, right)| crop(sheet, *left, top, *right, bottom)).collect())
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
    use familiard_domain::{Life, Pointer};
    use image::Rgba;
    use std::time::Duration;

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
    fn a_spritesheet_outside_the_folder_is_refused() {
        let dir = std::env::temp_dir().join(format!("familiard-escape-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for path in ["../sheet.png", "/etc/passwd", ".."] {
            let manifest = format!(r#"{{"displayName":"A","spritesheetPath":"{path}"}}"#);
            std::fs::write(dir.join("pet.json"), manifest).unwrap();
            let error = PetPack::new(&dir).load().unwrap_err().to_string();
            assert!(error.contains("same folder"), "{path}: {error}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
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

    /// A standard Codex sheet with this many frames in each row, one block per 192x208 cell,
    /// saved with a plain `pet.json` that has nothing but the standard fields.
    fn standard_pack(name: &str, size: (u32, u32), frames_per_row: &[u32]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("familiard-standard-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut blocks = Vec::new();
        for (row, frames) in frames_per_row.iter().enumerate() {
            blocks.extend((0..*frames).map(|column| (column * 192 + 20, row as u32 * 208 + 20, 100, 150)));
        }
        sheet(size.0, size.1, &blocks).save(dir.join("spritesheet.png")).unwrap();
        let manifest = r#"{"id":"std","displayName":"Std","description":"d","spriteVersionNumber":2,"spritesheetPath":"spritesheet.png"}"#;
        std::fs::write(dir.join("pet.json"), manifest).unwrap();
        dir
    }

    fn secs(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    /// Everything a pet does from these rows, told through the same interface a stage uses.
    /// Returns (glances, strolls, doze frame it holds, waves).
    fn behaviours(pet: &Pet) -> (bool, bool, Option<usize>, bool) {
        let idle_time = |life: &mut Life, time| life.advance(pet, time, true);
        let mut glancer = Life::new(Box::new(|| 0)).rooted();
        idle_time(&mut glancer, secs(6));
        let glances = glancer.sprite(pet).is_some();

        let mut stroller = Life::new(Box::new(|| 0));
        idle_time(&mut stroller, secs(60));
        idle_time(&mut stroller, Duration::from_millis(100));
        let strolls = stroller.step().is_some();

        let mut dozer = Life::new(Box::new(|| 0)).rooted();
        idle_time(&mut dozer, secs(300));
        idle_time(&mut dozer, secs(60));
        let held = dozer.sprite(pet).map(|sprite| {
            let row = pet.own_clip(Mood::Failed).unwrap();
            (0..row.len()).find(|at| std::ptr::eq(row.frame(*at), sprite)).unwrap()
        });

        let mut waver = Life::new(Box::new(|| 0));
        waver.pointer(pet, Pointer::Pressed);
        waver.pointer(pet, Pointer::Released);
        (glances, strolls, held, waver.sprite(pet).is_some())
    }

    #[test]
    fn a_standard_version_2_sheet_gets_everything_with_no_extra_field() {
        let dir = standard_pack("v2", (1536, 2288), &[6, 8, 8, 4, 5, 8, 6, 6, 6, 8, 8]);
        let pet = PetPack::new(&dir).load().unwrap();
        assert_eq!(pet.moves().looks.iter().map(Clip::len).collect::<Vec<_>>(), [8, 8]);
        assert_eq!(behaviours(&pet), (true, true, Some(4), true));
        assert_eq!(pet.bounds(), (100, 150));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_standard_version_1_sheet_has_no_looks_and_does_all_the_rest() {
        let dir = standard_pack("v1", (1536, 1872), &[6, 8, 8, 4, 5, 8, 6, 6, 6]);
        let pet = PetPack::new(&dir).load().unwrap();
        assert!(pet.moves().looks.is_empty());
        assert_eq!(behaviours(&pet), (false, true, Some(4), true));
        let mut life = Life::new(Box::new(|| 0));
        life.advance(&pet, Duration::ZERO, true);
        assert!(!life.pointer(&pet, Pointer::Entered), "no head turn to show");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sheets_with_short_rows_get_what_their_rows_allow() {
        let short = |name, row_5: u32, looks: u32| {
            let dir = standard_pack(name, (1536, 2288), &[1, 2, 2, 1, 1, row_5, 1, 1, 1, looks, 1]);
            let pet = PetPack::new(&dir).load().unwrap();
            std::fs::remove_dir_all(&dir).unwrap();
            pet
        };
        assert_eq!(behaviours(&short("one", 1, 1)), (true, true, Some(0), true));
        assert_eq!(behaviours(&short("three", 3, 2)), (true, true, Some(1), true));
        assert_eq!(behaviours(&short("five", 5, 3)), (true, true, Some(2), true));
    }

    #[test]
    fn a_sheet_with_only_some_rows_skips_what_it_lacks() {
        let dir = standard_pack("idle", (1536, 208), &[3]);
        let pet = PetPack::new(&dir).load().unwrap();
        assert_eq!(behaviours(&pet), (false, false, None, false));
        std::fs::remove_dir_all(&dir).unwrap();
        let two_rows = PetPack::new(standard_pack("run", (1536, 416), &[3, 3])).load().unwrap();
        assert!(two_rows.moves().running.is_none(), "turning around takes both directions");
    }

    #[test]
    fn the_running_and_look_rows_do_not_disturb_the_moods() {
        let dir = standard_pack("moods", (1536, 2288), &[6, 8, 8, 4, 5, 8, 6, 7, 3, 8, 8]);
        let pet = PetPack::new(&dir).load().unwrap();
        assert_eq!(pet.clip(Mood::Idle).len(), 6);
        assert_eq!(pet.clip(Mood::Done).len(), 4);
        assert_eq!(pet.clip(Mood::Celebrating).len(), 5);
        assert_eq!(pet.clip(Mood::Failed).len(), 8);
        assert_eq!(pet.clip(Mood::Waiting).len(), 6);
        assert_eq!(pet.clip(Mood::Working).len(), 7);
        assert_eq!(pet.clip(Mood::Reviewing).len(), 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
