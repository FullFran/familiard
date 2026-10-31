//! `add petdex:<slug>` and `search`, through the command line, with a fake `curl` so that no test
//! touches the network. It logs each URL, so a test can prove what was never requested.
#![cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const FAKE_CURL: &str = r#"#!/bin/sh
first=$1
while [ $# -gt 1 ]; do
  [ "$1" = --output ] && out=$2
  shift
done
echo "$1" >> "$ROOT/requests"
echo "$first" >> "$ROOT/first-arguments"
/bin/ls -ld "${out%/*}" >> "$ROOT/folders"
case "$1" in
  *redirected*) echo moved > "$out"; printf 302; exit 0 ;;
  https://petdex.dev/api/manifest) /bin/cp "$ROOT/manifest.json" "$out" ;;
  https://assets.petdex.dev/pets/*/petjson.json) /bin/cp "$ROOT/pet.json" "$out" ;;
  https://assets.petdex.dev/pets/*/sprite.png) /bin/cp "$DEMO/spritesheet.png" "$out" ;;
  *) echo "curl: (22) not found" >&2; exit 22 ;;
esac
printf 200
"#;

struct Root(PathBuf);

fn entry(slug: &str, name: &str, json: &str, sheet: &str) -> String {
    format!(
        r#"{{"slug":"{slug}","displayName":"{name}","submittedBy":"someone","spriteVersionNumber":1,"petJsonUrl":"{json}","spritesheetUrl":"{sheet}","zipUrl":"unused"}}"#
    )
}

impl Root {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "familiard-petdex-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let curl = root.join("bin/curl");
        std::fs::write(&curl, FAKE_CURL).unwrap();
        std::fs::set_permissions(&curl, std::fs::Permissions::from_mode(0o755)).unwrap();
        let good = "https://assets.petdex.dev/pets/elaina-1/";
        let pets = [
            entry("elaina", "Elaina", &format!("{good}petjson.json"), &format!("{good}sprite.png")),
            entry("elaina-2", "Elaina\\u001b[31m", &format!("{good}petjson.json"), &format!("{good}sprite.png")),
            entry("evil-json", "Evil", "https://evil.example/petjson.json", &format!("{good}sprite.png")),
            entry("evil-sheet", "Evil", &format!("{good}petjson.json"), "https://assets.petdex.dev.evil.example/pets/x/sprite.png"),
            entry("climber", "Climber", &format!("{good}petjson.json"), "https://assets.petdex.dev/pets/../../sprite.png"),
            entry("moved", "Moved", &format!("{good}redirected.json"), &format!("{good}sprite.png")),
            entry("community", "Community", "https://assets.petdex.dev/community/x/petjson.json", "https://assets.petdex.dev/community/x/sprite.png"),
            r#"{"slug":"incomplete"}"#.to_string(),
            entry("broken", "Broken", &format!("{good}missing.json"), &format!("{good}sprite.png")),
            r#"{"slug":"anonymous","displayName":"Anonymous","submittedBy":null,"petJsonUrl":"unused","spritesheetUrl":"unused"}"#.to_string(),
            entry("odd-format", "Odd", &format!("{good}petjson.json"), &format!("{good}sprite.exe")),
        ];
        std::fs::write(root.join("manifest.json"), format!(r#"{{"pets":[{}]}}"#, pets.join(","))).unwrap();
        // A hostile pet.json: its sheet path climbs out, and its name has an escape sequence.
        std::fs::write(
            root.join("pet.json"),
            r#"{"id":"other","displayName":"Elaina\u001b[31m","description":"kept","spritesheetPath":"../../../etc/passwd"}"#,
        )
        .unwrap();
        Self(root)
    }

    fn with_path(&self, args: &[&str], path: &Path) -> Output {
        let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/demo");
        Command::new(env!("CARGO_BIN_EXE_familiard"))
            .args(args)
            .env("FAMILIARD_HOME", self.0.join("home"))
            .env("HOME", self.0.join("user"))
            .env("TMPDIR", self.0.join("tmp"))
            .env("PATH", path)
            .env("ROOT", &self.0)
            .env("DEMO", demo)
            .output()
            .unwrap()
    }

    fn run(&self, args: &[&str]) -> Output {
        std::fs::create_dir_all(self.0.join("tmp")).unwrap();
        self.with_path(args, &self.0.join("bin"))
    }

    fn ok(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(output.status.success(), "{args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    }

    fn fails(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(!output.status.success(), "{args:?} should fail");
        String::from_utf8(output.stderr).unwrap()
    }

    fn requests(&self) -> String {
        std::fs::read_to_string(self.0.join("requests")).unwrap_or_default()
    }

    fn pets_folder(&self) -> PathBuf {
        self.0.join("home/pets")
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_pet_lands_in_the_gallery_under_its_slug_and_nowhere_else() {
    let root = Root::new();
    assert!(root.ok(&["add", "petdex:elaina"]).contains("familiard summon elaina"));
    assert!(root.ok(&["pets"]).contains("elaina  Elaina"));
    assert!(root.pets_folder().join("elaina/spritesheet.png").exists());
    assert!(!root.0.join("user/.codex").exists(), "nothing goes to the Codex folder");
    assert!(!root.0.join("user/.petdex").exists());
    assert_eq!(std::fs::read_dir(root.0.join("tmp")).unwrap().count(), 0, "no temporary folder left");
}

#[test]
fn a_hostile_manifest_field_is_replaced_and_other_fields_are_kept() {
    let root = Root::new();
    root.ok(&["add", "petdex:elaina"]);
    let saved = std::fs::read_to_string(root.pets_folder().join("elaina/pet.json")).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(saved["spritesheetPath"], "spritesheet.png");
    assert_eq!(saved["displayName"], "Elaina[31m");
    assert_eq!(saved["description"], "kept");
}

#[test]
fn as_still_chooses_the_id() {
    let root = Root::new();
    assert!(root.ok(&["add", "petdex:elaina", "--as", "witch"]).contains("familiard summon witch"));
    assert!(root.pets_folder().join("witch/pet.json").exists());
}

#[test]
fn files_outside_the_petdex_assets_are_never_requested() {
    let root = Root::new();
    for slug in ["evil-json", "evil-sheet", "climber"] {
        let error = root.fails(&["add", &format!("petdex:{slug}")]);
        assert!(error.contains("not a Petdex asset"), "{slug}: {error}");
        assert!(!root.pets_folder().join(slug).exists());
    }
    assert_eq!(root.requests().lines().filter(|url| *url != "https://petdex.dev/api/manifest").count(), 0);
}

#[test]
fn clear_errors_and_nothing_left_behind() {
    let root = Root::new();
    assert!(root.fails(&["add", "petdex:nobody"]).contains("familiard search nobody"));
    assert!(root.fails(&["add", "petdex:broken"]).contains("could not download"));
    assert!(root.fails(&["add", "petdex:odd-format"]).contains("unexpected sheet format"));
    assert!(!root.pets_folder().join("broken").exists());
    root.ok(&["add", "petdex:elaina"]);
    assert!(root.fails(&["add", "petdex:elaina"]).contains("elaina"));
}

#[test]
fn without_curl_it_says_so() {
    let root = Root::new();
    std::fs::create_dir_all(root.0.join("tmp")).unwrap();
    let empty = root.0.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let output = root.with_path(&["search", "elaina"], &empty);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("curl"));
}

#[test]
fn search_lists_matches_without_terminal_escapes() {
    let root = Root::new();
    let found = root.ok(&["search", "ELAINA"]);
    assert!(found.contains("elaina  Elaina  by someone  (v1)"), "{found}");
    assert!(found.contains("elaina-2"), "{found}");
    assert!(!found.contains('\u{1b}'), "{found}");
    assert!(found.contains("familiard add petdex:<name>"));
    assert!(root.ok(&["search", "zzz"]).contains("nothing on Petdex"));
    assert!(root.fails(&["search"]).contains("needs some text"));
}

#[test]
fn a_folder_still_works_as_before() {
    let root = Root::new();
    let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/demo");
    assert!(root.ok(&["add", demo.to_str().unwrap()]).contains("familiard summon demo"));
    assert_eq!(root.requests(), "", "a local folder touches no network");
}

#[test]
fn curl_ignores_the_users_config_and_works_in_a_private_folder() {
    let root = Root::new();
    root.ok(&["add", "petdex:elaina"]);
    let first = std::fs::read_to_string(root.0.join("first-arguments")).unwrap();
    assert!(first.lines().all(|argument| argument == "-q"), "{first}");
    let folders = std::fs::read_to_string(root.0.join("folders")).unwrap();
    assert!(folders.lines().all(|line| line.starts_with("drwx------")), "{folders}");
}

#[test]
fn a_folder_already_at_a_guessable_name_is_never_used() {
    let root = Root::new();
    root.ok(&["add", "petdex:elaina"]);
    let folders = std::fs::read_to_string(root.0.join("folders")).unwrap();
    let names: std::collections::HashSet<_> = folders.lines().map(|line| line.rsplit('/').next().unwrap()).collect();
    assert_eq!(names.len(), 2, "each download has its own unpredictable folder: {folders}");
    assert!(names.iter().all(|name| name.len() > "familiard-petdex-1234-".len() + 8), "{names:?}");
}

#[test]
fn a_redirect_is_a_failure_and_never_saved_as_the_pet() {
    let root = Root::new();
    let error = root.fails(&["add", "petdex:moved"]);
    assert!(error.contains("could not download"), "{error}");
    assert!(!root.pets_folder().join("moved").exists());
}

#[test]
fn every_path_on_the_asset_host_is_accepted() {
    let root = Root::new();
    let found = root.fails(&["add", "petdex:community"]);
    assert!(found.contains("could not download") && !found.contains("not a Petdex asset"), "{found}");
}

#[test]
fn one_bad_entry_does_not_break_the_list() {
    let root = Root::new();
    assert!(root.ok(&["search", "elaina"]).contains("elaina  Elaina"));
    assert!(root.fails(&["add", "petdex:incomplete"]).contains("no pet called"));
}

#[test]
fn a_name_is_cut_and_loses_invisible_characters() {
    let root = Root::new();
    let long = "A".repeat(500);
    let hostile = format!("{{\"id\":\"x\",\"displayName\":\"Eli\\u202eaina{long}\",\"spritesheetPath\":\"x\"}}");
    std::fs::write(root.0.join("pet.json"), hostile).unwrap();
    root.ok(&["add", "petdex:elaina"]);
    let saved = std::fs::read_to_string(root.pets_folder().join("elaina/pet.json")).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&saved).unwrap();
    let name = saved["displayName"].as_str().unwrap();
    assert!(name.starts_with("Eliaina") && name.chars().count() <= 80, "{name}");
}
