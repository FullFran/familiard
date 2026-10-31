//! `familiard browser install|uninstall`: tells each browser where the native host is. Each
//! browser reads a small JSON manifest, from a folder or, on Windows, through the registry. See
//! docs/browser.md for the sources of these locations.
use familiard_application::Failure;
use familiard_gallery::home;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const HOST_NAME: &str = "io.github.fullfran.familiard";
/// Derived from the `key` in extension/manifest.json; a test keeps the two together.
pub const CHROMIUM_ID: &str = "epmjbipjlabgmcignobpeeggbdjidllp";
/// `browser_specific_settings.gecko.id` in extension/manifest.json.
pub const GECKO_ID: &str = "familiard@fullfran.github.io";
const DESCRIPTION: &str = "Lets the familiard extension bring pets to a conversation";
const USAGE: &str = "usage: familiard browser install | familiard browser uninstall";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Gecko,
    Chromium,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum System {
    Linux,
    MacOs,
    Windows,
}

/// One manifest, and the browsers that read it.
#[derive(Debug, PartialEq, Eq)]
struct Target {
    browsers: &'static str,
    family: Family,
    file: PathBuf,
    /// The key under HKCU whose default value points at `file`, on Windows.
    registry: Option<String>,
}

fn system() -> System {
    if cfg!(windows) {
        System::Windows
    } else if cfg!(target_os = "macos") {
        System::MacOs
    } else {
        System::Linux
    }
}

/// `user` is the home folder, `config` where Chromium browsers keep their data on Linux, and
/// `own` familiard's own folder, which holds the manifests on Windows.
fn targets(system: System, user: &Path, config: &Path, own: &Path) -> Vec<Target> {
    let file_name = format!("{HOST_NAME}.json");
    let in_folder = |browsers, family, folder: PathBuf| Target {
        browsers,
        family,
        file: folder.join(&file_name),
        registry: None,
    };
    match system {
        // Gecko hard-codes this folder, which is why Zen reads it and not ~/.zen.
        System::Linux => vec![
            in_folder("Firefox, Zen", Family::Gecko, user.join(".mozilla/native-messaging-hosts")),
            in_folder("Chromium", Family::Chromium, config.join("chromium/NativeMessagingHosts")),
            in_folder("Chrome", Family::Chromium, config.join("google-chrome/NativeMessagingHosts")),
            in_folder("Brave", Family::Chromium, config.join("BraveSoftware/Brave-Browser/NativeMessagingHosts")),
        ],
        System::MacOs => {
            let support = user.join("Library/Application Support");
            vec![
                in_folder("Firefox, Zen", Family::Gecko, support.join("Mozilla/NativeMessagingHosts")),
                in_folder("Chromium", Family::Chromium, support.join("Chromium/NativeMessagingHosts")),
                in_folder("Chrome", Family::Chromium, support.join("Google/Chrome/NativeMessagingHosts")),
                in_folder(
                    "Brave",
                    Family::Chromium,
                    support.join("BraveSoftware/Brave-Browser/NativeMessagingHosts"),
                ),
            ]
        }
        System::Windows => {
            let in_registry = |browsers, family, name: &str, vendor: &str| Target {
                browsers,
                family,
                file: own.join("browser").join(name).join(&file_name),
                registry: Some(format!("HKCU\\SOFTWARE\\{vendor}\\NativeMessagingHosts\\{HOST_NAME}")),
            };
            vec![
                in_registry("Firefox, Zen", Family::Gecko, "gecko", "Mozilla"),
                in_registry("Chromium", Family::Chromium, "chromium", "Chromium"),
                in_registry("Chrome, Brave", Family::Chromium, "chrome", "Google\\Chrome"),
            ]
        }
    }
}

fn manifest(family: Family, binary: &Path) -> Value {
    let mut manifest = json!({
        "name": HOST_NAME,
        "description": DESCRIPTION,
        "path": binary,
        "type": "stdio",
    });
    match family {
        Family::Gecko => manifest["allowed_extensions"] = json!([GECKO_ID]),
        Family::Chromium => manifest["allowed_origins"] = json!([format!("chrome-extension://{CHROMIUM_ID}/")]),
    }
    manifest
}

/// Whether a manifest's text names familiard. Text that cannot be read as a manifest does not.
fn names_familiard(text: &str) -> bool {
    serde_json::from_str::<Value>(text).is_ok_and(|manifest| manifest["name"] == HOST_NAME)
}

fn file_is_foreign(path: &Path) -> bool {
    path.exists() && !fs::read_to_string(path).is_ok_and(|text| names_familiard(&text))
}

/// The manifest a key points at, from what `reg query <key> /ve` prints.
fn registered_file(reg_output: &str) -> Option<PathBuf> {
    let line = reg_output.lines().find(|line| line.contains("REG_SZ"))?;
    let path = line.split_once("REG_SZ")?.1.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

fn reg(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("reg").args(args).stdin(Stdio::null()).output()
}

fn registry_is_foreign(key: &str, own: &Path) -> bool {
    let Ok(output) = reg(&["query", key, "/ve"]) else { return false };
    let Some(file) = registered_file(&String::from_utf8_lossy(&output.stdout)) else { return false };
    file != own && file_is_foreign(&file)
}

fn is_foreign(target: &Target) -> bool {
    file_is_foreign(&target.file) || target.registry.as_deref().is_some_and(|key| registry_is_foreign(key, &target.file))
}

fn install_one(target: &Target, binary: &Path) -> Result<(), Failure> {
    if let Some(folder) = target.file.parent() {
        fs::create_dir_all(folder)?;
    }
    fs::write(&target.file, serde_json::to_string_pretty(&manifest(target.family, binary))?)?;
    if let Some(key) = &target.registry {
        let path = target.file.to_string_lossy();
        let status = reg(&["add", key, "/ve", "/t", "REG_SZ", "/d", &path, "/f"])?;
        if !status.status.success() {
            return Err(format!("reg add {key} failed").into());
        }
    }
    Ok(())
}

fn install(targets: &[Target]) -> Result<(), Failure> {
    let binary = fs::canonicalize(std::env::current_exe()?)?;
    let mut refused = false;
    for target in targets {
        if is_foreign(target) {
            eprintln!("{}: left alone: {} does not name familiard", target.browsers, target.file.display());
            refused = true;
            continue;
        }
        install_one(target, &binary).map_err(|error| format!("{}: {error}", target.browsers))?;
        println!("{}: {}", target.browsers, target.file.display());
    }
    if refused {
        return Err("some browsers were left alone; move those files away and run this again".into());
    }
    println!("Now load the extension in your browser: see docs/browser.md");
    Ok(())
}

fn uninstall(targets: &[Target]) -> Result<(), Failure> {
    for target in targets {
        if is_foreign(target) {
            println!("{}: left alone: {} does not name familiard", target.browsers, target.file.display());
            continue;
        }
        if let Some(key) = &target.registry {
            let _ = reg(&["delete", key, "/f"]);
        }
        if target.file.exists() {
            fs::remove_file(&target.file).map_err(|error| format!("{}: {error}", target.file.display()))?;
            println!("{}: removed {}", target.browsers, target.file.display());
            // Only goes if nothing else lives there.
            if let Some(folder) = target.file.parent() {
                let _ = fs::remove_dir(folder);
            }
        } else {
            println!("{}: nothing to remove", target.browsers);
        }
    }
    Ok(())
}

pub fn run(args: Vec<String>) -> Result<(), Failure> {
    let user = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    let user = user.ok_or("HOME is not set")?;
    let config = std::env::var_os("XDG_CONFIG_HOME").map_or_else(|| user.join(".config"), PathBuf::from);
    let targets = targets(system(), &user, &config, &home());
    match args.first().map(String::as_str) {
        Some("install") => install(&targets),
        Some("uninstall") => uninstall(&targets),
        _ => Err(USAGE.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn places(system: System) -> Vec<(String, String, Option<String>)> {
        targets(system, Path::new("/u"), Path::new("/u/.config"), Path::new("/own"))
            .into_iter()
            .map(|target| {
                let file = target.file.to_string_lossy().replace('\\', "/");
                let folder = file.strip_suffix(&format!("/{HOST_NAME}.json")).unwrap().to_string();
                (target.browsers.to_string(), folder, target.registry)
            })
            .collect()
    }

    #[test]
    fn linux_uses_each_browsers_documented_folder() {
        let folders: Vec<_> = places(System::Linux).into_iter().map(|place| place.1).collect();
        assert_eq!(
            folders,
            [
                "/u/.mozilla/native-messaging-hosts",
                "/u/.config/chromium/NativeMessagingHosts",
                "/u/.config/google-chrome/NativeMessagingHosts",
                "/u/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts",
            ]
        );
    }

    #[test]
    fn macos_uses_each_browsers_documented_folder() {
        let folders: Vec<_> = places(System::MacOs).into_iter().map(|place| place.1).collect();
        assert_eq!(
            folders,
            [
                "/u/Library/Application Support/Mozilla/NativeMessagingHosts",
                "/u/Library/Application Support/Chromium/NativeMessagingHosts",
                "/u/Library/Application Support/Google/Chrome/NativeMessagingHosts",
                "/u/Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts",
            ]
        );
    }

    #[test]
    fn windows_registers_a_manifest_of_our_own_under_the_current_user() {
        let keys: Vec<_> = places(System::Windows).into_iter().map(|place| place.2.unwrap()).collect();
        assert_eq!(
            keys,
            [
                format!("HKCU\\SOFTWARE\\Mozilla\\NativeMessagingHosts\\{HOST_NAME}"),
                format!("HKCU\\SOFTWARE\\Chromium\\NativeMessagingHosts\\{HOST_NAME}"),
                format!("HKCU\\SOFTWARE\\Google\\Chrome\\NativeMessagingHosts\\{HOST_NAME}"),
            ]
        );
        assert!(places(System::Windows).iter().all(|place| place.1.starts_with("/own/browser/")));
    }

    #[test]
    fn reads_the_manifest_out_of_what_reg_prints() {
        let printed = "\r\nHKEY_CURRENT_USER\\SOFTWARE\\Mozilla\\x\r\n    (Default)    REG_SZ    C:\\Users\\a b\\m.json\r\n\r\n";
        assert_eq!(registered_file(printed), Some(PathBuf::from("C:\\Users\\a b\\m.json")));
        assert_eq!(registered_file("ERROR: nothing here"), None);
        assert_eq!(registered_file("    (Default)    REG_SZ    \r\n"), None);
    }

    #[test]
    fn only_a_manifest_with_our_name_is_ours() {
        assert!(names_familiard(&format!(r#"{{"name":"{HOST_NAME}"}}"#)));
        assert!(!names_familiard(r#"{"name":"com.someone.else"}"#));
        assert!(!names_familiard("[]"));
        assert!(!names_familiard("not json"));
    }

    #[test]
    fn each_family_gets_the_field_its_browsers_read() {
        let gecko = manifest(Family::Gecko, Path::new("/bin/familiard"));
        assert_eq!(gecko["allowed_extensions"][0], GECKO_ID);
        assert!(gecko.get("allowed_origins").is_none());
        let chromium = manifest(Family::Chromium, Path::new("/bin/familiard"));
        assert_eq!(chromium["allowed_origins"][0], format!("chrome-extension://{CHROMIUM_ID}/"));
        assert!(chromium.get("allowed_extensions").is_none());
    }
}
