//! Hears from coding agents. Their hooks run `familiard hook`, which translates what the
//! agent sends ([`interpret`]) and passes it over a loopback socket ([`send`]) to the
//! running pet ([`LocalFeed`]). A plain TCP line keeps this the same on every platform.
//!
//! The event tables are ported from AgentPet (MIT), github.com/ntd4996/agentpet.
use familiard_application::{ActivityFeed, Failure};
use familiard_domain::{Report, Signal};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::time::Duration;

pub const DEFAULT_PORT: u16 = 47629;
const MAX_LINE: u64 = 4096;

/// The port to use: `FAMILIARD_PORT` if it holds a valid one, else the default.
pub fn port() -> u16 {
    std::env::var("FAMILIARD_PORT").ok().and_then(|value| value.parse().ok()).unwrap_or(DEFAULT_PORT)
}

/// Translates an agent's own event name. `None` means the event changes nothing.
pub fn signal_for(agent: &str, event: &str) -> Option<Signal> {
    match event {
        "registered" => return Some(Signal::Registered),
        "working" => return Some(Signal::Working),
        "waiting" => return Some(Signal::Waiting),
        "done" => return Some(Signal::Done),
        "ended" => return Some(Signal::Ended),
        _ => {}
    }
    match (agent, event) {
        ("claude" | "gemini", "SessionEnd") => Some(Signal::Ended),
        ("claude" | "codex" | "gemini", "SessionStart") => Some(Signal::Registered),
        ("claude" | "codex", "UserPromptSubmit" | "PreToolUse" | "PostToolUse") => Some(Signal::Working),
        ("codex", "SubagentStart") => Some(Signal::Working),
        ("claude" | "gemini", "Notification") => Some(Signal::Waiting),
        ("codex", "PermissionRequest") => Some(Signal::Waiting),
        ("claude" | "codex", "Stop") => Some(Signal::Done),
        ("codex", "SubagentStop") => Some(Signal::Done),
        ("gemini", "BeforeAgent" | "BeforeModel" | "BeforeTool" | "AfterTool" | "BeforeToolSelection" | "AfterModel") => {
            Some(Signal::Working)
        }
        ("gemini", "AfterAgent") => Some(Signal::Done),
        ("antigravity", "PreInvocation" | "PreToolUse" | "PostToolUse" | "PostInvocation") => Some(Signal::Working),
        ("antigravity", "Stop") => Some(Signal::Done),
        ("opencode", "session.created") => Some(Signal::Working),
        ("opencode", "session.idle") => Some(Signal::Done),
        _ => None,
    }
}

/// Builds a report from a hook's flags and the JSON on its stdin. Flags win.
pub fn interpret(agent: &str, event: Option<&str>, session: Option<&str>, payload: &str) -> Option<Report> {
    let json: Value = serde_json::from_str(payload).unwrap_or(Value::Null);
    let first = |keys: &[&str]| {
        keys.iter().find_map(|key| json.get(key)?.as_str().filter(|text| !text.is_empty()).map(String::from))
    };
    let event = match event {
        Some(event) => Some(event.to_string()),
        None if agent == "antigravity" => antigravity_event(&json),
        None => first(&["hook_event_name", "hookEventName", "agent_action_name", "eventName"]),
    };
    let signal = signal_for(agent, &event?)?;
    let session = session
        .map(String::from)
        .or_else(|| first(&["session_id", "conversation_id", "trajectory_id", "sessionId", "conversationId"]))
        .or_else(|| first(&["cwd", "projectRoot"]))
        .unwrap_or_else(|| "default".into());
    Some(Report { session: format!("{agent}:{session}"), signal })
}

/// Antigravity sends no event name, so the fields present say what happened.
fn antigravity_event(json: &Value) -> Option<String> {
    if json.get("terminationReason").is_some() || json.get("fullyIdle").is_some() {
        Some("done".into())
    } else if ["toolCall", "invocationNum", "stepIdx"].iter().any(|key| json.get(key).is_some()) {
        Some("working".into())
    } else {
        None
    }
}

#[derive(Serialize, Deserialize)]
struct Line {
    session: String,
    signal: String,
}

fn name_of(signal: Signal) -> &'static str {
    match signal {
        Signal::Registered => "registered",
        Signal::Working => "working",
        Signal::Waiting => "waiting",
        Signal::Done => "done",
        Signal::Ended => "ended",
    }
}

fn decode(line: &str) -> Option<Report> {
    let line: Line = serde_json::from_str(line).ok()?;
    Some(Report { signal: signal_for("", &line.signal)?, session: line.session })
}

/// Tells the running pet. Fails quickly when there is none, so an agent never waits.
pub fn send(report: &Report, port: u16) -> Result<(), Failure> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let timeout = Duration::from_millis(200);
    let mut stream = TcpStream::connect_timeout(&address, timeout)?;
    stream.set_write_timeout(Some(timeout))?;
    let line = Line { session: report.session.clone(), signal: name_of(report.signal).into() };
    writeln!(stream, "{}", serde_json::to_string(&line)?)?;
    Ok(())
}

/// Listens on loopback only, so nothing outside this machine can reach the pet. Local
/// programs can make it change mood, and that is all they can do.
pub struct LocalFeed {
    port: u16,
}

impl LocalFeed {
    pub fn new(port: u16) -> Self {
        Self { port }
    }
}

impl ActivityFeed for LocalFeed {
    fn listen(self: Box<Self>, inbox: Sender<Report>) -> Result<(), Failure> {
        let listener = TcpListener::bind(("127.0.0.1", self.port))
            .map_err(|e| format!("cannot listen on port {}: {e}", self.port))?;
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                if let Some(report) = read_report(stream) {
                    if inbox.send(report).is_err() {
                        return;
                    }
                }
            }
        });
        Ok(())
    }
}

fn read_report(stream: TcpStream) -> Option<Report> {
    stream.set_read_timeout(Some(Duration::from_millis(200))).ok()?;
    let mut line = String::new();
    BufReader::new(stream.take(MAX_LINE)).read_line(&mut line).ok()?;
    decode(&line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn signal(agent: &str, payload: &str) -> Option<Signal> {
        interpret(agent, None, None, payload).map(|report| report.signal)
    }

    #[test]
    fn understands_each_agents_events() {
        let cases = [
            ("claude", "SessionStart", Signal::Registered),
            ("claude", "PreToolUse", Signal::Working),
            ("claude", "Notification", Signal::Waiting),
            ("claude", "Stop", Signal::Done),
            ("claude", "SessionEnd", Signal::Ended),
            ("codex", "PermissionRequest", Signal::Waiting),
            ("codex", "SubagentStop", Signal::Done),
            ("gemini", "BeforeTool", Signal::Working),
            ("gemini", "AfterAgent", Signal::Done),
            ("gemini", "SessionEnd", Signal::Ended),
            ("antigravity", "PreToolUse", Signal::Working),
        ];
        for (agent, event, expected) in cases {
            assert_eq!(signal_for(agent, event), Some(expected), "{agent} {event}");
        }
    }

    #[test]
    fn a_subagent_finishing_does_not_mean_claude_finished() {
        assert_eq!(signal_for("claude", "SubagentStop"), None);
    }

    #[test]
    fn reads_the_session_from_the_payload_and_keeps_agents_apart() {
        let payload = r#"{"hook_event_name":"Stop","session_id":"abc","cwd":"/x"}"#;
        let report = interpret("claude", None, None, payload).unwrap();
        assert_eq!(report, Report { session: "claude:abc".into(), signal: Signal::Done });
    }

    #[test]
    fn flags_win_over_the_payload() {
        let payload = r#"{"hook_event_name":"Stop","session_id":"abc"}"#;
        let report = interpret("pi", Some("working"), Some("s1"), payload).unwrap();
        assert_eq!(report, Report { session: "pi:s1".into(), signal: Signal::Working });
    }

    #[test]
    fn falls_back_to_the_folder_and_then_to_a_default_session() {
        let with_cwd = r#"{"hook_event_name":"Stop","cwd":"/x"}"#;
        assert_eq!(interpret("claude", None, None, with_cwd).unwrap().session, "claude:/x");
        assert_eq!(interpret("pi", Some("done"), None, "").unwrap().session, "pi:default");
    }

    #[test]
    fn antigravity_is_recognised_by_its_fields() {
        assert_eq!(signal("antigravity", r#"{"toolCall":{}}"#), Some(Signal::Working));
        assert_eq!(signal("antigravity", r#"{"fullyIdle":true}"#), Some(Signal::Done));
        assert_eq!(signal("antigravity", r#"{"other":1}"#), None);
    }

    #[test]
    fn garbage_and_unknown_events_produce_nothing() {
        assert_eq!(signal("claude", "not json"), None);
        assert_eq!(signal("claude", ""), None);
        assert_eq!(signal("claude", r#"{"hook_event_name":"Whatever"}"#), None);
        assert_eq!(signal("claude", r#"{"hook_event_name":5}"#), None);
    }

    #[test]
    fn a_report_travels_from_send_to_the_inbox() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let (outbox, inbox) = channel();
        Box::new(LocalFeed::new(port)).listen(outbox).unwrap();
        let report = Report { session: "claude:a b".into(), signal: Signal::Waiting };
        send(&report, port).unwrap();
        assert_eq!(inbox.recv_timeout(Duration::from_secs(2)).unwrap(), report);
    }

    #[test]
    fn a_silent_or_oversized_client_does_not_stop_the_feed() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let (outbox, inbox) = channel();
        Box::new(LocalFeed::new(port)).listen(outbox).unwrap();
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        let silent = TcpStream::connect(address).unwrap();
        let mut noisy = TcpStream::connect(address).unwrap();
        let _ = noisy.write_all(&vec![b'x'; 100_000]);
        drop(noisy);
        let report = Report { session: "s".into(), signal: Signal::Done };
        send(&report, port).unwrap();
        assert_eq!(inbox.recv_timeout(Duration::from_secs(3)).unwrap(), report);
        drop(silent);
    }

    #[test]
    fn a_port_in_use_is_reported() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let error = Box::new(LocalFeed::new(port)).listen(channel().0).unwrap_err();
        assert!(error.to_string().contains(&port.to_string()));
    }
}
