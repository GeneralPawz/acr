//! Savegame storage: `Save\<name>.sav` in the game folder on the desktop, the browser's
//! `localStorage` on the web (the browser has no writable game folder). The game decides
//! what goes into a file; this only stores bytes by name.

use std::collections::BTreeMap;

/// A place for savegames. Names are without folder and extension (`dgs` for
/// `Save\dgs.sav`).
pub trait SaveStore {
    /// Names of all stored savegames.
    fn list(&self) -> Vec<String>;
    fn read(&self, name: &str) -> Option<Vec<u8>>;
    fn write(&mut self, name: &str, bytes: &[u8]) -> Result<(), String>;
}

/// Savegames kept in memory (tests, or no storage available).
#[derive(Default)]
pub struct MemorySaves(BTreeMap<String, Vec<u8>>);

impl SaveStore for MemorySaves {
    fn list(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }

    fn read(&self, name: &str) -> Option<Vec<u8>> {
        self.0.get(name).cloned()
    }

    fn write(&mut self, name: &str, bytes: &[u8]) -> Result<(), String> {
        self.0.insert(name.to_owned(), bytes.to_vec());
        Ok(())
    }
}

/// `<game dir>\Save\*.sav`, the original's save folder.
#[cfg(not(target_arch = "wasm32"))]
pub struct FolderSaves {
    dir: std::path::PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl FolderSaves {
    /// The `Save` folder of the game installed in `game_dir` (created on the first save).
    pub fn new(game_dir: &std::path::Path) -> Self {
        Self { dir: game_dir.join("Save") }
    }

    fn path(&self, name: &str) -> std::path::PathBuf {
        self.dir.join(format!("{name}.sav"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl SaveStore for FolderSaves {
    fn list(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return Vec::new() };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let stem = name.len().checked_sub(4).filter(|&n| name[n..].eq_ignore_ascii_case(".sav"))?;
                Some(name[..stem].to_owned())
            })
            .collect();
        names.sort();
        names
    }

    fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(name)).ok()
    }

    fn write(&mut self, name: &str, bytes: &[u8]) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let path = self.path(name);
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// `localStorage` entries `acr-save:<name>` holding the file as base64.
#[cfg(target_arch = "wasm32")]
#[derive(Default)]
pub struct BrowserSaves;

#[cfg(target_arch = "wasm32")]
const PREFIX: &str = "acr-save:";

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
impl SaveStore for BrowserSaves {
    fn list(&self) -> Vec<String> {
        let Some(s) = storage() else { return Vec::new() };
        let n = s.length().unwrap_or(0);
        let mut names: Vec<String> =
            (0..n).filter_map(|i| s.key(i).ok()?).filter_map(|k| k.strip_prefix(PREFIX).map(str::to_owned)).collect();
        names.sort();
        names
    }

    fn read(&self, name: &str) -> Option<Vec<u8>> {
        base64_decode(&storage()?.get_item(&format!("{PREFIX}{name}")).ok()??)
    }

    fn write(&mut self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let s = storage().ok_or("no localStorage")?;
        s.set_item(&format!("{PREFIX}{name}"), &base64_encode(bytes)).map_err(|e| format!("{e:?}"))
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.bytes().filter(|&c| c != b'=') {
        acc = (acc << 6) | B64.iter().position(|&b| b == c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// The wall clock: seconds since 1970 (UTC) and the local date and time as year, month,
/// day, hour, minute, second. Used for savegame dates only.
pub fn local_time() -> (u64, [u16; 6]) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let now = jiff::Zoned::now();
        let unix = now.timestamp().as_second().max(0) as u64;
        let d = now.datetime();
        let f = |v: i64| v.clamp(0, i64::from(u16::MAX)) as u16;
        (
            unix,
            [
                f(d.year().into()),
                f(d.month().into()),
                f(d.day().into()),
                f(d.hour().into()),
                f(d.minute().into()),
                f(d.second().into()),
            ],
        )
    }
    #[cfg(target_arch = "wasm32")]
    {
        let d = js_sys::Date::new_0();
        let unix = (d.get_time() / 1000.0).max(0.0) as u64;
        (
            unix,
            [
                d.get_full_year() as u16,
                (d.get_month() + 1) as u16,
                d.get_date() as u16,
                d.get_hours() as u16,
                d.get_minutes() as u16,
                d.get_seconds() as u16,
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trip() {
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_encode(b"M"), "TQ==");
        for n in 0..20 {
            let data: Vec<u8> = (0..n).map(|i| (i * 37 + 5) as u8).collect();
            assert_eq!(base64_decode(&base64_encode(&data)).unwrap(), data);
        }
    }

    #[test]
    fn memory_store() {
        let mut s = MemorySaves::default();
        s.write("b", b"2").unwrap();
        s.write("a", b"1").unwrap();
        assert_eq!(s.list(), ["a", "b"]);
        assert_eq!(s.read("a").unwrap(), b"1");
        assert!(s.read("c").is_none());
    }
}
