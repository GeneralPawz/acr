//! The performance recorder (docs/testing.md "Performance recorder"): always on (native),
//! it keeps the last [`FRAMES`] frames in fixed ring buffers (frame time, the profiler's
//! scopes, the GPU time of sampled frames, the 3D and world counts) and a 1 Hz summary of the
//! last [`SECONDS`] seconds, logs one `perf:` line a second while a battle runs, and writes a
//! snapshot (`perf/snap-<time>.json` next to the log) on Ctrl + Space, R (modern controls),
//! the driver's `perf`, or by itself when the frames stay slower than [`SLOW_MS`] for
//! [`SLOW_FOR`] seconds (at most one per [`AUTO_EVERY`] seconds). Nothing is allocated per
//! frame; `ACR_PERF=off` switches it off. Render only: it never touches the world.
// In the browser the recorder stays off: no snapshots, so most of it is unused there.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use web_time::Instant;

/// Frames kept: 68 s at 60 fps.
pub const FRAMES: usize = 4096;
/// Profiler scope names kept per frame; later names are dropped.
pub const SCOPES: usize = 64;
/// 1 Hz summaries kept.
pub const SECONDS: usize = 300;
/// A frame slower than this counts towards an automatic snapshot.
pub const SLOW_MS: f32 = 250.0;
/// For this long in a row (seconds).
pub const SLOW_FOR: f32 = 3.0;
/// At most one automatic snapshot per this many seconds.
pub const AUTO_EVERY: f32 = 60.0;
/// How often a frame's GPU passes are timed (seconds; timestamp queries, read back later).
const GPU_EVERY: f32 = 0.25;
/// The working budgets of a battle frame at 30 fps on an integrated GPU (the Snapdragon X,
/// docs/rendering-3d.md "Performance"): triangles drawn, shadow-cast triangles, draws.
pub const BUDGET_TRIANGLES: u32 = 3_000_000;
pub const BUDGET_SHADOW_TRIANGLES: u32 = 1_500_000;
pub const BUDGET_DRAWS: u32 = 1500;
/// The toast on the message list.
pub const SAVED: &str = "Leistungsdaten gespeichert";

/// One frame's counts, filled by the game (`InGame::perf_counts`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Counts {
    pub in_game: bool,
    /// Model instances built, drawn, palettes uploaded.
    pub instances: u32,
    pub drawn: u32,
    pub poses: u32,
    /// Triangles and draws of the models pass and of the shadow casters.
    pub triangles: u32,
    pub draws: u32,
    pub shadow_triangles: u32,
    pub shadow_draws: u32,
    /// Instances drawn per level of detail (0 the full mesh).
    pub lod: [u32; 4],
    /// Living units and corpses in the world.
    pub units: u32,
    pub corpses: u32,
    pub ragdolls_awake: u32,
    pub particles: u32,
    /// Blood pools and other decals of the frame.
    pub decals: u32,
    /// The camera's scale (1 the original's view, smaller zoomed out).
    pub zoom: f32,
    pub free_camera: bool,
    pub models_phase: bool,
    pub shadows: bool,
    /// The game speed (ticks per second).
    pub ticks_per_second: u16,
    /// The window in px.
    pub window: [u16; 2],
}

/// GPU milliseconds of a sampled frame; negative: not sampled.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Gpu {
    total: f32,
    ground: f32,
    shadow: f32,
    pass: f32,
    interface: f32,
}

impl Default for Gpu {
    fn default() -> Self {
        Self { total: -1.0, ground: 0.0, shadow: 0.0, pass: 0.0, interface: 0.0 }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Frame {
    /// Seconds since the recorder started.
    t: f32,
    /// Wall time since the last frame.
    ms: f32,
    ticks: u16,
    gpu: Gpu,
    counts: Counts,
    /// The recorder's own time this frame (µs).
    own_us: f32,
    /// Profiler scope calls this frame (each costs two clock reads).
    calls: u32,
}

#[derive(Clone, Copy, Debug, Default)]
struct Second {
    t: f32,
    frames: u32,
    mean_ms: f32,
    max_ms: f32,
    gpu_ms: f32,
    counts: Counts,
    gpu_mb: f32,
    texture_mb: f32,
}

/// The second being summed.
#[derive(Clone, Copy, Debug, Default)]
struct Acc {
    since: f32,
    frames: u32,
    sum: f32,
    max: f32,
    gpu_sum: f32,
    gpu_n: u32,
    ticks: u32,
    own_sum: f32,
    advance: f32,
    render: f32,
    acquire: f32,
}

/// Why a snapshot was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Key,
    Auto,
    Driver,
}

impl Reason {
    fn name(self) -> &'static str {
        match self {
            Reason::Key => "key",
            Reason::Auto => "auto",
            Reason::Driver => "driver",
        }
    }
}

/// What a snapshot says about the machine and the game besides the buffers.
pub struct Context<'a> {
    pub settings: &'a str,
    pub adapter: Option<&'a wgpu::AdapterInfo>,
    pub features: Option<wgpu::Features>,
    /// The GPU allocator's bytes allocated and reserved (DX12, Vulkan, Metal).
    pub gpu_memory: Option<(u64, u64)>,
}

pub struct Recorder {
    on: bool,
    start: Instant,
    last: Option<Instant>,
    frames: Vec<Frame>,
    scopes: Vec<[f32; SCOPES]>,
    names: [&'static str; SCOPES],
    named: usize,
    head: usize,
    len: usize,
    seconds: Vec<Second>,
    s_head: usize,
    s_len: usize,
    acc: Acc,
    slow_since: Option<f32>,
    last_auto: Option<f32>,
    gpu: acr_render::gpu_timer::Sampler,
    gpu_out: [(&'static str, f32); acr_render::gpu_timer::SAMPLED],
    gpu_segments: usize,
    next_gpu: f32,
    gpu_supported: bool,
    /// GPU allocator bytes (allocated, reserved; at snapshots) and model texture bytes
    /// (1 Hz).
    memory: [u64; 3],
    pub snapshots: u32,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new(true)
    }
}

impl Recorder {
    pub fn new(on: bool) -> Self {
        let on = on && cfg!(not(target_arch = "wasm32"));
        acr_sim::prof::record(on);
        let n = if on { FRAMES } else { 1 };
        Self {
            on,
            start: Instant::now(),
            last: None,
            frames: vec![Frame::default(); n],
            scopes: vec![[0.0; SCOPES]; n],
            names: [""; SCOPES],
            named: 0,
            head: 0,
            len: 0,
            seconds: vec![Second::default(); if on { SECONDS } else { 1 }],
            s_head: 0,
            s_len: 0,
            acc: Acc::default(),
            slow_since: None,
            last_auto: None,
            gpu: Default::default(),
            gpu_out: [("", 0.0); acr_render::gpu_timer::SAMPLED],
            gpu_segments: 0,
            next_gpu: 0.0,
            gpu_supported: false,
            memory: [0; 3],
            snapshots: 0,
        }
    }

    /// On unless `ACR_PERF=off` (native; off in the browser).
    pub fn from_env() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let on = !std::env::var("ACR_PERF").is_ok_and(|v| v == "off" || v == "0");
        #[cfg(target_arch = "wasm32")]
        let on = false;
        Self::new(on)
    }

    pub fn on(&self) -> bool {
        self.on
    }

    fn now(&self) -> f32 {
        self.start.elapsed().as_secs_f32()
    }

    /// The recorder's own mean time per frame over the frames kept (µs).
    pub fn mean_own_us(&self) -> f64 {
        let sum: f64 = self.ordered().map(|i| f64::from(self.frames[i].own_us)).sum();
        sum / self.len.max(1) as f64
    }

    /// The mean profiler scope calls per frame over the frames kept.
    pub fn mean_calls(&self) -> f64 {
        let sum: f64 = self.ordered().map(|i| f64::from(self.frames[i].calls)).sum();
        sum / self.len.max(1) as f64
    }

    /// Before a frame is drawn: times its GPU passes if one is due.
    pub fn gpu_begin(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if !self.on || self.now() < self.next_gpu {
            return;
        }
        if self.gpu.begin(device, queue) {
            self.gpu_supported = true;
            self.next_gpu = self.now() + GPU_EVERY;
        }
    }

    /// After the frame was submitted.
    pub fn gpu_end(&mut self) {
        if self.on {
            self.gpu.end();
        }
    }

    /// The texture figures are due (once a second): the caller passes them to
    /// [`Self::memory`]; the GPU allocator's report only at a snapshot (it lists every
    /// allocation).
    pub fn wants_memory(&self) -> bool {
        self.on && self.now() - self.acc.since >= 1.0
    }

    /// GPU allocator bytes allocated and reserved, model texture bytes.
    pub fn memory(&mut self, gpu: Option<(u64, u64)>, textures: Option<u64>) {
        if let Some((a, r)) = gpu {
            self.memory[0] = a;
            self.memory[1] = r;
        }
        if let Some(t) = textures {
            self.memory[2] = t;
        }
    }

    /// Records one frame ending now with `counts`; `device` polls the GPU timer's stamps.
    /// Returns `Some(Reason::Auto)` when an automatic snapshot is due.
    pub fn frame(&mut self, counts: &Counts, device: Option<&wgpu::Device>) -> Option<Reason> {
        if !self.on {
            return None;
        }
        let begun = Instant::now();
        let ms = self.last.map_or(0.0, |l| (begun - l).as_secs_f32() * 1000.0);
        self.last = Some(begun);
        let t = (begun - self.start).as_secs_f32();
        let slot = self.head;
        let row = &mut self.scopes[slot];
        *row = [0.0; SCOPES];
        let mut ticks = 0u64;
        let mut all_calls = 0u32;
        let (names, named) = (&mut self.names, &mut self.named);
        acr_sim::prof::drain_frame(|name, ns, calls| {
            all_calls = all_calls.saturating_add(calls);
            if name == "ticks" {
                ticks = ns;
                return;
            }
            let i = match names[..*named].iter().position(|n| std::ptr::eq(*n, name) || *n == name) {
                Some(i) => i,
                None if *named < SCOPES => {
                    names[*named] = name;
                    *named += 1;
                    *named - 1
                }
                None => return,
            };
            row[i] = ns as f32 / 1e6;
        });
        let mut gpu = Gpu::default();
        if let Some(device) = device
            && let Some(n) = self.gpu.poll(device, &mut self.gpu_out)
        {
            self.gpu_segments = n;
            let mut g = Gpu { total: 0.0, ..Gpu::default() };
            for &(label, v) in &self.gpu_out[..n] {
                g.total += v;
                match label {
                    "terrain" | "ground" => g.ground += v,
                    "shadow map" => g.shadow += v,
                    "interface" => g.interface += v,
                    _ => g.pass += v,
                }
            }
            gpu = g;
        }
        let f = Frame {
            t,
            ms,
            ticks: ticks.min(u64::from(u16::MAX)) as u16,
            gpu,
            counts: *counts,
            own_us: 0.0,
            calls: all_calls,
        };
        self.frames[slot] = f;
        self.head = (self.head + 1) % self.frames.len();
        self.len = (self.len + 1).min(self.frames.len());
        // The second's sums and the 1 Hz line.
        let scope =
            |name: &str| self.names[..self.named].iter().position(|n| *n == name).map_or(0.0, |i| self.scopes[slot][i]);
        let (advance, render, acquire) = (scope("frame.advance"), scope("frame.render_cpu"), scope("frame.acquire"));
        let a = &mut self.acc;
        a.frames += 1;
        a.sum += ms;
        a.max = a.max.max(ms);
        a.ticks += u32::from(f.ticks);
        a.advance += advance;
        a.render += render;
        a.acquire += acquire;
        if gpu.total >= 0.0 {
            a.gpu_sum += gpu.total;
            a.gpu_n += 1;
        }
        if t - a.since >= 1.0 {
            self.close_second(t, counts);
        }
        // An automatic snapshot after SLOW_FOR seconds of slow frames.
        let mut due = None;
        if ms > SLOW_MS && counts.in_game {
            let since = *self.slow_since.get_or_insert(t);
            if t - since >= SLOW_FOR && self.last_auto.is_none_or(|l| t - l >= AUTO_EVERY) {
                self.last_auto = Some(t);
                self.slow_since = None;
                due = Some(Reason::Auto);
            }
        } else {
            self.slow_since = None;
        }
        let own = begun.elapsed().as_secs_f32() * 1e6;
        self.frames[slot].own_us = own;
        self.acc.own_sum += own;
        due
    }

    fn close_second(&mut self, t: f32, counts: &Counts) {
        let a = self.acc;
        let n = a.frames.max(1) as f32;
        let gpu = if a.gpu_n > 0 { a.gpu_sum / a.gpu_n as f32 } else { -1.0 };
        let mb = |b: u64| b as f32 / 1048576.0;
        self.seconds[self.s_head] = Second {
            t,
            frames: a.frames,
            mean_ms: a.sum / n,
            max_ms: a.max,
            gpu_ms: gpu,
            counts: *counts,
            gpu_mb: mb(self.memory[0]),
            texture_mb: mb(self.memory[2]),
        };
        self.s_head = (self.s_head + 1) % self.seconds.len();
        self.s_len = (self.s_len + 1).min(self.seconds.len());
        if counts.in_game {
            let c = counts;
            let gpu = if gpu >= 0.0 { format!("{gpu:.1} ms") } else { "-".into() };
            log::info!(
                "perf: {:.1} fps, frame {:.1} ms (max {:.0}), gpu {gpu}, cpu advance {:.1} render {:.1}, acquire {:.1}, {:.2} ticks/frame; {}k tri {} draws, {}k cast {} draws, {} of {} models drawn, lod {:?}; {} units {} corpses {} ragdolls {} particles {} decals; zoom {:.2}{}, {} µs recorder",
                a.frames as f32 / (t - a.since).max(1e-3),
                a.sum / n,
                a.max,
                a.advance / n,
                a.render / n,
                a.acquire / n,
                a.ticks as f32 / n,
                c.triangles / 1000,
                c.draws,
                c.shadow_triangles / 1000,
                c.shadow_draws,
                c.drawn,
                c.instances,
                c.lod,
                c.units,
                c.corpses,
                c.ragdolls_awake,
                c.particles,
                c.decals,
                c.zoom,
                if c.free_camera { " free" } else { "" },
                (a.own_sum / n).round()
            );
        }
        self.acc = Acc { since: t, ..Acc::default() };
    }

    /// The frames in order, oldest first.
    fn ordered(&self) -> impl Iterator<Item = usize> + '_ {
        let n = self.frames.len();
        (0..self.len).map(move |k| (self.head + n - self.len + k) % n)
    }

    /// The snapshot: the buffers, a summary of the last 10 s, the top scopes, the worst
    /// frames, the diagnosis, the adapter and the settings (docs/testing.md).
    pub fn snapshot_json(&self, reason: Reason, ctx: &Context<'_>) -> serde_json::Value {
        use serde_json::json;
        let r2 = |v: f32| (f64::from(v) * 100.0).round() / 100.0;
        let idx: Vec<usize> = self.ordered().collect();
        let frames: Vec<&Frame> = idx.iter().map(|&i| &self.frames[i]).collect();
        let last_t = frames.last().map_or(0.0, |f| f.t);
        let col = |f: &dyn Fn(&Frame) -> serde_json::Value| -> serde_json::Value {
            frames.iter().map(|x| f(x)).collect::<Vec<_>>().into()
        };
        let named = &self.names[..self.named];
        let scope_of = |name: &str| named.iter().position(|n| *n == name);
        let mean_scope = |i: usize, from: f32| -> f32 {
            let rows: Vec<f32> =
                idx.iter().filter(|&&k| self.frames[k].t >= from).map(|&k| self.scopes[k][i]).collect();
            rows.iter().sum::<f32>() / rows.len().max(1) as f32
        };
        // Window of the diagnosis: the last 10 s.
        let from = last_t - 10.0;
        let recent: Vec<&Frame> = frames.iter().copied().filter(|f| f.t >= from && f.ms > 0.0).collect();
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
        let ms: Vec<f32> = recent.iter().map(|f| f.ms).collect();
        let mut sorted = ms.clone();
        sorted.sort_by(f32::total_cmp);
        let pct = |p: f32| sorted.get(((sorted.len() as f32 - 1.0) * p).round() as usize).copied().unwrap_or(0.0);
        let frame_ms = mean(&ms);
        let gpu: Vec<&Frame> = recent.iter().copied().filter(|f| f.gpu.total >= 0.0).collect();
        let g = |f: &dyn Fn(&Gpu) -> f32| mean(&gpu.iter().map(|x| f(&x.gpu)).collect::<Vec<_>>());
        let gpu_ms = (!gpu.is_empty()).then(|| g(&|x| x.total));
        let cpu = |name: &str| scope_of(name).map_or(0.0, |i| mean_scope(i, from));
        let (advance, render, acquire, present) =
            (cpu("frame.advance"), cpu("frame.render_cpu"), cpu("frame.acquire"), cpu("frame.present"));
        let last = recent.last().map(|f| f.counts).unwrap_or_default();
        let max_of = |f: &dyn Fn(&Counts) -> u32| frames.iter().map(|x| f(&x.counts)).max().unwrap_or(0);
        // Top scopes over the window (nested scopes count in their parents too).
        let mut top: Vec<(&str, f32, f32)> = named
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.ends_with("(count)"))
            .map(|(i, n)| {
                let max =
                    idx.iter().filter(|&&k| self.frames[k].t >= from).map(|&k| self.scopes[k][i]).fold(0.0, f32::max);
                (*n, mean_scope(i, from), max)
            })
            .collect();
        top.sort_by(|a, b| b.1.total_cmp(&a.1));
        let diagnosis = diagnose(frame_ms, gpu_ms, advance, render, acquire, &last, &top);
        // The worst frames of the whole window.
        let mut worst: Vec<usize> = idx.clone();
        worst.sort_by(|&a, &b| self.frames[b].ms.total_cmp(&self.frames[a].ms));
        worst.truncate(10);
        let worst: Vec<serde_json::Value> = worst
            .iter()
            .map(|&k| {
                let f = &self.frames[k];
                let mut s: Vec<(&str, f32)> = named
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| !n.ends_with("(count)"))
                    .map(|(i, n)| (*n, self.scopes[k][i]))
                    .collect();
                s.sort_by(|a, b| b.1.total_cmp(&a.1));
                s.truncate(6);
                json!({
                    "t": r2(f.t), "ms": r2(f.ms), "ticks": f.ticks,
                    "gpu_ms": (f.gpu.total >= 0.0).then(|| r2(f.gpu.total)),
                    "triangles": f.counts.triangles, "draws": f.counts.draws,
                    "units": f.counts.units, "ragdolls": f.counts.ragdolls_awake,
                    "scopes": s.iter().map(|(n, v)| json!([n, r2(*v)])).collect::<Vec<_>>(),
                })
            })
            .collect();
        let mut scopes = serde_json::Map::new();
        for (i, n) in named.iter().enumerate() {
            scopes.insert((*n).to_owned(), idx.iter().map(|&k| r2(self.scopes[k][i])).collect::<Vec<_>>().into());
        }
        let sec: Vec<&Second> = {
            let n = self.seconds.len();
            (0..self.s_len).map(|k| &self.seconds[(self.s_head + n - self.s_len + k) % n]).collect()
        };
        let adapter = ctx.adapter.map(|a| {
            json!({"name": a.name, "driver": a.driver, "driver_info": a.driver_info,
                   "backend": format!("{:?}", a.backend), "device_type": format!("{:?}", a.device_type),
                   "vendor": a.vendor, "device": a.device})
        });
        let mb = |b: u64| r2(b as f32 / 1048576.0);
        json!({
            "format": "acr-perf/1",
            "reason": reason.name(),
            "time_utc": utc_stamp(true),
            "uptime_s": r2(self.now()),
            "adapter": adapter,
            "features": ctx.features.map(|f| format!("{f:?}")),
            "gpu_timing": if self.gpu_supported { "timestamp queries, one frame every 0.25 s" } else { "none (the acquire wait stands in)" },
            "settings": ctx.settings.lines().collect::<Vec<_>>(),
            "budgets": {"triangles": BUDGET_TRIANGLES, "shadow_triangles": BUDGET_SHADOW_TRIANGLES, "draws": BUDGET_DRAWS},
            "summary": {
                "window_s": 10, "frames": recent.len(),
                "fps": r2(1000.0 / frame_ms.max(1e-3)),
                "frame_ms": {"mean": r2(frame_ms), "p50": r2(pct(0.5)), "p95": r2(pct(0.95)), "p99": r2(pct(0.99)), "max": r2(pct(1.0))},
                "cpu_ms": {"advance": r2(advance), "render_cpu": r2(render), "acquire": r2(acquire), "present": r2(present)},
                "gpu_ms": gpu_ms.map(|_| json!({"total": r2(g(&|x| x.total)), "ground": r2(g(&|x| x.ground)), "shadow_map": r2(g(&|x| x.shadow)), "3d_pass": r2(g(&|x| x.pass)), "interface": r2(g(&|x| x.interface)), "samples": gpu.len()})),
                "gpu_segments_last": self.gpu_out[..self.gpu_segments].iter().map(|(l, v)| json!([l, r2(*v)])).collect::<Vec<_>>(),
                "counts": counts_json(&last),
                "max": {"triangles": max_of(&|c| c.triangles), "shadow_triangles": max_of(&|c| c.shadow_triangles), "draws": max_of(&|c| c.draws), "units": max_of(&|c| c.units), "ragdolls_awake": max_of(&|c| c.ragdolls_awake), "particles": max_of(&|c| c.particles)},
                "recorder_us": r2(mean(&recent.iter().map(|f| f.own_us).collect::<Vec<_>>())),
                "scope_calls": r2(mean(&recent.iter().map(|f| f.calls as f32).collect::<Vec<_>>())),
            },
            "memory_mb": {"gpu_allocated": mb(self.memory[0]), "gpu_reserved": mb(self.memory[1]), "model_textures": mb(self.memory[2])},
            "diagnosis": diagnosis,
            "top_scopes": top.iter().take(20).map(|(n, m, x)| json!({"name": n, "mean_ms": r2(*m), "max_ms": r2(*x)})).collect::<Vec<_>>(),
            "worst_frames": worst,
            "seconds": {
                "t": sec.iter().map(|s| r2(s.t)).collect::<Vec<_>>(),
                "fps": sec.iter().map(|s| s.frames).collect::<Vec<_>>(),
                "mean_ms": sec.iter().map(|s| r2(s.mean_ms)).collect::<Vec<_>>(),
                "max_ms": sec.iter().map(|s| r2(s.max_ms)).collect::<Vec<_>>(),
                "gpu_ms": sec.iter().map(|s| (s.gpu_ms >= 0.0).then(|| r2(s.gpu_ms))).collect::<Vec<_>>(),
                "triangles": sec.iter().map(|s| s.counts.triangles).collect::<Vec<_>>(),
                "draws": sec.iter().map(|s| s.counts.draws).collect::<Vec<_>>(),
                "units": sec.iter().map(|s| s.counts.units).collect::<Vec<_>>(),
                "gpu_mb": sec.iter().map(|s| r2(s.gpu_mb)).collect::<Vec<_>>(),
                "texture_mb": sec.iter().map(|s| r2(s.texture_mb)).collect::<Vec<_>>(),
            },
            "frames": {
                "t": col(&|f| r2(f.t).into()),
                "ms": col(&|f| r2(f.ms).into()),
                "ticks": col(&|f| f.ticks.into()),
                "gpu_ms": col(&|f| (f.gpu.total >= 0.0).then(|| r2(f.gpu.total)).into()),
                "gpu_shadow_ms": col(&|f| (f.gpu.total >= 0.0).then(|| r2(f.gpu.shadow)).into()),
                "gpu_3d_ms": col(&|f| (f.gpu.total >= 0.0).then(|| r2(f.gpu.pass)).into()),
                "instances": col(&|f| f.counts.instances.into()),
                "drawn": col(&|f| f.counts.drawn.into()),
                "triangles": col(&|f| f.counts.triangles.into()),
                "draws": col(&|f| f.counts.draws.into()),
                "shadow_triangles": col(&|f| f.counts.shadow_triangles.into()),
                "shadow_draws": col(&|f| f.counts.shadow_draws.into()),
                "lod": col(&|f| f.counts.lod.to_vec().into()),
                "units": col(&|f| f.counts.units.into()),
                "corpses": col(&|f| f.counts.corpses.into()),
                "ragdolls_awake": col(&|f| f.counts.ragdolls_awake.into()),
                "particles": col(&|f| f.counts.particles.into()),
                "decals": col(&|f| f.counts.decals.into()),
                "zoom": col(&|f| r2(f.counts.zoom).into()),
                "free_camera": col(&|f| f.counts.free_camera.into()),
                "ticks_per_second": col(&|f| f.counts.ticks_per_second.into()),
                "window": col(&|f| f.counts.window.to_vec().into()),
                "recorder_us": col(&|f| r2(f.own_us).into()),
                "scope_calls": col(&|f| f.calls.into()),
                "scopes_ms": scopes,
            },
        })
    }

    /// Writes a snapshot into [`dir`]: its path, or why not.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn snapshot(&mut self, reason: Reason, ctx: &Context<'_>) -> Result<std::path::PathBuf, String> {
        if !self.on {
            return Err("the recorder is off (ACR_PERF=off)".into());
        }
        let started = Instant::now();
        let dir = dir().ok_or("no folder for the snapshots")?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("snap-{}-{}.json", utc_stamp(false), reason.name()));
        self.memory(ctx.gpu_memory, None);
        let value = self.snapshot_json(reason, ctx);
        std::fs::write(&path, serde_json::to_vec(&value).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        self.snapshots += 1;
        log::info!("perf: snapshot ({}) written to {} in {:?}", reason.name(), path.display(), started.elapsed());
        Ok(path)
    }
}

fn counts_json(c: &Counts) -> serde_json::Value {
    serde_json::json!({
        "instances": c.instances, "drawn": c.drawn, "poses": c.poses,
        "triangles": c.triangles, "draws": c.draws,
        "shadow_triangles": c.shadow_triangles, "shadow_draws": c.shadow_draws,
        "lod": c.lod, "units": c.units, "corpses": c.corpses,
        "ragdolls_awake": c.ragdolls_awake, "particles": c.particles, "decals": c.decals,
        "zoom": (f64::from(c.zoom) * 100.0).round() / 100.0, "free_camera": c.free_camera,
        "models_drawn": c.models_phase, "shadows": c.shadows,
        "ticks_per_second": c.ticks_per_second, "window": c.window,
    })
}

/// A short automatic reading of the last seconds: what bounds the frame, the top scopes, the
/// counts against the budgets, and levers to try.
fn diagnose(
    frame: f32,
    gpu: Option<f32>,
    advance: f32,
    render: f32,
    acquire: f32,
    c: &Counts,
    top: &[(&str, f32, f32)],
) -> serde_json::Value {
    let mut lines = Vec::new();
    let cpu = advance + render;
    let share = |v: f32| (v / frame.max(1e-3) * 100.0).round();
    // Waiting for the swapchain means the GPU is behind, unless the GPU is mostly idle
    // (then it is the display's refresh).
    let waits = acquire > 0.4 * frame;
    let bound = match gpu {
        Some(g) if g > 0.75 * frame || (waits && g > 0.5 * frame && g > cpu) => "gpu",
        None if waits && !(15.5..17.5).contains(&frame) => "gpu",
        _ if cpu > 0.6 * frame => "cpu",
        _ if (15.5..17.5).contains(&frame) => "vsync",
        _ => "mixed",
    };
    lines.push(format!("{:.1} fps, frame {frame:.1} ms: {bound}-bound", 1000.0 / frame.max(1e-3)));
    match gpu {
        Some(g) => lines.push(format!("GPU {g:.1} ms ({}% of the frame, timestamp queries)", share(g))),
        None => lines.push("no GPU timestamps on this adapter: the acquire wait stands in".into()),
    }
    lines.push(format!(
        "CPU: advance {advance:.1} ms ({}%), render {render:.1} ms ({}%), acquire wait {acquire:.1} ms ({}%)",
        share(advance),
        share(render),
        share(acquire)
    ));
    let inner: Vec<String> = top
        .iter()
        .filter(|(n, ..)| !n.starts_with("frame."))
        .take(5)
        .map(|(n, m, _)| format!("{n} {m:.2} ms"))
        .collect();
    if !inner.is_empty() {
        lines.push(format!("top scopes: {}", inner.join(", ")));
    }
    let over = |v: u32, b: u32| {
        let budget = if b >= 100_000 { format!("{:.1} M", b as f32 / 1e6) } else { b.to_string() };
        if v > b { format!(" (budget {budget}, {:.1}x)", v as f32 / b as f32) } else { String::new() }
    };
    lines.push(format!(
        "triangles {:.2} M{}, shadow cast {:.2} M{}, draws {}{}, shadow draws {}",
        c.triangles as f32 / 1e6,
        over(c.triangles, BUDGET_TRIANGLES),
        c.shadow_triangles as f32 / 1e6,
        over(c.shadow_triangles, BUDGET_SHADOW_TRIANGLES),
        c.draws,
        over(c.draws, BUDGET_DRAWS),
        c.shadow_draws
    ));
    let mut levers = Vec::new();
    if bound == "gpu" {
        if c.shadows && c.shadow_triangles > BUDGET_SHADOW_TRIANGLES {
            levers.push("shadows=off (shadow casters over budget)");
        }
        let near: u32 = c.lod[0];
        let all: u32 = c.lod.iter().sum();
        if all > 200 && near * 2 > all {
            levers.push("most models at the full mesh: check lod=on and the LOD sizes");
        }
        if c.triangles > BUDGET_TRIANGLES {
            levers.push("models=units or zoom in (triangles over budget)");
        }
        if c.draws > BUDGET_DRAWS {
            levers.push("too many draws: batching or fewer model types");
        }
    }
    if bound == "cpu" && c.ragdolls_awake > 60 {
        levers.push("many awake ragdolls (rag.* scopes)");
    }
    serde_json::json!({ "bound": bound, "lines": lines, "levers": levers })
}

/// Where snapshots go: `ACR_PERF_DIR`, else `perf/` next to `ACR_LOG`, else
/// `%LOCALAPPDATA%\ACR\perf`.
#[cfg(not(target_arch = "wasm32"))]
pub fn dir() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if let Some(d) = std::env::var_os("ACR_PERF_DIR") {
        return Some(PathBuf::from(d));
    }
    if let Some(log) = std::env::var_os("ACR_LOG") {
        return PathBuf::from(log).parent().map(|d| d.join("perf"));
    }
    #[cfg(target_os = "android")]
    return crate::android::data_dir().map(|d| d.join("perf"));
    #[cfg(not(target_os = "android"))]
    crate::native::data_dir().map(|d| d.join("perf"))
}

/// The UTC time now: `2026-10-25T09:09:12Z`, or `20261025-090912` for file names.
fn utc_stamp(iso: bool) -> String {
    let secs = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    // Days to civil date (Howard Hinnant's algorithm).
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let (hh, mm, ss) = (rem / 3600, rem / 60 % 60, rem % 60);
    if iso {
        format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
    } else {
        format!("{y:04}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_keeps_the_last_frames_and_names_the_scopes() {
        let mut r = Recorder::new(true);
        let counts = Counts { in_game: true, triangles: 10_100_000, draws: 2627, ..Counts::default() };
        for _ in 0..(FRAMES + 10) {
            acr_sim::prof::add("test.scope", 2_000_000);
            r.frame(&counts, None);
        }
        assert_eq!(r.len, FRAMES);
        let v = r.snapshot_json(
            Reason::Driver,
            &Context { settings: "models=all\n", adapter: None, features: None, gpu_memory: None },
        );
        assert_eq!(v["frames"]["ms"].as_array().map(Vec::len), Some(FRAMES));
        assert_eq!(v["frames"]["scopes_ms"]["test.scope"][0], 2.0);
        assert_eq!(v["settings"][0], "models=all");
        let lines = v["diagnosis"]["lines"].to_string();
        assert!(lines.contains("10.10 M (budget 3.0 M, 3.4x)"), "{lines}");
    }

    #[test]
    fn slow_frames_ask_for_one_automatic_snapshot() {
        let mut r = Recorder::new(true);
        let counts = Counts { in_game: true, ..Counts::default() };
        // Fake slow frames by moving the last frame's time back.
        let mut due = 0;
        for _ in 0..20 {
            r.last = Some(Instant::now() - std::time::Duration::from_millis(300));
            r.start -= std::time::Duration::from_millis(300);
            due += usize::from(r.frame(&counts, None).is_some());
        }
        assert_eq!(due, 1);
    }

    #[test]
    fn utc_stamps_have_their_shape() {
        assert_eq!(utc_stamp(true).len(), 20);
        assert_eq!(utc_stamp(false).len(), 15);
    }
}
