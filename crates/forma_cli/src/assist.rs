//! Helpers that make the CLI forgiving and self-explaining for scripts and
//! LLM agents: in-place editing when `--output` is omitted, cleaned-up argument
//! errors with a usage line and a next step, and "did you mean" ID suggestions.
use crate::{failure, Cli, Failure};
use clap::CommandFactory;
use std::ffi::OsString;

/// Commands whose `--output` is not the edited project (or that convert formats);
/// they keep requiring an explicit `--output`.
const EXPLICIT_OUTPUT: &[&str] = &["export", "render", "convert", "import-svg", "batch"];

/// Whether a leaf command edits its input project (and may omit --output).
fn edits_in_place(path: &[String], command: &clap::Command) -> bool {
    let explicit = EXPLICIT_OUTPUT.contains(&path[0].as_str()) || path[0] == "plugin";
    let takes_input = command
        .get_positionals()
        .next()
        .is_some_and(|a| a.get_id() == "input");
    let required_output = command
        .get_arguments()
        .any(|a| a.get_id() == "output" && a.is_required_set());
    !explicit && takes_input && required_output
}

fn relax(mut command: clap::Command, path: &mut Vec<String>) -> clap::Command {
    let names: Vec<String> = command
        .get_subcommands()
        .map(|s| s.get_name().to_owned())
        .collect();
    for name in names {
        command = command.mut_subcommand(&name, |sub| {
            path.push(name.clone());
            let sub = if sub.has_subcommands() {
                relax(sub, path)
            } else if edits_in_place(path, &sub) {
                sub.mut_arg("output", |a| {
                    a.required(false)
                        .help("Where to save the project (default: overwrite the input file)")
                })
            } else {
                sub
            };
            path.pop();
            sub
        });
    }
    command
}

/// The clap tree as users see it: `--output` optional where it defaults to the input.
pub fn command() -> clap::Command {
    relax(Cli::command(), &mut vec![])
}

/// Where the subcommand chain ends and which tokens are positionals.
pub struct Located {
    /// e.g. ["node", "add"]
    pub path: Vec<String>,
    /// Index (in the token list without the program name) of the first positional.
    pub input: Option<usize>,
    pub has_output: bool,
    pub takes_input: bool,
    pub takes_output: bool,
    /// Edits the input project; `--output` defaults to it.
    pub in_place: bool,
}

/// Analyse arguments (without the program name) against the clap tree.
pub fn locate(tokens: &[String]) -> Option<Located> {
    let mut command = Cli::command();
    let mut path = vec![];
    let mut i = 0;
    while i < tokens.len() {
        let Some(sub) = command
            .get_subcommands()
            .find(|s| s.get_name() == tokens[i] || s.get_all_aliases().any(|a| a == tokens[i]))
            .cloned()
        else {
            break;
        };
        path.push(sub.get_name().to_owned());
        command = sub;
        i += 1;
    }
    if path.is_empty() {
        return None;
    }
    let takes_input = command
        .get_positionals()
        .next()
        .is_some_and(|a| a.get_id() == "input");
    let takes_output = command.get_arguments().any(|a| a.get_id() == "output");
    let in_place = edits_in_place(&path, &command);
    let mut input = None;
    let mut has_output = false;
    let mut j = i;
    while j < tokens.len() {
        let token = &tokens[j];
        if token == "--" {
            if input.is_none() && j + 1 < tokens.len() {
                input = Some(j + 1);
            }
            break;
        }
        if let Some(long) = token.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((name, _)) => (name, true),
                None => (long, false),
            };
            if name == "output" {
                has_output = true;
            }
            let takes_value = command
                .get_arguments()
                .find(|a| {
                    a.get_long() == Some(name)
                        || a.get_all_aliases().is_some_and(|al| al.contains(&name))
                })
                .is_some_and(|a| a.get_action().takes_values());
            if takes_value && !inline {
                j += 1;
            }
        } else if token.starts_with('-') && token.len() > 1 && token.parse::<f64>().is_err() {
            // Short flags such as -h take no value here.
        } else if input.is_none() {
            input = Some(j);
        }
        j += 1;
    }
    Some(Located {
        path,
        input,
        has_output,
        takes_input,
        takes_output,
        in_place,
    })
}

/// Edit in place when `--output` is omitted: `node add scene.frm …` saves back
/// to scene.frm (validated, atomic), and `-` input writes JSON to stdout.
pub fn prepare_args(args: Vec<OsString>) -> Vec<OsString> {
    let Some(strings) = args
        .iter()
        .skip(1)
        .map(|a| a.clone().into_string().ok())
        .collect::<Option<Vec<String>>>()
    else {
        return args;
    };
    let Some(found) = locate(&strings) else {
        return args;
    };
    if !found.in_place || found.has_output {
        return args;
    }
    let Some(input) = found.input.map(|i| strings[i].clone()) else {
        return args;
    };
    let mut args = args;
    args.push("--output".into());
    args.push(input.into());
    args
}

/// Turn a clap error into a short message, the usage line and a next step.
pub fn argument_failure(args: &[OsString], error: &clap::Error) -> Failure {
    let rendered = error.render().to_string();
    let mut message = rendered
        .trim()
        .trim_start_matches("error: ")
        .replace("\n\nFor more information, try '--help'.", "");
    let mut usage = None;
    if let Some(at) = message.find("\n\nUsage: ") {
        usage = Some(message[at + 2..].trim().to_owned());
        message.truncate(at);
    }
    let tokens: Vec<String> = args
        .iter()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    // clap's "to pass '--x' as a value, use '-- --x'" misleads; list the real flags.
    if let Some(at) = message.find("\n\n  tip: to pass") {
        message.truncate(at);
    }
    let located = locate(&tokens);
    let flags = located.as_ref().and_then(|l| {
        let mut command = Cli::command();
        for name in &l.path {
            command = command.find_subcommand(name)?.clone();
        }
        let flags: Vec<String> = command
            .get_arguments()
            .filter(|a| !matches!(a.get_id().as_str(), "help" | "output"))
            .filter_map(|a| a.get_long().map(|l| format!("--{l}")))
            .collect();
        (!command.has_subcommands()).then(|| flags.join(" "))
    });
    if message.starts_with("unexpected argument") {
        if let Some(flags) = &flags {
            message.push_str(&format!(". Valid flags: {flags}"));
        }
    }
    let path = located.map(|l| l.path.join(" "));
    let mut hint = match &path {
        Some(path) => format!("Run `forma-cli {path} --help` for the exact flags."),
        None => {
            "Run `forma-cli guide` for a short workflow, or `forma-cli --help` for all commands."
                .into()
        }
    };
    if let Some(usage) = usage {
        hint = format!("{usage}. {hint}");
    }
    failure("invalid_arguments", message.trim()).hint(hint)
}

/// Generic next step per error code (used when a failure carries no hint).
pub fn default_hint(code: &str) -> Option<String> {
    Some(match code {
        "validation_error" => "Nothing was written. Fix the reported value; `forma-cli check FILE` lists problems with fixes.",
        "not_found" => "IDs are case-sensitive. `forma-cli node list FILE` (or `inspect FILE`) shows all IDs.",
        "unknown_field" => "Field names are camelCase and must exist in `forma-cli schema` (model section). Remove or rename the field.",
        "invalid_json" => "The file or patch is not strict JSON (no comments, no trailing commas, double quotes).",
        "invalid_project" => "Compare with `forma-cli example` or `forma-cli schema` (examples.complete) for the exact shape.",
        "data_loss" => "The value cannot be stored exactly; use plain finite numbers and keep arrays complete when patching.",
        "read_error" => "Check the path. Create a project first with `forma-cli new --output FILE.frm`.",
        "write_error" => "The output directory must exist and be writable.",
        _ => return None,
    }
    .into())
}

/// Levenshtein distance for short IDs.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.to_lowercase().chars().collect();
    let b: Vec<char> = b.to_lowercase().chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let current = row[j + 1];
            row[j + 1] = (previous + usize::from(ca != cb))
                .min(row[j] + 1)
                .min(row[j + 1] + 1);
            previous = current;
        }
    }
    row[b.len()]
}

/// "No node with ID x" plus the closest existing IDs.
pub fn not_found<'a>(what: &str, id: &str, existing: impl IntoIterator<Item = &'a str>) -> Failure {
    let existing: Vec<&str> = existing.into_iter().collect();
    let mut close: Vec<(usize, &str)> = existing
        .iter()
        .map(|e| (distance(id, e), *e))
        .filter(|(d, e)| *d <= 2.max(id.len() / 3) || e.to_lowercase().contains(&id.to_lowercase()))
        .collect();
    close.sort();
    let mut hint = String::new();
    if let Some((_, best)) = close.first() {
        hint.push_str(&format!("Did you mean '{best}'? "));
    }
    if existing.is_empty() {
        hint.push_str(&format!("The project has no {what} yet."));
    } else {
        let shown: Vec<&str> = existing.iter().take(30).copied().collect();
        hint.push_str(&format!(
            "Existing {what} IDs: {}{}",
            shown.join(", "),
            if existing.len() > 30 { ", …" } else { "" }
        ));
    }
    failure("not_found", format!("No {what} with ID '{id}'")).hint(hint)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn output_defaults_to_the_input_for_project_edits() {
        let args = |a: &[&str]| -> Vec<String> {
            let mut all = vec![OsString::from("forma-cli")];
            all.extend(a.iter().map(OsString::from));
            prepare_args(all)
                .into_iter()
                .skip(1)
                .map(|s| s.into_string().unwrap())
                .collect()
        };
        assert_eq!(
            args(&["node", "add", "--kind", "rect", "s.frm"]),
            strings(&["node", "add", "--kind", "rect", "s.frm", "--output", "s.frm"])
        );
        // Values of options are not mistaken for the input.
        assert_eq!(
            args(&["keyframe", "--clip", "c", "s.frm", "--time", "-1"])[6..],
            strings(&["--output", "s.frm"])
        );
        // Explicit output, exports and renders stay untouched.
        for a in [
            &["node", "add", "s.frm", "--output", "o.frm"][..],
            &["export", "s.frm"],
            &["render", "s.frm"],
            &["new"],
            &["validate", "s.frm"],
        ] {
            assert_eq!(args(a), strings(a));
        }
    }

    #[test]
    fn unknown_ids_suggest_the_closest() {
        let error = not_found("node", "sunn", ["sky", "sun", "ground"]);
        assert_eq!(error.code, "not_found");
        assert!(error
            .hint
            .as_deref()
            .unwrap()
            .starts_with("Did you mean 'sun'?"));
        let error = not_found("node", "zzz", ["sky"]);
        assert!(error.hint.unwrap().starts_with("Existing node IDs: sky"));
    }
}
