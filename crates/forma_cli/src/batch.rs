//! `batch`: run many edit commands against one project in a single call. Each
//! step works on a private copy; only if every step succeeds is the result
//! validated and saved, so a failing step never leaves a half-edited file.
use crate::assist::locate;
use crate::{failure, load, save_project, Cli, Result};
use clap::Parser;
use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;

/// Commands that create a project from nothing (allowed as the first step).
const CREATORS: &[&str] = &["new", "rig-demo", "water-drop", "random-figures", "example"];
/// Commands that make no sense inside a batch.
const FORBIDDEN: &[&str] = &[
    "batch",
    "guide",
    "schema",
    "capabilities",
    "plugin",
    "convert",
    "import-svg",
];
/// Commands whose --output is a separate file the step must name itself.
const OWN_OUTPUT: &[&str] = &["export", "render"];

/// Shell-like splitting: whitespace, 'single' and "double" quotes, backslash escapes.
pub fn split(line: &str) -> std::result::Result<Vec<String>, String> {
    let mut words = vec![];
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => word.push(c),
                        None => return Err("unclosed ' quote".into()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(c) => word.push(c),
                            None => return Err("dangling backslash".into()),
                        },
                        Some(c) => word.push(c),
                        None => return Err("unclosed \" quote".into()),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(c) = chars.next() {
                    word.push(c);
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                word.push(c);
            }
        }
    }
    if in_word {
        words.push(word);
    }
    Ok(words)
}

/// Steps from text: a JSON array (of strings or argument arrays) or lines.
fn parse_steps(text: &str) -> Result<Vec<(String, Vec<String>)>> {
    let bad = |e: String| failure("invalid_arguments", e);
    let mut steps = vec![];
    if text.trim_start().starts_with('[') {
        let value: Value = serde_json::from_str(text).map_err(|e| failure("invalid_json", e))?;
        for item in value.as_array().into_iter().flatten() {
            match item {
                Value::String(line) => steps.push((line.clone(), split(line).map_err(bad)?)),
                Value::Array(args) => {
                    let args: Vec<String> = args
                        .iter()
                        .map(|a| match a {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect();
                    steps.push((args.join(" "), args));
                }
                _ => {
                    return Err(bad(
                        "Each JSON step must be a string or an array of strings".into(),
                    ))
                }
            }
        }
    } else {
        let mut pending = String::new();
        for line in text.lines() {
            let line = line.trim();
            if let Some(start) = line.strip_suffix('\\') {
                pending.push_str(start);
                pending.push(' ');
                continue;
            }
            pending.push_str(line);
            let full = std::mem::take(&mut pending);
            let full = full.trim();
            if full.is_empty() || full.starts_with('#') {
                continue;
            }
            steps.push((full.to_owned(), split(full).map_err(bad)?));
        }
    }
    for (_, args) in &mut steps {
        if args
            .first()
            .is_some_and(|a| a == "forma-cli" || a.ends_with("/forma-cli"))
        {
            args.remove(0);
        }
    }
    steps.retain(|(_, args)| !args.is_empty());
    if steps.is_empty() {
        return Err(bad("No steps given".into()));
    }
    Ok(steps)
}

pub fn run(input: &str, steps_path: &str, output: Option<&str>) -> Result<Value> {
    if input == "-" {
        return Err(failure(
            "invalid_arguments",
            "batch needs a project file path, not -",
        ));
    }
    let output = output.unwrap_or(input);
    let mut text = String::new();
    let reader: Box<dyn Read> = if steps_path == "-" {
        Box::new(std::io::stdin())
    } else {
        Box::new(
            std::fs::File::open(steps_path)
                .map_err(|e| failure("read_error", format!("{steps_path}: {e}")))?,
        )
    };
    reader
        .take(4 * 1024 * 1024)
        .read_to_string(&mut text)
        .map_err(|e| failure("read_error", format!("{steps_path}: {e}")))?;
    let steps = parse_steps(&text)?;
    let dir = Path::new(output)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let work = tempfile::Builder::new()
        .prefix(".forma-batch-")
        .suffix(".frm")
        .tempfile_in(dir)
        .map_err(|e| failure("write_error", format!("{}: {e}", dir.display())))?
        .into_temp_path();
    let work_path = work.to_string_lossy().into_owned();
    let starts_new = steps
        .first()
        .and_then(|(_, a)| a.first())
        .is_some_and(|c| CREATORS.contains(&c.as_str()));
    if !starts_new {
        let bytes = std::fs::read(input).map_err(|e| {
            failure("read_error", format!("{input}: {e}"))
                .hint("Start the steps with `new --width W --height H` to create the project in the batch.")
        })?;
        std::fs::write(&work_path, bytes).map_err(|e| failure("write_error", e))?;
    }
    let mut results = vec![];
    for (n, (line, args)) in steps.into_iter().enumerate() {
        let step = n + 1;
        let fail = |e: crate::Failure| {
            let hint = e
                .hint
                .clone()
                .or_else(|| crate::assist::default_hint(e.code));
            let mut f = failure(
                e.code,
                format!("Step {step} `{line}` failed: {}", e.message),
            );
            f.hint = Some(format!(
                "{}Nothing was saved; fix this step and rerun the whole batch.",
                hint.map(|h| format!("{} ", h.trim_end_matches('.').to_owned() + "."))
                    .unwrap_or_default()
            ));
            f
        };
        let found = locate(&args).ok_or_else(|| {
            fail(failure("invalid_arguments", format!("Unknown command {:?}", args[0])).hint("Steps start with a command such as `node add`, `keyframe`, `arrange move`, `clip update` or `on`."))
        })?;
        let command = found.path[0].as_str();
        if FORBIDDEN.contains(&command) {
            return Err(fail(failure(
                "invalid_arguments",
                format!("`{command}` cannot run inside batch"),
            )));
        }
        if args.iter().any(|a| a == "-") {
            return Err(fail(failure(
                "invalid_arguments",
                "Steps cannot read stdin (-)",
            )));
        }
        let creator = CREATORS.contains(&command);
        if creator && n > 0 {
            return Err(fail(failure(
                "invalid_arguments",
                format!("`{command}` is only allowed as the first step"),
            )));
        }
        let mut args = args;
        let own_output = OWN_OUTPUT.contains(&command);
        if found.has_output && !own_output {
            return Err(fail(failure("invalid_arguments", "Omit --output in batch steps").hint("The batch saves the result itself (to --output of the batch, default: the input file).")));
        }
        if found.takes_input {
            // Forgive steps that repeat the project path as their input.
            if let Some(i) = found.input {
                let given = &args[i];
                if given == input
                    || given == output
                    || given.ends_with(".frm")
                    || given.ends_with(".json")
                {
                    args.remove(i);
                }
            }
            args.insert(found.path.len(), work_path.clone());
        }
        if (found.takes_output && !own_output) || creator {
            args.push("--output".into());
            args.push(work_path.clone());
        }
        let cli = Cli::try_parse_from(
            std::iter::once("forma-cli".to_owned()).chain(args.iter().cloned()),
        )
        .map_err(|e| {
            let os: Vec<std::ffi::OsString> = std::iter::once("forma-cli".into())
                .chain(args.iter().map(Into::into))
                .collect();
            fail(crate::assist::argument_failure(&os, &e))
        })?;
        let mut result = crate::run(cli).map_err(fail)?;
        if let Some(object) = result.as_object_mut() {
            object.remove("output");
            object.remove("format");
            object.remove("ok");
        }
        let text = serde_json::to_string(&result)
            .unwrap_or_default()
            .replace(&work_path, output);
        let result: Value = serde_json::from_str(&text).unwrap_or(result);
        results.push(json!({"step": step, "command": found.path.join(" "), "result": result}));
    }
    let project = load(&work_path)?;
    let mut saved = save_project(&project, output)?;
    saved["steps"] = json!(results.len());
    saved["results"] = json!(results);
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitting_handles_quotes_and_escapes() {
        assert_eq!(
            split(r#"node add --text "Hello world" --fill '#ff0000' a\ b"#).unwrap(),
            [
                "node",
                "add",
                "--text",
                "Hello world",
                "--fill",
                "#ff0000",
                "a b"
            ]
        );
        assert!(split("x 'open").is_err());
        let steps =
            parse_steps("# comment\n\nnode add --kind rect \\\n --id a\nforma-cli node list\n")
                .unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].1, ["node", "add", "--kind", "rect", "--id", "a"]);
        assert_eq!(steps[1].1, ["node", "list"]);
        let steps = parse_steps(r#"[["node","add","--kind","circle"], "clip list"]"#).unwrap();
        assert_eq!(steps[1].1, ["clip", "list"]);
    }
}
