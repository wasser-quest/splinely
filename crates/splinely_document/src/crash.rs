//! Opt-in crash reports for the editor and the CLI. A panic hook records the
//! version, system, message, location and a trimmed backtrace — never project
//! data — and turns it into a pre-filled GitHub issue link. The editor saves
//! the report and offers the link on the next start; the CLI prints it in its
//! JSON error. Nothing is sent anywhere unless the user submits the issue.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;

pub const ISSUES_URL: &str = "https://github.com/wasser-quest/splinely/issues/new";
/// GitHub rejects very long URLs; stay well below its limit.
const MAX_URL_LEN: usize = 7500;
const MAX_BACKTRACE_LINES: usize = 40;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrashReport {
    /// "editor", "cli" …
    #[serde(default)]
    pub app: String,
    pub version: String,
    pub os: String,
    pub language: String,
    pub message: String,
    pub location: String,
    pub backtrace: String,
    /// Seconds since the Unix epoch.
    pub time: u64,
}

/// Where the last crash report is kept until the user reports or dismisses it.
pub fn report_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
            Some(PathBuf::from(home).join(".local").join("state"))
        })?;
    Some(base.join("splinely").join("last-crash.json"))
}

pub fn system_description() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Replace the home directory so reports don't reveal the user name.
fn redact(text: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if home.len() > 1 => text.replace(&home, "~"),
        _ => text.to_owned(),
    }
}

/// Keep Splinely's own frames (with their source lines) and drop the
/// standard-library and panic machinery around them.
fn trim_backtrace(backtrace: &str) -> String {
    let mut out = Vec::new();
    let mut keep = false;
    for line in backtrace.lines() {
        let trimmed = line.trim_start();
        let is_frame = trimmed
            .split_once(": ")
            .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if is_frame {
            keep = trimmed.contains("splinely") && !trimmed.contains("crash::");
        }
        if keep {
            out.push(line);
        }
        if out.len() >= MAX_BACKTRACE_LINES {
            break;
        }
    }
    if out.is_empty() {
        backtrace
            .lines()
            .take(MAX_BACKTRACE_LINES)
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        out.join("\n")
    }
}

static LANGUAGE: OnceLock<fn() -> String> = OnceLock::new();

fn current_language() -> String {
    LANGUAGE
        .get()
        .map_or_else(|| "en".to_owned(), |language| language())
}

/// Install the panic hook for `app`. `language` reports the UI language code.
/// An interactive app (the editor) saves the report for the next start and
/// keeps the default panic message; the CLI instead prints one JSON error.
pub fn install(
    app: &'static str,
    version: &'static str,
    language: fn() -> String,
    interactive: bool,
) {
    let _ = LANGUAGE.set(language);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".into());
        let report = CrashReport {
            app: app.into(),
            version: version.into(),
            os: system_description(),
            language: current_language(),
            message: redact(&message),
            location: info
                .location()
                .map(|l| format!("{}:{}", l.file(), l.line()))
                .unwrap_or_default(),
            backtrace: redact(&trim_backtrace(
                &std::backtrace::Backtrace::force_capture().to_string(),
            )),
            time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };
        let url = issue_url(Some(&report));
        if !interactive {
            let error = serde_json::json!({"ok": false, "error": {
                "code": "internal_error",
                "message": format!("Splinely crashed: {} ({})", report.message, report.location),
                "hint": "This is a bug in Splinely, not in your input. Reporting it is optional: open reportUrl for a pre-filled GitHub issue.",
                "reportUrl": url,
            }});
            eprintln!("{error}");
            return;
        }
        if let Some(path) = report_path() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(json) = serde_json::to_string_pretty(&report) {
                let _ = std::fs::write(&path, json);
            }
        }
        previous(info);
        eprintln!("\nSplinely crashed. To report it (optional), open:\n{url}\n");
    }));
}

/// The report saved by the last crash, if any.
pub fn pending() -> Option<CrashReport> {
    let text = std::fs::read_to_string(report_path()?).ok()?;
    serde_json::from_str(&text).ok()
}

/// Forget the saved report (after reporting or dismissing it).
pub fn dismiss() {
    if let Some(path) = report_path() {
        let _ = std::fs::remove_file(path);
    }
}

fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn body(report: Option<&CrashReport>, backtrace: &str) -> String {
    let mut body = String::from(
        "## What happened?\n\n<!-- Describe what you were doing when the problem appeared. -->\n\n\
         ## Steps to reproduce\n\n1. \n2. \n3. \n\n\
         ## Expected behavior\n\n\n\
         ## Environment\n\n",
    );
    let version = report.map_or(env!("CARGO_PKG_VERSION"), |r| &r.version);
    let os = report.map_or_else(system_description, |r| r.os.clone());
    let language = report.map_or_else(current_language, |r| r.language.clone());
    body.push_str(&format!(
        "- Splinely: {version}\n- System: {os}\n- UI language: {language}\n"
    ));
    if let Some(app) = report.map(|r| r.app.as_str()).filter(|a| !a.is_empty()) {
        body.push_str(&format!("- Component: {app}\n"));
    }
    if let Some(r) = report {
        body.push_str(&format!(
            "\n## Crash details\n\n- Message: `{}`\n- Location: `{}`\n\n<details><summary>Backtrace</summary>\n\n```\n{}\n```\n\n</details>\n",
            r.message.replace('`', "'"),
            r.location,
            backtrace
        ));
    }
    body
}

/// A GitHub "new issue" link with title and body filled in. With a report it
/// is a crash report, otherwise an ordinary bug report.
pub fn issue_url(report: Option<&CrashReport>) -> String {
    let title = match report {
        Some(r) => {
            let short: String = r
                .message
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(80)
                .collect();
            match r.app.as_str() {
                "" => format!("Crash: {short}"),
                app => format!("Crash ({app}): {short}"),
            }
        }
        None => "Bug: ".into(),
    };
    let labels = if report.is_some() { "bug,crash" } else { "bug" };
    let mut lines: Vec<&str> = report.map_or(vec![], |r| r.backtrace.lines().collect());
    loop {
        let url = format!(
            "{ISSUES_URL}?title={}&labels={}&body={}",
            encode(&title),
            encode(labels),
            encode(&body(report, &lines.join("\n")))
        );
        if url.len() <= MAX_URL_LEN || lines.is_empty() {
            return url;
        }
        let keep = lines.len() * 3 / 4;
        lines.truncate(keep);
    }
}

/// Open a URL in the default browser.
pub fn open_in_browser(url: &str) -> std::io::Result<()> {
    let mut command = if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else {
        std::process::Command::new("xdg-open")
    };
    command
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(backtrace: String) -> CrashReport {
        CrashReport {
            app: String::new(),
            version: "0.4.0".into(),
            os: "linux x86_64".into(),
            language: "en".into(),
            message: "index out of bounds".into(),
            location: "src/app.rs:1".into(),
            backtrace,
            time: 0,
        }
    }

    #[test]
    fn issue_links_are_prefilled_and_bounded() {
        let url = issue_url(Some(&report("frame\n".repeat(5000))));
        assert!(url.starts_with(ISSUES_URL));
        assert!(url.len() <= MAX_URL_LEN);
        assert!(url.contains("title=Crash%3A%20index%20out%20of%20bounds"));
        assert!(url.contains("Steps%20to%20reproduce"));
        assert!(url.contains("labels=bug%2Ccrash"));
        let bug = issue_url(None);
        assert!(bug.contains("title=Bug%3A%20") && !bug.contains("Crash%20details"));
    }

    #[test]
    fn backtraces_keep_only_splinely_frames() {
        let trace = "   0: std::rt\n             at /rustc/x\n   1: splinely_document::crash::install\n   2: splinely_cli::main\n             at ./crates/splinely_cli/src/main.rs:9:1\n   3: core::ops::call_once";
        assert_eq!(
            trim_backtrace(trace),
            "   2: splinely_cli::main\n             at ./crates/splinely_cli/src/main.rs:9:1"
        );
        assert!(trim_backtrace("  0: std::rt").contains("std::rt"));
    }
}
