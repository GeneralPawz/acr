//! The native host of the Svelte UI on Android (docs/webui.md "Native host (Android, #479)").
//!
//! A transparent `WebView` in a `PopupWindow` over the `NativeActivity`'s surface, which the spike
//! (#478) measured: the activity's own views are never drawn (`NativeActivity` takes the surface),
//! a popup is a window of its own above it. The Java side is `tools/android/java/.../WebHost.java`
//! (a small dex in the APK, `hasCode=true`); this module is the Rust half:
//!
//! - The class is loaded through the activity's class loader (a native thread's `FindClass` cannot
//!   see app classes) and its natives are registered with `RegisterNatives` (`NativeActivity`
//!   loads the library outside the class loader's lookup, so `Java_..._name` lookups fail).
//! - The page is served from `https://acr.local/` through `WebViewClient.shouldInterceptRequest`
//!   (not `WebViewAssetLoader`: that is an androidx library, which the APK does not carry, and its
//!   handlers answer on the loader's thread while the assets are made by the game's thread). Java
//!   asks [`native_request`]; a bundle file is answered at once, an asset goes out as
//!   `Event::UiAsset` like the Linux host's and is answered by the game's thread.
//! - Page to Rust: `@JavascriptInterface` `acr.post(json)` becomes `Event::Ui`, the browser's
//!   `acr_ui_send`. Rust to page: `evaluateJavascript` of `window.__acr_ui.recv(...)`, on the UI
//!   thread (Java hops).
//! - Touch: the page publishes its interactive rectangles; Java hands every other touch to
//!   [`native_touch`], which becomes `Event::UiTouch` and takes the window touches' path.

use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

use jni::objects::{JClass, JObject, JString, JValue};
use jni::refs::Global;
use jni::{EnvUnowned, JavaVM, NativeMethod, jni_sig, jni_str};
use winit::event_loop::EventLoopProxy;
use winit::platform::android::activity::AndroidApp;

use super::{AssetBody, AssetError, NativeHost, Sender, bundle, touch_phase};
use crate::Event;

/// The Java class (`tools/android/java`).
const CLASS: &str = "io.github.generalpawz.acr.WebHost";
/// Where the page is served from; the Java side intercepts every request to this origin.
const PAGE: &str = "https://acr.local/android.html";
/// The path prefix of the assets (`ui/src/lib/bridge.ts` `nativeBridge`).
const ASSET_PREFIX: &str = "/asset/v1/";

static VM: OnceLock<JavaVM> = OnceLock::new();
static CLASS_REF: OnceLock<Global<JClass<'static>>> = OnceLock::new();
static PROXY: Mutex<Option<EventLoopProxy<Event>>> = Mutex::new(None);

/// The WebView over the activity.
pub struct Host;

impl Host {
    /// Creates the WebView and its popup (on the UI thread, asynchronously); the page loads and
    /// announces itself with `ready` over `acr.post`. Once per process.
    pub fn new(app: &AndroidApp, proxy: EventLoopProxy<Event>) -> Result<Self, String> {
        if VM.get().is_some() {
            return Err("the WebView host exists already".into());
        }
        *PROXY.lock().map_err(|e| e.to_string())? = Some(proxy);
        // SAFETY: the pointer comes from the running activity.
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
        let activity_ptr = app.activity_as_ptr();
        vm.attach_current_thread(|env| -> Result<(), jni::errors::Error> {
            // SAFETY: the pointer comes from the running activity.
            let activity = unsafe { JObject::from_raw(env, activity_ptr.cast()) };
            let loader = env
                .call_method(&activity, jni_str!("getClassLoader"), jni_sig!(() -> java.lang.ClassLoader), &[])?
                .l()?;
            let name = env.new_string(CLASS)?;
            let class = env
                .call_method(
                    &loader,
                    jni_str!("loadClass"),
                    jni_sig!((java.lang.String) -> java.lang.Class),
                    &[JValue::Object(&name)],
                )?
                .l()?;
            let class = env.cast_local::<JClass>(class)?;
            // SAFETY: the signatures of the functions match the descriptors.
            let methods = unsafe {
                [
                    NativeMethod::from_raw_parts(
                        jni_str!("nativeMessage"),
                        jni_str!("(Ljava/lang/String;)V"),
                        native_message as *mut c_void,
                    ),
                    NativeMethod::from_raw_parts(
                        jni_str!("nativeTouch"),
                        jni_str!("(IIFF)V"),
                        native_touch as *mut c_void,
                    ),
                    NativeMethod::from_raw_parts(
                        jni_str!("nativeRequest"),
                        jni_str!("(Ljava/lang/String;I)V"),
                        native_request as *mut c_void,
                    ),
                ]
            };
            // SAFETY: as above.
            unsafe { env.register_native_methods(&class, &methods)? };
            let global = env.new_global_ref(&class)?;
            let _ = CLASS_REF.set(global);
            let url = env.new_string(PAGE)?;
            env.call_static_method(
                &class,
                jni_str!("attach"),
                jni_sig!((android.app.Activity, java.lang.String) -> void),
                &[JValue::Object(&activity), JValue::Object(&url)],
            )?;
            Ok(())
        })
        .map_err(|e| format!("WebView host: {e}"))?;
        VM.set(vm).map_err(|_| "the WebView host exists already".to_string())?;
        Ok(Self)
    }
}

impl NativeHost for Host {
    /// The way to the page: one `evaluateJavascript` per message.
    fn sender(&self) -> Sender {
        Box::new(|json| {
            // The JSON text goes in as a JS string literal; the page parses it.
            let literal = serde_json::to_string(json).expect("a string serializes");
            let script = format!("window.__acr_ui&&window.__acr_ui.recv({literal})");
            let sent = with_class(|env, class| {
                let script = env.new_string(&script)?;
                env.call_static_method(
                    class,
                    jni_str!("eval"),
                    jni_sig!((java.lang.String) -> void),
                    &[JValue::Object(&script)],
                )?;
                Ok(())
            });
            if let Err(e) = sent {
                log::warn!("webui: evaluateJavascript failed: {e}");
            }
        })
    }

    /// The popup is transparent and stays over the game (the page publishes the rectangles that
    /// are its own, everything else is the game's): there is nothing to show or hide.
    fn visible(&self) -> bool {
        true
    }

    fn show(&mut self, _show: bool) {}

    /// Pauses the page's timers and rendering while the app is away (`WebHost.setActive`).
    fn set_active(&mut self, active: bool) {
        let done = with_class(|env, class| {
            env.call_static_method(class, jni_str!("setActive"), jni_sig!((boolean) -> void), &[JValue::Bool(active)])?;
            Ok(())
        });
        if let Err(e) = done {
            log::warn!("webui: cannot set the WebView active={active}: {e}");
        }
    }
}

/// Runs `f` with a JNI env attached to this thread and the `WebHost` class.
fn with_class<R>(
    f: impl FnOnce(&mut jni::Env, &JClass) -> Result<R, jni::errors::Error>,
) -> Result<R, jni::errors::Error> {
    let (Some(vm), Some(class)) = (VM.get(), CLASS_REF.get()) else {
        return Err(jni::errors::Error::JniCall(jni::errors::JniError::InvalidArguments));
    };
    vm.attach_current_thread(|env| f(env, class))
}

fn post(event: Event) {
    let sent = match PROXY.lock() {
        Ok(proxy) => proxy.as_ref().map(|p| p.send_event(event).is_ok()),
        Err(_) => None,
    };
    if sent != Some(true) {
        // The event loop is gone: the game is closing.
        log::debug!("webui: an event from the WebView was dropped");
    }
}

/// `WebHost.nativeMessage(String)`: a page message, on a WebView binder thread.
extern "system" fn native_message<'c>(mut env: EnvUnowned<'c>, _class: JClass<'c>, json: JString<'c>) {
    let outcome = env.with_env(|env| -> Result<(), jni::errors::Error> {
        post(Event::Ui(json.try_to_string(env)?));
        Ok(())
    });
    outcome.resolve::<jni::errors::ThrowRuntimeExAndDefault>();
}

/// `WebHost.nativeTouch(int, int, float, float)`: a touch the page did not claim; `phase` is
/// 0 started, 1 moved, 2 ended, 3 cancelled (see [`touch_phase`]).
extern "system" fn native_touch<'c>(_env: EnvUnowned<'c>, _class: JClass<'c>, phase: i32, id: i32, x: f32, y: f32) {
    match (touch_phase(phase), u64::try_from(id)) {
        (Some(phase), Ok(id)) => post(Event::UiTouch { id, phase, x: f64::from(x), y: f64::from(y) }),
        _ => log::warn!("webui: bad touch from the WebView: phase {phase} id {id}"),
    }
}

/// `WebHost.nativeRequest(String, int)`: the page asks for `path` of the origin; the answer goes
/// back with `WebHost.reply(id, ...)`.
extern "system" fn native_request<'c>(mut env: EnvUnowned<'c>, _class: JClass<'c>, path: JString<'c>, id: i32) {
    let outcome = env.with_env(|env| -> Result<(), jni::errors::Error> {
        serve(path.try_to_string(env)?, id);
        Ok(())
    });
    outcome.resolve::<jni::errors::ThrowRuntimeExAndDefault>();
}

fn serve(path: String, id: i32) {
    if let Some(asset) = path.strip_prefix(ASSET_PREFIX) {
        let reply: super::AssetReply = Box::new(move |result| match result {
            Ok(body) => {
                let mime = body.mime();
                let bytes = match body {
                    AssetBody::Png(bytes) => bytes,
                    AssetBody::Json(text) => text.into_bytes(),
                };
                deliver(id, 200, mime, &bytes, None);
            }
            Err(e) => deliver(id, e.status(), "text/plain", &[], Some(&e.to_string())),
        });
        let sent = match PROXY.lock() {
            Ok(proxy) => proxy.as_ref().map(|p| p.send_event(Event::UiAsset { path: asset.to_owned(), reply })),
            Err(_) => None,
        };
        match sent {
            Some(Ok(())) => {}
            // The event loop is gone: the game is closing.
            Some(Err(closed)) => {
                if let Event::UiAsset { reply, .. } = closed.0 {
                    reply(Err(AssetError::NotFound("the game is closing".into())));
                }
            }
            None => deliver(id, 503, "text/plain", &[], Some("the game is not ready")),
        }
        return;
    }
    let name = match path.trim_start_matches('/') {
        "" => "index.html",
        other => other,
    };
    match bundle::file(name) {
        Some(bytes) => deliver(id, 200, bundle::mime(name), bytes, None),
        None => deliver(id, 404, "text/plain", &[], Some(&format!("not in the bundle: {name}"))),
    }
}

/// Hands the answer for request `id` to the waiting Java thread.
fn deliver(id: i32, status: u16, mime: &str, body: &[u8], error: Option<&str>) {
    let sent = with_class(|env, class| {
        let mime = env.new_string(mime)?;
        let body = env.byte_array_from_slice(body)?;
        let error = env.new_string(error.map(super::header_text).unwrap_or_default())?;
        env.call_static_method(
            class,
            jni_str!("reply"),
            jni_sig!("(IILjava/lang/String;[BLjava/lang/String;)V"),
            &[
                JValue::Int(id),
                JValue::Int(i32::from(status)),
                JValue::Object(&mime),
                JValue::Object(&body),
                JValue::Object(&error),
            ],
        )?;
        Ok(())
    });
    if let Err(e) = sent {
        log::warn!("webui: cannot answer the WebView's request {id}: {e}");
    }
}
