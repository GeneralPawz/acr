//! Desktop asset access: blocking reads through the VFS.

use std::cell::RefCell;
use std::path::Path;

use acr_formats::Vfs;

use crate::{Assets, LoadCallback};

pub struct NativeAssets(RefCell<Vfs>);

impl NativeAssets {
    /// Mounts the archives and loose files of an original installation.
    pub fn open(game_dir: &Path) -> Result<Self, String> {
        Vfs::open_game_dir(game_dir).map(|v| Self(RefCell::new(v))).map_err(|e| format!("{}: {e}", game_dir.display()))
    }
}

impl Assets for NativeAssets {
    fn paths(&self) -> Vec<String> {
        self.0.borrow().list().map(|(p, _)| p.to_owned()).collect()
    }

    fn request(&self, path: &str, done: LoadCallback) {
        let result = self.0.borrow_mut().read(path).map_err(|e| e.to_string());
        done(result);
    }
}
