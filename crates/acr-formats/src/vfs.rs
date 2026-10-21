//! Layered virtual file system over archives and loose files.
//!
//! Later mounts override earlier ones, which mirrors how the original game lets
//! `Patch01.GS1` / `Patch02.GS1` and loose files replace content in `ALL.GSC`.

use std::collections::BTreeMap;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};

use crate::gsc::GscArchive;
use crate::normalize_path;

pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

enum Layer {
    Archive {
        label: String,
        archive: GscArchive<Box<dyn ReadSeek>>,
    },
    Dir {
        root: PathBuf,
    },
}

#[derive(Clone, Copy)]
enum Location {
    Archive { layer: usize, index: usize },
    Loose { layer: usize },
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
        let label = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        self.mount_archive(label, Box::new(file))
    }

    /// Mounts an archive from any seekable source (e.g. a browser `File`).
    pub fn mount_archive(&mut self, label: String, source: Box<dyn ReadSeek>) -> io::Result<()> {
        let archive = GscArchive::open(source)?;
        let layer = self.layers.len();
        for (index, entry) in archive.entries().iter().enumerate() {
            self.index.insert(
                normalize_path(&entry.name),
                (entry.name.clone(), Location::Archive { layer, index }),
            );
        }
        self.layers.push(Layer::Archive { label, archive });
        Ok(())
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
                    self.index
                        .insert(normalize_path(&rel), (rel, Location::Loose { layer }));
                }
            }
        }
        self.layers.push(Layer::Dir {
            root: root.to_path_buf(),
        });
        Ok(())
    }

    pub fn contains(&self, path: &str) -> bool {
        self.index.contains_key(&normalize_path(path))
    }

    /// Reads a file by its (case-insensitive) game path, e.g. `0\AGEW_1.PAL`.
    pub fn read(&mut self, path: &str) -> io::Result<Vec<u8>> {
        let (original, location) =
            self.index
                .get(&normalize_path(path))
                .cloned()
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, format!("{path} not found in VFS"))
                })?;
        match (location, &mut self.layers[location_layer(location)]) {
            (Location::Archive { index, .. }, Layer::Archive { archive, .. }) => {
                archive.read(index)
            }
            (Location::Loose { .. }, Layer::Dir { root }) => {
                std::fs::read(root.join(original.replace('\\', std::path::MAIN_SEPARATOR_STR)))
            }
            _ => unreachable!("location always refers to a layer of matching kind"),
        }
    }

    /// All visible files as `(original path, source label)`, sorted by path.
    pub fn list(&self) -> impl Iterator<Item = (&str, &str)> {
        self.index.values().map(|(original, location)| {
            let label = match &self.layers[location_layer(*location)] {
                Layer::Archive { label, .. } => label.as_str(),
                Layer::Dir { .. } => "<loose>",
            };
            (original.as_str(), label)
        })
    }
}

fn location_layer(location: Location) -> usize {
    match location {
        Location::Archive { layer, .. } | Location::Loose { layer } => layer,
    }
}
