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

    fn run_in(&self, folder: &Path, args: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_familiard"));
        command.args(args).current_dir(folder).env("FAMILIARD_HOME", &self.0);
        command.output().unwrap()
    }

    /// A folder inside this home, which is removed with it.
    fn folder(&self, name: &str) -> PathBuf {
        let folder = self.0.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn summon_here(&self, folder: &Path) -> String {
        let output = self.run_in(folder, &["summon", "--here", "--stage", "terminal"]);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    }

    /// Reports from a folder, as an agent's hook would, and gives the pet time to hear it.
    fn report(&self, folder: &Path, session: &str, event: &str) {
        let args = ["hook", "--agent", "test", "--session", session, "--event", event];
        let output = self.run_in(folder, &args);
        assert!(output.status.success() && output.stdout.is_empty() && output.stderr.is_empty());
        std::thread::sleep(Duration::from_millis(600));
    }

    fn is_open(&self, id: &str) -> bool {
        self.ok(&["pets"]).lines().any(|line| line.contains(id) && line.contains("(open)"))
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
fn options_a_pet_already_out_cannot_honour_fail_instead_of_being_ignored() {
    let home = Home::new();
    home.ok(&["add", &demo()]);
    home.ok(&["summon", "--stage", "terminal"]);
    for options in [&["--here"][..], &["--session", "claude:s"], &["--name", "X"], &["--still"]] {
        let mut args = vec!["summon", "demo", "--stage", "terminal"];
        args.extend_from_slice(options);
        let error = home.fails(&args);
        assert!(error.contains("demo is already here"), "{options:?}: {error}");
        assert!(error.contains("familiard dismiss demo"), "{options:?}: {error}");
    }
    assert_eq!(home.ok(&["summon", "demo", "--stage", "terminal"]), "demo is already here\n");
    assert_eq!(home.ok(&["pets"]), "* demo  Demo  (open)\n");
    home.ok(&["dismiss"]);
    home.wait_until_closed("demo");
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

#[test]
fn a_pet_summoned_here_ignores_other_folders_and_leaves_when_its_session_ends() {
    let home = Home::new();
    home.ok(&["add", &demo()]);
    let (work, elsewhere) = (home.folder("work"), home.folder("elsewhere"));
    assert_eq!(home.summon_here(&work), "demo is here\n");
    assert_eq!(home.ok(&["pets"]), "* demo  Demo  (open)\n");

    home.report(&elsewhere, "other", "working");
    home.report(&elsewhere, "other", "ended");
    assert!(home.is_open("demo"), "another folder has no effect");

    home.report(&work, "mine", "working");
    home.report(&work, "second", "working");
    home.report(&work, "second", "ended");
    assert!(home.is_open("demo"), "another session in the same folder has no effect once claimed");

    home.report(&work, "mine", "done");
    assert!(home.is_open("demo"), "finishing a turn is not leaving");

    home.report(&work, "mine", "ended");
    home.wait_until_closed("demo");
    assert_eq!(home.ok(&["pets"]), "* demo  Demo\n");
    assert!(!home.0.join("running/demo").exists(), "the roster entry is gone");
    let log = std::fs::read_to_string(home.0.join("logs/demo.log")).unwrap();
    assert_eq!(log, "", "it left cleanly");
}

#[test]
fn a_session_in_a_subfolder_claims_the_pet_but_one_in_a_lookalike_folder_does_not() {
    let home = Home::new();
    home.ok(&["add", &demo()]);
    let (work, lookalike) = (home.folder("work"), home.folder("workshop"));
    let inside = home.folder("work/deep/down");
    home.summon_here(&work);
    home.report(&lookalike, "other", "working");
    home.report(&lookalike, "other", "ended");
    assert!(home.is_open("demo"));
    home.report(&inside, "mine", "working");
    home.report(&inside, "mine", "ended");
    home.wait_until_closed("demo");
}

#[cfg(unix)]
#[test]
fn a_pet_summoned_through_a_symlink_hears_the_real_folder() {
    let home = Home::new();
    home.ok(&["add", &demo()]);
    let real = home.folder("real");
    let link = home.0.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    home.summon_here(&link);
    home.report(&real, "mine", "working");
    home.report(&link, "mine", "ended");
    home.wait_until_closed("demo");
}

#[test]
fn dismiss_still_sends_away_a_pet_summoned_here() {
    let home = Home::new();
    home.ok(&["add", &demo()]);
    home.summon_here(&home.folder("work"));
    assert_eq!(home.ok(&["dismiss"]), "demo has left\n");
    home.wait_until_closed("demo");
}

#[test]
fn a_global_pet_stays_through_every_report_including_ended() {
    let home = Home::new();
    home.ok(&["add", &demo()]);
    let work = home.folder("work");
    assert_eq!(home.ok(&["summon", "--stage", "terminal", "--name", "All of it"]), "demo is here\n");
    home.report(&work, "any", "working");
    home.report(&work, "any", "ended");
    assert!(home.is_open("demo"));
    assert_eq!(home.ok(&["pets"]), "* demo  Demo  (open)\n");
}

/// The first frame in the foreground, split into its text lines.
fn first_frame(home: &Home, folder: &Path, extra: &[&str]) -> Vec<String> {
    let demo = demo();
    let mut args = vec!["run", "--pet", demo.as_str(), "--stage", "terminal", "--rows", "2", "--frames", "1"];
    args.extend_from_slice(extra);
    let output = home.run_in(folder, &args);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().lines().map(String::from).collect()
}

#[test]
fn the_terminal_prints_the_caption_on_its_own_line_only_when_there_is_a_label() {
    let home = Home::new();
    let work = home.folder("my-work");
    let plain = first_frame(&home, &work, &[]);
    assert_eq!(plain.len(), 2, "without a label, only the art");

    let named = first_frame(&home, &work, &["--name", "Bug del login"]);
    assert_eq!(named[0], "\x1b[2KBug del login");
    assert_eq!(named[1..], plain[..]);

    let folder = work.to_str().unwrap();
    let followed = first_frame(&home, &work, &["--follow", folder]);
    assert_eq!(followed[0], "\x1b[2Kmy-work", "the last component, never the whole path");

    let both = first_frame(&home, &work, &["--follow", folder, "--name", "Mine"]);
    assert_eq!(both[0], "\x1b[2KMine");
}

#[test]
fn a_running_pet_shows_its_state_in_the_caption_and_exits_by_itself() {
    use std::process::Stdio;
    let home = Home::new();
    let work = home.folder("work");
    let demo = demo();
    let follow = std::fs::canonicalize(&work).unwrap();
    let mut pet = Command::new(env!("CARGO_BIN_EXE_familiard"))
        .args(["run", "--id", "fg", "--stage", "terminal", "--rows", "1", "--pet", &demo, "--follow"])
        .arg(&follow)
        .env("FAMILIARD_HOME", &home.0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while !home.0.join("running/fg").exists() {
        assert!(started.elapsed() < Duration::from_secs(5), "the pet never listened");
        std::thread::sleep(Duration::from_millis(20));
    }
    home.report(&work, "mine", "working");
    home.report(&work, "mine", "waiting");
    home.report(&work, "mine", "done");
    home.report(&work, "mine", "ended");
    let started = Instant::now();
    let status = loop {
        if let Some(status) = pet.try_wait().unwrap() {
            break status;
        }
        assert!(started.elapsed() < Duration::from_secs(5), "the pet did not leave");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(0));
    let output = pet.wait_with_output().unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    let text = String::from_utf8_lossy(&output.stdout);
    for caption in ["work\n", "work \u{b7} working\n", "work \u{b7} needs you\n", "work \u{b7} done\n"] {
        assert!(text.contains(caption), "{caption:?} in {text:?}");
    }
    assert!(!home.0.join("running/fg").exists());
}

#[test]
fn summon_rejects_options_it_does_not_know_and_a_name_without_a_value() {
    let home = Home::new();
    home.ok(&["add", &demo()]);
    assert!(home.fails(&["summon", "--follow", "/x"]).contains("unknown option --follow"));
    assert!(home.fails(&["summon", "--name"]).contains("--name needs a value"));
    assert_eq!(home.ok(&["pets"]), "* demo  Demo\n");
}

#[test]
fn still_is_accepted_by_summon_and_run_and_explained_in_the_help() {
    let home = Home::new();
    let demo = concat!(env!("CARGO_MANIFEST_DIR"), "/../../pets/demo");
    home.ok(&["add", demo]);
    assert!(home.ok(&["summon", "demo", "--still", "--stage", "terminal"]).contains("demo is here"));
    assert!(home.is_open("demo"));
    home.ok(&["dismiss", "demo"]);
    home.ok(&["run", "--pet", demo, "--still", "--stage", "terminal", "--rows", "4", "--frames", "1"]);
    assert!(home.ok(&["--help"]).contains("--still: never walk around on its own"));
}

/// Brings a pet out and sends it away, which is what teaches familiard that it gets used.
fn use_pet(home: &Home, id: &str) {
    assert_eq!(home.ok(&["summon", id, "--stage", "terminal"]), format!("{id} is here\n"));
    home.ok(&["dismiss", id]);
    home.wait_until_closed(id);
}

fn three_elainas(home: &Home) {
    for id in ["elaina-2", "elaina-5", "kurisu"] {
        home.ok(&["add", &demo(), "--as", id]);
    }
}

#[test]
fn part_of_a_name_summons_the_pet_it_can_only_be() {
    let home = Home::new();
    three_elainas(&home);
    assert_eq!(home.ok(&["summon", "kur", "--stage", "terminal"]), "kurisu is here\n");
    assert_eq!(home.ok(&["dismiss", "kur"]), "kurisu has left\n");
}

#[test]
fn a_name_that_fits_several_new_pets_says_which_instead_of_guessing() {
    let home = Home::new();
    three_elainas(&home);
    let error = home.fails(&["summon", "ela", "--stage", "terminal"]);
    assert!(error.contains("'ela' could be elaina-2 or elaina-5"), "{error}");
    assert!(error.contains("familiard pick"), "{error}");
    assert!(!home.is_open("elaina-2") && !home.is_open("elaina-5"));
}

#[test]
fn among_several_the_one_used_most_wins() {
    let home = Home::new();
    three_elainas(&home);
    use_pet(&home, "elaina-5");
    assert_eq!(home.ok(&["summon", "ela", "--stage", "terminal"]), "elaina-5 is here\n");
    home.ok(&["dismiss", "ela"]);
    home.wait_until_closed("elaina-5");
    for _ in 0..3 {
        use_pet(&home, "elaina-2");
    }
    assert_eq!(home.ok(&["summon", "ela", "--stage", "terminal"]), "elaina-2 is here\n");
}

#[test]
fn an_exact_id_always_wins_over_what_gets_used_most() {
    let home = Home::new();
    home.ok(&["add", &demo(), "--as", "eli"]);
    home.ok(&["add", &demo(), "--as", "elina"]);
    use_pet(&home, "elina");
    use_pet(&home, "elina");
    assert_eq!(home.ok(&["summon", "eli", "--stage", "terminal"]), "eli is here\n");
}

#[test]
fn what_matches_nothing_fails_as_it_always_did() {
    let home = Home::new();
    three_elainas(&home);
    let error = home.fails(&["summon", "nadie", "--stage", "terminal"]);
    assert!(error.contains("no pet 'nadie' in the gallery"), "{error}");
    assert_eq!(home.ok(&["dismiss", "nadie"]), "nadie was not here\n");
    assert!(home.fails(&["summon", "", "--stage", "terminal"]).contains("no pet ''"));
}

#[test]
fn dismissing_by_part_of_a_name_only_looks_at_who_is_out() {
    let home = Home::new();
    three_elainas(&home);
    assert_eq!(home.ok(&["dismiss", "ela"]), "ela was not here\n", "nobody is out, nothing to guess");
    home.ok(&["summon", "elaina-5", "--stage", "terminal"]);
    assert_eq!(home.ok(&["dismiss", "ela"]), "elaina-5 has left\n", "only one of them is out");
    home.wait_until_closed("elaina-5");
    assert!(home.ok(&["pets"]).contains("elaina-2"), "and nothing else happened to the gallery");
}

#[test]
fn removing_needs_the_whole_id_because_it_cannot_be_undone() {
    let home = Home::new();
    three_elainas(&home);
    assert!(home.fails(&["remove", "kur"]).contains("no pet 'kur'"));
    home.ok(&["alias", "kurisu", "kuri"]);
    assert!(home.fails(&["remove", "kuri"]).contains("no pet 'kuri'"));
    assert!(home.ok(&["pets"]).contains("kurisu"));
}

#[test]
fn an_alias_works_wherever_a_pet_is_named() {
    let home = Home::new();
    three_elainas(&home);
    assert_eq!(home.ok(&["alias", "elaina-5", "eli"]), "elaina-5 now also answers to eli\n");
    home.ok(&["alias", "eli", "fav"]);
    assert_eq!(home.ok(&["alias"]), "eli  elaina-5\nfav  elaina-5\n");
    assert!(home.ok(&["pets"]).contains("elaina-5  Demo  [eli, fav]\n"), "{}", home.ok(&["pets"]));

    assert_eq!(home.ok(&["summon", "eli", "--stage", "terminal"]), "elaina-5 is here\n");
    assert_eq!(home.ok(&["dismiss", "fav"]), "elaina-5 has left\n");
    home.wait_until_closed("elaina-5");
    assert_eq!(home.ok(&["default", "eli"]), "elaina-5 is now the default pet\n");

    assert_eq!(home.ok(&["alias", "--remove", "eli"]), "removed the alias eli\n");
    assert_eq!(home.ok(&["alias"]), "fav  elaina-5\n");
    assert!(home.fails(&["alias", "--remove", "eli"]).contains("no alias 'eli'"));
}

#[test]
fn an_alias_cannot_hide_another_pet_or_be_a_bad_name() {
    let home = Home::new();
    three_elainas(&home);
    home.ok(&["alias", "elaina-5", "eli"]);
    assert_eq!(home.ok(&["alias", "elaina-5", "eli"]), "elaina-5 now also answers to eli\n");
    assert!(home.fails(&["alias", "kurisu", "eli"]).contains("'eli' already means elaina-5"));
    assert!(home.fails(&["alias", "kurisu", "elaina-2"]).contains("'elaina-2' is already a pet"));
    assert!(home.fails(&["alias", "kurisu", "../mal"]).contains("invalid"));
    assert!(home.fails(&["alias", "nadie", "x"]).contains("no pet 'nadie'"));
    assert!(home.fails(&["alias", "kurisu"]).contains("alias needs"));
}

#[test]
fn removing_a_pet_forgets_its_aliases_and_its_uses() {
    let home = Home::new();
    three_elainas(&home);
    home.ok(&["alias", "elaina-5", "eli"]);
    use_pet(&home, "elaina-5");
    home.ok(&["remove", "elaina-5"]);
    assert_eq!(home.ok(&["alias"]), "");
    home.ok(&["add", &demo(), "--as", "elaina-5"]);
    assert!(home.fails(&["summon", "ela", "--stage", "terminal"]).contains("could be"), "starts with no history");
}

#[test]
fn damaged_memory_only_loses_the_memory() {
    let home = Home::new();
    three_elainas(&home);
    std::fs::write(home.0.join("uses.json"), "{ not json").unwrap();
    std::fs::write(home.0.join("aliases.json"), "[1, 2").unwrap();
    assert_eq!(home.ok(&["summon", "kur", "--stage", "terminal"]), "kurisu is here\n");
    assert_eq!(home.ok(&["alias"]), "");
}

/// `pick` with no fzf to be found, answering its numbered list.
fn pick(home: &Home, answer: &str, extra: &[&str]) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_familiard"))
        .arg("pick")
        .args(extra)
        .env("FAMILIARD_HOME", &home.0)
        .env("PATH", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(answer.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn pick_lists_the_most_used_first_and_summons_the_choice() {
    let home = Home::new();
    three_elainas(&home);
    home.ok(&["alias", "kurisu", "kuri"]);
    use_pet(&home, "kurisu");
    let output = pick(&home, "1\n", &["--stage", "terminal"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let shown = String::from_utf8(output.stdout).unwrap();
    assert!(shown.starts_with("1) kurisu  Demo  [kuri]\n2) elaina-2  Demo\n3) elaina-5  Demo\n"), "{shown}");
    assert!(shown.ends_with("kurisu is here\n"), "{shown}");
}

#[test]
fn pick_can_be_left_without_choosing() {
    let home = Home::new();
    three_elainas(&home);
    for answer in ["\n", "", "9\n", "nada\n"] {
        let output = pick(&home, answer, &["--stage", "terminal"]);
        assert!(output.status.success(), "{answer:?}");
        assert!(String::from_utf8(output.stdout).unwrap().ends_with("nothing chosen\n"), "{answer:?}");
    }
    assert!(!home.is_open("elaina-2") && !home.is_open("kurisu"));
}

#[test]
fn pick_on_an_empty_gallery_says_so() {
    let home = Home::new();
    let output = pick(&home, "", &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stderr).unwrap().contains("gallery is empty"));
}
