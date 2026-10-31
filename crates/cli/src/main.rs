//! Composition root: chooses the pet and the stage from the command line.
use familiard_application::{summon, Failure, Stage};
use familiard_pack::PetPack;
use familiard_stage_terminal::TerminalStage;

const USAGE: &str = "usage: familiard --pet <dir> [--stage desktop|terminal] [--rows N] [--frames N] [--at X,Y]";

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

fn run() -> Result<(), Failure> {
    let options = parse(std::env::args().skip(1))?;
    summon(&PetPack::new(&options.pet), stage(&options)?)
}

fn main() {
    if let Err(error) = run() {
        eprintln!("familiard: {error}");
        std::process::exit(1);
    }
}
