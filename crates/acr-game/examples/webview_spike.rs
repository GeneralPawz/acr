//! Spike for the native Svelte UI host (tracking issue #316 section 3, #333): a winit window with
//! a wgpu clear that changes colour every frame, and a transparent wry child webview (WebKitGTK)
//! with one button and a full-size forwarding area. It measures what the section 3 table asks for
//! and prints `SPIKE <key> <value>` lines.
//!
//! ```text
//! cargo run --release -p acr-game --features webview-gtk --example webview_spike
//! SPIKE_AUTO=1 xvfb-run -a cargo run ...      # scripted run, prints the numbers and exits
//! SPIKE_NO_WEBVIEW=1 ...                      # the same without the webview (baseline)
//! ```
//!
//! `SPIKE_EVAL_SECS` (default 10) is the `evaluate_script` run; keys come from outside
//! (`xdotool`-like synthetic X events) and are counted when they reach the page or winit.

#[cfg(target_os = "linux")]
mod linux {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use acr_app::Gpu;
    use gtk::prelude::DisplayExtManual;
    use winit::application::ApplicationHandler;
    use winit::dpi::{LogicalSize, PhysicalSize};
    use winit::event::{ElementState, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::platform::x11::{EventLoopBuilderExtX11, WindowAttributesExtX11};
    use winit::window::{Window, WindowId};
    use wry::dpi::{LogicalPosition as WryPosition, LogicalSize as WrySize};
    use wry::http::Response;
    use wry::{Rect, WebView, WebViewBuilder};

    const PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"><style>
html,body{margin:0;height:100%;background:transparent;overflow:hidden;font:16px sans-serif;color:#fff}
#fwd{position:fixed;inset:0}
#panel{pointer-events:none;position:fixed;left:25%;top:25%;width:50%;height:50%;background:rgba(20,20,20,.55);border:2px solid #fff}
#b{position:fixed;left:30%;top:35%;font-size:24px;padding:12px 24px}
</style></head><body>
<div id="fwd"></div><div id="panel"></div><button id="b">click</button>
<script>
const post = (o) => window.ipc.postMessage(JSON.stringify(o));
const now = () => performance.timeOrigin + performance.now();
const s = window.__spike = { recvd: 0, bytes: 0 };
s.recv = (p) => { s.recvd++; s.bytes += p.length; };
document.getElementById('b').onclick = () => post({ k: 'click' });
let last = 0;
document.getElementById('fwd').addEventListener('pointermove', (e) => {
  if (e.timeStamp - last > 8) { last = e.timeStamp; post({ k: 'ptr', x: e.clientX, y: e.clientY, t: now() }); }
});
addEventListener('keydown', (e) => { e.preventDefault(); post({ k: 'key', key: e.key, code: e.code, alt: e.altKey }); });
s.ipc = async (n) => { for (let i = 0; i < n; i++) { post({ k: 'ping', t: now() }); await new Promise(r => setTimeout(r, 4)); } post({ k: 'ipc_done' }); };
s.proto = async (n) => {
  const ms = [];
  for (let i = 0; i < n; i++) { const t = performance.now(); const r = await fetch('acr://localhost/p.png?' + i, { cache: 'no-store' }); const b = await r.arrayBuffer(); ms.push([performance.now() - t, b.byteLength]); }
  post({ k: 'proto', ms });
};
s.report = () => post({ k: 'recvd', n: s.recvd, bytes: s.bytes });
post({ k: 'ready', ua: navigator.userAgent });
</script></body></html>"#;

    enum Phase {
        Warm,
        Ipc,
        Eval,
        Proto,
        Resize(u32),
        Keys,
        Done,
    }

    #[derive(Default)]
    struct Stats {
        ready: bool,
        clicks: u32,
        pointer_msgs: u32,
        keys: Vec<String>,
        pings: Vec<f64>,
        ipc_done: bool,
        proto: Vec<(f64, usize)>,
        recvd: Option<(u64, u64)>,
    }

    struct Spike {
        window: Option<Arc<Window>>,
        gpu: Option<Gpu>,
        webview: Option<WebView>,
        stats: Rc<RefCell<Stats>>,
        auto: bool,
        no_webview: bool,
        eval_secs: u64,
        started: Instant,
        frames: Vec<f64>,
        last_frame: Instant,
        phase: Phase,
        phase_start: Instant,
        eval_sent: u64,
        eval_ms: Vec<f64>,
        phase_frames: Vec<(&'static str, Vec<f64>)>,
        winit_keys: Vec<String>,
        scale: f64,
        fixed: Option<[f64; 3]>,
    }

    fn report(key: &str, value: impl std::fmt::Display) {
        println!("SPIKE {key} {value}");
    }

    fn percentile(v: &mut [f64], p: f64) -> f64 {
        if v.is_empty() {
            return f64::NAN;
        }
        v.sort_by(|a, b| a.total_cmp(b));
        v[((v.len() - 1) as f64 * p).round() as usize]
    }

    /// Resident memory in MiB of this process and of its children (the WebKit processes).
    fn rss_mib() -> (f64, f64) {
        let pages = |pid: &str| -> f64 {
            std::fs::read_to_string(format!("/proc/{pid}/statm"))
                .ok()
                .and_then(|s| s.split_whitespace().nth(1)?.parse::<f64>().ok())
                .unwrap_or(0.0)
                * 4096.0
                / 1048576.0
        };
        let me = std::process::id().to_string();
        let mut children = 0.0;
        for entry in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.chars().all(|c| c.is_ascii_digit())
                && let Ok(stat) = std::fs::read_to_string(format!("/proc/{name}/stat"))
                && let Some(rest) = stat.rsplit_once(')').map(|x| x.1)
                && rest.split_whitespace().nth(1) == Some(me.as_str())
            {
                children += pages(&name);
            }
        }
        (pages(&me), children)
    }

    fn png_512() -> Vec<u8> {
        let mut data = Vec::with_capacity(512 * 512 * 4);
        let mut x = 12345u32;
        for _ in 0..512 * 512 {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            data.extend_from_slice(&[(x >> 24) as u8, (x >> 16) as u8, (x >> 8) as u8, 255]);
        }
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, 512, 512);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&data).unwrap();
        out
    }

    impl Spike {
        fn eval(&self, js: &str) {
            if let Some(w) = &self.webview
                && let Err(e) = w.evaluate_script(js)
            {
                eprintln!("evaluate_script: {e}");
            }
        }

        fn bounds(&self) -> Rect {
            let size = self.window.as_ref().unwrap().inner_size().to_logical::<u32>(self.scale);
            Rect { position: WryPosition::new(0, 0).into(), size: WrySize::new(size.width, size.height).into() }
        }

        fn next(&mut self, phase: Phase, label: &'static str) {
            let frames = std::mem::take(&mut self.frames);
            self.phase_frames.push((label, frames));
            self.phase = phase;
            self.phase_start = Instant::now();
        }

        fn render(&mut self) {
            let t = self.started.elapsed().as_secs_f64();
            let Some(gpu) = &mut self.gpu else { return };
            let Some(frame) = gpu.next_frame() else { return };
            let view = frame.texture.create_view(&Default::default());
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            {
                let mut colour = wgpu::Color {
                    r: 0.5 + 0.5 * t.sin(),
                    g: 0.5 + 0.5 * (t * 0.7).sin(),
                    b: 0.5 + 0.5 * (t * 1.3).sin(),
                    a: 1.0,
                };
                // A fixed colour (`SPIKE_FIXED=r,g,b`, 0..1) lets a screenshot tell what shows through.
                if let Some([r, g, b]) = self.fixed {
                    colour = wgpu::Color { r, g, b, a: 1.0 };
                }
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(colour), store: wgpu::StoreOp::Store },
                    })],
                    ..Default::default()
                });
            }
            gpu.queue.submit(Some(encoder.finish()));
            self.window.as_ref().unwrap().pre_present_notify();
            gpu.queue.present(frame);
            let now = Instant::now();
            self.frames.push(now.duration_since(self.last_frame).as_secs_f64() * 1000.0);
            self.last_frame = now;
        }

        fn finish(&mut self, event_loop: &ActiveEventLoop) {
            let (own, children) = rss_mib();
            report("rss_self_mib", format!("{own:.0}"));
            report("rss_children_mib", format!("{children:.0}"));
            for (label, frames) in &mut self.phase_frames {
                let skip = frames.len().min(5);
                let mut v: Vec<f64> = frames[skip..].to_vec();
                if v.is_empty() {
                    continue;
                }
                let mean = v.iter().sum::<f64>() / v.len() as f64;
                let over = v.iter().filter(|&&f| f > 25.0).count();
                report(
                    &format!("frame_ms[{label}]"),
                    format!(
                        "n={} mean={mean:.2} p50={:.2} p99={:.2} max={:.2} over25ms={over}",
                        v.len(),
                        percentile(&mut v, 0.5),
                        percentile(&mut v, 0.99),
                        percentile(&mut v, 1.0)
                    ),
                );
            }
            let st = self.stats.borrow();
            report("clicks", st.clicks);
            report("pointer_msgs", st.pointer_msgs);
            report("keys_in_page", st.keys.join(","));
            report("keys_in_winit", self.winit_keys.join(","));
            let mut pings = st.pings.clone();
            report(
                "ipc_one_way_ms",
                format!(
                    "n={} median={:.2} p99={:.2} max={:.2}",
                    pings.len(),
                    percentile(&mut pings, 0.5),
                    percentile(&mut pings, 0.99),
                    percentile(&mut pings, 1.0)
                ),
            );
            let mut calls = self.eval_ms.clone();
            report(
                "eval_call_ms",
                format!(
                    "n={} median={:.3} p99={:.3} max={:.3}",
                    calls.len(),
                    percentile(&mut calls, 0.5),
                    percentile(&mut calls, 0.99),
                    percentile(&mut calls, 1.0)
                ),
            );
            report("eval_sent", self.eval_sent);
            report("eval_received", format!("{:?}", st.recvd));
            let mut ms: Vec<f64> = st.proto.iter().map(|p| p.0).collect();
            report(
                "proto_png_ms",
                format!(
                    "n={} bytes={} median={:.2} p99={:.2}",
                    ms.len(),
                    st.proto.first().map_or(0, |p| p.1),
                    percentile(&mut ms, 0.5),
                    percentile(&mut ms, 0.99)
                ),
            );
            event_loop.exit();
        }

        fn step(&mut self, event_loop: &ActiveEventLoop) {
            if !self.auto {
                return;
            }
            let elapsed = self.phase_start.elapsed();
            let have_wv = self.webview.is_some();
            match self.phase {
                Phase::Warm if elapsed > Duration::from_secs(3) && (self.stats.borrow().ready || !have_wv) => {
                    if have_wv {
                        self.eval("window.__spike.ipc(500)");
                        self.next(Phase::Ipc, "idle");
                    } else {
                        self.next(Phase::Eval, "idle");
                    }
                }
                Phase::Warm if elapsed > Duration::from_secs(20) => {
                    eprintln!("the page never became ready");
                    event_loop.exit();
                }
                Phase::Ipc if self.stats.borrow().ipc_done => {
                    self.eval("window.__spike.proto(30)");
                    self.next(Phase::Proto, "ipc");
                }
                Phase::Proto if !self.stats.borrow().proto.is_empty() => {
                    self.next(Phase::Eval, "proto");
                }
                Phase::Eval => {
                    // 4 KB at 60 Hz: one call per 16.67 ms.
                    let due = (elapsed.as_secs_f64() * 60.0) as u64;
                    while self.eval_sent < due && have_wv {
                        let payload = "x".repeat(4096);
                        let t = Instant::now();
                        self.eval(&format!("window.__spike.recv('{payload}')"));
                        self.eval_ms.push(t.elapsed().as_secs_f64() * 1000.0);
                        self.eval_sent += 1;
                    }
                    if elapsed > Duration::from_secs(self.eval_secs) {
                        self.eval("window.__spike.report()");
                        self.next(Phase::Resize(0), "eval");
                    }
                }
                Phase::Resize(n) if elapsed > Duration::from_millis(60) => {
                    let w = self.window.clone().unwrap();
                    let size = if n % 2 == 0 { PhysicalSize::new(900, 700) } else { PhysicalSize::new(1100, 800) };
                    let _ = w.request_inner_size(size);
                    if n >= 20 {
                        self.next(Phase::Keys, "resize");
                    } else {
                        self.phase = Phase::Resize(n + 1);
                        self.phase_start = Instant::now();
                    }
                }
                Phase::Keys
                    if elapsed
                        > Duration::from_secs(
                            std::env::var("SPIKE_KEY_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(4),
                        ) =>
                {
                    self.next(Phase::Done, "keys");
                }
                Phase::Done if elapsed > Duration::from_millis(300) => self.finish(event_loop),
                _ => {}
            }
        }
    }

    impl ApplicationHandler for Spike {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            let attributes = Window::default_attributes()
                .with_title("webview spike")
                .with_transparent(true)
                .with_inner_size(LogicalSize::new(1000, 700))
                .with_name("acr-spike", "acr-spike");
            let window = Arc::new(event_loop.create_window(attributes).expect("window"));
            self.scale = window.scale_factor();
            let (tx, rx) = std::sync::mpsc::channel();
            let w = window.clone();
            acr_app::spawn(async move {
                let _ = tx.send(Gpu::new(w).await);
            });
            let gpu = rx.recv().expect("gpu thread").expect("gpu");
            report(
                "adapter",
                format!(
                    "{} {:?} {:?} alpha={:?}",
                    gpu.info.name, gpu.info.backend, gpu.config.present_mode, gpu.config.alpha_mode
                ),
            );
            self.gpu = Some(gpu);
            self.window = Some(window.clone());
            if !self.no_webview {
                let stats = self.stats.clone();
                let png: Arc<Vec<u8>> = Arc::new(png_512());
                let built = WebViewBuilder::new()
                    .with_transparent(true)
                    .with_background_color((0, 0, 0, 0))
                    .with_bounds(self.bounds())
                    .with_asynchronous_custom_protocol("acr".into(), move |_id, request, responder| {
                        let path = request.uri().path().to_owned();
                        let (kind, body): (&str, std::borrow::Cow<'static, [u8]>) = if path.starts_with("/p.png") {
                            ("image/png", png.to_vec().into())
                        } else {
                            ("text/html", PAGE.as_bytes().into())
                        };
                        responder.respond(
                            Response::builder()
                                .header("Content-Type", kind)
                                .header("Cache-Control", "no-cache")
                                .body(body)
                                .unwrap(),
                        );
                    })
                    .with_ipc_handler(move |req| {
                        let received = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64() * 1000.0;
                        let Ok(v) = serde_json::from_str::<serde_json::Value>(req.body()) else { return };
                        let mut st = stats.borrow_mut();
                        match v["k"].as_str() {
                            Some("ready") => {
                                st.ready = true;
                                report("webview_ua", v["ua"].as_str().unwrap_or(""));
                            }
                            Some("click") => st.clicks += 1,
                            Some("ptr") => st.pointer_msgs += 1,
                            Some("key") => st.keys.push(format!(
                                "{}{}",
                                if v["alt"] == true { "Alt+" } else { "" },
                                v["code"].as_str().unwrap_or("")
                            )),
                            Some("ping") => st.pings.push(received - v["t"].as_f64().unwrap_or(received)),
                            Some("ipc_done") => st.ipc_done = true,
                            Some("proto") => {
                                st.proto = v["ms"]
                                    .as_array()
                                    .map(|a| {
                                        a.iter()
                                            .map(|p| {
                                                (p[0].as_f64().unwrap_or(0.0), p[1].as_u64().unwrap_or(0) as usize)
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();
                            }
                            Some("recvd") => {
                                st.recvd = Some((v["n"].as_u64().unwrap_or(0), v["bytes"].as_u64().unwrap_or(0)))
                            }
                            _ => {}
                        }
                    })
                    .with_url("acr://localhost/index.html")
                    .build_as_child(&*window);
                match built {
                    Ok(w) => {
                        let _ = w.focus();
                        self.webview = Some(w);
                    }
                    Err(e) => {
                        report("webview_error", &e);
                        event_loop.exit();
                    }
                }
            }
            self.started = Instant::now();
            self.phase_start = Instant::now();
            self.last_frame = Instant::now();
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Resized(size) => {
                    if let Some(g) = &mut self.gpu {
                        g.resize(size.width, size.height);
                    }
                    if let Some(w) = &self.webview {
                        let _ = w.set_bounds(self.bounds());
                    }
                }
                WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                    self.scale = scale_factor;
                    if let Some(w) = &self.webview {
                        let _ = w.set_bounds(self.bounds());
                    }
                }
                WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                    self.winit_keys.push(format!("{:?}", event.logical_key));
                }
                WindowEvent::RedrawRequested => self.render(),
                _ => {}
            }
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            event_loop.set_control_flow(ControlFlow::Poll);
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
            self.step(event_loop);
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }

    pub fn main() {
        gtk::init().expect("gtk");
        if gtk::gdk::Display::default().expect("display").backend().is_wayland() {
            panic!("wry child webviews need X11: run with GDK_BACKEND=x11 (and WAYLAND_DISPLAY unset)");
        }
        // wry's X11 child window raises extension errors (170; GLXBadWindow, 168, on XWayland) that
        // winit would otherwise treat as fatal. Count and log them.
        winit::platform::x11::register_xlib_error_hook(Box::new(|_display, error| {
            let error = error as *mut x11_dl::xlib::XErrorEvent;
            let (code, request) = unsafe { ((*error).error_code, (*error).request_code) };
            let ignored = code >= 128;
            eprintln!("SPIKE x_error code={code} request={request} ignored={ignored}");
            ignored
        }));
        let event_loop = EventLoop::builder().with_x11().build().expect("event loop");
        let mut spike = Spike {
            window: None,
            gpu: None,
            webview: None,
            stats: Rc::default(),
            auto: std::env::var_os("SPIKE_AUTO").is_some(),
            no_webview: std::env::var_os("SPIKE_NO_WEBVIEW").is_some(),
            eval_secs: std::env::var("SPIKE_EVAL_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(10),
            started: Instant::now(),
            frames: Vec::new(),
            last_frame: Instant::now(),
            phase: Phase::Warm,
            phase_start: Instant::now(),
            eval_sent: 0,
            eval_ms: Vec::new(),
            phase_frames: Vec::new(),
            winit_keys: Vec::new(),
            scale: 1.0,
            fixed: std::env::var("SPIKE_FIXED").ok().and_then(|v| {
                let p: Vec<f64> = v.split(',').filter_map(|x| x.parse().ok()).collect();
                <[f64; 3]>::try_from(p).ok()
            }),
        };
        event_loop.run_app(&mut spike).expect("run");
    }
}

#[cfg(target_os = "linux")]
fn main() {
    linux::main();
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("the webview spike runs on Linux (WebKitGTK) only");
}
