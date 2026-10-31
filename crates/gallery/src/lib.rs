//! The pets the user keeps: one folder per pet inside the gallery, plus which one is the
//! default. A pet is checked by loading it before it gets here; this crate only files it.
use serde::Deserialize;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_FILE: &str = "default";

/// Where familiard keeps its data: `FAMILIARD_HOME`, or the usual place on each system.
pub fn home() -> PathBuf {
    if let Some(home) = std::env::var_os("FAMILIARD_HOME") {
        return PathBuf::from(home);
    }
    let variable = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let base = if cfg!(windows) {
        variable("APPDATA")
    } else if cfg!(target_os = "macos") {
        variable("HOME").map(|home| home.join("Library/Application Support"))
    } else {
        variable("XDG_DATA_HOME").or_else(|| variable("HOME").map(|home| home.join(".local/share")))
    };
    base.unwrap_or_else(std::env::temp_dir).join("familiard")
}

#[derive(Debug, PartialEq, Eq)]
pub enum GalleryError {
    InvalidId(String),
    AlreadyThere(String),
    NotThere(String),
    Empty,
    Io(String),
}

impl fmt::Display for GalleryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GalleryError::InvalidId(id) => {
                write!(f, "invalid pet id '{id}': use letters, digits, '-' and '_'")
            }
            GalleryError::AlreadyThere(id) => write!(f, "'{id}' is already in the gallery; remove it first"),
            GalleryError::NotThere(id) => write!(f, "no pet '{id}' in the gallery; see: familiard pets"),
            GalleryError::Empty => write!(f, "the gallery is empty; add a pet with: familiard add <folder>"),
            GalleryError::Io(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for GalleryError {}

fn io(context: &Path) -> impl Fn(std::io::Error) -> GalleryError + '_ {
    move |error| GalleryError::Io(format!("{}: {error}", context.display()))
}

/// Lowercase letters, digits, `-` and `_`: an id becomes a file name, so nothing else.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with(['-', '.'])
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    id: Option<String>,
    display_name: String,
    spritesheet_path: PathBuf,
}

fn manifest_of(folder: &Path) -> Result<Manifest, GalleryError> {
    let path = folder.join("pet.json");
    let text = fs::read_to_string(&path).map_err(io(&path))?;
    serde_json::from_str(&text).map_err(|e| GalleryError::Io(format!("{}: {e}", path.display())))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

pub struct Gallery {
    root: PathBuf,
}

impl Gallery {
    pub fn new(home: &Path) -> Self {
        Self { root: home.join("pets") }
    }

    /// Copies only the manifest and the spritesheet of a pet. The first pet becomes the
    /// default. `id` overrides the one in `pet.json`, which is otherwise used.
    pub fn add(&self, folder: &Path, id: Option<&str>) -> Result<Entry, GalleryError> {
        let manifest = manifest_of(folder)?;
        let id = id.map(String::from).or(manifest.id).unwrap_or_default().to_lowercase();
        if !valid_id(&id) {
            return Err(GalleryError::InvalidId(id));
        }
        let target = self.root.join(&id);
        if target.exists() {
            return Err(GalleryError::AlreadyThere(id));
        }
        fs::create_dir_all(&target).map_err(io(&target))?;
        let copied = fs::copy(folder.join("pet.json"), target.join("pet.json")).and_then(|_| {
            let sheet = &manifest.spritesheet_path;
            fs::copy(folder.join(sheet), target.join(sheet))
        });
        if let Err(error) = copied {
            let _ = fs::remove_dir_all(&target);
            return Err(io(folder)(error));
        }
        let is_default = self.default_id().is_none();
        if is_default {
            self.set_default(&id)?;
        }
        Ok(Entry { id, name: manifest.display_name, is_default })
    }

    pub fn list(&self) -> Vec<Entry> {
        let chosen = self.default_id();
        let Ok(folders) = fs::read_dir(&self.root) else { return Vec::new() };
        let mut entries: Vec<Entry> = folders
            .flatten()
            .filter_map(|folder| {
                let id = folder.file_name().into_string().ok()?;
                let name = manifest_of(&folder.path()).ok()?.display_name;
                let is_default = chosen.as_deref() == Some(&id);
                Some(Entry { id, name, is_default })
            })
            .collect();
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        entries
    }

    pub fn folder(&self, id: &str) -> Result<PathBuf, GalleryError> {
        let folder = self.root.join(id);
        if valid_id(id) && folder.join("pet.json").is_file() {
            Ok(folder)
        } else {
            Err(GalleryError::NotThere(id.into()))
        }
    }

    /// The default pet. If its folder is gone, any remaining pet takes over.
    pub fn default_id(&self) -> Option<String> {
        let chosen = fs::read_to_string(self.default_file()).ok()?;
        let chosen = chosen.trim();
        self.folder(chosen).ok().map(|_| chosen.to_string())
    }

    pub fn set_default(&self, id: &str) -> Result<(), GalleryError> {
        self.folder(id)?;
        fs::write(self.default_file(), id).map_err(io(&self.default_file()))
    }

    /// Removes a pet. If it was the default, the first remaining pet takes over.
    pub fn remove(&self, id: &str) -> Result<(), GalleryError> {
        let folder = self.folder(id)?;
        let was_default = self.default_id().as_deref() == Some(id);
        fs::remove_dir_all(&folder).map_err(io(&folder))?;
        if was_default {
            let _ = fs::remove_file(self.default_file());
            if let Some(next) = self.list().first() {
                self.set_default(&next.id)?;
            }
        }
        Ok(())
    }

    /// The pet to summon when none is named.
    pub fn chosen(&self) -> Result<String, GalleryError> {
        self.default_id().ok_or(GalleryError::Empty)
    }

    fn default_file(&self) -> PathBuf {
        self.root.parent().unwrap_or(&self.root).join(DEFAULT_FILE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let unique = format!("familiard-gallery-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
            let dir = std::env::temp_dir().join(unique);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn pack(&self, id: &str) -> PathBuf {
            let folder = self.0.join(format!("source-{id}"));
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join("sheet.png"), "image").unwrap();
            fs::write(folder.join("notes.sh"), "not a pet file").unwrap();
            let manifest = format!(r#"{{"id":"{id}","displayName":"Pet {id}","spritesheetPath":"sheet.png"}}"#);
            fs::write(folder.join("pet.json"), manifest).unwrap();
            folder
        }

        fn gallery(&self) -> Gallery {
            Gallery::new(&self.0.join("home"))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_first_pet_becomes_the_default_and_later_ones_do_not() {
        let scratch = Scratch::new();
        let gallery = scratch.gallery();
        assert!(gallery.add(&scratch.pack("ana"), None).unwrap().is_default);
        assert!(!gallery.add(&scratch.pack("xiao"), None).unwrap().is_default);
        assert_eq!(gallery.chosen().unwrap(), "ana");
        let ids: Vec<_> = gallery.list().into_iter().map(|entry| (entry.id, entry.is_default)).collect();
        assert_eq!(ids, [("ana".to_string(), true), ("xiao".to_string(), false)]);
    }

    #[test]
    fn copies_only_the_pet_files() {
        let scratch = Scratch::new();
        let gallery = scratch.gallery();
        gallery.add(&scratch.pack("ana"), None).unwrap();
        let folder = gallery.folder("ana").unwrap();
        assert!(folder.join("sheet.png").exists());
        assert!(!folder.join("notes.sh").exists());
    }

    #[test]
    fn an_explicit_id_wins_and_ids_are_lowercased() {
        let scratch = Scratch::new();
        let gallery = scratch.gallery();
        assert_eq!(gallery.add(&scratch.pack("ana"), Some("Mine")).unwrap().id, "mine");
    }

    #[test]
    fn refuses_ids_that_could_escape_the_gallery() {
        for id in ["../evil", "a/b", "a\\b", "", ".hidden", "-flag", "con espacios"] {
            assert!(!valid_id(id), "{id}");
        }
        let scratch = Scratch::new();
        let error = scratch.gallery().add(&scratch.pack("ana"), Some("../evil")).unwrap_err();
        assert_eq!(error, GalleryError::InvalidId("../evil".into()));
        assert_eq!(scratch.gallery().folder("../source-iria").unwrap_err(), GalleryError::NotThere("../source-iria".into()));
    }

    #[test]
    fn adding_twice_is_refused_and_a_failed_copy_leaves_nothing() {
        let scratch = Scratch::new();
        let gallery = scratch.gallery();
        let pack = scratch.pack("ana");
        gallery.add(&pack, None).unwrap();
        assert_eq!(gallery.add(&pack, None).unwrap_err(), GalleryError::AlreadyThere("ana".into()));
        let broken = scratch.pack("broken");
        fs::remove_file(broken.join("sheet.png")).unwrap();
        assert!(gallery.add(&broken, None).is_err());
        assert!(gallery.folder("broken").is_err());
    }

    #[test]
    fn removing_the_default_hands_over_and_removing_the_last_empties() {
        let scratch = Scratch::new();
        let gallery = scratch.gallery();
        gallery.add(&scratch.pack("a"), None).unwrap();
        gallery.add(&scratch.pack("b"), None).unwrap();
        gallery.remove("a").unwrap();
        assert_eq!(gallery.chosen().unwrap(), "b");
        gallery.remove("b").unwrap();
        assert_eq!(gallery.chosen().unwrap_err(), GalleryError::Empty);
        assert_eq!(gallery.remove("b").unwrap_err(), GalleryError::NotThere("b".into()));
    }

    #[test]
    fn the_default_can_be_changed_only_to_a_pet_that_exists() {
        let scratch = Scratch::new();
        let gallery = scratch.gallery();
        gallery.add(&scratch.pack("a"), None).unwrap();
        gallery.add(&scratch.pack("b"), None).unwrap();
        gallery.set_default("b").unwrap();
        assert_eq!(gallery.chosen().unwrap(), "b");
        assert!(gallery.set_default("nope").is_err());
        assert_eq!(gallery.chosen().unwrap(), "b");
    }

    #[test]
    fn an_empty_gallery_lists_nothing() {
        assert!(Scratch::new().gallery().list().is_empty());
    }
}
