//! Browser entry points.
//!
//! The archives come either from the player's game folder (`start`, fed by an
//! `<input webkitdirectory>`) or from a URL that supports HTTP range requests
//! (`start_url`, e.g. `cargo xtask serve --game-dir ...`). Only the archive tables are
//! read up front; sprite data is fetched on demand, so the 700 MB archive never sits in
//! memory.

use std::rc::Rc;

use acr_formats::{GscIndex, Located, Vfs};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use winit::event_loop::EventLoopProxy;
use winit::platform::web::EventLoopExtWebSys;

use crate::{App, Assets, UserEvent};

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

struct WebAssets {
    vfs: Vfs,
    /// Archive sources, indexed by VFS layer.
    archives: Vec<Source>,
}

struct WebHandle(Rc<WebAssets>);

impl Assets for WebHandle {
    fn sprite_paths(&self) -> Vec<String> {
        self.0
            .vfs
            .list()
            .map(|(p, _)| p)
            .filter(|p| p.to_ascii_uppercase().ends_with(".GP"))
            .map(str::to_owned)
            .collect()
    }

    fn request(&self, path: &str, proxy: &EventLoopProxy<UserEvent>) {
        let assets = self.0.clone();
        let proxy = proxy.clone();
        let path = path.to_owned();
        wasm_bindgen_futures::spawn_local(async move {
            let result = assets.read(&path).await;
            let _ = proxy.send_event(UserEvent::Loaded { path, result });
        });
    }
}

impl WebAssets {
    async fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        match self.vfs.locate(path) {
            Some(Located::Archive { layer, index, entry, .. }) => {
                let mut data = self.archives[layer].read(entry.offset, u64::from(entry.size)).await?;
                index.decrypt(entry, &mut data);
                Ok(data)
            }
            Some(Located::Loose { .. } | Located::Listed { .. }) => {
                Err(format!("{path}: loose files are not loaded in the viewer"))
            }
            None => Err(format!("{path} not found")),
        }
    }
}

/// Called by `index.html` with the files of the selected game folder.
#[wasm_bindgen]
pub async fn start(files: js_sys::Array) -> Result<(), JsValue> {
    let sources = files.iter().filter_map(|f| f.dyn_into().ok()).map(Source::File).collect();
    run(sources).await
}

/// Loads archives from `base` (ending in `/`), which must serve `ALL.GSC` and the
/// `*.GS1` names listed in `patches` with HTTP range support.
#[wasm_bindgen]
pub async fn start_url(base: String, patches: Vec<String>) -> Result<(), JsValue> {
    let mut sources = Vec::new();
    for name in std::iter::once("ALL.GSC".to_owned()).chain(patches) {
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
    run(sources).await
}

async fn run(mut sources: Vec<Source>) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    // Same mount order as the original game: ALL.GSC, then patches by name.
    sources.retain(|s| {
        let name = s.name().to_ascii_uppercase();
        name == "ALL.GSC" || name.ends_with(".GS1")
    });
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

    let start = web_sys::window()
        .and_then(|w| w.location().hash().ok())
        .map(|h| h.trim_start_matches('#').to_owned())
        .filter(|h| !h.is_empty());
    let event_loop = crate::event_loop()?;
    let assets = WebHandle(Rc::new(WebAssets { vfs, archives: sources }));
    let app = App::new(event_loop.create_proxy(), Box::new(assets), start.as_deref());
    event_loop.spawn_app(app);
    Ok(())
}
