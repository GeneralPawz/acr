//! Model textures loaded when first needed, with mipmaps, and dropped when unused
//! (docs/rendering-3d.md "Texture streaming").
//!
//! A glb with authored levels ([`crate::model::ModelData::encoded`]) keeps the pictures of its
//! finer levels encoded until a model first draws at such a level. [`TexCache`] tracks those
//! pictures (pending, resident, failed) and picks the ones to drop: unused for
//! [`TexCache::idle`] frames, or the least recently used beyond the memory budget once unused
//! for [`TexCache::grace`] frames. [`Decoder`] decodes them off the main thread (native) or
//! one per frame (wasm, no threads), and builds the mip chain ([`mip_chain`]).

use std::collections::HashMap;
use std::sync::Arc;

use crate::model::Image;

/// A picture of a model: (model index, image index).
pub type Key = (usize, usize);

/// Memory budget of the streamed textures: 512 MB native, 192 MB in the browser.
pub const BUDGET: u64 = if cfg!(target_arch = "wasm32") { 192 << 20 } else { 512 << 20 };
/// Bytes of streamed pictures written to the GPU per frame (a 4 k colour map with mips,
/// 89 MB, goes up in about 11 frames native, 45 in the browser).
pub const UPLOAD_PER_FRAME: u64 = if cfg!(target_arch = "wasm32") { 2 << 20 } else { 8 << 20 };
/// Frames a picture stays after its last use whatever the budget (about 30 s at 60 fps).
pub const IDLE: u64 = 1800;
/// Frames a picture stays after its last use when the budget is exceeded (about 2 s).
pub const GRACE: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Pending,
    Resident {
        bytes: u64,
        used: u64,
    },
    /// Decoding failed: not asked again (the stand-in level stays).
    Failed,
}

/// The bookkeeping of the streamed pictures (no GPU objects: the renderer holds those).
#[derive(Debug)]
pub struct TexCache {
    pub budget: u64,
    pub idle: u64,
    pub grace: u64,
    frame: u64,
    states: HashMap<Key, State>,
    resident: u64,
    pub loads: u64,
    pub evictions: u64,
}

impl Default for TexCache {
    fn default() -> Self {
        Self::new(BUDGET, IDLE, GRACE)
    }
}

impl TexCache {
    pub fn new(budget: u64, idle: u64, grace: u64) -> Self {
        Self { budget, idle, grace, frame: 0, states: HashMap::new(), resident: 0, loads: 0, evictions: 0 }
    }

    /// The next frame.
    pub fn tick(&mut self) {
        self.frame += 1;
    }

    /// The picture is drawn this frame: marks a resident one used; true when it is absent
    /// and must be decoded (it is pending from now).
    pub fn want(&mut self, key: Key) -> bool {
        match self.states.get_mut(&key) {
            Some(State::Resident { used, .. }) => {
                *used = self.frame;
                false
            }
            Some(_) => false,
            None => {
                self.states.insert(key, State::Pending);
                true
            }
        }
    }

    /// A pending picture arrived with `bytes` of GPU memory; false when it is no longer
    /// wanted (its model went away).
    pub fn loaded(&mut self, key: Key, bytes: u64) -> bool {
        match self.states.get_mut(&key) {
            Some(s @ State::Pending) => {
                *s = State::Resident { bytes, used: self.frame };
                self.resident += bytes;
                self.loads += 1;
                true
            }
            _ => false,
        }
    }

    pub fn failed(&mut self, key: Key) {
        if let Some(s @ State::Pending) = self.states.get_mut(&key) {
            *s = State::Failed;
        }
    }

    pub fn resident(&self, key: Key) -> bool {
        matches!(self.states.get(&key), Some(State::Resident { .. }))
    }

    pub fn resident_bytes(&self) -> u64 {
        self.resident
    }

    pub fn pending(&self) -> usize {
        self.states.values().filter(|s| **s == State::Pending).count()
    }

    /// The pictures to drop now: those unused for [`Self::idle`] frames, then the least
    /// recently used ones unused for [`Self::grace`] frames while over the budget. They are
    /// forgotten: a later [`Self::want`] asks for them again.
    pub fn evict(&mut self) -> Vec<Key> {
        let mut residents: Vec<(u64, Key, u64)> = self
            .states
            .iter()
            .filter_map(|(&k, s)| match *s {
                State::Resident { bytes, used } => Some((used, k, bytes)),
                _ => None,
            })
            .collect();
        residents.sort_unstable();
        let mut out = Vec::new();
        for (used, key, bytes) in residents {
            let unused = self.frame - used;
            if unused >= self.idle || (self.resident > self.budget && unused >= self.grace) {
                self.states.remove(&key);
                self.resident -= bytes;
                self.evictions += 1;
                out.push(key);
            }
        }
        out
    }
}

/// The mip chain of `image`, the full size first, each level a 2 × 2 box of the one above
/// (odd sizes clamp the last row and column). sRGB pictures average in linear light.
pub fn mip_chain(image: &Image, srgb: bool) -> Vec<Image> {
    let to_linear: Vec<f32> = (0..256)
        .map(|c| {
            let c = c as f32 / 255.0;
            if srgb { if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) } } else { c }
        })
        .collect();
    let to_byte = |v: f32| -> u8 {
        let v = v.clamp(0.0, 1.0);
        let e = if srgb { if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 } } else { v };
        (e * 255.0 + 0.5) as u8
    };
    let mut chain = vec![image.clone()];
    loop {
        let top = chain.last().expect("the full size");
        if (top.width <= 1 && top.height <= 1) || top.rgba.len() != (top.width * top.height * 4) as usize {
            break;
        }
        let (w, h) = ((top.width / 2).max(1), (top.height / 2).max(1));
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let at = |dx: u32, dy: u32| {
                    let (sx, sy) = ((2 * x + dx).min(top.width - 1), (2 * y + dy).min(top.height - 1));
                    ((sy * top.width + sx) * 4) as usize
                };
                let px = [at(0, 0), at(1, 0), at(0, 1), at(1, 1)];
                for c in 0..3 {
                    let sum: f32 = px.iter().map(|&p| to_linear[usize::from(top.rgba[p + c])]).sum();
                    rgba.push(to_byte(sum / 4.0));
                }
                let alpha: u32 = px.iter().map(|&p| u32::from(top.rgba[p + 3])).sum();
                rgba.push(((alpha + 2) / 4) as u8);
            }
        }
        chain.push(Image { width: w, height: h, rgba });
    }
    chain
}

/// GPU bytes of a picture of this size with (or without) its mip chain.
pub fn texture_bytes(width: u32, height: u32, mips: bool) -> u64 {
    let mut total = 0;
    let (mut w, mut h) = (u64::from(width), u64::from(height));
    loop {
        total += w * h * 4;
        if !mips || (w <= 1 && h <= 1) {
            return total;
        }
        (w, h) = ((w / 2).max(1), (h / 2).max(1));
    }
}

/// One picture to decode.
pub struct Job {
    pub key: Key,
    pub bytes: Arc<[u8]>,
    pub srgb: bool,
    pub mips: bool,
}

/// A decoded picture: its mip chain, or why it failed.
pub struct Done {
    pub key: Key,
    pub srgb: bool,
    pub chain: Result<Vec<Image>, String>,
}

fn run(job: Job) -> Done {
    let chain =
        crate::model::decode_image(&job.bytes).map(|img| if job.mips { mip_chain(&img, job.srgb) } else { vec![img] });
    Done { key: job.key, srgb: job.srgb, chain }
}

/// Decodes [`Job`]s: on a thread each (native), or one per [`Decoder::poll`] on the caller's
/// thread (wasm32 has no threads).
pub struct Decoder {
    #[cfg(not(target_arch = "wasm32"))]
    sender: std::sync::mpsc::Sender<Done>,
    #[cfg(not(target_arch = "wasm32"))]
    receiver: std::sync::mpsc::Receiver<Done>,
    #[cfg(target_arch = "wasm32")]
    queue: std::collections::VecDeque<Job>,
}

#[allow(clippy::derivable_impls)] // derivable on wasm32 only
impl Default for Decoder {
    fn default() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (sender, receiver) = std::sync::mpsc::channel();
            Self { sender, receiver }
        }
        #[cfg(target_arch = "wasm32")]
        Self { queue: Default::default() }
    }
}

impl Decoder {
    pub fn push(&mut self, job: Job) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let sender = self.sender.clone();
            let spawned = std::thread::Builder::new().name("acr-texture".into()).spawn(move || {
                let _ = sender.send(run(job));
            });
            if let Err(e) = spawned {
                log::warn!("textures: no decoding thread: {e}");
            }
        }
        #[cfg(target_arch = "wasm32")]
        self.queue.push_back(job);
    }

    /// The pictures decoded since the last call.
    pub fn poll(&mut self) -> Vec<Done> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.receiver.try_iter().collect()
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.queue.pop_front().map(run).into_iter().collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wanted_pictures_load_once() {
        let mut c = TexCache::new(100, 10, 2);
        assert!(c.want((0, 1)));
        assert!(!c.want((0, 1)), "pending: not asked twice");
        assert_eq!(c.pending(), 1);
        assert!(c.loaded((0, 1), 40));
        assert!(c.resident((0, 1)));
        assert_eq!(c.resident_bytes(), 40);
        assert!(!c.want((0, 1)));
        assert!(!c.loaded((0, 2), 5), "never asked for");
        assert_eq!(c.resident_bytes(), 40);
    }

    #[test]
    fn idle_pictures_go() {
        let mut c = TexCache::new(1000, 5, 2);
        c.want((0, 0));
        c.loaded((0, 0), 10);
        for _ in 0..4 {
            c.tick();
            assert!(c.evict().is_empty(), "under the budget and recently used");
        }
        c.tick();
        assert_eq!(c.evict(), vec![(0, 0)]);
        assert_eq!(c.resident_bytes(), 0);
        assert!(c.want((0, 0)), "asked again once dropped");
    }

    #[test]
    fn over_budget_drops_least_recent_after_grace() {
        let mut c = TexCache::new(100, 1000, 2);
        for i in 0..3 {
            c.want((0, i));
            c.loaded((0, i), 50);
            c.tick();
        }
        // 150 of 100: (0, 0) unused for 3 frames, (0, 1) for 2, (0, 2) for 1.
        assert_eq!(c.evict(), vec![(0, 0)]);
        assert_eq!(c.resident_bytes(), 100);
        // In use: kept although over the budget.
        c.want((0, 3));
        c.loaded((0, 3), 50);
        c.want((0, 1));
        c.want((0, 2));
        c.want((0, 3));
        assert!(c.evict().is_empty());
        c.tick();
        c.tick();
        c.want((0, 3));
        assert_eq!(c.evict(), vec![(0, 1)], "the oldest of the unused, until within the budget");
        assert_eq!(c.resident_bytes(), 100);
        assert_eq!(c.evictions, 2);
    }

    #[test]
    fn failed_pictures_are_not_asked_again() {
        let mut c = TexCache::default();
        assert!(c.want((1, 0)));
        c.failed((1, 0));
        assert!(!c.want((1, 0)));
        assert_eq!(c.pending(), 0);
    }

    #[test]
    fn mip_chain_halves_to_one_texel() {
        let img = Image { width: 5, height: 2, rgba: (0..40).map(|i| if i % 4 == 3 { 255 } else { 200 }).collect() };
        let chain = mip_chain(&img, true);
        let sizes: Vec<(u32, u32)> = chain.iter().map(|i| (i.width, i.height)).collect();
        assert_eq!(sizes, vec![(5, 2), (2, 1), (1, 1)]);
        assert_eq!(chain[2].rgba, vec![200, 200, 200, 255], "a flat colour stays");
        let bytes: u64 = chain.iter().map(|i| i.rgba.len() as u64).sum();
        assert_eq!(bytes, texture_bytes(5, 2, true));
        // Black and white average to mid grey in linear light, brighter than 128 in sRGB.
        let bw = Image { width: 2, height: 1, rgba: vec![0, 0, 0, 255, 255, 255, 255, 255] };
        assert_eq!(mip_chain(&bw, true)[1].rgba[0], 188);
        assert_eq!(mip_chain(&bw, false)[1].rgba[0], 128);
    }

    #[test]
    fn texture_bytes_count_the_chain() {
        assert_eq!(texture_bytes(4096, 4096, false), 64 << 20);
        let with = texture_bytes(4096, 4096, true);
        assert!(with > 85 << 20 && with < 86 << 20, "{with}");
    }
}
