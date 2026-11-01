//! What a person calls their pets: aliases, and the memory of which ones they use, so that part
//! of a name finds the pet they most likely mean. It stays on this machine and out of the packs.
//!
//! Ranked like zoxide ranks folders: how often, weighted by how recently.
use super::{io, valid_id, Entry, Gallery, GalleryError};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const ALIASES_FILE: &str = "aliases.json";
const USES_FILE: &str = "uses.json";
const HOUR: u64 = 3600;
const DAY: u64 = 24 * HOUR;
const WEEK: u64 = 7 * DAY;

#[derive(Serialize, Deserialize, Clone, Copy, Default)]
struct Use {
    count: u64,
    last: u64,
}

impl Use {
    /// Whole numbers, so that equal scores are really equal and a tie can be told apart.
    fn frecency(self, now: u64) -> u64 {
        let weight = match now.saturating_sub(self.last) {
            age if age < HOUR => 16,
            age if age < DAY => 8,
            age if age < WEEK => 2,
            _ => 1,
        };
        self.count.saturating_mul(weight)
    }
}

/// How likely a pet is meant: its score, and for equal scores whichever was used last.
fn rank(uses: &BTreeMap<String, Use>, id: &str, now: u64) -> (u64, u64) {
    let used = uses.get(id).copied().unwrap_or_default();
    (used.frecency(now), used.last)
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

/// A damaged file only costs the memory in it: it must never stop a pet from coming out.
fn read_map<V: DeserializeOwned>(path: &Path) -> BTreeMap<String, V> {
    fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

/// Whole or not at all: a reader never finds it half written.
fn write_map<V: Serialize>(path: &Path, map: &BTreeMap<String, V>) -> Result<(), GalleryError> {
    let text = serde_json::to_string_pretty(map).map_err(|e| GalleryError::Io(e.to_string()))?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::create_dir_all(path.parent().unwrap_or(path)).map_err(io(path))?;
    fs::write(&temporary, text).map_err(io(&temporary))?;
    fs::rename(&temporary, path).map_err(io(path))
}

impl Gallery {
    fn beside(&self, file: &str) -> PathBuf {
        self.root.parent().unwrap_or(&self.root).join(file)
    }

    /// Every alias and the pet it means, leaving out pets that are gone.
    pub fn aliases(&self) -> BTreeMap<String, String> {
        let mut aliases: BTreeMap<String, String> = read_map(&self.beside(ALIASES_FILE));
        aliases.retain(|_, id| self.folder(id).is_ok());
        aliases
    }

    pub fn aliases_of(&self, id: &str) -> Vec<String> {
        self.aliases().into_iter().filter(|(_, target)| target == id).map(|(alias, _)| alias).collect()
    }

    /// `pet` may be an id or another alias. Returns the id of the pet.
    pub fn add_alias(&self, pet: &str, alias: &str) -> Result<String, GalleryError> {
        let id = self.exact(pet)?;
        if !valid_id(alias) {
            return Err(GalleryError::InvalidId(alias.into()));
        }
        if self.folder(alias).is_ok() {
            return Err(GalleryError::AliasIsAPet(alias.into()));
        }
        let mut aliases = self.aliases();
        match aliases.get(alias) {
            Some(target) if *target != id => {
                return Err(GalleryError::AliasTaken(alias.into(), target.clone()));
            }
            _ => {}
        }
        aliases.insert(alias.into(), id.clone());
        write_map(&self.beside(ALIASES_FILE), &aliases)?;
        Ok(id)
    }

    pub fn remove_alias(&self, alias: &str) -> Result<(), GalleryError> {
        let mut aliases = self.aliases();
        if aliases.remove(alias).is_none() {
            return Err(GalleryError::NoAlias(alias.into()));
        }
        write_map(&self.beside(ALIASES_FILE), &aliases)
    }

    /// A pet by its whole id or an alias, for what should not be guessed.
    pub fn exact(&self, name: &str) -> Result<String, GalleryError> {
        if self.folder(name).is_ok() {
            return Ok(name.into());
        }
        self.aliases().remove(name).ok_or_else(|| GalleryError::NotThere(name.into()))
    }

    /// A pet by its id or alias, or else by part of its id, an alias or its name.
    pub fn find(&self, name: &str) -> Result<String, GalleryError> {
        if let Ok(id) = self.exact(name) {
            return Ok(id);
        }
        let ids: Vec<String> = self.list().into_iter().map(|entry| entry.id).collect();
        self.best_of(name, &ids)?.ok_or_else(|| GalleryError::NotThere(name.into()))
    }

    /// The most used of these pets that part of a name fits, if any. Several just as likely are
    /// an error, because guessing would bring out a pet nobody asked for.
    pub fn best_of(&self, name: &str, ids: &[String]) -> Result<Option<String>, GalleryError> {
        if name.is_empty() {
            return Ok(None);
        }
        let name = name.to_lowercase();
        let entries = self.list();
        let aliases = self.aliases();
        let fits = |id: &String| {
            id.contains(&name)
                || aliases.iter().any(|(alias, target)| target == id && alias.contains(&name))
                || entries.iter().any(|entry| entry.id == *id && entry.name.to_lowercase().contains(&name))
        };
        let uses: BTreeMap<String, Use> = read_map(&self.beside(USES_FILE));
        let score = |id: &String| rank(&uses, id, now());
        let fitting: Vec<&String> = ids.iter().filter(|id| fits(id)).collect();
        let Some(top) = fitting.iter().map(|id| score(id)).max() else { return Ok(None) };
        let mut tied: Vec<String> = fitting.into_iter().filter(|id| score(id) == top).cloned().collect();
        if tied.len() > 1 {
            tied.sort();
            return Err(GalleryError::Ambiguous(name, tied));
        }
        Ok(tied.pop())
    }

    /// The gallery with the pets used most first.
    pub fn by_use(&self) -> Vec<Entry> {
        let uses: BTreeMap<String, Use> = read_map(&self.beside(USES_FILE));
        let mut entries = self.list();
        entries.sort_by_key(|entry| Reverse(rank(&uses, &entry.id, now())));
        entries
    }

    /// Best effort: losing count of a use must not get in the way of the pet that was used.
    pub fn record_use(&self, id: &str) {
        let path = self.beside(USES_FILE);
        let mut uses: BTreeMap<String, Use> = read_map(&path);
        let entry = uses.entry(id.into()).or_default();
        entry.count = entry.count.saturating_add(1);
        entry.last = now();
        let _ = write_map(&path, &uses);
    }

    /// What is remembered about a pet that has left the gallery.
    pub(crate) fn forget(&self, id: &str) {
        let path = self.beside(USES_FILE);
        let mut uses: BTreeMap<String, Use> = read_map(&path);
        if uses.remove(id).is_some() {
            let _ = write_map(&path, &uses);
        }
        let path = self.beside(ALIASES_FILE);
        let mut aliases: BTreeMap<String, String> = read_map(&path);
        let before = aliases.len();
        aliases.retain(|_, target| target != id);
        if aliases.len() != before {
            let _ = write_map(&path, &aliases);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_use_counts_for_more_than_old_use() {
        let now = 100 * WEEK;
        let used = |count, age| Use { count, last: now - age }.frecency(now);
        assert_eq!(used(1, 0), 16);
        assert_eq!(used(1, HOUR), 8);
        assert_eq!(used(1, DAY), 2);
        assert_eq!(used(1, WEEK), 1);
        assert!(used(1, 0) > used(10, WEEK), "yesterday's habit loses to what is in use now");
        assert!(used(20, WEEK) > used(1, 0), "but a real habit survives a one-off");
        assert_eq!(Use::default().frecency(now), 0);
    }

    #[test]
    fn a_clock_going_backwards_does_not_panic() {
        assert_eq!(Use { count: 1, last: 50 }.frecency(10), 16);
    }
}
