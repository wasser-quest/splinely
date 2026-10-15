//! Tick timing: `cargo run --release -p splinely_runtime --example tick_bench [project ...]`
//! Without arguments it runs the bundled desert-stickfight, stickfight and demo examples.
use splinely_runtime::Runtime;
use std::time::Instant;

fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/");
    let mut paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        paths = [
            "desert-stickfight.splinely.json",
            "stickfight.splinely.json",
            "demo.splinely.json",
        ]
        .map(|name| format!("{root}{name}"))
        .into();
    }
    for path in paths {
        let Ok(source) = std::fs::read_to_string(&path) else {
            eprintln!("skip {path}: not readable as JSON text");
            continue;
        };
        let mut runtime = Runtime::from_json(&source).expect("project loads");
        let ticks = 300;
        for _ in 0..30 {
            runtime.tick(1.0 / 60.0).unwrap();
        }
        let start = Instant::now();
        for _ in 0..ticks {
            runtime.tick(1.0 / 60.0).unwrap();
        }
        let tick = start.elapsed().as_secs_f64() * 1e3 / f64::from(ticks);
        let start = Instant::now();
        for _ in 0..ticks {
            runtime.evaluate().unwrap();
        }
        let evaluate = start.elapsed().as_secs_f64() * 1e3 / f64::from(ticks);
        // Full C-ABI round trip (JSON request in, response out) per tick.
        let (string_api, buffer_api) = ffi_round_trips(&source, ticks);
        let name = path.rsplit('/').next().unwrap_or(&path);
        println!(
            "{name:<32} tick {tick:.4} ms   evaluate {evaluate:.4} ms   ffi request {string_api:.4} ms   ffi tick_into {buffer_api:.4} ms"
        );
    }
}

/// Milliseconds per tick through `splinely_runtime_request` (allocated, NUL-terminated
/// result) and through `splinely_runtime_tick_into` (reused caller buffer).
fn ffi_round_trips(source: &str, ticks: u32) -> (f64, f64) {
    use splinely_runtime::ffi::*;
    let mut handle = std::ptr::null_mut();
    let request = br#"{"type":"tick","delta":0.016666666666666666}"#;
    unsafe {
        let created = splinely_runtime_create(source.as_ptr(), source.len(), &mut handle);
        splinely_runtime_result_free(created);
        let start = Instant::now();
        for _ in 0..ticks {
            let result = splinely_runtime_request(handle, request.as_ptr(), request.len());
            splinely_runtime_result_free(result);
        }
        let string_api = start.elapsed().as_secs_f64() * 1e3 / f64::from(ticks);
        let mut buffer = vec![0u8; 1 << 16];
        let mut length = 0;
        let start = Instant::now();
        for _ in 0..ticks {
            let mut status = splinely_runtime_tick_into(
                handle,
                1.0 / 60.0,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut length,
            );
            if status == STATUS_BUFFER_TOO_SMALL {
                buffer.resize(length, 0);
                status = splinely_runtime_response_copy(
                    handle,
                    buffer.as_mut_ptr(),
                    buffer.len(),
                    &mut length,
                );
            }
            assert_eq!(status, STATUS_OK);
        }
        let buffer_api = start.elapsed().as_secs_f64() * 1e3 / f64::from(ticks);
        splinely_runtime_destroy(handle);
        (string_api, buffer_api)
    }
}
