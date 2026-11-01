//! What the browser extension talks to: length-prefixed JSON on stdio. It can list the gallery,
//! bring a pet to a conversation, report that conversation's state and send a pet away, using
//! the same code as `pets --json`, `summon` and `hook`. Nothing else, whatever it is asked.
//!
//! stdout is the protocol, so nothing else may ever be printed here.
use crate::browser::{CHROMIUM_ID, GECKO_ID};
use crate::{bring_out, pet_rows, roster, Binding};
use familiard_agents::interpret;
use familiard_application::Failure;
use familiard_gallery::{home, Gallery};
use serde_json::{json, Map, Value};
use std::io::{ErrorKind, Read, Write};

/// Far more than any message of this protocol, which carry an id, a name and a little more.
const MAX_MESSAGE: u32 = 64 * 1024;
/// What browsers accept from a host, in both Chromium and Firefox.
const MAX_REPLY: usize = 1024 * 1024;
const MAX_CONVERSATION: usize = 128;
const MAX_NAME: usize = 256;
/// The first half of a session id, the way a hook would write `--agent chatgpt`.
const AGENT: &str = "chatgpt";
/// Which stage pets from the browser use. The browser has no way to choose one.
const STAGE_VARIABLE: &str = "FAMILIARD_BROWSER_STAGE";

/// A manifest's `path` cannot carry arguments, so the browser launches the binary with its own:
/// the caller's origin on Chromium, then a window handle on Windows; the manifest's path and the
/// add-on's id on Firefox.
pub fn called_by_browser(first: &str, rest: &[String]) -> bool {
    let chromium = first.strip_prefix("chrome-extension://").map(|origin| origin.trim_end_matches('/'));
    chromium == Some(CHROMIUM_ID) || rest.first().is_some_and(|id| id == GECKO_ID)
}

pub fn run() -> Result<(), Failure> {
    let (mut input, mut output) = (std::io::stdin().lock(), std::io::stdout().lock());
    while let Some(message) = read_message(&mut input, &mut output)? {
        write_message(&mut output, &answer(&message))?;
    }
    Ok(())
}

/// The next message, or `None` when the browser closed the pipe between two messages. Anything
/// else that goes wrong leaves the stream out of step, so it ends the host.
fn read_message(input: &mut impl Read, output: &mut impl Write) -> Result<Option<Vec<u8>>, Failure> {
    let mut prefix = [0u8; 4];
    let mut got = 0;
    while got < prefix.len() {
        match input.read(&mut prefix[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err("the browser closed the pipe in the middle of a message".into()),
            Ok(count) => got += count,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }
    }
    let length = u32::from_ne_bytes(prefix);
    if length > MAX_MESSAGE {
        write_message(output, &refusal(&format!("a message can have {MAX_MESSAGE} bytes at most")))?;
        return Err("a message was over the limit".into());
    }
    let mut body = vec![0u8; length as usize];
    input.read_exact(&mut body)?;
    Ok(Some(body))
}

fn write_message(output: &mut impl Write, reply: &Value) -> Result<(), Failure> {
    let mut body = serde_json::to_vec(reply)?;
    if body.len() > MAX_REPLY {
        body = serde_json::to_vec(&refusal("the reply is too large for the browser"))?;
    }
    output.write_all(&(body.len() as u32).to_ne_bytes())?;
    output.write_all(&body)?;
    output.flush()?;
    Ok(())
}

fn refusal(reason: &str) -> Value {
    json!({"ok": false, "error": reason})
}

fn answer(message: &[u8]) -> Value {
    let reply = serde_json::from_slice::<Value>(message)
        .map_err(|_| Failure::from("a message must be JSON"))
        .and_then(|message| obey(&message));
    reply.unwrap_or_else(|error| refusal(&error.to_string()))
}

fn obey(message: &Value) -> Result<Value, Failure> {
    let fields = message.as_object().ok_or("a message must be an object")?;
    let kind = fields.get("type").and_then(Value::as_str).ok_or("a message needs a type")?;
    match kind {
        "list" => {
            only(fields, &["type"])?;
            Ok(json!({"ok": true, "pets": pet_rows(), "strays": strays()}))
        }
        "summon" => {
            only(fields, &["type", "pet", "conversation", "name"])?;
            summon(pet(fields)?, conversation(fields)?, name(fields)?)
        }
        "signal" => {
            only(fields, &["type", "conversation", "signal"])?;
            signal(conversation(fields)?, signal_name(fields)?)
        }
        "dismiss" => {
            only(fields, &["type", "pet"])?;
            Ok(json!({"ok": true, "was_there": roster().dismiss(leaving_pet(fields)?)}))
        }
        other => Err(format!("unknown message type {other}").into()),
    }
}

/// Anything more than the protocol asks for is refused, so that nothing else, such as what a
/// conversation says, can ever travel this way by accident.
fn only(fields: &Map<String, Value>, allowed: &[&str]) -> Result<(), Failure> {
    match fields.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(key) => Err(format!("unexpected field {key}").into()),
        None => Ok(()),
    }
}

fn text<'a>(fields: &'a Map<String, Value>, field: &str) -> Result<&'a str, Failure> {
    fields.get(field).and_then(Value::as_str).ok_or_else(|| format!("{field} must be a string").into())
}

/// Must be in the gallery. The id goes through the gallery's own check, so every id a pack can
/// have there is accepted and nothing else is.
fn pet(fields: &Map<String, Value>) -> Result<&str, Failure> {
    let id = text(fields, "pet")?;
    Gallery::new(&home()).folder(id)?;
    Ok(id)
}

/// Pets that are out without being in the gallery, as `familiard run` leaves them.
fn strays() -> Vec<String> {
    let gallery = Gallery::new(&home());
    roster().open().into_iter().filter(|id| gallery.folder(id).is_err()).collect()
}

/// A pet that is out can leave wherever it came from. Anything else must be in the gallery, so an
/// id that is neither never gets as far as the roster's files.
fn leaving_pet(fields: &Map<String, Value>) -> Result<&str, Failure> {
    let id = text(fields, "pet")?;
    if roster().open().iter().any(|open| open == id) {
        return Ok(id);
    }
    pet(fields)
}

fn conversation(fields: &Map<String, Value>) -> Result<&str, Failure> {
    let id = text(fields, "conversation")?;
    let allowed = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    if id.is_empty() || id.len() > MAX_CONVERSATION || !id.chars().all(allowed) {
        return Err("invalid conversation id".into());
    }
    Ok(id)
}

fn name(fields: &Map<String, Value>) -> Result<&str, Failure> {
    let name = text(fields, "name")?;
    if name.chars().count() > MAX_NAME || name.chars().any(char::is_control) {
        return Err(format!("a name has {MAX_NAME} characters at most and no control characters").into());
    }
    Ok(name)
}

fn signal_name(fields: &Map<String, Value>) -> Result<&str, Failure> {
    let signal = text(fields, "signal")?;
    match signal {
        "registered" | "working" | "waiting" | "done" | "ended" => Ok(signal),
        other => Err(format!("unknown signal {other}").into()),
    }
}

fn session_of(conversation: &str) -> String {
    format!("{AGENT}:{conversation}")
}

fn summon(pet: &str, conversation: &str, name: &str) -> Result<Value, Failure> {
    let stage = std::env::var(STAGE_VARIABLE).unwrap_or_else(|_| "desktop".into());
    let binding = Binding::Session(session_of(conversation));
    if bring_out(pet, None, &stage, &binding, Some(name), false)? {
        Ok(json!({"ok": true}))
    } else {
        Ok(json!({"ok": false, "already_out": true, "error": format!("{pet} is already out")}))
    }
}

/// Delivered as a hook delivers it. There is no folder: a conversation has none, so it can never
/// claim a pet summoned with `--here`.
fn signal(conversation: &str, signal: &str) -> Result<Value, Failure> {
    let report = interpret(AGENT, Some(signal), Some(conversation), None, "").ok_or("unknown signal")?;
    roster().broadcast(&report);
    Ok(json!({"ok": true}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn only_our_own_extension_counts_as_the_browser() {
        let chromium = format!("chrome-extension://{CHROMIUM_ID}/");
        assert!(called_by_browser(&chromium, &[]));
        assert!(called_by_browser(&chromium, &args(&["--parent-window=0"])));
        assert!(called_by_browser("/path/manifest.json", &args(&[GECKO_ID])));
        assert!(!called_by_browser("chrome-extension://other/", &[]));
        assert!(!called_by_browser("/path/manifest.json", &args(&["other@example.org"])));
        assert!(!called_by_browser("frobnicate", &[]));
    }

    #[test]
    fn a_session_is_named_as_a_hook_would_name_it() {
        let report = interpret(AGENT, Some("working"), Some("abc"), None, "").unwrap();
        assert_eq!(report.session, session_of("abc"));
        assert_eq!(report.folder, None);
    }

    #[test]
    fn a_reply_too_large_for_the_browser_is_replaced_by_a_refusal() {
        let mut output = Vec::new();
        write_message(&mut output, &json!({"pets": "x".repeat(MAX_REPLY)})).unwrap();
        let body: Value = serde_json::from_slice(&output[4..]).unwrap();
        assert_eq!(body["ok"], false);
    }
}
