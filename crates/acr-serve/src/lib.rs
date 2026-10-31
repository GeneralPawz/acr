//! A small static file server for the browser version on localhost, used by
//! `cargo xtask serve` and by `AmericanConquest-Web`, the release's launcher for the
//! browser version.
//!
//! The app files are served from one folder. With a game folder, its files are served
//! under `/game/` (a listing at `/game/`, files with range support), so
//! `/?game=/game/` opens the game without the browser's folder picker.

use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Answers requests on `listener` (bind it to localhost) until the process ends: the app
/// from `dir`, the game files from `game_dir` under `/game/`.
pub fn serve(listener: TcpListener, dir: &Path, game_dir: Option<&Path>) -> Result<()> {
    for stream in listener.incoming() {
        if let Err(e) = respond(stream?, dir, game_dir) {
            eprintln!("request failed: {e}");
        }
    }
    Ok(())
}

fn is_archive(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.ends_with(".GSC") || upper.ends_with(".GS1")
}

fn respond(mut stream: TcpStream, dir: &Path, game_dir: Option<&Path>) -> Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut range = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("range")
        {
            range = parse_range(value.trim());
        }
    }
    let path = request_line.split_whitespace().nth(1).unwrap_or("/");
    let path = path.split(['?', '#']).next().unwrap_or("/").trim_start_matches('/');
    // Only plain names are served: no traversal. Folders of the app (`ui/`) are fine.
    let plain = |name: &str| name.split('/').all(|seg| !seg.is_empty() && !seg.contains('\\') && !seg.starts_with('.'));

    if let (Some(rest), Some(game)) = (path.strip_prefix("game/"), game_dir) {
        if rest.is_empty() {
            // Archives first (mounted in order), then every other file as a loose override.
            let mut files = Vec::new();
            list_files(game, game, &mut files)?;
            files.sort_by_key(|f| (!is_archive(f) || f.contains('/'), f.to_ascii_uppercase()));
            return send(
                &mut stream,
                "200 OK",
                "text/plain; charset=utf-8",
                &[],
                files
                    .join(
                        "
",
                    )
                    .as_bytes(),
            );
        }
        let rest = percent_decode(rest);
        let safe = !rest.split('/').any(|seg| seg.is_empty() || seg == "..") && !rest.contains('\\');
        if safe && game.join(&rest).is_file() {
            return send_range(&mut stream, &game.join(&rest), range);
        }
        return send(&mut stream, "404 Not Found", "text/plain", &[], b"");
    }

    let path = if path.is_empty() { "index.html" } else { path };
    let Some(file) = plain(path).then(|| dir.join(path)).filter(|p| p.is_file()) else {
        return send(&mut stream, "404 Not Found", "text/plain", &[], b"");
    };
    let mime = match file.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css; charset=utf-8",
        Some("png") => "image/png",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    };
    send(&mut stream, "200 OK", mime, &[], &std::fs::read(&file)?)
}

/// Every file below `dir`, as `/`-separated paths relative to `root`.
fn list_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            list_files(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// Decodes `%XX` escapes of a URL path (file names contain spaces and umlauts).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(h), Some(l)) = (bytes.get(i + 1).and_then(|&b| hex(b)), bytes.get(i + 2).and_then(|&b| hex(b)))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parses `bytes=START-END` (inclusive end, as sent by the viewer).
fn parse_range(value: &str) -> Option<(u64, u64)> {
    let (start, end) = value.strip_prefix("bytes=")?.split_once('-')?;
    Some((start.parse().ok()?, end.parse().ok()?))
}

fn send_range(stream: &mut TcpStream, file: &Path, range: Option<(u64, u64)>) -> Result<()> {
    let mut f = std::fs::File::open(file)?;
    let total = f.metadata()?.len();
    let Some((start, end)) = Some(range.unwrap_or((0, total.saturating_sub(1)))).filter(|&(s, e)| s <= e && e < total)
    else {
        return send(
            stream,
            "416 Range Not Satisfiable",
            "text/plain",
            &[format!("Content-Range: bytes */{total}")],
            b"",
        );
    };
    let mut body = vec![0; (end - start + 1) as usize];
    f.seek(SeekFrom::Start(start))?;
    f.read_exact(&mut body)?;
    let content_range = format!("Content-Range: bytes {start}-{end}/{total}");
    let status = if range.is_some() { "206 Partial Content" } else { "200 OK" };
    send(stream, status, "application/octet-stream", &[content_range], &body)
}

fn send(stream: &mut TcpStream, status: &str, mime: &str, extra: &[String], body: &[u8]) -> Result<()> {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n",
        body.len()
    );
    for header in extra {
        head.push_str(header);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    Ok(())
}
