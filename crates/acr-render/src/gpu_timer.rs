//! A GPU timer from timestamp queries, for the driver's `gputime` (docs/ground.md).
//!
//! The renderers call [`mark_encoder`] between passes and [`mark_pass`] inside the 3D pass;
//! each mark closes the segment named by its label since the previous mark. The marks do
//! nothing unless a measurement is running ([`begin`] … [`finish`]) and the device has
//! `TIMESTAMP_QUERY` and `TIMESTAMP_QUERY_INSIDE_ENCODERS` (`..._INSIDE_PASSES` is optional)
//! (WebGL2 has none of them: [`supported`] says why).
//!
//! [`Sampler`] times a frame now and then for the game's performance recorder without
//! splitting the 3D pass and without waiting: the stamps are read back a few frames later.

use std::cell::RefCell;

/// The most marks one frame may write.
const MAX_MARKS: u32 = 32;

/// The features a measurement needs.
pub fn features() -> wgpu::Features {
    wgpu::Features::TIMESTAMP_QUERY
        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES
}

/// `Ok` when `device` can time passes, else why not.
pub fn supported(device: &wgpu::Device) -> Result<(), String> {
    // Inside-pass stamps are optional: the 3D pass is split at the marks while timing.
    let missing = (features() - wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES) - device.features();
    if missing.is_empty() { Ok(()) } else { Err(format!("unsupported: the device lacks {missing:?}")) }
}

struct Timer {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    labels: Vec<&'static str>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    period_ns: f32,
    /// The 3D pass is split at the marks (`gputime`); a [`Sampler`]'s frame is not.
    split: bool,
}

thread_local! {
    static TIMER: RefCell<Option<Timer>> = const { RefCell::new(None) };
}

/// Starts timing one frame: the next marks are recorded.
pub fn begin(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<(), String> {
    supported(device)?;
    let timer = Timer::new(device, queue, true);
    TIMER.with(|t| *t.borrow_mut() = Some(timer));
    Ok(())
}

impl Timer {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, split: bool) -> Self {
        let size = u64::from(MAX_MARKS) * 8;
        Timer {
            set: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("gpu timer"),
                ty: wgpu::QueryType::Timestamp,
                count: MAX_MARKS,
            }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu timer resolve"),
                size,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu timer readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            labels: Vec::new(),
            period_ns: queue.get_timestamp_period(),
            split,
        }
    }
}

/// True while a measurement runs: callers may split a pass at the marks, because
/// timestamps inside a pass are not reliable on tiling GPUs (Adreno, Apple, Mali). False
/// for a [`Sampler`]'s frame, which keeps the frame's passes as they are.
pub fn active() -> bool {
    TIMER.with(|t| t.borrow().as_ref().is_some_and(|t| t.split))
}

/// The next query index for a mark labelled `label`, if timing and not full.
fn next(label: &'static str) -> Option<(wgpu::QuerySet, u32)> {
    TIMER.with(|t| {
        let mut t = t.borrow_mut();
        let timer = t.as_mut()?;
        let index = timer.labels.len() as u32;
        (index < MAX_MARKS).then(|| {
            timer.labels.push(label);
            (timer.set.clone(), index)
        })
    })
}

/// Marks the end of the segment `label` between passes.
pub fn mark_encoder(encoder: &mut wgpu::CommandEncoder, label: &'static str) {
    if let Some((set, index)) = next(label) {
        encoder.write_timestamp(&set, index);
    }
}

/// Marks the end of the segment `label` inside a pass.
pub fn mark_pass(pass: &mut wgpu::RenderPass<'_>, label: &'static str) {
    if let Some((set, index)) = next(label) {
        pass.write_timestamp(&set, index);
    }
}

/// Resolves the frame's marks into the readback buffer; call before submitting `encoder`.
pub fn resolve(encoder: &mut wgpu::CommandEncoder) {
    TIMER.with(|t| {
        let t = t.borrow();
        let Some(timer) = t.as_ref() else { return };
        let n = timer.labels.len() as u32;
        if n > 0 {
            encoder.resolve_query_set(&timer.set, 0..n, &timer.resolve, 0);
            encoder.copy_buffer_to_buffer(&timer.resolve, 0, &timer.readback, 0, u64::from(n) * 8);
        }
    })
}

/// Ends the measurement after the frame was submitted: waits for the GPU and returns the
/// milliseconds per segment, in mark order (the first mark only opens the frame). Native
/// only (it blocks on the device).
#[cfg(not(target_arch = "wasm32"))]
pub fn finish(device: &wgpu::Device) -> Result<Vec<(&'static str, f64)>, String> {
    let Some(timer) = TIMER.with(|t| t.borrow_mut().take()) else { return Err("not timing".into()) };
    let n = timer.labels.len();
    if n < 2 {
        return Ok(Vec::new());
    }
    let (tx, rx) = std::sync::mpsc::channel();
    timer.readback.map_async(wgpu::MapMode::Read, .., move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(10)) })
        .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    let stamps: Vec<u64> = {
        let data = timer.readback.get_mapped_range(..).map_err(|e| e.to_string())?;
        data[..n * 8].as_chunks::<8>().0.iter().map(|b| u64::from_le_bytes(*b)).collect()
    };
    timer.readback.unmap();
    let ms = f64::from(timer.period_ns) / 1e6;
    Ok(timer.labels[1..]
        .iter()
        .zip(stamps.windows(2))
        .map(|(label, w)| (*label, w[1].saturating_sub(w[0]) as f64 * ms))
        .collect())
}

/// The most segments a [`Sampler`] reports.
pub const SAMPLED: usize = MAX_MARKS as usize - 1;

/// Times one frame now and then without waiting for the GPU (the performance recorder):
/// [`Sampler::begin`] before the frame, [`Sampler::end`] after its submit, and
/// [`Sampler::poll`] on the next frames until the stamps are back. One query set and its
/// buffers are made once and reused.
#[derive(Default)]
pub struct Sampler {
    idle: Option<Timer>,
    pending: Option<(Timer, std::sync::Arc<std::sync::atomic::AtomicU8>)>,
    unsupported: bool,
}

impl Sampler {
    /// Starts timing the next frame: false when the device cannot time or a frame is still
    /// read back.
    pub fn begin(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        if self.unsupported || self.pending.is_some() || active() {
            return false;
        }
        if supported(device).is_err() {
            self.unsupported = true;
            return false;
        }
        let mut timer = self.idle.take().unwrap_or_else(|| Timer::new(device, queue, false));
        timer.labels.clear();
        TIMER.with(|t| *t.borrow_mut() = Some(timer));
        true
    }

    /// After the frame was submitted: asks for its stamps.
    pub fn end(&mut self) {
        let Some(timer) = TIMER.with(|t| {
            let mut t = t.borrow_mut();
            if t.as_ref().is_some_and(|t| !t.split) { t.take() } else { None }
        }) else {
            return;
        };
        if timer.labels.len() < 2 {
            self.idle = Some(timer);
            return;
        }
        // 0: waiting, 1: mapped, 2: failed.
        let state = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
        let flag = state.clone();
        timer.readback.map_async(wgpu::MapMode::Read, .., move |r| {
            flag.store(if r.is_ok() { 1 } else { 2 }, std::sync::atomic::Ordering::Release);
        });
        self.pending = Some((timer, state));
    }

    /// The timed frame's milliseconds per segment once they are back (written into `out`,
    /// in mark order; the count is returned), else `None`. Never blocks.
    pub fn poll(&mut self, device: &wgpu::Device, out: &mut [(&'static str, f32); SAMPLED]) -> Option<usize> {
        let (_, state) = self.pending.as_ref()?;
        if state.load(std::sync::atomic::Ordering::Acquire) == 0 {
            let _ = device.poll(wgpu::PollType::Poll);
        }
        let done = state.load(std::sync::atomic::Ordering::Acquire);
        if done == 0 {
            return None;
        }
        let (timer, _) = self.pending.take()?;
        let n = timer.labels.len().min(MAX_MARKS as usize);
        let mut count = None;
        if done == 1 {
            if let Ok(data) = timer.readback.get_mapped_range(..) {
                let ms = timer.period_ns / 1e6;
                let mut last = None;
                let mut k = 0;
                for (i, b) in data[..n * 8].as_chunks::<8>().0.iter().enumerate() {
                    let stamp = u64::from_le_bytes(*b);
                    if let Some(prev) = last
                        && k < SAMPLED
                    {
                        out[k] = (timer.labels[i], stamp.saturating_sub(prev) as f32 * ms);
                        k += 1;
                    }
                    last = Some(stamp);
                }
                count = Some(k);
            }
            timer.readback.unmap();
        }
        self.idle = Some(timer);
        count
    }
}
