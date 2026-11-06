//! Browser entry points (wasm32-unknown-emscripten; native/web/host.js calls them through cwrap).
//! Every call returns a NUL-terminated JSON string owned by the module until the next call:
//! `{"ok": <result>}` or `{"error": "<message>"}`.
use std::{
    cell::RefCell,
    ffi::{CStr, CString, c_char},
};

use serde_json::{Value, json};

thread_local! {
    static RESULT: RefCell<CString> = RefCell::new(CString::default());
}

fn reply(result: anyhow::Result<Value>) -> *const c_char {
    let text = match result {
        Ok(value) => json!({ "ok": value }).to_string(),
        Err(error) => json!({ "error": format!("{error:#}") }).to_string(),
    };
    RESULT.with(|slot| {
        *slot.borrow_mut() = CString::new(text).unwrap_or_default();
        slot.borrow().as_ptr()
    })
}

fn text(pointer: *const c_char) -> String {
    if pointer.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned()
}

fn guarded(run: impl FnOnce() -> anyhow::Result<Value>) -> *const c_char {
    // A panic must not leave the page with a dead module and no message
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)) {
        Ok(result) => reply(result),
        Err(panic) => {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "panic".into());
            reply(Err(anyhow::anyhow!("panic: {message}")))
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rur_web_init(config: *const c_char) -> *const c_char {
    let config = text(config);
    guarded(|| rur_native::web::init(&config))
}

#[unsafe(no_mangle)]
pub extern "C" fn rur_web_frame(dt: f64, events: *const c_char, force: i32) -> *const c_char {
    let events = text(events);
    guarded(|| rur_native::web::frame(dt, &events, force != 0))
}

#[unsafe(no_mangle)]
pub extern "C" fn rur_web_gpu() -> *const c_char {
    guarded(rur_native::web::prepare_gpu)
}

#[unsafe(no_mangle)]
pub extern "C" fn rur_web_drop() -> *const c_char {
    guarded(|| {
        rur_native::web::drop_story();
        Ok(Value::Null)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rur_web_scene() -> *const c_char {
    guarded(|| Ok(Value::String(rur_native::web::scene_json()?)))
}

fn main() {
    // The page calls rur_web_init once its bundle is in the file system
    eprintln!("rur-web ready");
}
