//! Model textures loaded when first needed, with mipmaps, and dropped when unused
//! (docs/rendering-3d.md "Texture streaming").
//!
//! A glb with authored levels ([`crate::model::ModelData::encoded`]) keeps the pictures of its
//! finer levels encoded until a model first draws at such a level. [`TexCache`] tracks those
//! pictures (pending, resident, failed) and picks the ones to drop: unused for
//! [`TexCache::idle`] frames, or the least recently used beyond the memory budget once unused
//! for [`TexCache::grace`] frames. [`Decoder`] decodes them off the main thread: on threads
//! (native) or with the browser's `createImageBitmap` (wasm). The mips are made on the GPU
//! ([`crate::mipgen`]).

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

/// Rows per upload strip of a picture `width` wide ([`UPLOAD_PER_FRAME`] bytes, at least one).
pub fn strip_rows(width: u32) -> u32 {
    (UPLOAD_PER_FRAME / (u64::from(width.max(1)) * 4)).clamp(1, u64::from(u32::MAX)) as u32
}

/// One picture to decode.
pub struct Job {
    pub key: Key,
    pub bytes: Arc<[u8]>,
    pub srgb: bool,
    pub mips: bool,
}

/// The full-size pixels of a decoded picture (the mips are made on the GPU, [`crate::mipgen`]).
pub enum Pixels {
    Rgba(Image),
    /// wasm32: decoded by the browser off the main thread (`createImageBitmap`), cut into
    /// strips of [`strip_rows`] rows: (first row, strip).
    #[cfg(target_arch = "wasm32")]
    Bitmaps {
        width: u32,
        height: u32,
        strips: Vec<(u32, web_sys::ImageBitmap)>,
    },
}

impl Pixels {
    pub fn size(&self) -> (u32, u32) {
        match self {
            Pixels::Rgba(img) => (img.width, img.height),
            #[cfg(target_arch = "wasm32")]
            Pixels::Bitmaps { width, height, .. } => (*width, *height),
        }
    }
}

/// A decoded picture, or why it failed.
pub struct Done {
    pub key: Key,
    pub srgb: bool,
    pub mips: bool,
    pub pixels: Result<Pixels, String>,
}

fn run(job: Job) -> Done {
    let pixels = crate::model::decode_image(&job.bytes).map(Pixels::Rgba);
    Done { key: job.key, srgb: job.srgb, mips: job.mips, pixels }
}

/// Decodes a PNG or JPEG with the browser's `createImageBitmap` (off the main thread), then
/// cuts it into strips for the per-frame upload budget. Colours stay as stored: no colour
/// space conversion, no premultiplied alpha.
#[cfg(target_arch = "wasm32")]
async fn browser_decode(bytes: &[u8]) -> Result<Pixels, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let err = |e: wasm_bindgen::JsValue| format!("createImageBitmap: {e:?}");
    let window = web_sys::window().ok_or("no window")?;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts).map_err(err)?;
    let options = web_sys::ImageBitmapOptions::new();
    options.set_premultiply_alpha(web_sys::PremultiplyAlpha::None);
    options.set_color_space_conversion(web_sys::ColorSpaceConversion::None);
    let promise = window.create_image_bitmap_with_blob_and_image_bitmap_options(&blob, &options).map_err(err)?;
    let full: web_sys::ImageBitmap = JsFuture::from(promise).await.map_err(err)?.dyn_into().map_err(err)?;
    let (width, height) = (full.width(), full.height());
    let rows = strip_rows(width);
    if rows >= height {
        return Ok(Pixels::Bitmaps { width, height, strips: vec![(0, full)] });
    }
    let mut strips = Vec::new();
    for y in (0..height).step_by(rows as usize) {
        let n = rows.min(height - y);
        let promise = window
            .create_image_bitmap_with_image_bitmap_and_a_sx_and_a_sy_and_a_sw_and_a_sh_and_a_options(
                &full,
                0,
                y as i32,
                width as i32,
                n as i32,
                &options,
            )
            .map_err(err)?;
        strips.push((y, JsFuture::from(promise).await.map_err(err)?.dyn_into().map_err(err)?));
    }
    full.close();
    Ok(Pixels::Bitmaps { width, height, strips })
}

/// Decodes [`Job`]s: on a thread each (native), or by the browser's `createImageBitmap`
/// (wasm32, no threads; one per [`Decoder::poll`] on the caller's thread without a window).
pub struct Decoder {
    #[cfg(not(target_arch = "wasm32"))]
    sender: std::sync::mpsc::Sender<Done>,
    #[cfg(not(target_arch = "wasm32"))]
    receiver: std::sync::mpsc::Receiver<Done>,
    #[cfg(target_arch = "wasm32")]
    done: std::rc::Rc<std::cell::RefCell<Vec<Done>>>,
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
        Self { done: Default::default(), queue: Default::default() }
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
        {
            if web_sys::window().is_none() {
                self.queue.push_back(job);
                return;
            }
            let done = self.done.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let pixels = browser_decode(&job.bytes).await;
                done.borrow_mut().push(Done { key: job.key, srgb: job.srgb, mips: job.mips, pixels });
            });
        }
    }

    /// The pictures decoded since the last call.
    pub fn poll(&mut self) -> Vec<Done> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.receiver.try_iter().collect()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let mut out = std::mem::take(&mut *self.done.borrow_mut());
            out.extend(self.queue.pop_front().map(run));
            out
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
    fn strips_fit_the_budget() {
        assert_eq!(u64::from(strip_rows(4096)) * 4096 * 4, UPLOAD_PER_FRAME);
        assert_eq!(strip_rows(u32::MAX), 1, "at least one row");
        assert!(u64::from(strip_rows(16)) * 64 <= UPLOAD_PER_FRAME);
    }

    #[test]
    fn texture_bytes_count_the_chain() {
        assert_eq!(texture_bytes(4096, 4096, false), 64 << 20);
        let with = texture_bytes(4096, 4096, true);
        assert!(with > 85 << 20 && with < 86 << 20, "{with}");
    }
}
