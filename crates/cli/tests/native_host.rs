//! The native-messaging host and `summon --session`, through the binary, the way a browser and
//! a person use them. Pets use the terminal stage, so this runs without a display.
use familiard_agents::{LocalFeed, Roster};
use familiard_application::ActivityFeed;
use familiard_domain::{Report, Signal};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

const CONVERSATION: &str = "0a1b2c3d-1111-2222-3333-444455556666";
const OTHER: &str = "ffffffff-1111-2222-3333-444455556666";

struct Home(PathBuf);

impl Home {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let unique = format!("familiard-host-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let home = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&home).unwrap();
        Self(home)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_familiard"));
        command.env("FAMILIARD_HOME", &self.0).env("FAMILIARD_BROWSER_STAGE", "terminal");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
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

    fn add_demo(&self) {
        self.ok(&["add", &demo()]);
    }

    fn host(&self) -> Host {
        Host::spawn(self.command().arg("native-host"))
    }

    fn is_open(&self, id: &str) -> bool {
        self.ok(&["pets"]).lines().any(|line| line.contains(id) && line.contains("(open)"))
    }

    fn wait_until_closed(&self, id: &str) {
        let started = Instant::now();
        while self.is_open(id) {
            assert!(started.elapsed() < Duration::from_secs(5), "{id} did not leave");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// A hook from any agent, reported and given time to arrive.
    fn hook(&self, agent: &str, session: &str, event: &str) {
        self.ok(&["hook", "--agent", agent, "--session", session, "--event", event]);
        std::thread::sleep(Duration::from_millis(600));
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        self.run(&["dismiss", "--all"]);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn demo() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/demo").to_str().unwrap().to_string()
}

/// The host as a browser sees it: frames over a pipe.
struct Host {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: ChildStdout,
}

impl Host {
    fn spawn(command: &mut Command) -> Self {
        let mut child =
            command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        let (stdin, stdout) = (child.stdin.take(), child.stdout.take().unwrap());
        Self { child, stdin, stdout }
    }

    fn send_bytes(&mut self, body: &[u8]) {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(&(body.len() as u32).to_ne_bytes()).unwrap();
        stdin.write_all(body).unwrap();
        stdin.flush().unwrap();
    }

    fn receive(&mut self) -> Value {
        let mut length = [0u8; 4];
        self.stdout.read_exact(&mut length).expect("a length prefix");
        let mut body = vec![0u8; u32::from_ne_bytes(length) as usize];
        self.stdout.read_exact(&mut body).expect("a message body");
        serde_json::from_slice(&body).expect("a JSON body")
    }

    fn ask(&mut self, message: Value) -> Value {
        self.send_bytes(message.to_string().as_bytes());
        self.receive()
    }

    /// Closes stdin as a browser does, and returns what is left of the process.
    fn finish(mut self) -> Output {
        drop(self.stdin.take());
        let mut rest = Vec::new();
        self.stdout.read_to_end(&mut rest).unwrap();
        let mut output = self.child.wait_with_output().unwrap();
        output.stdout = rest;
        output
    }
}

fn assert_refused(reply: &Value, context: &str) {
    assert_eq!(reply["ok"], false, "{context}: {reply}");
    assert!(reply["error"].is_string(), "{context}: {reply}");
}

fn summon_message(pet: &str, conversation: &str) -> Value {
    json!({"type": "summon", "pet": pet, "conversation": conversation, "name": "Bug del login"})
}

fn signal_message(conversation: &str, signal: &str) -> Value {
    json!({"type": "signal", "conversation": conversation, "signal": signal})
}

// --- The protocol ---

#[test]
fn it_speaks_length_prefixed_json_and_exits_cleanly_on_eof() {
    let home = Home::new();
    home.add_demo();
    let mut host = home.host();
    for _ in 0..3 {
        let reply = host.ask(json!({"type": "list"}));
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(reply["pets"][0]["id"], "demo");
    }
    let output = host.finish();
    assert!(output.status.success(), "{:?}", output.status);
    assert!(output.stdout.is_empty(), "nothing but replies goes to stdout: {:?}", output.stdout);
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn a_message_over_the_limit_is_refused_without_being_read_and_the_host_exits() {
    let home = Home::new();
    let mut host = home.host();
    let stdin = host.stdin.as_mut().unwrap();
    stdin.write_all(&(1024u32 * 1024).to_ne_bytes()).unwrap();
    stdin.flush().unwrap();
    assert_refused(&host.receive(), "oversized");
    let status = host.child.wait().unwrap();
    assert_eq!(status.code(), Some(1));
}

#[test]
fn a_message_cut_short_ends_the_host_without_a_reply() {
    let home = Home::new();
    let mut host = home.host();
    let stdin = host.stdin.as_mut().unwrap();
    stdin.write_all(&100u32.to_ne_bytes()).unwrap();
    stdin.write_all(b"{\"ty").unwrap();
    let output = host.finish();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

#[test]
fn a_length_prefix_cut_short_ends_the_host() {
    let home = Home::new();
    let mut host = home.host();
    host.stdin.as_mut().unwrap().write_all(&[1, 2]).unwrap();
    assert_eq!(host.finish().status.code(), Some(1));
}

#[test]
fn every_bad_field_is_refused_and_changes_nothing() {
    let home = Home::new();
    home.add_demo();
    let mut host = home.host();
    let long_id = "a".repeat(129);
    let long_name = "n".repeat(257);
    let bad = [
        json!(null),
        json!([]),
        json!("list"),
        json!({}),
        json!({"type": 5}),
        json!({"type": "run"}),
        json!({"type": "list", "extra": 1}),
        // summon
        json!({"type": "summon"}),
        json!({"type": "summon", "pet": "demo", "name": "x"}),
        json!({"type": "summon", "conversation": CONVERSATION, "name": "x"}),
        json!({"type": "summon", "pet": 5, "conversation": CONVERSATION, "name": "x"}),
        json!({"type": "summon", "pet": "../../etc", "conversation": CONVERSATION, "name": "x"}),
        json!({"type": "summon", "pet": "--stage", "conversation": CONVERSATION, "name": "x"}),
        json!({"type": "summon", "pet": "nope", "conversation": CONVERSATION, "name": "x"}),
        json!({"type": "summon", "pet": "demo", "conversation": "", "name": "x"}),
        json!({"type": "summon", "pet": "demo", "conversation": "a/b", "name": "x"}),
        json!({"type": "summon", "pet": "demo", "conversation": "a b", "name": "x"}),
        json!({"type": "summon", "pet": "demo", "conversation": "../x", "name": "x"}),
        json!({"type": "summon", "pet": "demo", "conversation": long_id, "name": "x"}),
        json!({"type": "summon", "pet": "demo", "conversation": CONVERSATION, "name": 5}),
        json!({"type": "summon", "pet": "demo", "conversation": CONVERSATION, "name": "a\nb"}),
        json!({"type": "summon", "pet": "demo", "conversation": CONVERSATION, "name": "a\u{0}b"}),
        json!({"type": "summon", "pet": "demo", "conversation": CONVERSATION, "name": long_name}),
        json!({"type": "summon", "pet": "demo", "conversation": CONVERSATION, "name": "x", "text": "hello"}),
        // signal
        json!({"type": "signal", "conversation": CONVERSATION}),
        json!({"type": "signal", "signal": "working"}),
        json!({"type": "signal", "conversation": CONVERSATION, "signal": "exploded"}),
        json!({"type": "signal", "conversation": CONVERSATION, "signal": "Stop"}),
        json!({"type": "signal", "conversation": CONVERSATION, "signal": 1}),
        json!({"type": "signal", "conversation": "a:b", "signal": "working"}),
        json!({"type": "signal", "conversation": CONVERSATION, "signal": "working", "text": "hello"}),
        // dismiss
        json!({"type": "dismiss"}),
        json!({"type": "dismiss", "pet": "../x"}),
        json!({"type": "dismiss", "pet": 5}),
        json!({"type": "dismiss", "pet": "demo", "all": true}),
    ];
    for message in bad {
        assert_refused(&host.ask(message.clone()), &message.to_string());
    }
    host.send_bytes(b"not json");
    assert_refused(&host.receive(), "not json");
    host.send_bytes(&[0xff, 0xfe, 0x00]);
    assert_refused(&host.receive(), "not utf-8");

    assert_eq!(host.ask(json!({"type": "list"}))["pets"][0]["open"], false, "the host survives them all");
    assert!(host.finish().status.success());
    assert!(!home.is_open("demo"));
}

#[test]
fn the_browser_launches_the_binary_with_its_own_arguments() {
    let home = Home::new();
    home.add_demo();
    let chromium = "chrome-extension://epmjbipjlabgmcignobpeeggbdjidllp/";
    let gecko = ["/some/where/io.github.fullfran.familiard.json", "familiard@fullfran.github.io"];
    let windows = [chromium, "--parent-window=0"];
    for args in [&[chromium][..], &gecko[..], &windows[..]] {
        let mut host = Host::spawn(home.command().args(args));
        assert_eq!(host.ask(json!({"type": "list"}))["ok"], true, "{args:?}");
        assert!(host.finish().status.success(), "{args:?}");
    }
}

#[test]
fn other_callers_are_not_taken_for_a_browser() {
    let home = Home::new();
    for args in [
        &["chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"][..],
        &["/some/manifest.json", "someone-else@example.org"][..],
    ] {
        assert!(home.fails(args).contains("unknown command"), "{args:?}");
    }
}

// --- What it does ---

#[test]
fn a_pet_brought_to_a_conversation_hears_only_that_conversation_and_leaves_when_it_ends() {
    let home = Home::new();
    home.add_demo();
    let mut host = home.host();
    let reply = host.ask(summon_message("demo", CONVERSATION));
    assert_eq!(reply, json!({"ok": true}));
    assert!(home.is_open("demo"));

    assert_eq!(host.ask(signal_message(OTHER, "working")), json!({"ok": true}));
    assert_eq!(host.ask(signal_message(OTHER, "ended")), json!({"ok": true}));
    home.hook("claude", "any", "ended");
    assert!(home.is_open("demo"), "other sessions have no effect");

    assert_eq!(host.ask(signal_message(CONVERSATION, "working")), json!({"ok": true}));
    assert_eq!(host.ask(signal_message(CONVERSATION, "done")), json!({"ok": true}));
    std::thread::sleep(Duration::from_millis(600));
    assert!(home.is_open("demo"), "finishing a turn is not leaving");

    assert_eq!(host.ask(signal_message(CONVERSATION, "ended")), json!({"ok": true}));
    home.wait_until_closed("demo");
    assert!(host.finish().status.success());
    assert_eq!(std::fs::read_to_string(home.0.join("logs/demo.log")).unwrap(), "", "it left cleanly");
}

#[test]
fn a_signal_reaches_every_pet_as_a_chatgpt_session_with_no_folder() {
    let home = Home::new();
    home.add_demo();
    let mut host = home.host();
    let inbox = {
        let (outbox, inbox) = channel();
        Box::new(LocalFeed::new(Roster::new(home.0.join("running")), "listener")).listen(outbox).unwrap();
        inbox
    };
    let reply = host.ask(signal_message(CONVERSATION, "working"));
    let report = inbox.recv_timeout(Duration::from_secs(2));
    // The listener lives in this process and a feed exits its process on `dismiss`, so if `Home`
    // could still reach it, cleaning up would end the whole test run with a success status.
    std::fs::remove_file(home.0.join("running/listener")).unwrap();
    assert_eq!(reply, json!({"ok": true}));
    let expected = Report { session: format!("chatgpt:{CONVERSATION}"), signal: Signal::Working, folder: None, pane: None };
    assert_eq!(report.unwrap(), expected, "no folder, so --here never claims it");
    drop(host.finish());
}

#[test]
fn a_conversation_never_claims_a_pet_bound_to_a_folder() {
    let home = Home::new();
    home.add_demo();
    let work = home.0.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let output = home.command().args(["summon", "--here", "--stage", "terminal"]).current_dir(&work).output().unwrap();
    assert!(output.status.success());
    let mut host = home.host();
    host.ask(signal_message(CONVERSATION, "working"));
    host.ask(signal_message(CONVERSATION, "ended"));
    std::thread::sleep(Duration::from_millis(600));
    assert!(home.is_open("demo"), "the conversation has no folder, so it cannot be this pet's session");
    drop(host.finish());
}

#[test]
fn the_pet_outlives_the_host_and_does_not_hold_its_pipes() {
    let home = Home::new();
    home.add_demo();
    let mut host = home.host();
    host.ask(summon_message("demo", CONVERSATION));
    let started = Instant::now();
    let output = host.finish();
    assert!(started.elapsed() < Duration::from_secs(3), "the browser would be left waiting");
    assert!(output.status.success());
    assert!(home.is_open("demo"), "the pet stays after the host is gone");
}

#[test]
fn a_pet_already_out_is_refused_and_keeps_its_binding() {
    let home = Home::new();
    home.add_demo();
    let mut host = home.host();
    host.ask(summon_message("demo", CONVERSATION));
    let reply = host.ask(summon_message("demo", OTHER));
    assert_refused(&reply, "already out");
    assert_eq!(reply["already_out"], true, "{reply}");
    host.ask(signal_message(OTHER, "ended"));
    std::thread::sleep(Duration::from_millis(600));
    assert!(home.is_open("demo"), "it still belongs to the first conversation");
    drop(host.finish());
}

#[test]
fn dismiss_sends_a_pet_away_and_says_when_it_was_not_there() {
    let home = Home::new();
    home.add_demo();
    let mut host = home.host();
    host.ask(summon_message("demo", CONVERSATION));
    assert_eq!(host.ask(json!({"type": "dismiss", "pet": "demo"})), json!({"ok": true, "was_there": true}));
    home.wait_until_closed("demo");
    assert_eq!(host.ask(json!({"type": "dismiss", "pet": "demo"})), json!({"ok": true, "was_there": false}));
    drop(host.finish());
}

#[test]
fn a_pet_that_cannot_appear_is_reported_rather_than_printed() {
    let home = Home::new();
    home.add_demo();
    let mut command = home.command();
    command.env("FAMILIARD_BROWSER_STAGE", "hologram").arg("native-host");
    let mut host = Host::spawn(&mut command);
    let reply = host.ask(summon_message("demo", CONVERSATION));
    assert_refused(&reply, "unknown stage");
    assert!(reply["error"].as_str().unwrap().contains("could not appear"), "{reply}");
    let output = host.finish();
    assert!(output.stdout.is_empty());
}

// --- Other people's Codex packs stay welcome ---

/// A sheet of the standard size with a few frames in the idle row, and nothing else.
fn codex_pack(home: &Home, folder: &str, manifest: Value, size: (u32, u32)) -> PathBuf {
    let folder = home.0.join(folder);
    std::fs::create_dir_all(&folder).unwrap();
    let mut sheet = image::RgbaImage::new(size.0, size.1);
    for frame in 0..4u32 {
        for y in 10..190 {
            for x in 10..190 {
                sheet.put_pixel(frame * 256 + x, y, image::Rgba([200, 100, 50, 255]));
            }
        }
    }
    sheet.save(folder.join("spritesheet.png")).unwrap();
    std::fs::write(folder.join("pet.json"), manifest.to_string()).unwrap();
    folder
}

#[test]
fn standard_codex_packs_of_both_versions_are_listed_and_brought_with_only_the_standard_fields() {
    let home = Home::new();
    let packs = [
        ("one", json!({"id": "codex-v1", "displayName": "Versión Uno", "description": "A v1 sheet", "spritesheetPath": "spritesheet.png"}), (1536, 1872)),
        ("two", json!({"id": "codex-v2", "displayName": "Versión Dos", "description": "A v2 sheet", "spritesheetPath": "spritesheet.png"}), (1536, 2288)),
        ("three", json!({"id": "No_Description", "displayName": "Solo lo mínimo", "spritesheetPath": "spritesheet.png"}), (1536, 1872)),
    ];
    for (folder, manifest, size) in packs {
        let folder = codex_pack(&home, folder, manifest, size);
        home.ok(&["add", folder.to_str().unwrap()]);
    }
    let before = home.ok(&["pets", "--json"]);
    let mut host = home.host();
    let reply = host.ask(json!({"type": "list"}));
    assert_eq!(reply["pets"], serde_json::from_str::<Value>(&before).unwrap(), "the same listing as pets --json");
    let ids: Vec<_> = reply["pets"].as_array().unwrap().iter().map(|pet| pet["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["codex-v1", "codex-v2", "no_description"]);
    assert_eq!(reply["pets"][1]["name"], "Versión Dos");

    for id in ids {
        assert_eq!(host.ask(summon_message(id, CONVERSATION)), json!({"ok": true}), "{id}");
        assert!(home.is_open(id), "{id}");
        assert_eq!(host.ask(json!({"type": "dismiss", "pet": id}))["was_there"], true, "{id}");
        home.wait_until_closed(id);
    }
    drop(host.finish());
}

#[test]
fn adding_a_pack_changes_nothing_in_its_manifest() {
    let home = Home::new();
    let manifest = json!({"id": "untouched", "displayName": "U", "description": "d", "spritesheetPath": "spritesheet.png"});
    let folder = codex_pack(&home, "pack", manifest.clone(), (1536, 1872));
    home.ok(&["add", folder.to_str().unwrap()]);
    let mut host = home.host();
    host.ask(summon_message("untouched", CONVERSATION));
    host.ask(json!({"type": "dismiss", "pet": "untouched"}));
    drop(host.finish());
    let kept: Value = serde_json::from_str(&std::fs::read_to_string(home.0.join("pets/untouched/pet.json")).unwrap()).unwrap();
    assert_eq!(kept, manifest);
}

// --- summon --session ---

#[test]
fn summon_session_binds_to_that_exact_session_and_leaves_when_it_ends() {
    let home = Home::new();
    home.add_demo();
    let args = ["summon", "demo", "--session", "chatgpt:abc", "--stage", "terminal", "--name", "Mine"];
    assert_eq!(home.ok(&args), "demo is here\n");
    home.hook("chatgpt", "other", "working");
    home.hook("chatgpt", "other", "ended");
    home.hook("claude", "abc", "ended");
    assert!(home.is_open("demo"), "the agent is part of the session's name");
    home.hook("chatgpt", "abc", "working");
    home.hook("chatgpt", "abc", "done");
    assert!(home.is_open("demo"));
    home.hook("chatgpt", "abc", "ended");
    home.wait_until_closed("demo");
}

#[test]
fn summon_session_works_for_any_session_and_dismiss_still_works() {
    let home = Home::new();
    home.add_demo();
    home.ok(&["summon", "--session", "claude:s1", "--stage", "terminal"]);
    assert_eq!(home.ok(&["dismiss"]), "demo has left\n");
    home.wait_until_closed("demo");
}

#[test]
fn summon_session_refuses_what_it_cannot_honour() {
    let home = Home::new();
    home.add_demo();
    assert!(home.fails(&["summon", "--session"]).contains("--session needs a value"));
    assert!(home.fails(&["summon", "--session", ""]).contains("invalid session"));
    assert!(home.fails(&["summon", "--session", "a\nb"]).contains("invalid session"));
    let long = "s".repeat(1025);
    assert!(home.fails(&["summon", "--session", &long]).contains("invalid session"));
    assert!(home.fails(&["summon", "--here", "--session", "a"]).contains("--here and --session"));
    assert!(home.fails(&["run", "--pet", &demo(), "--follow", "/x", "--session", "a"]).contains("--follow and --session"));
    assert_eq!(home.ok(&["pets"]), "* demo  Demo\n", "none of them brought a pet out");
}

// --- Everything from before still holds ---

#[test]
fn plain_summon_and_here_still_behave_as_before() {
    let home = Home::new();
    home.add_demo();
    assert_eq!(home.ok(&["summon", "--stage", "terminal"]), "demo is here\n");
    assert_eq!(home.ok(&["summon", "--stage", "terminal"]), "demo is already here\n");
    home.hook("chatgpt", CONVERSATION, "working");
    home.hook("chatgpt", CONVERSATION, "ended");
    assert!(home.is_open("demo"), "a global pet hears conversations too and never leaves");
    assert_eq!(home.ok(&["dismiss"]), "demo has left\n");
    home.wait_until_closed("demo");
    assert_eq!(home.ok(&["dismiss"]), "demo was not here\n");
    let usage = home.fails(&["summon", "--follow", "/x"]);
    assert!(usage.contains("unknown option --follow"));

    let work = home.0.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let output = home.command().args(["summon", "--here", "--stage", "terminal"]).current_dir(&work).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout), "demo is here\n");
    let report = home.command().args(["hook", "--agent", "x", "--session", "s", "--event", "ended"]).current_dir(&work).output().unwrap();
    assert!(report.status.success() && report.stdout.is_empty() && report.stderr.is_empty());
    assert!(home.is_open("demo"), "an end nobody claimed is not a claim");
}

#[test]
fn a_pet_out_of_the_gallery_is_listed_and_can_be_dismissed() {
    let home = Home::new();
    home.add_demo();
    let mut stray = home
        .command()
        .args(["run", "--id", "stray", "--stage", "terminal", "--pet", &demo()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while !home.0.join("running/stray").exists() {
        assert!(started.elapsed() < Duration::from_secs(5), "the pet never appeared");
    }
    let mut host = home.host();
    let list = host.ask(json!({"type": "list"}));
    assert_eq!(list["strays"], json!(["stray"]));
    assert_eq!(list["pets"].as_array().unwrap().len(), 1, "the gallery is still only the gallery");
    assert_eq!(host.ask(json!({"type": "dismiss", "pet": "stray"})), json!({"ok": true, "was_there": true}));
    stray.wait().unwrap();
    assert_eq!(host.ask(json!({"type": "list"}))["strays"], json!([]));
    assert_refused(&host.ask(json!({"type": "dismiss", "pet": "stray"})), "gone, and never in the gallery");
}
