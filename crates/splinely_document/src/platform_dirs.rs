//! Per-user directories for settings, data (autosave) and state (crash report).
//!
//! Linux and other Unix: `$XDG_CONFIG_HOME` / `$XDG_DATA_HOME` / `$XDG_STATE_HOME`, else
//! `~/.config`, `~/.local/share`, `~/.local/state`. macOS: the XDG variables still win
//! when set (handy for tests and portable setups), else `~/Library/Application Support`.
//! Windows: the XDG variables win when set, else `%APPDATA%` (settings) and
//! `%LOCALAPPDATA%` (data, state), falling back to `%USERPROFILE%\AppData\...`.
//! The returned path is the *base*; callers append `splinely`.
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Config,
    Data,
    State,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Unix,
    MacOs,
    Windows,
}

impl Platform {
    pub const CURRENT: Platform = if cfg!(windows) {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::MacOs
    } else {
        Platform::Unix
    };
}

/// Base directory for `kind` in the current process environment.
pub fn base_dir(kind: Kind) -> Option<PathBuf> {
    resolve(Platform::CURRENT, kind, &|name| std::env::var_os(name))
}

/// Pure resolution so every platform's rules can be tested on any host.
pub fn resolve(
    platform: Platform,
    kind: Kind,
    env: &dyn Fn(&str) -> Option<OsString>,
) -> Option<PathBuf> {
    let var = |name: &str| env(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let xdg = match kind {
        Kind::Config => "XDG_CONFIG_HOME",
        Kind::Data => "XDG_DATA_HOME",
        Kind::State => "XDG_STATE_HOME",
    };
    if let Some(path) = var(xdg) {
        return Some(path);
    }
    match platform {
        Platform::Unix => {
            let relative: &[&str] = match kind {
                Kind::Config => &[".config"],
                Kind::Data => &[".local", "share"],
                Kind::State => &[".local", "state"],
            };
            Some(join(var("HOME")?, relative))
        }
        Platform::MacOs => Some(join(var("HOME")?, &["Library", "Application Support"])),
        Platform::Windows => {
            let (variable, relative): (&str, &[&str]) = match kind {
                Kind::Config => ("APPDATA", &["AppData", "Roaming"]),
                Kind::Data | Kind::State => ("LOCALAPPDATA", &["AppData", "Local"]),
            };
            var(variable).or_else(|| var("USERPROFILE").map(|home| join(home, relative)))
        }
    }
}

fn join(base: PathBuf, parts: &[&str]) -> PathBuf {
    parts.iter().fold(base, |path, part| path.join(part))
}

/// Home directory text that crash reports must hide (`HOME` and, on Windows, `USERPROFILE`).
pub fn home_dirs() -> Vec<String> {
    ["HOME", "USERPROFILE"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .filter(|home| Path::new(home).components().count() > 1)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn unix_keeps_xdg_behaviour() {
        let home = [("HOME", "/home/u")];
        let get = |kind, pairs: &[(&str, &str)]| resolve(Platform::Unix, kind, &env(pairs));
        assert_eq!(get(Kind::Config, &home), Some("/home/u/.config".into()));
        assert_eq!(get(Kind::Data, &home), Some("/home/u/.local/share".into()));
        assert_eq!(get(Kind::State, &home), Some("/home/u/.local/state".into()));
        let both = [("HOME", "/home/u"), ("XDG_CONFIG_HOME", "/x/c")];
        assert_eq!(get(Kind::Config, &both), Some("/x/c".into()));
        let empty = [("HOME", "/home/u"), ("XDG_CONFIG_HOME", "")];
        assert_eq!(get(Kind::Config, &empty), Some("/home/u/.config".into()));
        assert_eq!(get(Kind::Config, &[]), None);
    }

    #[test]
    fn macos_uses_application_support_unless_overridden() {
        let home = [("HOME", "/Users/u")];
        for kind in [Kind::Config, Kind::Data, Kind::State] {
            assert_eq!(
                resolve(Platform::MacOs, kind, &env(&home)),
                Some("/Users/u/Library/Application Support".into())
            );
        }
        let overridden = [("HOME", "/Users/u"), ("XDG_DATA_HOME", "/d")];
        assert_eq!(
            resolve(Platform::MacOs, Kind::Data, &env(&overridden)),
            Some("/d".into())
        );
    }

    #[test]
    fn windows_uses_appdata_and_falls_back_to_profile() {
        let vars = [
            ("APPDATA", "C:/Users/u/AppData/Roaming"),
            ("LOCALAPPDATA", "C:/Users/u/AppData/Local"),
        ];
        let get = |kind, pairs: &[(&str, &str)]| resolve(Platform::Windows, kind, &env(pairs));
        assert_eq!(
            get(Kind::Config, &vars),
            Some("C:/Users/u/AppData/Roaming".into())
        );
        assert_eq!(
            get(Kind::Data, &vars),
            Some("C:/Users/u/AppData/Local".into())
        );
        assert_eq!(
            get(Kind::State, &vars),
            Some("C:/Users/u/AppData/Local".into())
        );
        let profile = [("USERPROFILE", "C:/Users/u")];
        assert_eq!(
            get(Kind::Config, &profile),
            Some(["C:/Users/u", "AppData", "Roaming"].iter().collect())
        );
        assert_eq!(get(Kind::Config, &[]), None);
    }
}
