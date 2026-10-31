//! Pets from Petdex (https://petdex.dev), its public manifest and nothing else. The pet lands in
//! the gallery only: Petdex's own installer also copies it to `~/.codex/pets`, and has telemetry.
//!
//! Downloads go through `curl`, present on Linux, macOS and Windows 10 and later, so that one
//! optional command does not add a TLS stack to a binary that otherwise never touches the network.
use familiard_application::Failure;
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MANIFEST: &str = "https://petdex.dev/api/manifest";
/// Every file must come from here, whatever the manifest says.
const ASSETS: &str = "https://assets.petdex.dev/pets/";
const MAX_DOWNLOAD: &str = "67108864";
const MAX_RESULTS: usize = 30;

#[derive(Deserialize)]
struct Manifest {
    pets: Vec<Listing>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Listing {
    slug: String,
    display_name: String,
    submitted_by: Option<String>,
    sprite_version_number: Option<u32>,
    pet_json_url: String,
    spritesheet_url: String,
}

/// A downloaded pet in a temporary folder, removed when this is dropped.
pub struct Download(PathBuf);

impl Download {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Download {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scratch() -> Result<Download, Failure> {
    let folder = std::env::temp_dir().join(format!("familiard-petdex-{}", std::process::id()));
    fs::create_dir_all(&folder)?;
    Ok(Download(folder))
}

/// Only the list may redirect, because it does today. A pet's files must come straight from
/// `ASSETS`, or the check on their address would mean nothing.
fn fetch(url: &str, to: &Path, redirects: bool) -> Result<(), Failure> {
    let mut command = Command::new("curl");
    if redirects {
        command.args(["--location", "--proto-redir", "=https"]);
    }
    let result = command
        .args(["--proto", "=https", "--tlsv1.2", "--fail", "--silent", "--show-error"])
        .args(["--max-time", "180", "--max-filesize", MAX_DOWNLOAD])
        .args(["--user-agent", concat!("familiard/", env!("CARGO_PKG_VERSION"))])
        .arg("--output")
        .arg(to)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .output();
    match result {
        Err(error) if error.kind() == ErrorKind::NotFound => {
            Err("Petdex downloads need curl, which was not found".into())
        }
        Err(error) => Err(error.into()),
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            Err(format!("could not download {url}: {}", String::from_utf8_lossy(&output.stderr).trim()).into())
        }
    }
}

fn manifest() -> Result<Manifest, Failure> {
    let folder = scratch()?;
    let path = folder.path().join("manifest.json");
    fetch(MANIFEST, &path, true)?;
    serde_json::from_slice(&fs::read(&path)?).map_err(|error| format!("Petdex's list is not readable: {error}").into())
}

/// Text from strangers, on its way to a terminal.
fn printable(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

fn asset(url: &str) -> Result<&str, Failure> {
    let path = url.strip_prefix(ASSETS).filter(|path| {
        !path.is_empty() && path.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c)) && !path.contains("..")
    });
    path.ok_or_else(|| format!("refusing to download from {}: not a Petdex asset", printable(url)).into())
}

fn sheet_name(url: &str) -> Result<&'static str, Failure> {
    match url.rsplit('.').next() {
        Some("webp") => Ok("spritesheet.webp"),
        Some("png") => Ok("spritesheet.png"),
        _ => Err(format!("unexpected sheet format in {}", printable(url)).into()),
    }
}

/// Downloads one pet by its slug into a temporary folder holding a standard pack.
pub fn download(slug: &str) -> Result<Download, Failure> {
    let manifest = manifest()?;
    let listing = manifest
        .pets
        .iter()
        .find(|pet| pet.slug == slug)
        .ok_or_else(|| format!("Petdex has no pet called {slug}. Try: familiard search {slug}"))?;
    asset(&listing.pet_json_url)?;
    asset(&listing.spritesheet_url)?;
    let sheet = sheet_name(&listing.spritesheet_url)?;

    let folder = scratch()?;
    let manifest_path = folder.path().join("pet.json");
    fetch(&listing.pet_json_url, &manifest_path, false)?;
    fetch(&listing.spritesheet_url, &folder.path().join(sheet), false)?;

    // The sheet's path and name come from a stranger's file, so neither is taken as it arrives.
    let mut pet: Value = serde_json::from_slice(&fs::read(&manifest_path)?)
        .map_err(|error| format!("{slug}'s pet.json is not readable: {error}"))?;
    let fields = pet.as_object_mut().ok_or_else(|| format!("{slug}'s pet.json is not an object"))?;
    fields.insert("spritesheetPath".into(), sheet.into());
    let name = fields.get("displayName").and_then(Value::as_str).map(printable);
    fields.insert("displayName".into(), name.unwrap_or_else(|| printable(&listing.display_name)).into());
    fs::write(&manifest_path, serde_json::to_vec_pretty(&pet)?)?;
    Ok(folder)
}

/// `familiard search <text>`: pets whose slug or name contains the text.
pub fn search(args: Vec<String>) -> Result<(), Failure> {
    let text = args.join(" ").to_lowercase();
    if text.trim().is_empty() {
        return Err("search needs some text, like: familiard search elaina".into());
    }
    let manifest = manifest()?;
    let found: Vec<_> = manifest
        .pets
        .iter()
        .filter(|pet| pet.slug.contains(&text) || pet.display_name.to_lowercase().contains(&text))
        .collect();
    for pet in found.iter().take(MAX_RESULTS) {
        println!(
            "{}  {}  by {}  (v{})",
            printable(&pet.slug),
            printable(&pet.display_name),
            printable(pet.submitted_by.as_deref().unwrap_or("unknown")),
            pet.sprite_version_number.unwrap_or(1)
        );
    }
    if found.is_empty() {
        println!("nothing on Petdex matches {}", printable(&text));
    } else if found.len() > MAX_RESULTS {
        println!("... and {} more", found.len() - MAX_RESULTS);
    }
    if !found.is_empty() {
        println!("to install one: familiard add petdex:<name>");
    }
    Ok(())
}
