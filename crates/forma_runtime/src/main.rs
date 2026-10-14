use forma_runtime::{Request, Runtime};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};

const MAX_LINE: usize = 64 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Load {
    #[serde(rename = "type")]
    kind: String,
    project: Box<serde_json::value::RawValue>,
}

fn dispatch(runtime: &mut Option<Runtime>, line: &str) -> Result<Value, String> {
    let value: Value = forma_document::from_json_strict(line).map_err(|e| e.to_string())?;
    if value.get("type").and_then(Value::as_str) == Some("load") {
        let load: Load = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if load.kind != "load" {
            return Err("invalid load request".into());
        }
        let mut next = Runtime::from_json(load.project.get()).map_err(|e| e.to_string())?;
        let response = next.execute(Request::Evaluate).map_err(|e| e.to_string())?;
        *runtime = Some(next);
        Ok(json!(response))
    } else {
        let request = forma_document::from_json_strict(line).map_err(|e| e.to_string())?;
        let response = runtime
            .as_mut()
            .ok_or("load a project first")?
            .execute(request)
            .map_err(|e| e.to_string())?;
        Ok(json!(response))
    }
}
// Discard overlong lines without allocating their full size; keep framing intact.
fn line(reader: &mut impl BufRead) -> io::Result<Option<Result<Vec<u8>, String>>> {
    let mut bytes = Vec::new();
    let mut too_long = false;
    let mut seen = false;
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return if seen {
                Ok(Some(if too_long {
                    Err("JSON line too large".into())
                } else {
                    Ok(bytes)
                }))
            } else {
                Ok(None)
            };
        }
        seen = true;
        let count = buffer
            .iter()
            .position(|b| *b == b'\n')
            .map_or(buffer.len(), |i| i + 1);
        let complete = buffer[count - 1] == b'\n';
        if !too_long && bytes.len() + count <= MAX_LINE {
            bytes.extend_from_slice(&buffer[..count]);
        } else {
            too_long = true;
            bytes.clear();
        }
        reader.consume(count);
        if complete {
            return Ok(Some(if too_long {
                Err("JSON line too large".into())
            } else {
                Ok(bytes)
            }));
        }
    }
}
fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut output = io::BufWriter::new(io::stdout().lock());
    let mut runtime = None;
    while let Some(line) = line(&mut input)? {
        let result = catch_unwind(AssertUnwindSafe(|| {
            let bytes = line?;
            let text =
                std::str::from_utf8(&bytes).map_err(|_| "JSON input is not UTF-8".to_string())?;
            dispatch(&mut runtime, text)
        }))
        .unwrap_or_else(|_| Err("runtime panic".into()));
        let response = match result {
            Ok(result) => json!({"ok":true,"result":result}),
            Err(error) => json!({"ok":false,"error":error}),
        };
        writeln!(output, "{response}")?;
        output.flush()?;
    }
    Ok(())
}
