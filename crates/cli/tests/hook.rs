//! The hook runs inside other people's agents, so its contract is tested on the binary.
use familiard_agents::{LocalFeed, Roster};
use familiard_application::ActivityFeed;
use familiard_domain::{Report, Signal};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

fn here() -> Option<String> {
    Some(std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap().to_string_lossy().into_owned())
}

fn scratch_home() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let unique = format!("familiard-hook-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    std::env::temp_dir().join(unique)
}

fn listening(home: &PathBuf, id: &str) -> Receiver<Report> {
    let (outbox, inbox) = channel();
    Box::new(LocalFeed::new(Roster::new(home.join("running")), id)).listen(outbox).unwrap();
    inbox
}

fn hook(home: &PathBuf, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_familiard"))
        .arg("hook")
        .args(args)
        .env("FAMILIARD_HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(stdin.as_bytes());
    child.wait_with_output().unwrap()
}

fn assert_silent_success(output: &Output) {
    assert!(output.status.success());
    assert!(output.stdout.is_empty(), "stdout is read by agents as a decision");
    assert!(output.stderr.is_empty());
}

#[test]
fn delivers_an_agent_event_silently_to_every_open_pet() {
    let home = scratch_home();
    let first = listening(&home, "a");
    let second = listening(&home, "b");
    let payload = r#"{"hook_event_name":"PreToolUse","session_id":"s1","tool_name":"Bash"}"#;
    assert_silent_success(&hook(&home, &["--agent", "claude"], payload));
    let expected = Report { session: "claude:s1".into(), signal: Signal::Working, folder: here() };
    assert_eq!(first.recv_timeout(Duration::from_secs(2)).unwrap(), expected);
    assert_eq!(second.recv_timeout(Duration::from_secs(2)).unwrap(), expected);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn explicit_flags_work_without_a_payload() {
    let home = scratch_home();
    let inbox = listening(&home, "a");
    assert_silent_success(&hook(&home, &["--agent", "pi", "--event", "done", "--session", "one"], ""));
    let expected = Report { session: "pi:one".into(), signal: Signal::Done, folder: here() };
    assert_eq!(inbox.recv_timeout(Duration::from_secs(2)).unwrap(), expected);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn the_folder_comes_from_the_payload_then_the_flag_then_the_current_directory() {
    let home = scratch_home();
    let inbox = listening(&home, "a");
    let folder_of = |args: &[&str], stdin: &str| {
        assert_silent_success(&hook(&home, args, stdin));
        inbox.recv_timeout(Duration::from_secs(2)).unwrap().folder
    };
    let claude = ["--agent", "claude", "--project", "/from/flag"];
    let payload = r#"{"hook_event_name":"Stop","session_id":"s","cwd":"/from/payload"}"#;
    assert_eq!(folder_of(&claude, payload).as_deref(), Some("/from/payload"));
    let payload = r#"{"hook_event_name":"Stop","session_id":"s","workspace_roots":["/from/roots"]}"#;
    assert_eq!(folder_of(&claude, payload).as_deref(), Some("/from/roots"));
    let payload = r#"{"hook_event_name":"Stop","session_id":"s"}"#;
    assert_eq!(folder_of(&claude, payload).as_deref(), Some("/from/flag"));
    assert_eq!(folder_of(&["--agent", "claude"], payload), here());
    let _ = std::fs::remove_dir_all(home);
}

#[cfg(unix)]
#[test]
fn a_symlinked_folder_is_reported_as_its_real_path() {
    let home = scratch_home();
    let inbox = listening(&home, "a");
    let real = home.join("real");
    std::fs::create_dir_all(&real).unwrap();
    let link = home.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let args = ["--agent", "pi", "--event", "working", "--project", link.to_str().unwrap()];
    assert_silent_success(&hook(&home, &args, ""));
    let report = inbox.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(report.folder.as_deref(), std::fs::canonicalize(&real).unwrap().to_str());
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_line_without_a_folder_from_an_older_hook_still_reaches_a_pet() {
    let home = scratch_home();
    let inbox = listening(&home, "a");
    let port: u16 = std::fs::read_to_string(home.join("running/a")).unwrap().trim().parse().unwrap();
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    writeln!(stream, r#"{{"session":"claude:old","signal":"working"}}"#).unwrap();
    let expected = Report { session: "claude:old".into(), signal: Signal::Working, folder: None };
    assert_eq!(inbox.recv_timeout(Duration::from_secs(2)).unwrap(), expected);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn never_blocks_an_agent_when_no_pet_is_open() {
    let started = Instant::now();
    let output = hook(&scratch_home(), &["--agent", "claude"], r#"{"hook_event_name":"Stop"}"#);
    assert_silent_success(&output);
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn an_agent_that_never_closes_stdin_does_not_hang_it() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_familiard"))
        .args(["hook", "--agent", "claude"])
        .env("FAMILIARD_HOME", scratch_home())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdin_kept_open = child.stdin.take();
    let started = Instant::now();
    while child.try_wait().unwrap().is_none() {
        assert!(started.elapsed() < Duration::from_secs(3), "the hook is hanging");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(child.wait().unwrap().success());
    drop(stdin_kept_open);
}

#[test]
fn bad_input_is_ignored_silently() {
    for (args, stdin) in [
        (&["--agent", "claude"][..], "not json"),
        (&["--agent"][..], ""),
        (&[][..], ""),
        (&["--nonsense", "x"][..], "{}"),
    ] {
        assert_silent_success(&hook(&scratch_home(), args, stdin));
    }
}

#[test]
fn reports_its_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_familiard")).arg("--version").output().unwrap();
    assert!(output.status.success());
    let expected = format!("familiard {}\n", env!("CARGO_PKG_VERSION"));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
}
