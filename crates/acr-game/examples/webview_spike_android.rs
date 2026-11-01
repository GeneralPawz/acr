//! Spike for the native Svelte UI host on Android (tracking issue #316 section 3, #333): can a
//! transparent WebView sit over the game's wgpu (Vulkan) surface and show it through, with the
//! HUD on top? A `NativeActivity` (the game's own host) draws an animated wgpu clear; a small
//! Java class (`tools/android/webview-spike/`) puts a transparent hardware-layer WebView into a
//! `PopupWindow` over it. The page has a half-transparent panel, swatches of known alpha, a
//! button that posts to Rust (`@JavascriptInterface`) and receives Rust's `evaluateJavascript`.
//!
//! Build and install under its own package id (it never touches the game's app or data):
//!
//! ```text
//! source tools/android-env.sh
//! tools/android/build-webview-spike.sh && adb install -r target/android/WebViewSpike.apk
//! adb shell am start -n io.github.generalpawz.acr.webviewspike/android.app.NativeActivity \
//!     --es mode full --es hold 60 --es fixed 255,0,0
//! adb logcat -s spike
//! ```
//!
//! Intent extras: `mode` = `full` (full-screen popup, touches outside the page's interactive
//! regions are forwarded to the game, default), `capture` (full-screen, the popup keeps every
//! touch) or `shrink` (the popup is only the middle half of the screen); `hold` = seconds the
//! finished run keeps the window for screenshots and taps (default 60); `fixed` = `r,g,b` clear
//! colour instead of the animation (pixel checks); `webview` = `0` runs the baseline only.
//! The run prints `SPIKE <key> <value>` lines to logcat (tag `spike`).
#![cfg(target_os = "android")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use acr_app::Gpu;
use jni::NativeMethod;
use jni::objects::{JClass, JObject, JString, JValue};
use jni::{EnvUnowned, jni_sig, jni_str};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::platform::android::activity::AndroidApp;
use winit::window::{Window, WindowId};

const PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<style>
html,body{margin:0;height:100%;background:transparent;overflow:hidden;font:18px sans-serif;color:#fff}
#panel{position:fixed;left:25%;top:25%;width:50%;height:50%;background:rgba(20,20,20,.55);border:2px solid #fff;box-sizing:border-box}
#b{position:fixed;left:30%;top:35%;font-size:24px;padding:14px 28px}
.sw{position:fixed;bottom:6%;width:12%;height:8%;background:#fff}
body.shrink #panel{left:0;top:0;width:100%;height:100%}
body.shrink #b{left:10%;top:30%}
</style></head><body>
<div id="panel"></div><button id="b" data-hit>click</button>
<div class="sw" style="left:5%;opacity:1"></div>
<div class="sw" style="left:20%;opacity:.5"></div>
<div class="sw" style="left:35%;opacity:.25"></div>
<script>
const mode = "__MODE__";
if (mode === "shrink") document.body.className = "shrink";
const post = (o) => acr.post(JSON.stringify(o));
const nowUs = () => (performance.timeOrigin + performance.now()) * 1000;
const s = window.__spike = { lat: [], bytes: 0 };
s.rx = (t, p) => { s.lat.push(nowUs() - t); s.bytes += p.length; };
s.rxDone = () => { post({ k: 'rx', lat: s.lat, bytes: s.bytes }); s.lat = []; s.bytes = 0; };
s.ipc = async (n) => {
  for (let i = 0; i < n; i++) { post({ k: 'ping', t: nowUs() }); await new Promise(r => setTimeout(r, 16)); }
  post({ k: 'ipc_done' });
};
const rects = () => acr.regions([...document.querySelectorAll('[data-hit]')].map((e) => {
  const r = e.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].join(',');
}).join(';'));
document.getElementById('b').onclick = () => post({ k: 'click', t: nowUs() });
addEventListener('resize', rects);
rects();
post({ k: 'ready', ua: navigator.userAgent, dpr: devicePixelRatio, w: innerWidth, h: innerHeight });
</script></body></html>"#;

/// Messages from the page: arrival time (µs since the epoch, taken at the JNI entry) and JSON.
static INBOX: Mutex<Vec<(f64, String)>> = Mutex::new(Vec::new());
static FORWARDED: AtomicU32 = AtomicU32::new(0);
static HOST: OnceLock<jni::refs::Global<JClass<'static>>> = OnceLock::new();

fn wall_us() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64() * 1e6)
}

/// `Host.nativeMessage(String)`: a page message on a WebView binder thread.
extern "system" fn native_message<'c>(mut env: EnvUnowned<'c>, _class: JClass<'c>, json: JString<'c>) {
    let arrived = wall_us();
    let outcome = env.with_env(|env| -> Result<(), jni::errors::Error> {
        let text = json.try_to_string(env)?;
        INBOX.lock().unwrap().push((arrived, text));
        Ok(())
    });
    outcome.resolve::<jni::errors::ThrowRuntimeExAndDefault>();
}

/// `Host.nativeTouch(int, float, float)`: a touch the page did not claim.
extern "system" fn native_touch<'c>(_env: EnvUnowned<'c>, _class: JClass<'c>, action: i32, x: f32, y: f32) {
    if action == 0 {
        FORWARDED.fetch_add(1, Ordering::Relaxed);
        log::info!("SPIKE touch forwarded-to-game {x:.0} {y:.0}");
    }
}

/// Median and 99th percentile of `v` (sorted in place).
fn stats(v: &mut [f64]) -> (f64, f64) {
    if v.is_empty() {
        return (f64::NAN, f64::NAN);
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let at = |q: f64| v[(((v.len() - 1) as f64) * q).round() as usize];
    (at(0.5), at(0.99))
}

fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    /// Frames without a webview.
    Baseline,
    /// Webview attached, waiting for the page's `ready`.
    WaitReady,
    /// Page loaded, nothing happening.
    Idle,
    /// The page posts 300 pings (JS to Rust).
    IpcUp,
    /// Rust sends 300 small `evaluateJavascript` calls at about 60 Hz.
    RxSmall,
    /// The same with a 4 KB payload.
    RxBig,
    /// Finished; the window stays for screenshots and taps.
    Hold,
}

struct Spike {
    app: AndroidApp,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    start: Instant,
    mode: String,
    fixed: Option<[f64; 3]>,
    hold: Duration,
    webview: bool,
    phase: Phase,
    phase_start: Instant,
    last_frame: Option<Instant>,
    /// Frame interval and CPU time (ms) of the current phase.
    intervals: Vec<f64>,
    cpu: Vec<f64>,
    ipc_up: Vec<f64>,
    rx_sent: u32,
    rx_next: Instant,
    eval_cost: Vec<f64>,
    rx_lat: Vec<f64>,
}

impl Spike {
    fn new(app: AndroidApp) -> Self {
        let extra = |key: &str| host_extra(&app, key).unwrap_or_default();
        let mode = Some(extra("mode")).filter(|m| !m.is_empty()).unwrap_or_else(|| "full".into());
        let fixed = extra("fixed");
        let fixed = fixed.split(',').map(|n| n.trim().parse::<f64>().ok()).collect::<Option<Vec<_>>>();
        let hold = extra("hold").parse::<u64>().unwrap_or(60);
        let webview = extra("webview") != "0";
        let now = Instant::now();
        Self {
            app,
            window: None,
            gpu: None,
            start: now,
            mode,
            fixed: fixed.filter(|f| f.len() == 3).map(|f| [f[0] / 255.0, f[1] / 255.0, f[2] / 255.0]),
            hold: Duration::from_secs(hold),
            webview,
            phase: Phase::Baseline,
            phase_start: now,
            last_frame: None,
            intervals: Vec::new(),
            cpu: Vec::new(),
            ipc_up: Vec::new(),
            rx_sent: 0,
            rx_next: now,
            eval_cost: Vec::new(),
            rx_lat: Vec::new(),
        }
    }

    fn goto(&mut self, next: Phase) {
        let (mi, pi) = stats(&mut self.intervals);
        let (mc, pc) = stats(&mut self.cpu);
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
        log::info!(
            "SPIKE frames {:?} n={} interval_mean={:.2} median={mi:.2} p99={pi:.2} max={:.2} cpu_mean={:.3} median={mc:.3} p99={pc:.3} ms",
            self.phase,
            self.intervals.len(),
            mean(&self.intervals),
            self.intervals.iter().copied().fold(0.0, f64::max),
            mean(&self.cpu),
        );
        self.intervals.clear();
        self.cpu.clear();
        log::info!("SPIKE phase {next:?}");
        self.phase = next;
        self.phase_start = Instant::now();
    }

    fn inbox(&mut self) {
        let messages = std::mem::take(&mut *INBOX.lock().unwrap());
        for (arrived, text) in messages {
            let Ok(m) = serde_json::from_str::<serde_json::Value>(&text) else {
                log::error!("SPIKE bad message {text}");
                continue;
            };
            match m["k"].as_str() {
                Some("ready") => {
                    log::info!("SPIKE ready {} dpr={} css={}x{}", m["ua"], m["dpr"], m["w"], m["h"]);
                    if self.phase == Phase::WaitReady {
                        self.goto(Phase::Idle);
                    }
                }
                Some("ping") => self.ipc_up.push((arrived - m["t"].as_f64().unwrap_or(0.0)) / 1000.0),
                Some("ipc_done") => {
                    let (med, p99) = stats(&mut self.ipc_up);
                    log::info!("SPIKE ipc js->rust n={} median={med:.2} p99={p99:.2} ms", self.ipc_up.len());
                    self.goto(Phase::RxSmall);
                    self.rx_sent = 0;
                    self.rx_next = Instant::now();
                }
                Some("click") => {
                    let t = m["t"].as_f64().unwrap_or(0.0);
                    log::info!("SPIKE click (button) page-to-rust {:.2} ms", (arrived - t) / 1000.0);
                }
                Some("rx") => {
                    self.rx_lat =
                        m["lat"].as_array().map_or(Vec::new(), |a| a.iter().filter_map(|v| v.as_f64()).collect());
                    for l in &mut self.rx_lat {
                        *l /= 1000.0;
                    }
                    let (med, p99) = stats(&mut self.rx_lat);
                    let (cm, cp) = stats(&mut self.eval_cost);
                    log::info!(
                        "SPIKE ipc rust->js {:?} n={} median={med:.2} p99={p99:.2} ms; call cost median={cm:.3} p99={cp:.3} ms; {} bytes",
                        self.phase,
                        self.rx_lat.len(),
                        m["bytes"]
                    );
                    self.eval_cost.clear();
                    self.rx_sent = 0;
                    self.rx_next = Instant::now();
                    if self.phase == Phase::RxSmall {
                        self.goto(Phase::RxBig);
                    } else {
                        self.goto(Phase::Hold);
                    }
                }
                other => log::warn!("SPIKE unknown message {other:?}"),
            }
        }
    }

    /// Advances the scripted run once per frame.
    fn step(&mut self) {
        self.inbox();
        let elapsed = self.phase_start.elapsed();
        match self.phase {
            Phase::Baseline if elapsed > Duration::from_secs(6) => {
                if self.webview {
                    self.goto(Phase::WaitReady);
                    if let Err(e) = self.attach() {
                        log::error!("SPIKE attach failed: {e}");
                    }
                } else {
                    self.goto(Phase::Hold);
                }
            }
            Phase::WaitReady if elapsed > Duration::from_secs(20) => {
                log::error!("SPIKE the page never reported ready");
                self.goto(Phase::Hold);
            }
            Phase::Idle if elapsed > Duration::from_secs(6) => {
                self.goto(Phase::IpcUp);
                self.eval("window.__spike.ipc(300)");
            }
            Phase::RxSmall | Phase::RxBig => self.rx_step(),
            Phase::Hold if elapsed > self.hold => {
                log::info!("SPIKE done");
                let _ = with_host(&self.app, |env, host| {
                    env.call_static_method(host, jni_str!("detach"), jni_sig!(() -> void), &[])?;
                    Ok(())
                });
                std::process::exit(0);
            }
            _ => {}
        }
    }

    fn rx_step(&mut self) {
        let now = Instant::now();
        if now < self.rx_next {
            return;
        }
        if self.rx_sent == 300 {
            // Give the last calls time to arrive, then ask for the page's latencies.
            if self.phase_start.elapsed() > Duration::from_millis(1500) {
                self.eval("window.__spike.rxDone()");
                self.rx_next = now + Duration::from_secs(3600);
            }
            return;
        }
        self.rx_next = now + Duration::from_millis(16);
        self.rx_sent += 1;
        let payload = if self.phase == Phase::RxBig { "x".repeat(4096) } else { String::from("ok") };
        let js = format!("window.__spike.rx({:.0},'{payload}')", wall_us());
        let t = Instant::now();
        self.eval(&js);
        self.eval_cost.push(t.elapsed().as_secs_f64() * 1000.0);
        if self.rx_sent == 300 {
            self.phase_start = Instant::now();
        }
    }

    fn eval(&self, js: &str) {
        let r = with_host(&self.app, |env, host| {
            let js = env.new_string(js)?;
            env.call_static_method(
                host,
                jni_str!("eval"),
                jni_sig!((java.lang.String) -> void),
                &[JValue::Object(&js)],
            )?;
            Ok(())
        });
        if let Err(e) = r {
            log::error!("SPIKE eval failed: {e}");
        }
    }

    fn attach(&self) -> Result<(), jni::errors::Error> {
        let mode = self.mode.clone();
        let activity_ptr = self.app.activity_as_ptr();
        with_host(&self.app, move |env, host| {
            // SAFETY: the pointer comes from the running activity.
            let activity = unsafe { JObject::from_raw(env, activity_ptr.cast()) };
            let html = env.new_string(PAGE.replace("__MODE__", &mode))?;
            let mode = env.new_string(&mode)?;
            env.call_static_method(
                host,
                jni_str!("attach"),
                jni_sig!((android.app.Activity, java.lang.String, java.lang.String) -> void),
                &[JValue::Object(&activity), JValue::Object(&html), JValue::Object(&mode)],
            )?;
            Ok(())
        })
    }

    fn draw(&mut self) {
        let frame_start = Instant::now();
        if let Some(last) = self.last_frame.replace(frame_start) {
            self.intervals.push((frame_start - last).as_secs_f64() * 1000.0);
        }
        self.step();
        let (Some(gpu), Some(window)) = (&mut self.gpu, &self.window) else { return };
        let Some(frame) = gpu.next_frame() else { return };
        let t = self.start.elapsed().as_secs_f64();
        let [r, g, b] =
            self.fixed.unwrap_or_else(|| [0.0f64, 2.09, 4.19].map(|phase| 0.5 + 0.5 * (t * 1.5 + phase).sin()));
        let c = if gpu.config.format.is_srgb() { [r, g, b].map(srgb_to_linear) } else { [r, g, b] };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("spike clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: c[0], g: c[1], b: c[2], a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }));
        gpu.queue.submit([encoder.finish()]);
        window.pre_present_notify();
        gpu.queue.present(frame);
        self.cpu.push(frame_start.elapsed().as_secs_f64() * 1000.0);
    }
}

/// Runs `f` with a JNI env attached to this thread and the spike's `Host` class (loaded through
/// the app's class loader, which a native thread's `FindClass` cannot see).
fn with_host<R>(
    app: &AndroidApp,
    f: impl FnOnce(&mut jni::Env, &JClass) -> Result<R, jni::errors::Error>,
) -> Result<R, jni::errors::Error> {
    // SAFETY: the pointer comes from the running activity.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    vm.attach_current_thread(|env| {
        let host = match HOST.get() {
            Some(h) => h,
            None => {
                // SAFETY: the pointer comes from the running activity.
                let activity = unsafe { JObject::from_raw(env, app.activity_as_ptr().cast()) };
                let loader = env
                    .call_method(&activity, jni_str!("getClassLoader"), jni_sig!(() -> java.lang.ClassLoader), &[])?
                    .l()?;
                let name = env.new_string("io.github.generalpawz.acr.webviewspike.Host")?;
                let class = env
                    .call_method(
                        &loader,
                        jni_str!("loadClass"),
                        jni_sig!((java.lang.String) -> java.lang.Class),
                        &[JValue::Object(&name)],
                    )?
                    .l()?;
                let class = env.cast_local::<JClass>(class)?;
                // NativeActivity loads the library outside the class loader's lookup, so the
                // JNI names are not found by `dlsym`: register the two natives.
                let methods = unsafe {
                    [
                        NativeMethod::from_raw_parts(
                            jni_str!("nativeMessage"),
                            jni_str!("(Ljava/lang/String;)V"),
                            native_message as *mut std::ffi::c_void,
                        ),
                        NativeMethod::from_raw_parts(
                            jni_str!("nativeTouch"),
                            jni_str!("(IFF)V"),
                            native_touch as *mut std::ffi::c_void,
                        ),
                    ]
                };
                // SAFETY: the signatures of the two functions match the descriptors.
                unsafe { env.register_native_methods(&class, &methods)? };
                let global = env.new_global_ref(&class)?;
                HOST.get_or_init(|| global)
            }
        };
        f(env, host)
    })
}

fn host_extra(app: &AndroidApp, key: &str) -> Result<String, jni::errors::Error> {
    with_host(app, |env, host| {
        let activity = unsafe { JObject::from_raw(env, app.activity_as_ptr().cast()) };
        let key = env.new_string(key)?;
        let value = env
            .call_static_method(
                host,
                jni_str!("extra"),
                jni_sig!((android.app.Activity, java.lang.String) -> java.lang.String),
                &[JValue::Object(&activity), JValue::Object(&key)],
            )?
            .l()?;
        let value = env.cast_local::<JString>(value)?;
        value.try_to_string(env)
    })
}

impl ApplicationHandler for Spike {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = match event_loop.create_window(Window::default_attributes()) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("window: {e}");
                return event_loop.exit();
            }
        };
        match &mut self.gpu {
            Some(gpu) => {
                if let Err(e) = gpu.resume(window.clone()) {
                    log::error!("{e}");
                }
            }
            None => {
                let slot = Rc::new(RefCell::new(None));
                let out = slot.clone();
                let w = window.clone();
                acr_app::spawn(async move { *out.borrow_mut() = Some(Gpu::new(w).await) });
                match slot.take() {
                    Some(Ok(gpu)) => {
                        log::info!("SPIKE gpu {} {:?} format {:?}", gpu.info.name, gpu.info.backend, gpu.config.format);
                        self.gpu = Some(gpu);
                    }
                    other => {
                        log::error!("gpu: {:?}", other.map(|r| r.err()));
                        return event_loop.exit();
                    }
                }
            }
        }
        window.request_redraw();
        self.window = Some(window);
        log::info!("SPIKE resumed in phase {:?} mode={}", self.phase, self.mode);
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        if let Some(gpu) = &mut self.gpu {
            gpu.suspend();
        }
        self.window = None;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::Touch(t) if t.phase == winit::event::TouchPhase::Started => {
                log::info!("SPIKE touch game {:.0} {:.0}", t.location.x, t.location.y);
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, .. } => log::info!("SPIKE mouse game"),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_tag("spike").with_max_level(log::LevelFilter::Info),
    );
    std::panic::set_hook(Box::new(|info| log::error!("{info}")));
    let result = EventLoop::builder().with_android_app(app.clone()).build().and_then(|event_loop| {
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop.run_app(&mut Spike::new(app))
    });
    if let Err(e) = result {
        log::error!("{e}");
    }
    std::process::exit(0);
}
