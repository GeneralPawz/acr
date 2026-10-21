//! Game files the menus need, loaded on demand.
//!
//! Screens ask for files by their game path; missing files are queued for the platform
//! [`acr_app::Assets`] and arrive later (immediately on native, asynchronously in the
//! browser). Parsed forms (bitmaps, sprite packs, palettes, sounds) are cached.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use acr_formats::{Bitmap, GpFile, Palette, normalize_path};

/// State of one requested file.
enum Entry {
    Pending,
    Loaded(Rc<[u8]>),
    Failed,
}

#[derive(Default)]
pub struct Library {
    /// All game paths (normalized) that exist.
    known: HashSet<String>,
    files: HashMap<String, Entry>,
    queue: RefCell<Vec<String>>,
    wanted: RefCell<HashSet<String>>,
    bitmaps: RefCell<HashMap<String, Option<Rc<Bitmap>>>>,
    sprites: RefCell<HashMap<String, Option<Rc<GpFile>>>>,
    palettes: RefCell<HashMap<String, Option<Rc<Palette>>>>,
    sizes: RefCell<HashMap<(String, usize), [i32; 2]>>,
}

impl Library {
    pub fn new(paths: &[String]) -> Self {
        Self { known: paths.iter().map(|p| normalize_path(p)).collect(), ..Default::default() }
    }

    /// Whether the game has a file at `path`.
    pub fn exists(&self, path: &str) -> bool {
        self.known.contains(&normalize_path(path))
    }

    /// Visible game paths (normalized, `/` separated) starting with `prefix`, sorted.
    pub fn list(&self, prefix: &str) -> Vec<String> {
        let prefix = normalize_path(prefix);
        let mut found: Vec<String> = self.known.iter().filter(|p| p.starts_with(&prefix)).cloned().collect();
        found.sort();
        found
    }

    /// Asks for `path` to be loaded if it is not yet. Unknown paths fail right away.
    pub fn want(&self, path: &str) {
        let key = normalize_path(path);
        if !self.known.contains(&key) || self.files.contains_key(&key) || !self.wanted.borrow_mut().insert(key.clone())
        {
            return;
        }
        self.queue.borrow_mut().push(path.to_owned());
    }

    /// Paths to hand to the platform loader; each is reported back through [`Library::insert`].
    pub fn take_requests(&mut self) -> Vec<String> {
        let requests = std::mem::take(self.queue.get_mut());
        for path in &requests {
            self.files.insert(normalize_path(path), Entry::Pending);
        }
        requests
    }

    /// Stores a loaded file (or its failure). Returns whether it was requested.
    pub fn insert(&mut self, path: &str, result: Result<Vec<u8>, String>) -> bool {
        let key = normalize_path(path);
        if !matches!(self.files.get(&key), Some(Entry::Pending)) {
            return false;
        }
        let entry = match result {
            Ok(bytes) => Entry::Loaded(bytes.into()),
            Err(e) => {
                log::warn!("{path}: {e}");
                Entry::Failed
            }
        };
        self.files.insert(key, entry);
        true
    }

    /// True once `path` has been loaded or has failed (unknown paths count as failed).
    pub fn settled(&self, path: &str) -> bool {
        self.want(path);
        let key = normalize_path(path);
        matches!(self.files.get(&key), Some(Entry::Loaded(_) | Entry::Failed)) || !self.known.contains(&key)
    }

    /// The bytes of `path`, requesting it if needed.
    pub fn bytes(&self, path: &str) -> Option<Rc<[u8]>> {
        let key = normalize_path(path);
        if !self.known.contains(&key) {
            return None;
        }
        match self.files.get(&key) {
            Some(Entry::Loaded(bytes)) => Some(bytes.clone()),
            Some(_) => None,
            None => {
                self.want(path);
                None
            }
        }
    }

    pub fn bitmap(&self, path: &str) -> Option<Rc<Bitmap>> {
        cached(&self.bitmaps, path, || {
            let bytes = self.bytes(path)?;
            Some(Bitmap::parse(&bytes).map_err(|e| log::error!("{path}: {e}")).ok().map(Rc::new))
        })
    }

    pub fn sprites(&self, path: &str) -> Option<Rc<GpFile>> {
        cached(&self.sprites, path, || {
            let bytes = self.bytes(path)?;
            Some(GpFile::parse(bytes.to_vec()).map_err(|e| log::error!("{path}: {e}")).ok().map(Rc::new))
        })
    }

    /// `GetGPWidth` / `GetGPHeight` of a frame: the largest right and bottom edge of its
    /// chunks (`GP_Header::GetLx`/`GetLy`); zero if the pack is not loaded.
    pub fn frame_size(&self, gp: &str, frame: usize) -> [i32; 2] {
        let key = (normalize_path(gp), frame);
        if let Some(size) = self.sizes.borrow().get(&key) {
            return *size;
        }
        let Some(file) = self.sprites(gp) else { return [0, 0] };
        let size = match file.frame_count() > frame {
            true => file.decode_frame(frame).map_or([0, 0], |f| {
                f.layers.iter().fold([0, 0], |[w, h], l| {
                    [w.max(i32::from(l.x) + i32::from(l.width)), h.max(i32::from(l.y) + i32::from(l.height))]
                })
            }),
            false => [0, 0],
        };
        self.sizes.borrow_mut().insert(key, size);
        size
    }

    pub fn palette(&self, path: &str) -> Option<Rc<Palette>> {
        cached(&self.palettes, path, || {
            let bytes = self.bytes(path)?;
            Some(Palette::parse(&bytes).map(Rc::new))
        })
    }
}

/// Looks `path` up in `cache`, filling it with `parse` once the file is available.
/// `parse` returns `None` while the file is still loading and `Some(None)` if it is broken.
fn cached<T>(
    cache: &RefCell<HashMap<String, Option<Rc<T>>>>,
    path: &str,
    parse: impl FnOnce() -> Option<Option<Rc<T>>>,
) -> Option<Rc<T>> {
    let key = normalize_path(path);
    if let Some(hit) = cache.borrow().get(&key) {
        return hit.clone();
    }
    let parsed = parse()?;
    cache.borrow_mut().insert(key, parsed.clone());
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_once_and_stores() {
        let mut lib = Library::new(&["TEXT\\A.TXT".to_owned()]);
        assert!(lib.bytes("text\\a.txt").is_none());
        assert!(lib.bytes("TEXT/A.TXT").is_none());
        assert_eq!(lib.take_requests(), vec!["text\\a.txt".to_owned()]);
        assert!(lib.take_requests().is_empty());
        assert!(!lib.settled("TEXT\\A.TXT"));
        assert!(lib.insert("text\\a.txt", Ok(b"x".to_vec())));
        assert_eq!(lib.bytes("TEXT\\A.TXT").as_deref(), Some(&b"x"[..]));
        assert!(lib.settled("TEXT\\A.TXT"));
        assert!(!lib.insert("other", Ok(Vec::new())));
    }

    #[test]
    fn unknown_paths_settle_without_requests() {
        let mut lib = Library::new(&[]);
        assert!(lib.settled("MISSING.BMP"));
        assert!(lib.bytes("MISSING.BMP").is_none());
        assert!(lib.take_requests().is_empty());
    }
}
