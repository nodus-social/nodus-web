//! Loading a browser bundle once, and calling into the global it publishes.
//!
//! The bundles under `public/vendor` are fetched, so a component that mounts before the
//! script has executed would call something that does not exist yet. `load_script` waits for
//! the `<script onload>` and shares one promise per URL, which makes a second mount cheap and
//! keeps a failed fetch retryable.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::collections::HashMap;

use wasm_bindgen::JsCast as _;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;

thread_local! {
    static SCRIPTS: RefCell<HashMap<String, js_sys::Promise>> = RefCell::new(HashMap::new());
}

type LoadClosures = (Closure<dyn FnMut()>, Closure<dyn FnMut()>);

/// Loads `url` once and resolves when the global it defines exists.
pub fn load_script(url: &str, global: &str) -> js_sys::Promise {
    let Some(window) = web_sys::window() else {
        return js_sys::Promise::resolve(&JsValue::TRUE);
    };
    let loaded = js_sys::Reflect::get(&window, &global.into())
        .is_ok_and(|value| !value.is_undefined());
    if loaded {
        return js_sys::Promise::resolve(&JsValue::TRUE);
    }
    if let Some(in_flight) = SCRIPTS.with(|seen| seen.borrow().get(url).cloned()) {
        return in_flight;
    }

    let Some(document) = window.document() else {
        return js_sys::Promise::resolve(&JsValue::TRUE);
    };
    let Ok(script) = document.create_element("script") else {
        return js_sys::Promise::resolve(&JsValue::TRUE);
    };
    script.set_attribute("src", url).ok();
    script.set_attribute("defer", "defer").ok();
    let html_script: web_sys::HtmlElement = script.clone().unchecked_into();
    let mut closures: Option<LoadClosures> = None;

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve = resolve.clone();
        let reject = reject.clone();
        let on_load = Closure::once(move || {
            let _ = resolve.call0(&JsValue::UNDEFINED);
        });
        let on_error = Closure::once(move || {
            let _ = reject.call1(
                &JsValue::UNDEFINED,
                &JsValue::from_str("the bundled script failed to load"),
            );
        });
        html_script.set_onload(Some(on_load.as_ref().unchecked_ref()));
        html_script.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        closures = Some((on_load, on_error));
    });
    SCRIPTS.with(|seen| seen.borrow_mut().insert(url.to_owned(), promise.clone()));
    if let Some(head) = document.head() {
        let _ = head.append_child(&script);
    }

    // Evict a failed URL so a later attempt refetches, and drop the closures from outside
    // their own JS trampoline.
    let settled = promise.clone();
    let url_owned = url.to_owned();
    leptos::task::spawn_local(async move {
        if wasm_bindgen_futures::JsFuture::from(settled).await.is_err() {
            SCRIPTS.with(|seen| {
                seen.borrow_mut().remove(&url_owned);
            });
        }
        html_script.set_onload(None);
        html_script.set_onerror(None);
        drop(closures);
    });
    promise
}

/// Walks a dotted path from `window`, so a bundle's export is one lookup.
pub fn global(path: &str) -> Option<js_sys::Object> {
    let window = web_sys::window()?;
    let mut value: JsValue = window.into();
    for part in path.split('.') {
        value = js_sys::Reflect::get(&value, &part.into()).ok()?;
    }
    value.dyn_into::<js_sys::Object>().ok()
}

/// Calls `method` on a JS object; `None` when it is missing or threw.
pub fn call_method(object: &js_sys::Object, method: &str, arguments: &[JsValue]) -> Option<JsValue> {
    let function: js_sys::Function = js_sys::Reflect::get(object, &method.into())
        .ok()?
        .dyn_into()
        .ok()?;
    let args = js_sys::Array::new();
    for argument in arguments {
        args.push(argument);
    }
    function.apply(object, &args).ok()
}

/// One animation frame, so a mount can wait for the node to enter the document.
pub async fn next_frame() {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let Some(window) = web_sys::window() else {
            return;
        };
        let _ = window.request_animation_frame(&resolve);
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}
