//! Hears from coding agents. Their hooks run `familiard hook`, which translates what the
//! agent sends ([`interpret`]) and passes it over loopback to every pet in the
//! [`Roster`], each of which listens with a [`LocalFeed`]. A plain TCP line keeps this the
//! same on every platform.
//!
//! The event tables are ported from AgentPet (MIT), github.com/ntd4996/agentpet.
use familiard_application::{ActivityFeed, Failure};
use familiard_domain::{Report, Signal};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

/// Room for a session id and a folder, each as long as a path can get.
const MAX_LINE: u64 = 16 * 1024;
const TIMEOUT: Duration = Duration::from_millis(200);
const DISMISS: &str = "dismiss";

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

/// Builds a report from a hook's flags and the JSON on its stdin. Flags win, except for the
/// folder: what the agent says about itself beats `folder`, the hook's own guess.
pub fn interpret(
    agent: &str,
    event: Option<&str>,
    session: Option<&str>,
    folder: Option<&str>,
    payload: &str,
) -> Option<Report> {
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
    let first_of_list = |keys: &[&str]| {
        keys.iter().find_map(|key| {
            json.get(key)?.as_array()?.first()?.as_str().filter(|text| !text.is_empty()).map(String::from)
        })
    };
    let folder = first(&["cwd", "projectRoot"])
        .or_else(|| first_of_list(&["workspace_roots", "workspacePaths"]))
        .or_else(|| folder.filter(|text| !text.is_empty()).map(String::from));
    Some(Report { session: format!("{agent}:{session}"), signal, folder, pane: None })
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
    /// Absent in lines from older hooks, and when the session's folder is unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    folder: Option<String>,
    /// Absent in lines from older hooks, and outside a terminal multiplexer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pane: Option<String>,
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
    Some(Report { signal: signal_for("", &line.signal)?, session: line.session, folder: line.folder, pane: line.pane })
}

fn connect(port: u16) -> std::io::Result<TcpStream> {
    let stream = TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), TIMEOUT)?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    Ok(stream)
}

/// The pets open right now: one file per pet, holding the port it listens on. A pet that
/// died leaves its file behind, and the next one to fail to reach it removes it.
#[derive(Debug, Clone)]
pub struct Roster {
    directory: PathBuf,
}

impl Roster {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self { directory: directory.into() }
    }

    fn register(&self, id: &str, port: u16) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.directory)?;
        std::fs::write(self.directory.join(id), port.to_string())
    }

    pub fn unregister(&self, id: &str) {
        let _ = std::fs::remove_file(self.directory.join(id));
    }

    fn entries(&self) -> Vec<(String, u16)> {
        let Ok(files) = std::fs::read_dir(&self.directory) else { return Vec::new() };
        files
            .flatten()
            .filter_map(|file| {
                let port = std::fs::read_to_string(file.path()).ok()?.trim().parse().ok()?;
                Some((file.file_name().into_string().ok()?, port))
            })
            .collect()
    }

    /// The ids of the pets that answer, forgetting the ones that do not.
    pub fn open(&self) -> Vec<String> {
        let mut open: Vec<String> = self
            .entries()
            .into_iter()
            .filter_map(|(id, port)| match connect(port) {
                Ok(_) => Some(id),
                Err(_) => {
                    self.unregister(&id);
                    None
                }
            })
            .collect();
        open.sort();
        open
    }

    /// Tells every open pet. Fails quickly, so an agent never waits.
    pub fn broadcast(&self, report: &Report) {
        let line = Line {
            session: report.session.clone(),
            signal: name_of(report.signal).into(),
            folder: report.folder.clone(),
            pane: report.pane.clone(),
        };
        let Ok(text) = serde_json::to_string(&line) else { return };
        for (id, port) in self.entries() {
            match connect(port) {
                Ok(mut stream) => {
                    let _ = writeln!(stream, "{text}");
                }
                Err(_) => self.unregister(&id),
            }
        }
    }

    /// Asks a pet to leave. Returns whether it was there to ask.
    pub fn dismiss(&self, id: &str) -> bool {
        let found = self.entries().into_iter().find(|(entry, _)| entry == id);
        let Some(Ok(mut stream)) = found.map(|(_, port)| connect(port)) else {
            self.unregister(id);
            return false;
        };
        writeln!(stream, "{DISMISS}").is_ok()
    }
}

/// Listens on loopback only, so nothing outside this machine can reach the pet. Local
/// programs can make it change mood or leave, and that is all they can do.
pub struct LocalFeed {
    roster: Roster,
    id: String,
}

impl LocalFeed {
    pub fn new(roster: Roster, id: impl Into<String>) -> Self {
        Self { roster, id: id.into() }
    }
}

impl ActivityFeed for LocalFeed {
    fn listen(self: Box<Self>, inbox: Sender<Report>) -> Result<(), Failure> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| format!("cannot listen: {e}"))?;
        let port = listener.local_addr()?.port();
        self.roster.register(&self.id, port).map_err(|e| format!("cannot register the pet: {e}"))?;
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                // Its own thread, so that a slow client delays nobody else.
                let (roster, id, inbox) = (self.roster.clone(), self.id.clone(), inbox.clone());
                std::thread::spawn(move || match read_line(stream).as_deref().map(str::trim) {
                    Some(DISMISS) => {
                        roster.unregister(&id);
                        std::process::exit(0);
                    }
                    Some(line) => {
                        if let Some(report) = decode(line) {
                            let _ = inbox.send(report);
                        }
                    }
                    None => {}
                });
            }
        });
        Ok(())
    }
}

fn read_line(stream: TcpStream) -> Option<String> {
    stream.set_read_timeout(Some(TIMEOUT)).ok()?;
    let mut line = String::new();
    BufReader::new(stream.take(MAX_LINE)).read_line(&mut line).ok()?;
    Some(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn signal(agent: &str, payload: &str) -> Option<Signal> {
        interpret(agent, None, None, None, payload).map(|report| report.signal)
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
        let report = interpret("claude", None, None, None, payload).unwrap();
        assert_eq!(report, Report { session: "claude:abc".into(), signal: Signal::Done, folder: Some("/x".into()), pane: None });
    }

    #[test]
    fn flags_win_over_the_payload() {
        let payload = r#"{"hook_event_name":"Stop","session_id":"abc"}"#;
        let report = interpret("pi", Some("working"), Some("s1"), None, payload).unwrap();
        assert_eq!(report, Report { session: "pi:s1".into(), signal: Signal::Working, folder: None, pane: None });
    }

    #[test]
    fn falls_back_to_the_folder_and_then_to_a_default_session() {
        let with_cwd = r#"{"hook_event_name":"Stop","cwd":"/x"}"#;
        assert_eq!(interpret("claude", None, None, None, with_cwd).unwrap().session, "claude:/x");
        assert_eq!(interpret("pi", Some("done"), None, None, "").unwrap().session, "pi:default");
    }

    #[test]
    fn the_folder_comes_from_the_payload_then_from_what_the_hook_guessed() {
        let folder = |payload: &str, guess| interpret("claude", Some("working"), None, guess, payload).unwrap().folder;
        let cases = [
            (r#"{"cwd":"/cwd","projectRoot":"/root","workspace_roots":["/list"]}"#, "/cwd"),
            (r#"{"projectRoot":"/root","workspace_roots":["/list"]}"#, "/root"),
            (r#"{"workspace_roots":["/roots","/ignored"],"workspacePaths":["/paths"]}"#, "/roots"),
            (r#"{"workspacePaths":["/paths"]}"#, "/paths"),
            (r#"{"cwd":"","workspace_roots":[]}"#, "/guess"),
            ("not json", "/guess"),
        ];
        for (payload, expected) in cases {
            assert_eq!(folder(payload, Some("/guess")).as_deref(), Some(expected), "{payload}");
        }
        assert_eq!(folder("{}", None), None);
    }

    #[test]
    fn a_line_from_an_older_hook_still_works_and_a_new_line_carries_the_folder() {
        let old = decode(r#"{"session":"claude:s","signal":"working"}"#).unwrap();
        assert_eq!(old, Report { session: "claude:s".into(), signal: Signal::Working, folder: None, pane: None });
        let new = decode(r#"{"session":"claude:s","signal":"done","folder":"/x"}"#).unwrap();
        assert_eq!(new.folder.as_deref(), Some("/x"));
    }

    #[test]
    fn a_long_folder_still_arrives() {
        let roster = roster();
        let (outbox, inbox) = channel();
        Box::new(LocalFeed::new(roster.clone(), "p")).listen(outbox).unwrap();
        let folder = format!("/{}", "x".repeat(4000));
        let report = Report { session: "s".into(), signal: Signal::Done, folder: Some(folder), pane: None };
        roster.broadcast(&report);
        assert_eq!(inbox.recv_timeout(Duration::from_secs(2)).unwrap(), report);
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

    fn roster() -> Roster {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let unique = format!("familiard-roster-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        Roster::new(std::env::temp_dir().join(unique))
    }

    #[test]
    fn a_report_reaches_every_open_pet() {
        let roster = roster();
        let (first_outbox, first) = channel();
        let (second_outbox, second) = channel();
        Box::new(LocalFeed::new(roster.clone(), "a")).listen(first_outbox).unwrap();
        Box::new(LocalFeed::new(roster.clone(), "b")).listen(second_outbox).unwrap();
        assert_eq!(roster.open(), ["a", "b"]);
        let report = Report { session: "claude:a b".into(), signal: Signal::Waiting, folder: Some("/x y/z".into()), pane: None };
        roster.broadcast(&report);
        assert_eq!(first.recv_timeout(Duration::from_secs(2)).unwrap(), report);
        assert_eq!(second.recv_timeout(Duration::from_secs(2)).unwrap(), report);
    }

    #[test]
    fn a_pet_that_died_is_forgotten() {
        let roster = roster();
        let free = TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
        roster.register("gone", free).unwrap();
        assert!(roster.open().is_empty());
        assert!(roster.entries().is_empty());
        roster.register("gone", free).unwrap();
        roster.broadcast(&Report { session: "s".into(), signal: Signal::Done, folder: None, pane: None });
        assert!(roster.entries().is_empty());
        assert!(!roster.dismiss("gone"));
    }

    #[test]
    fn nobody_home_is_not_an_error() {
        let roster = roster();
        roster.broadcast(&Report { session: "s".into(), signal: Signal::Done, folder: None, pane: None });
        assert!(roster.open().is_empty());
        assert!(!roster.dismiss("nobody"));
    }

    #[test]
    fn many_silent_clients_do_not_delay_a_report() {
        let roster = roster();
        let (outbox, inbox) = channel();
        Box::new(LocalFeed::new(roster.clone(), "p")).listen(outbox).unwrap();
        let port = roster.entries()[0].1;
        let silent: Vec<_> = (0..20).map(|_| connect(port).unwrap()).collect();
        let report = Report { session: "s".into(), signal: Signal::Done, folder: None, pane: None };
        roster.broadcast(&report);
        assert_eq!(inbox.recv_timeout(Duration::from_millis(150)).unwrap(), report);
        drop(silent);
    }

    #[test]
    fn a_silent_or_oversized_client_does_not_stop_the_feed() {
        let roster = roster();
        let (outbox, inbox) = channel();
        Box::new(LocalFeed::new(roster.clone(), "p")).listen(outbox).unwrap();
        let port = roster.entries()[0].1;
        let silent = connect(port).unwrap();
        let mut noisy = connect(port).unwrap();
        let _ = noisy.write_all(&vec![b'x'; 100_000]);
        drop(noisy);
        let report = Report { session: "s".into(), signal: Signal::Done, folder: None, pane: None };
        roster.broadcast(&report);
        assert_eq!(inbox.recv_timeout(Duration::from_secs(3)).unwrap(), report);
        drop(silent);
    }
}
