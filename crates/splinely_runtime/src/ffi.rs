use crate::{Response, Runtime};
use serde::Serialize;
use serde_json::json;
use std::ffi::{c_char, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

pub const MAX_JSON_BYTES: usize = 64 * 1024 * 1024;

pub struct SplinelyRuntimeHandle {
    runtime: Runtime,
    /// Reused response buffer of the `_into` entry points (the last response).
    response: Vec<u8>,
}

/// `splinely_runtime_request_into`/`tick_into`/`response_copy` status codes.
pub const STATUS_OK: i32 = 0;
/// The response was produced and retained by the handle but did not fit.
pub const STATUS_BUFFER_TOO_SMALL: i32 = 1;
pub const STATUS_INVALID_ARGUMENT: i32 = 2;

#[derive(Serialize)]
#[serde(untagged)]
enum Envelope<'a> {
    Ok { ok: bool, result: &'a Response },
    Err { ok: bool, error: &'a str },
}

fn encode(result: std::result::Result<Response, String>) -> *mut c_char {
    let value = match result {
        Ok(result) => json!({"ok": true, "result": result}),
        Err(error) => json!({"ok": false, "error": error}),
    };
    // JSON serialization escapes interior NUL characters.
    CString::new(value.to_string())
        .expect("serialized JSON has no NUL")
        .into_raw()
}
fn guarded(f: impl FnOnce() -> std::result::Result<Response, String>) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| encode(f())))
        .unwrap_or_else(|_| encode(Err("runtime panic".into())))
}
unsafe fn input<'a>(data: *const u8, length: usize) -> std::result::Result<&'a str, String> {
    if data.is_null() {
        return Err("null JSON pointer".into());
    }
    if length > MAX_JSON_BYTES || length > isize::MAX as usize {
        return Err("JSON input too large".into());
    }
    std::str::from_utf8(std::slice::from_raw_parts(data, length))
        .map_err(|_| "JSON input is not UTF-8".into())
}

/// See include/splinely_runtime.h for the caller's memory-validity contract.
///
/// # Safety
/// `out` must be a writable pointer slot, and `data` must reference `length` readable
/// bytes for this call. These regions must not overlap. Null pointers are diagnosed.
#[no_mangle]
pub unsafe extern "C" fn splinely_runtime_create(
    data: *const u8,
    length: usize,
    out: *mut *mut SplinelyRuntimeHandle,
) -> *mut c_char {
    guarded(|| {
        if out.is_null() {
            return Err("null handle output pointer".into());
        }
        *out = std::ptr::null_mut();
        let mut runtime = Runtime::from_json(input(data, length)?).map_err(|e| e.to_string())?;
        let result = runtime
            .execute(crate::Request::Evaluate)
            .map_err(|e| e.to_string())?;
        *out = Box::into_raw(Box::new(SplinelyRuntimeHandle {
            runtime,
            response: Vec::new(),
        }));
        Ok(result)
    })
}

/// # Safety
/// `handle` must be a live handle returned by create, exclusively accessed for this
/// call. `data` must reference `length` readable bytes. Null pointers are diagnosed.
#[no_mangle]
pub unsafe extern "C" fn splinely_runtime_request(
    handle: *mut SplinelyRuntimeHandle,
    data: *const u8,
    length: usize,
) -> *mut c_char {
    guarded(|| {
        if handle.is_null() {
            return Err("null runtime handle".into());
        }
        let request =
            splinely_document::from_json_strict(input(data, length)?).map_err(|e| e.to_string())?;
        (*handle)
            .runtime
            .execute(request)
            .map_err(|e| e.to_string())
    })
}

/// Run `f`, store its JSON envelope in the handle buffer and copy it out.
///
/// # Safety
/// Same contract as `splinely_runtime_request_into`.
unsafe fn run_into(
    handle: *mut SplinelyRuntimeHandle,
    out: *mut u8,
    capacity: usize,
    out_length: *mut usize,
    f: impl FnOnce(&mut Runtime) -> std::result::Result<Response, String>,
) -> i32 {
    if handle.is_null() || out_length.is_null() || (out.is_null() && capacity != 0) {
        return STATUS_INVALID_ARGUMENT;
    }
    let handle = &mut *handle;
    let result = catch_unwind(AssertUnwindSafe(|| f(&mut handle.runtime)))
        .unwrap_or_else(|_| Err("runtime panic".into()));
    handle.response.clear();
    let written = match &result {
        Ok(result) => {
            serde_json::to_writer(&mut handle.response, &Envelope::Ok { ok: true, result })
        }
        Err(error) => {
            serde_json::to_writer(&mut handle.response, &Envelope::Err { ok: false, error })
        }
    };
    if let Err(error) = written {
        handle.response.clear();
        let message = format!("response serialization failed: {error}");
        let _ = serde_json::to_writer(
            &mut handle.response,
            &Envelope::Err {
                ok: false,
                error: &message,
            },
        );
    }
    copy_response(handle, out, capacity, out_length)
}
unsafe fn copy_response(
    handle: &SplinelyRuntimeHandle,
    out: *mut u8,
    capacity: usize,
    out_length: *mut usize,
) -> i32 {
    *out_length = handle.response.len();
    if handle.response.len() > capacity {
        return STATUS_BUFFER_TOO_SMALL;
    }
    if !handle.response.is_empty() {
        std::ptr::copy_nonoverlapping(handle.response.as_ptr(), out, handle.response.len());
    }
    STATUS_OK
}

/// Like `splinely_runtime_request`, but the JSON envelope is written into a
/// caller buffer (not NUL terminated) and nothing is allocated per call. See
/// include/splinely_runtime.h.
///
/// # Safety
/// `handle` must be a live handle, exclusively accessed for this call. `data` must
/// reference `length` readable bytes. `out` must reference `capacity` writable bytes
/// (it may be null when `capacity` is 0) and `out_length` a writable `size_t`; the
/// regions must not overlap. Invalid null arguments return STATUS_INVALID_ARGUMENT.
#[no_mangle]
pub unsafe extern "C" fn splinely_runtime_request_into(
    handle: *mut SplinelyRuntimeHandle,
    data: *const u8,
    length: usize,
    out: *mut u8,
    capacity: usize,
    out_length: *mut usize,
) -> i32 {
    let request = catch_unwind(AssertUnwindSafe(|| {
        input(data, length).and_then(|text| {
            splinely_document::from_json_strict::<crate::Request>(text).map_err(|e| e.to_string())
        })
    }))
    .unwrap_or_else(|_| Err("runtime panic".into()));
    run_into(handle, out, capacity, out_length, |runtime| {
        runtime.execute(request?).map_err(|e| e.to_string())
    })
}

/// `{"type":"tick","delta":delta}` without building or parsing the request JSON.
///
/// # Safety
/// Same contract as `splinely_runtime_request_into` (without `data`).
#[no_mangle]
pub unsafe extern "C" fn splinely_runtime_tick_into(
    handle: *mut SplinelyRuntimeHandle,
    delta: f64,
    out: *mut u8,
    capacity: usize,
    out_length: *mut usize,
) -> i32 {
    run_into(handle, out, capacity, out_length, |runtime| {
        runtime.tick(delta).map_err(|e| e.to_string())
    })
}

/// Copy the response retained by the last `_into` call again (for example into a
/// larger buffer after STATUS_BUFFER_TOO_SMALL). It is replaced by the next `_into`
/// call. Before any `_into` call it is empty (`*out_length` 0).
///
/// # Safety
/// Same buffer contract as `splinely_runtime_request_into`.
#[no_mangle]
pub unsafe extern "C" fn splinely_runtime_response_copy(
    handle: *mut SplinelyRuntimeHandle,
    out: *mut u8,
    capacity: usize,
    out_length: *mut usize,
) -> i32 {
    if handle.is_null() || out_length.is_null() || (out.is_null() && capacity != 0) {
        return STATUS_INVALID_ARGUMENT;
    }
    copy_response(&*handle, out, capacity, out_length)
}

/// # Safety
/// Non-null `handle` must be a live handle returned by create, destroyed exactly once
/// with no concurrent operations. Null is a no-op.
#[no_mangle]
pub unsafe extern "C" fn splinely_runtime_destroy(handle: *mut SplinelyRuntimeHandle) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !handle.is_null() {
            drop(Box::from_raw(handle));
        }
    }));
}

/// # Safety
/// Non-null `result` must be an unmodified allocation returned by create/request,
/// freed exactly once by this function. Null is a no-op.
#[no_mangle]
pub unsafe extern "C" fn splinely_runtime_result_free(result: *mut c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !result.is_null() {
            drop(CString::from_raw(result));
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe fn take(ptr: *mut c_char) -> serde_json::Value {
        let value = serde_json::from_str(std::ffi::CStr::from_ptr(ptr).to_str().unwrap()).unwrap();
        splinely_runtime_result_free(ptr);
        value
    }
    #[test]
    fn ffi_guards_and_lifetime() {
        unsafe {
            let mut handle = std::ptr::null_mut();
            assert_eq!(
                take(splinely_runtime_create(std::ptr::null(), 0, &mut handle))["ok"],
                false
            );
            assert!(handle.is_null());
            assert_eq!(
                take(splinely_runtime_create(
                    b"{}".as_ptr(),
                    2,
                    std::ptr::null_mut()
                ))["ok"],
                false
            );
            assert_eq!(
                take(splinely_runtime_create([255].as_ptr(), 1, &mut handle))["ok"],
                false
            );
            assert_eq!(
                take(splinely_runtime_create(b"{}".as_ptr(), 2, &mut handle))["ok"],
                true
            );
            assert!(!handle.is_null());
            let req = br#"{"type":"tick","delta":0.5}"#;
            assert_eq!(
                take(splinely_runtime_request(handle, req.as_ptr(), req.len()))["result"]["frame"]
                    ["time"],
                0.5
            );
            assert_eq!(
                take(splinely_runtime_request(handle, [255].as_ptr(), 1))["ok"],
                false
            );
            assert_eq!(
                take(splinely_runtime_request(
                    std::ptr::null_mut(),
                    req.as_ptr(),
                    req.len()
                ))["ok"],
                false
            );
            assert_eq!(
                take(guarded(|| panic!("test panic")))["error"],
                "runtime panic"
            );
            into_buffer_api(handle);
            splinely_runtime_destroy(handle);
            splinely_runtime_destroy(std::ptr::null_mut());
            splinely_runtime_result_free(std::ptr::null_mut());
        }
    }

    /// The buffer API returns the same documents as the string API.
    unsafe fn into_buffer_api(handle: *mut SplinelyRuntimeHandle) {
        let mut length = 0usize;
        let mut buffer = vec![0u8; 1 << 20];
        let json = |bytes: &[u8]| serde_json::from_slice::<serde_json::Value>(bytes).unwrap();
        let expected = {
            // Same request through the string API on a clone of the state.
            let mut twin = SplinelyRuntimeHandle {
                runtime: (*handle).runtime.clone(),
                response: vec![],
            };
            take(splinely_runtime_request(
                &mut twin,
                br#"{"type":"tick","delta":0.25}"#.as_ptr(),
                28,
            ))
        };
        let status = splinely_runtime_tick_into(
            handle,
            0.25,
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut length,
        );
        assert_eq!(status, STATUS_OK);
        assert_eq!(json(&buffer[..length]), expected);
        // Too small: reports the size, retains the response, response_copy retrieves it.
        let mut tiny = [0u8; 4];
        let mut needed = 0usize;
        let req = br#"{"type":"tick","delta":0.25}"#;
        assert_eq!(
            splinely_runtime_request_into(
                handle,
                req.as_ptr(),
                req.len(),
                tiny.as_mut_ptr(),
                4,
                &mut needed
            ),
            STATUS_BUFFER_TOO_SMALL
        );
        assert!(needed > 4);
        assert_eq!(
            splinely_runtime_response_copy(handle, buffer.as_mut_ptr(), buffer.len(), &mut length),
            STATUS_OK
        );
        assert_eq!(length, needed);
        assert_eq!(json(&buffer[..length])["result"]["frame"]["time"], 1.0);
        // Errors are envelopes with status OK; bad arguments are diagnosed.
        let bad = br#"{"type":"nope"}"#;
        assert_eq!(
            splinely_runtime_request_into(
                handle,
                bad.as_ptr(),
                bad.len(),
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut length
            ),
            STATUS_OK
        );
        assert_eq!(json(&buffer[..length])["ok"], false);
        assert_eq!(
            splinely_runtime_request_into(
                std::ptr::null_mut(),
                bad.as_ptr(),
                bad.len(),
                buffer.as_mut_ptr(),
                1,
                &mut length
            ),
            STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            splinely_runtime_tick_into(handle, 0.1, std::ptr::null_mut(), 1, &mut length),
            STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            splinely_runtime_tick_into(handle, 0.1, buffer.as_mut_ptr(), 1, std::ptr::null_mut()),
            STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            splinely_runtime_tick_into(handle, -1.0, std::ptr::null_mut(), 0, &mut length),
            STATUS_BUFFER_TOO_SMALL
        );
        assert_eq!(
            splinely_runtime_response_copy(handle, buffer.as_mut_ptr(), buffer.len(), &mut length),
            STATUS_OK
        );
        assert_eq!(json(&buffer[..length])["ok"], false);
    }
}
