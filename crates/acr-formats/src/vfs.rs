//! Layered virtual file system over archives and loose files.
//!
//! Later mounts override earlier ones, which mirrors how the original game lets
//! `Patch01.GS1` / `Patch02.GS1` and loose files replace content in `ALL.GSC`.
//!
//! Archives can be mounted with a blocking source ([`Vfs::mount_archive`]) or as a
//! bare index ([`Vfs::mount_index`]). For the latter, [`Vfs::locate`] tells the caller
//! which byte range to fetch, e.g. asynchronously from a browser `File`.

use std::collections::BTreeMap;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::gsc::{GscEntry, GscIndex};
use crate::normalize_path;

pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

enum Layer {
    Archive {
        label: String,
        index: GscIndex,
        source: Option<Box<dyn ReadSeek>>,
    },
    Dir {
        root: PathBuf,
    },
    /// Files known by name only; the caller reads them (e.g. browser `File` objects).
    Listed {
        label: String,
    },
}

#[derive(Clone, Copy)]
enum Location {
    Archive { layer: usize, entry: usize },
    Loose { layer: usize },
    Listed { layer: usize, item: usize },
}

impl Location {
    fn layer(self) -> usize {
        match self {
            Location::Archive { layer, .. } | Location::Loose { layer } | Location::Listed { layer, .. } => layer,
        }
    }
}

/// Where the winning copy of a file lives.
#[derive(Debug)]
pub enum Located<'a> {
    Archive {
        /// Position of the archive in mount order.
        layer: usize,
        label: &'a str,
        index: &'a GscIndex,
        entry: &'a GscEntry,
    },
    Loose {
        path: PathBuf,
    },
    /// The `item`-th path passed to [`Vfs::mount_listed`] for `layer`.
    Listed {
        layer: usize,
        item: usize,
    },
}

#[derive(Default)]
pub struct Vfs {
    layers: Vec<Layer>,
    /// Normalized path -> (original path, where the winning copy lives).
    index: BTreeMap<String, (String, Location)>,
}

impl Vfs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mounts the archives and loose files of an original installation directory.
    pub fn open_game_dir(dir: &Path) -> io::Result<Self> {
        let mut vfs = Self::new();
        vfs.mount_archive_file(&dir.join("ALL.GSC"))?;
        let mut patches: Vec<PathBuf> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("GS1")))
            .collect();
        patches.sort();
        for patch in patches {
            vfs.mount_archive_file(&patch)?;
        }
        vfs.mount_dir(dir)?;
        Ok(vfs)
    }

    pub fn mount_archive_file(&mut self, path: &Path) -> io::Result<()> {
        let file = io::BufReader::new(std::fs::File::open(path)?);
        let label = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        self.mount_archive(label, Box::new(file))
    }

    /// Mounts an archive read through a blocking source.
    pub fn mount_archive(&mut self, label: String, mut source: Box<dyn ReadSeek>) -> io::Result<()> {
        let archive_len = source.seek(SeekFrom::End(0))?;
        source.seek(SeekFrom::Start(0))?;
        let mut bytes = vec![0u8; crate::gsc::HEADER_LEN];
        source.read_exact(&mut bytes)?;
        bytes.resize(GscIndex::index_len(&bytes)?, 0);
        source.read_exact(&mut bytes[crate::gsc::HEADER_LEN..])?;
        let index = GscIndex::parse(&bytes, archive_len)?;
        self.push_archive(label, index, Some(source));
        Ok(())
    }

    /// Mounts an archive by its index only; read its files via [`Vfs::locate`].
    /// Returns the layer number to match against [`Located::Archive::layer`].
    pub fn mount_index(&mut self, label: String, index: GscIndex) -> usize {
        self.push_archive(label, index, None)
    }

    fn push_archive(&mut self, label: String, index: GscIndex, source: Option<Box<dyn ReadSeek>>) -> usize {
        let layer = self.layers.len();
        for (entry, e) in index.entries.iter().enumerate() {
            self.index.insert(normalize_path(&e.name), (e.name.clone(), Location::Archive { layer, entry }));
        }
        self.layers.push(Layer::Archive { label, index, source });
        layer
    }

    /// Mounts every file below `root` as loose overrides.
    pub fn mount_dir(&mut self, root: &Path) -> io::Result<()> {
        let layer = self.layers.len();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Ok(rel) = path.strip_prefix(root) {
                    let rel = rel.to_string_lossy().replace('/', "\\");
                    self.index.insert(normalize_path(&rel), (rel, Location::Loose { layer }));
                }
            }
        }
        self.layers.push(Layer::Dir { root: root.to_path_buf() });
        Ok(())
    }

    /// Mounts files known only by their game paths (e.g. `Sound\Baraban.wav`); read them
    /// via [`Vfs::locate`]. Returns the layer number.
    pub fn mount_listed(&mut self, label: String, paths: impl IntoIterator<Item = String>) -> usize {
        let layer = self.layers.len();
        for (item, path) in paths.into_iter().enumerate() {
            self.index.insert(normalize_path(&path), (path, Location::Listed { layer, item }));
        }
        self.layers.push(Layer::Listed { label });
        layer
    }

    pub fn contains(&self, path: &str) -> bool {
        self.index.contains_key(&normalize_path(path))
    }

    /// Finds the winning copy of a file without reading it.
    pub fn locate(&self, path: &str) -> Option<Located<'_>> {
        let (original, location) = self.index.get(&normalize_path(path))?;
        Some(match (location, &self.layers[location.layer()]) {
            (Location::Archive { layer, entry }, Layer::Archive { label, index, .. }) => {
                Located::Archive { layer: *layer, label, index, entry: &index.entries[*entry] }
            }
            (Location::Loose { .. }, Layer::Dir { root }) => {
                Located::Loose { path: root.join(original.replace('\\', std::path::MAIN_SEPARATOR_STR)) }
            }
            (Location::Listed { layer, item }, Layer::Listed { .. }) => Located::Listed { layer: *layer, item: *item },
            _ => unreachable!("location always refers to a layer of matching kind"),
        })
    }

    /// Reads a file by its (case-insensitive) game path, e.g. `0\AGEW_1.PAL`.
    pub fn read(&mut self, path: &str) -> io::Result<Vec<u8>> {
        let not_found = || io::Error::new(io::ErrorKind::NotFound, format!("{path} not found in VFS"));
        let (_, location) = *self.index.get(&normalize_path(path)).ok_or_else(not_found)?;
        match (location, &mut self.layers[location.layer()]) {
            (Location::Archive { entry, .. }, Layer::Archive { index, source, .. }) => {
                let source = source.as_mut().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::Unsupported, format!("{path} is in an index-only archive"))
                })?;
                let e = &index.entries[entry];
                let mut data = vec![0u8; e.size as usize];
                source.seek(SeekFrom::Start(e.offset))?;
                source.read_exact(&mut data)?;
                index.decrypt(e, &mut data);
                Ok(data)
            }
            (Location::Loose { .. }, Layer::Dir { .. }) => match self.locate(path) {
                Some(Located::Loose { path }) => std::fs::read(path),
                _ => unreachable!(),
            },
            (Location::Listed { .. }, Layer::Listed { .. }) => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("{path} is a listed file; read it through locate()"),
            )),
            _ => unreachable!("location always refers to a layer of matching kind"),
        }
    }

    /// All visible files as `(original path, source label)`, sorted by path.
    pub fn list(&self) -> impl Iterator<Item = (&str, &str)> {
        self.index.values().map(|(original, location)| {
            let label = match &self.layers[location.layer()] {
                Layer::Archive { label, .. } => label.as_str(),
                Layer::Dir { .. } => "<loose>",
                Layer::Listed { label } => label.as_str(),
            };
            (original.as_str(), label)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listed_files_override_and_locate() {
        let mut vfs = Vfs::new();
        let layer = vfs.mount_listed("folder".into(), ["Sound\\Baraban.wav".to_owned(), "Ac.ico".to_owned()]);
        assert!(vfs.contains("sound/BARABAN.WAV"));
        match vfs.locate("ac.ico") {
            Some(Located::Listed { layer: l, item }) => assert_eq!((l, item), (layer, 1)),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(vfs.list().map(|(_, label)| label).collect::<Vec<_>>(), ["folder", "folder"]);
        assert!(vfs.read("Ac.ico").is_err());
    }
}
