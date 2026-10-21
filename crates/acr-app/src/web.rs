//! Browser asset access.
//!
//! Archives come either from the player's game folder (`File` objects from an
//! `<input webkitdirectory>`) or from a URL that supports HTTP range requests. Only the
//! archive tables are read up front; file data is fetched on demand, so the 700 MB
//! archive never sits in memory.

use std::rc::Rc;

use acr_formats::{GscIndex, Located, Vfs};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::{Assets, LoadCallback};

/// Where an archive's bytes come from.
enum Source {
    File(web_sys::File),
    Url { url: String, len: u64 },
}

impl Source {
    fn name(&self) -> String {
        match self {
            Source::File(f) => f.name(),
            Source::Url { url, .. } => url.rsplit('/').next().unwrap_or(url).to_owned(),
        }
    }

    fn len(&self) -> u64 {
        match self {
            Source::File(f) => f.size() as u64,
            Source::Url { len, .. } => *len,
        }
    }

    async fn read(&self, start: u64, len: u64) -> Result<Vec<u8>, String> {
        let buffer = match self {
            Source::File(file) => {
                let blob =
                    file.slice_with_f64_and_f64(start as f64, (start + len) as f64).map_err(|e| format!("{e:?}"))?;
                JsFuture::from(blob.array_buffer()).await.map_err(|e| format!("{e:?}"))?
            }
            Source::Url { url, .. } => {
                let range = format!("bytes={}-{}", start, start + len - 1);
                let response = fetch(url, Some(&range)).await?;
                JsFuture::from(response.array_buffer().map_err(|e| format!("{e:?}"))?)
                    .await
                    .map_err(|e| format!("{e:?}"))?
            }
        };
        let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
        if bytes.len() as u64 != len {
            return Err(format!("{}: expected {len} bytes, got {}", self.name(), bytes.len()));
        }
        Ok(bytes)
    }
}

async fn fetch(url: &str, range: Option<&str>) -> Result<web_sys::Response, String> {
    let init = web_sys::RequestInit::new();
    if let Some(range) = range {
        let headers = web_sys::Headers::new().map_err(|e| format!("{e:?}"))?;
        headers.set("Range", range).map_err(|e| format!("{e:?}"))?;
        init.set_headers(&headers);
    }
    let request = web_sys::Request::new_with_str_and_init(url, &init).map_err(|e| format!("{e:?}"))?;
    let window = web_sys::window().ok_or("no window")?;
    let response: web_sys::Response =
        JsFuture::from(window.fetch_with_request(&request)).await.map_err(|e| format!("{e:?}"))?.unchecked_into();
    if !response.ok() {
        return Err(format!("{url}: HTTP {}", response.status()));
    }
    Ok(response)
}

struct Inner {
    vfs: Vfs,
    /// Archive sources, indexed by VFS layer.
    archives: Vec<Source>,
    /// Loose files of the selected folder, indexed by their item in the listed layer.
    loose: Vec<web_sys::File>,
    /// Or, when loading from a URL, the loose files' URLs.
    loose_urls: Vec<String>,
}

#[derive(Clone)]
pub struct WebAssets(Rc<Inner>);

impl WebAssets {
    /// From the files of the player's game folder. Archives are indexed; all other files
    /// override archive contents like loose files do in the original.
    pub async fn from_files(files: js_sys::Array) -> Result<Self, String> {
        let files: Vec<web_sys::File> = files.iter().filter_map(|f| f.dyn_into().ok()).collect();
        let (archives, loose): (Vec<_>, Vec<_>) = files.into_iter().partition(|f| is_archive(&f.name()));
        let mut assets = Self::mount(archives.into_iter().map(Source::File).collect(), Vec::new()).await?;
        let inner = Rc::get_mut(&mut assets.0).expect("not shared yet");
        let paths: Vec<String> = loose.iter().map(game_path).collect();
        inner.vfs.mount_listed("<folder>".to_owned(), paths);
        inner.loose = loose;
        Ok(assets)
    }

    /// From `base` (ending in `/`), which serves `ALL.GSC` with range support. `files` are
    /// the other files below `base` (`/`-separated): top-level `*.GS1` patches are mounted
    /// as archives, everything else as loose overrides fetched whole.
    pub async fn from_url(base: &str, files: &[String]) -> Result<Self, String> {
        let (patches, loose): (Vec<&String>, Vec<&String>) =
            files.iter().partition(|f| !f.contains('/') && is_archive(f));
        let mut sources = Vec::new();
        for name in std::iter::once("ALL.GSC").chain(patches.iter().map(|s| s.as_str())) {
            let url = format!("{base}{name}");
            // A 1-byte range request reveals the total size in Content-Range.
            let response = fetch(&url, Some("bytes=0-0")).await?;
            let len = response
                .headers()
                .get("Content-Range")
                .ok()
                .flatten()
                .and_then(|r| r.rsplit('/').next().and_then(|n| n.parse().ok()))
                .ok_or_else(|| format!("{url}: server does not support range requests"))?;
            sources.push(Source::Url { url, len });
        }
        let mut assets = Self::mount(sources, Vec::new()).await?;
        let inner = Rc::get_mut(&mut assets.0).expect("not shared yet");
        inner.vfs.mount_listed("<url>".to_owned(), loose.iter().map(|f| f.replace('/', "\\")));
        inner.loose_urls = loose
            .iter()
            .map(|f| format!("{base}{}", f.split('/').map(encode_segment).collect::<Vec<_>>().join("/")))
            .collect();
        Ok(assets)
    }

    async fn mount(mut sources: Vec<Source>, loose: Vec<web_sys::File>) -> Result<Self, String> {
        // Same mount order as the original game: ALL.GSC, then patches by name.
        sources.retain(|s| is_archive(&s.name()));
        sources.sort_by_key(|s| {
            let name = s.name().to_ascii_uppercase();
            (name != "ALL.GSC", name)
        });
        if sources.first().is_none_or(|s| !s.name().eq_ignore_ascii_case("ALL.GSC")) {
            return Err("ALL.GSC not found in the selected folder".into());
        }
        let mut vfs = Vfs::new();
        for source in &sources {
            let header = source.read(0, acr_formats::gsc::HEADER_LEN as u64).await?;
            let len = GscIndex::index_len(&header).map_err(|e| e.to_string())?;
            let table = source.read(0, len as u64).await?;
            let index = GscIndex::parse(&table, source.len()).map_err(|e| e.to_string())?;
            vfs.mount_index(source.name(), index);
        }
        Ok(Self(Rc::new(Inner { vfs, archives: sources, loose, loose_urls: Vec::new() })))
    }

    async fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        match self.0.vfs.locate(path) {
            Some(Located::Archive { layer, index, entry, .. }) => {
                let mut data = self.0.archives[layer].read(entry.offset, u64::from(entry.size)).await?;
                index.decrypt(entry, &mut data);
                Ok(data)
            }
            Some(Located::Listed { item, .. }) => {
                if let Some(file) = self.0.loose.get(item) {
                    return Source::File(file.clone()).read(0, file.size() as u64).await;
                }
                let url = self.0.loose_urls.get(item).ok_or_else(|| format!("{path}: no source"))?;
                let response = fetch(url, None).await?;
                let buffer = JsFuture::from(response.array_buffer().map_err(|e| format!("{e:?}"))?)
                    .await
                    .map_err(|e| format!("{e:?}"))?;
                Ok(js_sys::Uint8Array::new(&buffer).to_vec())
            }
            Some(Located::Loose { .. }) => Err(format!("{path}: unexpected local path in the browser")),
            None => Err(format!("{path} not found")),
        }
    }
}

/// Percent-encodes a URL path segment.
fn encode_segment(segment: &str) -> String {
    segment
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn is_archive(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    name == "ALL.GSC" || name.ends_with(".GS1")
}

/// The file's path inside the selected folder, with `\` separators like the game uses.
fn game_path(file: &web_sys::File) -> String {
    let relative = js_sys::Reflect::get(file, &"webkitRelativePath".into())
        .ok()
        .and_then(|v| v.as_string())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| file.name());
    // Drop the selected folder's own name.
    let inside = relative.split_once('/').map_or(relative.as_str(), |(_, rest)| rest);
    inside.replace('/', "\\")
}

impl Assets for WebAssets {
    fn paths(&self) -> Vec<String> {
        self.0.vfs.list().map(|(p, _)| p.to_owned()).collect()
    }

    fn request(&self, path: &str, done: LoadCallback) {
        let assets = self.clone();
        let path = path.to_owned();
        wasm_bindgen_futures::spawn_local(async move { done(assets.read(&path).await) });
    }
}
