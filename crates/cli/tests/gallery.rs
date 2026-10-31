//! The gallery as a person or an agent uses it: through the command line. Pets use the
//! terminal stage, so this runs on any system without a display.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

struct Home(PathBuf);

impl Home {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let unique = format!("familiard-cli-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let home = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&home).unwrap();
        Self(home)
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_familiard")).args(args).env("FAMILIARD_HOME", &self.0).output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(output.status.success(), "{args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    }

    fn fails(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        String::from_utf8(output.stderr).unwrap()
    }

    fn wait_until_closed(&self, id: &str) {
        let started = Instant::now();
        while self.ok(&["pets"]).lines().any(|line| line.contains(id) && line.contains("(open)")) {
            assert!(started.elapsed() < Duration::from_secs(5), "{id} did not leave");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        self.run(&["dismiss", "--all"]);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn demo() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/demo");
    path.to_str().unwrap().to_string()
}

#[test]
fn a_pet_goes_from_a_folder_to_being_summoned_and_dismissed() {
    let home = Home::new();
    assert!(home.ok(&["pets"]).contains("empty"));
    assert!(home.fails(&["summon"]).contains("gallery is empty"));

    assert!(home.ok(&["add", &demo()]).contains("(default)"));
    assert_eq!(home.ok(&["pets"]), "* demo  Demo\n");

    assert_eq!(home.ok(&["summon", "--stage", "terminal"]), "demo is here\n");
    assert_eq!(home.ok(&["pets"]), "* demo  Demo  (open)\n");
    assert_eq!(home.ok(&["summon", "demo", "--stage", "terminal"]), "demo is already here\n");

    assert_eq!(home.ok(&["dismiss"]), "demo has left\n");
    home.wait_until_closed("demo");
    assert_eq!(home.ok(&["dismiss", "demo"]), "demo was not here\n");
}

#[test]
fn the_short_forms_do_the_same() {
    let home = Home::new();
    home.ok(&["add", &demo()]);
    assert_eq!(home.ok(&["-s", "--stage", "terminal"]), "demo is here\n");
    assert_eq!(home.ok(&["-l"]), "* demo  Demo  (open)\n");
    assert_eq!(home.ok(&["-d", "demo"]), "demo has left\n");
    home.wait_until_closed("demo");
    assert_eq!(home.ok(&["-d", "--all"]), "no pets are open\n");
}

#[test]
fn several_pets_can_be_out_and_the_default_can_change() {
    let home = Home::new();
    home.ok(&["add", &demo()]);
    home.ok(&["add", &demo(), "--as", "second"]);
    assert_eq!(home.ok(&["pets"]), "* demo  Demo\n  second  Demo\n");

    home.ok(&["default", "second"]);
    assert_eq!(home.ok(&["summon", "--stage", "terminal"]), "second is here\n");
    home.ok(&["summon", "demo", "--stage", "terminal"]);
    assert_eq!(home.ok(&["pets"]), "  demo  Demo  (open)\n* second  Demo  (open)\n");

    let listed: serde_json::Value = serde_json::from_str(&home.ok(&["pets", "--json"])).unwrap();
    assert_eq!(listed[1]["id"], "second");
    assert_eq!(listed[1]["default"], true);
    assert_eq!(listed[1]["open"], true);

    let left = home.ok(&["dismiss", "--all"]);
    assert_eq!(left, "demo has left\nsecond has left\n");
    assert_eq!(home.ok(&["dismiss", "--all"]), "no pets are open\n");
}

#[test]
fn removing_an_open_pet_also_sends_it_away() {
    let home = Home::new();
    home.ok(&["add", &demo()]);
    home.ok(&["summon", "--stage", "terminal"]);
    assert_eq!(home.ok(&["remove", "demo"]), "removed demo\n");
    assert!(home.ok(&["pets"]).contains("empty"));
    assert_eq!(home.ok(&["dismiss", "demo"]), "demo was not here\n");
}

#[test]
fn clear_errors_for_things_that_are_not_there() {
    let home = Home::new();
    assert!(home.fails(&["summon", "nope"]).contains("no pet 'nope'"));
    assert!(home.fails(&["default", "nope"]).contains("no pet 'nope'"));
    assert!(home.fails(&["remove", "nope"]).contains("no pet 'nope'"));
    assert!(home.fails(&["add", "/does/not/exist"]).contains("pet.json"));
    assert!(home.fails(&["frobnicate"]).contains("unknown command"));
    assert!(home.fails(&["summon", "../../etc"]).contains("no pet"));
}

#[test]
fn a_stage_that_cannot_open_is_reported_by_summon() {
    let home = Home::new();
    home.ok(&["add", &demo()]);
    let error = home.fails(&["summon", "--stage", "hologram"]);
    assert!(error.contains("could not appear") && error.contains("unknown stage"), "{error}");
    assert!(home.ok(&["pets"]).ends_with("Demo\n"));
}
