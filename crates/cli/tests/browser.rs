//! `familiard browser`, through the binary, always against a temporary HOME.
#![cfg(not(windows))]
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const HOST: &str = "io.github.fullfran.familiard";
const CHROMIUM_ORIGIN: &str = "chrome-extension://epmjbipjlabgmcignobpeeggbdjidllp/";
const GECKO_ID: &str = "familiard@fullfran.github.io";

struct Sandbox(PathBuf);

impl Sandbox {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let unique = format!("familiard-browser-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let home = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&home).unwrap();
        Self(home)
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_familiard"));
        command
            .args(args)
            .env("HOME", &self.0)
            .env("FAMILIARD_HOME", self.0.join("familiard"))
            .env_remove("XDG_CONFIG_HOME");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    fn firefox(&self) -> PathBuf {
        self.manifest(if cfg!(target_os = "macos") {
            "Library/Application Support/Mozilla/NativeMessagingHosts"
        } else {
            ".mozilla/native-messaging-hosts"
        })
    }

    fn manifest(&self, folder: &str) -> PathBuf {
        self.0.join(folder).join(format!("{HOST}.json"))
    }

    fn chromium_family(&self) -> Vec<PathBuf> {
        let folders: &[&str] = if cfg!(target_os = "macos") {
            &[
                "Library/Application Support/Chromium/NativeMessagingHosts",
                "Library/Application Support/Google/Chrome/NativeMessagingHosts",
                "Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts",
            ]
        } else {
            &[
                ".config/chromium/NativeMessagingHosts",
                ".config/google-chrome/NativeMessagingHosts",
                ".config/BraveSoftware/Brave-Browser/NativeMessagingHosts",
            ]
        };
        folders.iter().map(|folder| self.manifest(folder)).collect()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn this_binary() -> String {
    std::fs::canonicalize(env!("CARGO_BIN_EXE_familiard")).unwrap().to_str().unwrap().to_string()
}

#[test]
fn install_writes_every_browsers_manifest_and_uninstall_removes_them() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["browser", "install"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let said = String::from_utf8(output.stdout).unwrap();
    for browser in ["Firefox", "Zen", "Chromium", "Chrome", "Brave"] {
        assert!(said.contains(browser), "{browser} in {said}");
    }

    let firefox = read_json(&sandbox.firefox());
    assert_eq!(firefox["name"], HOST);
    assert_eq!(firefox["type"], "stdio");
    assert_eq!(firefox["path"], this_binary());
    assert_eq!(firefox["allowed_extensions"], serde_json::json!([GECKO_ID]));
    assert!(firefox.get("allowed_origins").is_none());

    for path in sandbox.chromium_family() {
        let chromium = read_json(&path);
        assert_eq!(chromium["name"], HOST, "{path:?}");
        assert_eq!(chromium["type"], "stdio");
        assert_eq!(chromium["path"], this_binary());
        assert_eq!(chromium["allowed_origins"], serde_json::json!([CHROMIUM_ORIGIN]));
        assert!(chromium.get("allowed_extensions").is_none());
    }

    assert!(sandbox.run(&["browser", "install"]).status.success(), "installing twice is fine");

    let output = sandbox.run(&["browser", "uninstall"]);
    assert!(output.status.success());
    assert!(!sandbox.firefox().exists());
    for path in sandbox.chromium_family() {
        assert!(!path.exists(), "{path:?}");
    }
    assert!(sandbox.run(&["browser", "uninstall"]).status.success(), "uninstalling twice is fine");
}

#[test]
fn a_manifest_that_does_not_name_familiard_is_never_touched() {
    let foreign = r#"{"name":"com.someone.else","path":"/bin/else","type":"stdio"}"#;
    for content in [foreign, "not json at all"] {
        let sandbox = Sandbox::new();
        std::fs::create_dir_all(sandbox.firefox().parent().unwrap()).unwrap();
        std::fs::write(sandbox.firefox(), content).unwrap();

        let output = sandbox.run(&["browser", "install"]);
        assert_eq!(output.status.code(), Some(1), "it says so through the exit code");
        assert!(String::from_utf8_lossy(&output.stderr).contains("does not name familiard"));
        assert_eq!(std::fs::read_to_string(sandbox.firefox()).unwrap(), content);
        for path in sandbox.chromium_family() {
            assert!(path.exists(), "the other browsers still get theirs: {path:?}");
        }

        let output = sandbox.run(&["browser", "uninstall"]);
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("does not name familiard"));
        assert_eq!(std::fs::read_to_string(sandbox.firefox()).unwrap(), content);
        for path in sandbox.chromium_family() {
            assert!(!path.exists(), "{path:?}");
        }
    }
}

#[test]
fn a_folder_the_browser_already_uses_keeps_its_other_manifests() {
    let sandbox = Sandbox::new();
    let neighbour = sandbox.firefox().with_file_name("org.keepassxc.keepassxc_browser.json");
    std::fs::create_dir_all(neighbour.parent().unwrap()).unwrap();
    std::fs::write(&neighbour, "{}").unwrap();
    assert!(sandbox.run(&["browser", "install"]).status.success());
    assert!(sandbox.run(&["browser", "uninstall"]).status.success());
    assert_eq!(std::fs::read_to_string(&neighbour).unwrap(), "{}");
}

#[cfg(target_os = "linux")]
#[test]
fn chromium_browsers_follow_xdg_config_home_and_firefox_does_not() {
    let sandbox = Sandbox::new();
    let config = sandbox.0.join("elsewhere");
    let output = sandbox.command(&["browser", "install"]).env("XDG_CONFIG_HOME", &config).output().unwrap();
    assert!(output.status.success());
    assert!(config.join(format!("chromium/NativeMessagingHosts/{HOST}.json")).exists());
    assert!(config.join(format!("google-chrome/NativeMessagingHosts/{HOST}.json")).exists());
    assert!(config.join(format!("BraveSoftware/Brave-Browser/NativeMessagingHosts/{HOST}.json")).exists());
    assert!(sandbox.firefox().exists(), "Gecko always reads the legacy path");
    assert!(!sandbox.0.join(".config").exists());
}

#[test]
fn bad_usage_is_explained() {
    let sandbox = Sandbox::new();
    for args in [&["browser"][..], &["browser", "frobnicate"][..]] {
        let output = sandbox.run(args);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("browser install"), "{args:?}");
    }
    assert!(!sandbox.firefox().exists());
}
