//! Composition root: chooses the pet and the stage from the command line.
use familiard_agents::{interpret, port, send, LocalFeed};
use familiard_application::{summon, ActivityFeed, Failure, Stage};
use familiard_pack::PetPack;
use familiard_stage_terminal::TerminalStage;
use std::io::Read;
use std::sync::mpsc::channel;

const USAGE: &str = "usage: familiard --pet <dir> [--stage desktop|terminal] [--rows N] [--frames N] [--at X,Y]
       familiard hook --agent <name> [--event <event>] [--session <id>]";
const MAX_PAYLOAD: u64 = 1 << 20;

struct Options {
    pet: String,
    stage: String,
    rows: u32,
    frames: Option<u32>,
    at: (i32, i32),
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Options, Failure> {
    let mut options = Options { pet: String::new(), stage: "desktop".into(), rows: 24, frames: None, at: (80, 80) };
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value\n{USAGE}"));
        match flag.as_str() {
            "--pet" => options.pet = value()?,
            "--stage" => options.stage = value()?,
            "--rows" => options.rows = value()?.parse()?,
            "--frames" => options.frames = Some(value()?.parse()?),
            "--at" => {
                let text = value()?;
                let (x, y) = text.split_once(',').ok_or(format!("--at expects X,Y\n{USAGE}"))?;
                options.at = (x.parse()?, y.parse()?);
            }
            other => return Err(format!("unknown option {other}\n{USAGE}").into()),
        }
    }
    if options.pet.is_empty() {
        return Err(format!("--pet is required\n{USAGE}").into());
    }
    Ok(options)
}

fn stage(options: &Options) -> Result<Box<dyn Stage>, Failure> {
    match options.stage.as_str() {
        "terminal" => Ok(Box::new(TerminalStage::new(options.rows, options.frames))),
        "desktop" => desktop_stage(options),
        other => Err(format!("unknown stage {other}\n{USAGE}").into()),
    }
}

#[cfg(target_os = "linux")]
fn desktop_stage(options: &Options) -> Result<Box<dyn Stage>, Failure> {
    Ok(Box::new(familiard_stage_wayland::WaylandStage::new(options.at)))
}

#[cfg(not(target_os = "linux"))]
fn desktop_stage(_options: &Options) -> Result<Box<dyn Stage>, Failure> {
    Err("the desktop stage is not available on this platform yet; use --stage terminal".into())
}

fn run(args: impl Iterator<Item = String>) -> Result<(), Failure> {
    let options = parse(args)?;
    let (outbox, inbox) = channel();
    // A pet that cannot hear the agents is still a pet, for example a second one.
    if let Err(error) = Box::new(LocalFeed::new(port())).listen(outbox) {
        eprintln!("familiard: {error}; this pet will not react to agents");
    }
    summon(&PetPack::new(&options.pet), stage(&options)?, inbox)
}

/// Runs inside an agent, so it stays silent and never fails: anything on stdout would be
/// read as a decision, and a failing hook could get in the agent's way.
fn hook(args: impl Iterator<Item = String>) {
    let args: Vec<String> = args.collect();
    let flag = |name: &str| args.iter().position(|arg| arg == name).and_then(|at| args.get(at + 1));
    let Some(agent) = flag("--agent") else { return };
    let event = flag("--event").map(String::as_str);
    let mut payload = String::new();
    if event.is_none() {
        let _ = std::io::stdin().take(MAX_PAYLOAD).read_to_string(&mut payload);
    }
    if let Some(report) = interpret(agent, event, flag("--session").map(String::as_str), &payload) {
        let _ = send(&report, port());
    }
}

fn main() {
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().is_some_and(|first| first == "hook") {
        hook(args.skip(1));
        return;
    }
    if let Err(error) = run(args) {
        eprintln!("familiard: {error}");
        std::process::exit(1);
    }
}
