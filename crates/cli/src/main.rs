//! Composition root: turns the command line into calls to the adapters.
use familiard_agents::{interpret, LocalFeed, Roster};
use familiard_application::{summon, ActivityFeed, Failure, PetSource, Stage};
use familiard_caption::FontCaptioner;
use familiard_gallery::{home, Gallery};
use familiard_pack::PetPack;
use familiard_stage_terminal::TerminalStage;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

const USAGE: &str = "usage:
  familiard                        summon the default pet
  familiard summon, -s [id] [--here] [--name TEXT] [--at X,Y] [--stage desktop|terminal]
                                   --here: follow the next session in this folder, leave when it ends
  familiard dismiss, -d [id] | --all   send pets away (the default one if no id)
  familiard pets, -l [--json]      list the gallery and which pets are open
  familiard add <folder> [--as id] add a pet to the gallery
  familiard remove <id>            remove a pet from the gallery
  familiard default <id>           choose the default pet
  familiard run --pet <dir> [--follow DIR] [--name TEXT] [--stage desktop|terminal] [--rows N] [--frames N] [--at X,Y]
                                   run a pet in the foreground, without the gallery
  familiard hook --agent <name> [--event <event>] [--session <id>] [--project <dir>]
  familiard --version";
const MAX_PAYLOAD: u64 = 1 << 20;
const PAYLOAD_TIMEOUT: Duration = Duration::from_secs(1);
const SUMMON_TIMEOUT: Duration = Duration::from_secs(5);
const CASCADE: i32 = 140;

fn roster() -> Roster {
    Roster::new(home().join("running"))
}

struct Options {
    pet: String,
    id: Option<String>,
    stage: String,
    rows: u32,
    frames: Option<u32>,
    at: (i32, i32),
    follow: Option<String>,
    name: Option<String>,
}

fn parse_at(text: &str) -> Result<(i32, i32), Failure> {
    let (x, y) = text.split_once(',').ok_or(format!("--at expects X,Y\n{USAGE}"))?;
    Ok((x.parse()?, y.parse()?))
}

fn parse_run(args: Vec<String>) -> Result<Options, Failure> {
    let mut options =
        Options {
            pet: String::new(),
            id: None,
            stage: "desktop".into(),
            rows: 24,
            frames: None,
            at: (80, 80),
            follow: None,
            name: None,
        };
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value\n{USAGE}"));
        match flag.as_str() {
            "--pet" => options.pet = value()?,
            "--id" => options.id = Some(value()?),
            "--stage" => options.stage = value()?,
            "--rows" => options.rows = value()?.parse()?,
            "--frames" => options.frames = Some(value()?.parse()?),
            "--at" => options.at = parse_at(&value()?)?,
            "--follow" => options.follow = Some(canonical(&value()?)),
            "--name" => options.name = Some(value()?),
            other => return Err(format!("unknown option {other}\n{USAGE}").into()),
        }
    }
    if options.pet.is_empty() {
        return Err(format!("--pet is required\n{USAGE}").into());
    }
    Ok(options)
}

/// The real path, so that a symlink and what it points to count as the same folder. A path
/// that does not exist here is compared as given.
fn canonical(folder: &str) -> String {
    match fs::canonicalize(folder) {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(_) => folder.to_string(),
    }
}

/// What the caption says, if anything: the given name, or else the folder's last component.
fn label(options: &Options) -> Option<String> {
    let named = options.name.as_deref().map(str::trim).filter(|name| !name.is_empty());
    let folder_name = || {
        let folder = options.follow.as_deref()?;
        Some(Path::new(folder).file_name().map_or(folder.to_string(), |name| name.to_string_lossy().into_owned()))
    };
    named.map(String::from).or_else(folder_name)
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

#[cfg(windows)]
fn desktop_stage(options: &Options) -> Result<Box<dyn Stage>, Failure> {
    Ok(Box::new(familiard_stage_windows::WindowsStage::new(options.at)))
}

#[cfg(not(any(target_os = "linux", windows)))]
fn desktop_stage(_options: &Options) -> Result<Box<dyn Stage>, Failure> {
    Err("the desktop stage is not available on this platform yet; use --stage terminal".into())
}

fn run(args: Vec<String>) -> Result<(), Failure> {
    let options = parse_run(args)?;
    let id = options.id.clone().unwrap_or_else(|| format!("run-{}", std::process::id()));
    let roster = roster();
    let (outbox, inbox) = channel();
    // A pet that cannot hear the agents is still a pet.
    if let Err(error) = Box::new(LocalFeed::new(roster.clone(), id.as_str())).listen(outbox) {
        eprintln!("familiard: {error}; this pet will not react to agents");
    }
    let result = summon(&PetPack::new(&options.pet), stage(&options)?, inbox, |mut player| {
        if let Some(folder) = &options.follow {
            player = player.following(folder.as_str());
        }
        // The font is only read for a pet with something to say, and gone once it has been
        // used. A pet that cannot write its caption is still a pet.
        let Some(label) = label(&options) else { return player };
        match FontCaptioner::new() {
            Ok(captioner) => {
                let player = player.captioned(&label, &captioner);
                drop(captioner);
                give_back_freed_memory();
                player
            }
            Err(error) => {
                eprintln!("familiard: {error}; this pet will have no caption");
                player
            }
        }
    });
    roster.unregister(&id);
    result
}

/// The font takes several megabytes while it is open, in many small pieces with the captions
/// allocated after them, so the allocator would keep all of them for good without this.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn give_back_freed_memory() {
    extern "C" {
        fn malloc_trim(pad: usize) -> std::ffi::c_int;
    }
    // SAFETY: takes no pointers, and glibc, which this target always links, defines it.
    unsafe {
        malloc_trim(0);
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn give_back_freed_memory() {}

/// Starts a pet as its own process, and waits until it is really there.
fn summon_pet(args: Vec<String>) -> Result<(), Failure> {
    let mut id = None;
    let mut at = None;
    let mut stage = "desktop".to_string();
    let mut here = false;
    let mut name = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value\n{USAGE}"));
        match arg.as_str() {
            "--at" => at = Some(parse_at(&value()?)?),
            "--stage" => stage = value()?,
            "--here" => here = true,
            "--name" => name = Some(value()?),
            other if !other.starts_with('-') && id.is_none() => id = Some(other.to_string()),
            other => return Err(format!("unknown option {other}\n{USAGE}").into()),
        }
    }
    let gallery = Gallery::new(&home());
    let id = match id {
        Some(id) => id,
        None => gallery.chosen()?,
    };
    let folder = gallery.folder(&id)?;
    let roster = roster();
    let open = roster.open();
    if open.contains(&id) {
        println!("{id} is already here");
        return Ok(());
    }
    // New pets appear in a row, so they do not land on top of each other.
    let (x, y) = at.unwrap_or((80 + CASCADE * open.len() as i32, 80));

    let logs = home().join("logs");
    fs::create_dir_all(&logs)?;
    let log_path = logs.join(format!("{id}.log"));
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["run", "--id", &id, "--stage", &stage, "--at", &format!("{x},{y}"), "--pet"])
        .arg(&folder);
    if here {
        command.arg("--follow").arg(fs::canonicalize(std::env::current_dir()?)?);
    }
    if let Some(name) = &name {
        command.args(["--name", name]);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(File::create(&log_path)?);
    detach(&mut command);
    let mut child = command.spawn()?;

    let started = Instant::now();
    while started.elapsed() < SUMMON_TIMEOUT {
        if let Some(status) = child.try_wait()? {
            let log = fs::read_to_string(&log_path).unwrap_or_default();
            return Err(format!("{id} could not appear ({status}): {}", log.trim()).into());
        }
        if roster.open().contains(&id) {
            // Give a stage that fails right after listening the chance to say so.
            std::thread::sleep(Duration::from_millis(150));
            if let Some(status) = child.try_wait()? {
                let log = fs::read_to_string(&log_path).unwrap_or_default();
                return Err(format!("{id} could not appear ({status}): {}", log.trim()).into());
            }
            println!("{id} is here");
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(format!("{id} did not appear in time, see {}", log_path.display()).into())
}

#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT};
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    command.creation_flags(DETACHED_PROCESS);
    // Children inherit every inheritable handle, even when their own stdio is redirected.
    // A pet holding our stdout would hang whoever is waiting for this command to finish.
    for id in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // SAFETY: both calls only take a handle by value; failure leaves things as they were.
        unsafe {
            SetHandleInformation(GetStdHandle(id), HANDLE_FLAG_INHERIT, 0);
        }
    }
}

#[cfg(not(any(unix, windows)))]
fn detach(_command: &mut Command) {}

fn dismiss(args: Vec<String>) -> Result<(), Failure> {
    let roster = roster();
    let targets = match args.first().map(String::as_str) {
        Some("--all") => roster.open(),
        Some(id) => vec![id.to_string()],
        None => vec![Gallery::new(&home()).chosen()?],
    };
    if targets.is_empty() {
        println!("no pets are open");
    }
    for id in targets {
        if roster.dismiss(&id) {
            println!("{id} has left");
        } else {
            println!("{id} was not here");
        }
    }
    Ok(())
}

fn pets(args: Vec<String>) -> Result<(), Failure> {
    let gallery = Gallery::new(&home());
    let open = roster().open();
    let entries = gallery.list();
    if args.iter().any(|arg| arg == "--json") {
        let rows: Vec<_> = entries
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "id": entry.id, "name": entry.name, "default": entry.is_default,
                    "open": open.contains(&entry.id),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    if entries.is_empty() {
        println!("the gallery is empty; add a pet with: familiard add <folder>");
    }
    for entry in entries {
        let mark = if entry.is_default { "*" } else { " " };
        let state = if open.contains(&entry.id) { "  (open)" } else { "" };
        println!("{mark} {}  {}{state}", entry.id, entry.name);
    }
    Ok(())
}

fn add(args: Vec<String>) -> Result<(), Failure> {
    let mut folder = None;
    let mut id = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--as" => id = Some(args.next().ok_or(format!("--as needs a value\n{USAGE}"))?),
            other if !other.starts_with('-') && folder.is_none() => folder = Some(other.to_string()),
            other => return Err(format!("unknown option {other}\n{USAGE}").into()),
        }
    }
    let folder = folder.ok_or(format!("add needs a folder\n{USAGE}"))?;
    PetPack::new(&folder).load()?; // a pet that cannot load does not enter the gallery
    let entry = Gallery::new(&home()).add(Path::new(&folder), id.as_deref())?;
    let note = if entry.is_default { " (default)" } else { "" };
    println!("added {}{note}: familiard summon {}", entry.name, entry.id);
    Ok(())
}

fn one_id(args: Vec<String>, command: &str) -> Result<String, Failure> {
    args.into_iter().next().ok_or_else(|| format!("{command} needs a pet id\n{USAGE}").into())
}

fn remove(args: Vec<String>) -> Result<(), Failure> {
    let id = one_id(args, "remove")?;
    Gallery::new(&home()).remove(&id)?;
    roster().dismiss(&id);
    println!("removed {id}");
    Ok(())
}

fn set_default(args: Vec<String>) -> Result<(), Failure> {
    let id = one_id(args, "default")?;
    Gallery::new(&home()).set_default(&id)?;
    println!("{id} is now the default pet");
    Ok(())
}

/// Runs inside an agent, so it stays silent and never fails: anything on stdout would be
/// read as a decision, and a failing hook could get in the agent's way.
fn hook(args: Vec<String>) {
    let flag = |name: &str| args.iter().position(|arg| arg == name).and_then(|at| args.get(at + 1));
    let Some(agent) = flag("--agent") else { return };
    let event = flag("--event").map(String::as_str);
    let payload = if event.is_none() { read_payload() } else { String::new() };
    let guess = flag("--project").cloned().or_else(|| {
        std::env::current_dir().ok().map(|folder| folder.to_string_lossy().into_owned())
    });
    let session = flag("--session").map(String::as_str);
    if let Some(mut report) = interpret(agent, event, session, guess.as_deref(), &payload) {
        report.folder = report.folder.as_deref().map(canonical);
        roster().broadcast(&report);
    }
}

/// Reads the agent's JSON, giving up rather than hanging if it never closes stdin.
fn read_payload() -> String {
    let (outbox, inbox) = channel();
    std::thread::spawn(move || {
        let mut payload = String::new();
        let _ = std::io::stdin().take(MAX_PAYLOAD).read_to_string(&mut payload);
        let _ = outbox.send(payload);
    });
    inbox.recv_timeout(PAYLOAD_TIMEOUT).unwrap_or_default()
}

fn dispatch(mut args: Vec<String>) -> Result<(), Failure> {
    if args.is_empty() {
        return summon_pet(args);
    }
    let command = args.remove(0);
    match command.as_str() {
        "--version" => {
            println!("familiard {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "--help" | "help" => {
            println!("{USAGE}");
            Ok(())
        }
        "hook" => {
            hook(args);
            Ok(())
        }
        "summon" | "-s" => summon_pet(args),
        "dismiss" | "-d" => dismiss(args),
        "pets" | "-l" => pets(args),
        "add" => add(args),
        "remove" => remove(args),
        "default" => set_default(args),
        "run" => run(args),
        other => Err(format!("unknown command {other}\n{USAGE}").into()),
    }
}

fn main() {
    if let Err(error) = dispatch(std::env::args_os().skip(1).map(|arg| arg.to_string_lossy().into_owned()).collect()) {
        eprintln!("familiard: {error}");
        std::process::exit(1);
    }
}

