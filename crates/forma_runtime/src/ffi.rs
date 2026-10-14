use crate::{Response, Runtime};
use serde_json::json;
use std::ffi::{c_char, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

pub const MAX_JSON_BYTES: usize = 64 * 1024 * 1024;

pub struct FormaRuntimeHandle {
    runtime: Runtime,
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

/// See include/forma_runtime.h for the caller's memory-validity contract.
///
/// # Safety
/// `out` must be a writable pointer slot, and `data` must reference `length` readable
/// bytes for this call. These regions must not overlap. Null pointers are diagnosed.
#[no_mangle]
pub unsafe extern "C" fn forma_runtime_create(
    data: *const u8,
    length: usize,
    out: *mut *mut FormaRuntimeHandle,
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
        *out = Box::into_raw(Box::new(FormaRuntimeHandle { runtime }));
        Ok(result)
    })
}

/// # Safety
/// `handle` must be a live handle returned by create, exclusively accessed for this
/// call. `data` must reference `length` readable bytes. Null pointers are diagnosed.
#[no_mangle]
pub unsafe extern "C" fn forma_runtime_request(
    handle: *mut FormaRuntimeHandle,
    data: *const u8,
    length: usize,
) -> *mut c_char {
    guarded(|| {
        if handle.is_null() {
            return Err("null runtime handle".into());
        }
        let request =
            forma_document::from_json_strict(input(data, length)?).map_err(|e| e.to_string())?;
        (*handle)
            .runtime
            .execute(request)
            .map_err(|e| e.to_string())
    })
}

/// # Safety
/// Non-null `handle` must be a live handle returned by create, destroyed exactly once
/// with no concurrent operations. Null is a no-op.
#[no_mangle]
pub unsafe extern "C" fn forma_runtime_destroy(handle: *mut FormaRuntimeHandle) {
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
pub unsafe extern "C" fn forma_runtime_result_free(result: *mut c_char) {
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
        forma_runtime_result_free(ptr);
        value
    }
    #[test]
    fn ffi_guards_and_lifetime() {
        unsafe {
            let mut handle = std::ptr::null_mut();
            assert_eq!(
                take(forma_runtime_create(std::ptr::null(), 0, &mut handle))["ok"],
                false
            );
            assert!(handle.is_null());
            assert_eq!(
                take(forma_runtime_create(
                    b"{}".as_ptr(),
                    2,
                    std::ptr::null_mut()
                ))["ok"],
                false
            );
            assert_eq!(
                take(forma_runtime_create([255].as_ptr(), 1, &mut handle))["ok"],
                false
            );
            assert_eq!(
                take(forma_runtime_create(b"{}".as_ptr(), 2, &mut handle))["ok"],
                true
            );
            assert!(!handle.is_null());
            let req = br#"{"type":"tick","delta":0.5}"#;
            assert_eq!(
                take(forma_runtime_request(handle, req.as_ptr(), req.len()))["result"]["frame"]
                    ["time"],
                0.5
            );
            assert_eq!(
                take(forma_runtime_request(handle, [255].as_ptr(), 1))["ok"],
                false
            );
            assert_eq!(
                take(forma_runtime_request(
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
            forma_runtime_destroy(handle);
            forma_runtime_destroy(std::ptr::null_mut());
            forma_runtime_result_free(std::ptr::null_mut());
        }
    }
}
