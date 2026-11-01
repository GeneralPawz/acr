//! The Svelte UI built by `ui/` (`bun run build`), embedded in the executable by `build.rs`.
//!
//! `FILES` holds the bundle folder's files as (path with `/`, bytes) and `PRESENT` says whether
//! one was embedded. Without `ui/dist` (or `ACR_UI_DIST`) at build time `PRESENT` is false and
//! the game keeps drawing its own UI. The browser build embeds nothing: `cargo xtask web`
//! copies the bundle beside the page.

include!(concat!(env!("OUT_DIR"), "/ui_bundle.rs"));

/// The bytes of one bundle file, by its path below the bundle folder.
pub fn file(path: &str) -> Option<&'static [u8]> {
    FILES.iter().find(|(name, _)| *name == path).map(|(_, bytes)| *bytes)
}

/// The content type of a bundle file, by its extension.
pub fn mime(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("html") => "text/html",
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `ui` job of CI builds the bundle and points `ACR_UI_DIST` at it; this proves the
    /// build script embedded it. Without Bun the variable is unset and only the empty case holds.
    #[test]
    fn embeds_the_built_bundle() {
        if std::env::var_os("ACR_UI_DIST").is_some() {
            assert!(!FILES.is_empty(), "ACR_UI_DIST is set but nothing was embedded");
            assert!(FILES.iter().any(|(name, _)| *name == "index.html"));
            assert!(FILES.iter().any(|(name, bytes)| *name == "acr-ui.js" && !bytes.is_empty()));
            assert!(file("acr-ui.js").is_some());
        }
        assert_eq!(PRESENT, !FILES.is_empty());
        assert!(file("../escape").is_none());
    }

    #[test]
    fn bundle_files_get_their_type() {
        assert_eq!(mime("index.html"), "text/html");
        assert_eq!(mime("acr-ui.js"), "text/javascript");
        assert_eq!(mime("acr-ui.css"), "text/css");
        assert_eq!(mime("other.bin"), "application/octet-stream");
    }
}
