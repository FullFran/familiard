//! The hook runs inside other people's agents, so its contract is tested on the binary.
use familiard_agents::LocalFeed;
use familiard_application::ActivityFeed;
use familiard_domain::{Report, Signal};
use std::io::Write;
use std::net::TcpListener;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port()
}

fn hook(port: u16, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_familiard"))
        .arg("hook")
        .args(args)
        .env("FAMILIARD_PORT", port.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn assert_silent_success(output: &Output) {
    assert!(output.status.success());
    assert!(output.stdout.is_empty(), "stdout is read by agents as a decision");
    assert!(output.stderr.is_empty());
}

#[test]
fn delivers_an_agent_event_silently() {
    let port = free_port();
    let (outbox, inbox) = channel();
    Box::new(LocalFeed::new(port)).listen(outbox).unwrap();
    let payload = r#"{"hook_event_name":"PreToolUse","session_id":"s1","tool_name":"Bash"}"#;
    let output = hook(port, &["--agent", "claude"], payload);
    assert_silent_success(&output);
    let expected = Report { session: "claude:s1".into(), signal: Signal::Working };
    assert_eq!(inbox.recv_timeout(Duration::from_secs(2)).unwrap(), expected);
}

#[test]
fn explicit_flags_work_without_a_payload() {
    let port = free_port();
    let (outbox, inbox) = channel();
    Box::new(LocalFeed::new(port)).listen(outbox).unwrap();
    let args = ["--agent", "pi", "--event", "done", "--session", "pi:one"];
    assert_silent_success(&hook(port, &args, ""));
    let expected = Report { session: "pi:pi:one".into(), signal: Signal::Done };
    assert_eq!(inbox.recv_timeout(Duration::from_secs(2)).unwrap(), expected);
}

#[test]
fn never_blocks_an_agent_when_the_pet_is_closed() {
    let started = Instant::now();
    let output = hook(free_port(), &["--agent", "claude"], r#"{"hook_event_name":"Stop"}"#);
    assert_silent_success(&output);
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn bad_input_is_ignored_silently() {
    for (args, stdin) in [
        (&["--agent", "claude"][..], "not json"),
        (&["--agent"][..], ""),
        (&[][..], ""),
        (&["--nonsense", "x"][..], "{}"),
    ] {
        assert_silent_success(&hook(free_port(), args, stdin));
    }
}

#[test]
fn reports_its_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_familiard")).arg("--version").output().unwrap();
    assert!(output.status.success());
    let expected = format!("familiard {}\n", env!("CARGO_PKG_VERSION"));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
}
