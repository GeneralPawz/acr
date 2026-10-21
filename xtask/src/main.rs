//! Build automation, kept in Rust so the project needs no extra tools.
//!
//! - `cargo xtask web [--release] [--app game|viewer]`    build a browser app into `target/web/<app>`
//! - `cargo xtask serve [--release] [--port N] [--game-dir DIR]`   build, then serve it on
//!   localhost; with `--game-dir`, the archives are served under `/game/` with range
//!   support and `http://127.0.0.1:N/?game=/game/` opens the app without a file picker
//! - `cargo xtask dist [--game-dir DIR] [--install]`   release `AmericanConquest.exe` in `dist/`

use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const WASM_TARGET: &str = "wasm32-unknown-unknown";
/// Apps with a browser build: `--app game` (default) or `--app viewer`.
const APPS: [&str; 2] = ["game", "viewer"];
/// Name of the game executable (without extension).
const GAME_EXE: &str = "AmericanConquest";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let release = args.iter().any(|a| a == "--release");
    let option = |name: &str| args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone());
    let port = option("--port").and_then(|p| p.parse().ok()).unwrap_or(8080);
    let game_dir = option("--game-dir").map(PathBuf::from);
    let app = option("--app").unwrap_or_else(|| APPS[0].to_owned());
    if !APPS.contains(&app.as_str()) {
        eprintln!("xtask: unknown app {app}, expected one of {APPS:?}");
        return ExitCode::from(2);
    }
    let result = match args.first().map(String::as_str) {
        Some("web") => build_web(&app, release).map(|_| ()),
        Some("serve") => build_web(&app, release).and_then(|dir| serve(&dir, game_dir.as_deref(), port)),
        Some("dist") => dist(game_dir.as_deref(), args.iter().any(|a| a == "--install")),
        _ => {
            eprintln!(
                "usage: cargo xtask web [--release] [--app game|viewer]\n       \
                 cargo xtask serve [--release] [--app game|viewer] [--port N] [--game-dir DIR]\n       \
                 cargo xtask dist [--game-dir DIR] [--install]"
            );
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The workspace `cargo xtask` was started in. Found at runtime because a shared
/// `CARGO_TARGET_DIR` may hold an xtask binary built from another checkout.
fn workspace_root() -> PathBuf {
    let start = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    start
        .ancestors()
        .find(|dir| std::fs::read_to_string(dir.join("Cargo.toml")).is_ok_and(|toml| toml.contains("[workspace]")))
        .map(Path::to_path_buf)
        .unwrap_or(start)
}

/// Cargo's build output directory, honouring `CARGO_TARGET_DIR`.
fn target_dir(root: &Path) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from)
}

/// Compiles the viewer to wasm, generates JS bindings and copies the static page.
fn build_web(app: &str, release: bool) -> Result<PathBuf> {
    let krate = format!("acr-{app}");
    let lib = format!("acr_{app}");
    let root = workspace_root();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut build = Command::new(cargo);
    build.current_dir(&root).args(["build", "--package", &krate, "--lib", "--target", WASM_TARGET]);
    if release {
        build.arg("--release");
    }
    if !build.status()?.success() {
        return Err("cargo build for wasm failed".into());
    }

    let profile = if release { "release" } else { "debug" };
    let wasm = target_dir(&root).join(WASM_TARGET).join(profile).join(format!("{lib}.wasm"));
    let out = root.join("target").join("web").join(app);
    std::fs::create_dir_all(&out)?;
    wasm_bindgen_cli_support::Bindgen::new()
        .input_path(&wasm)
        .out_name(&lib)
        .web(true)?
        .typescript(false)
        .debug(!release)
        .generate(&out)?;
    std::fs::copy(root.join("crates").join(&krate).join("web/index.html"), out.join("index.html"))?;
    eprintln!("web build ready in {}", out.display());
    Ok(out)
}

/// Builds the release game executable into `dist/`. With a game folder, the original icon
/// is embedded; `install` also copies the executable into that folder, so it can be
/// started from there with a double-click.
fn dist(game_dir: Option<&Path>, install: bool) -> Result<()> {
    let root = workspace_root();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut build = Command::new(cargo);
    build.current_dir(&root).args(["build", "--release", "--package", "acr-game", "--bin", GAME_EXE]);
    if let Some(dir) = game_dir {
        if !dir.join("ALL.GSC").is_file() {
            return Err(format!("{} is not an American Conquest folder (no ALL.GSC)", dir.display()).into());
        }
        build.env("ACR_GAME_DIR", dir);
    }
    if !build.status()?.success() {
        return Err("release build failed".into());
    }
    let target = target_dir(&root);
    let exe = format!("{GAME_EXE}{}", std::env::consts::EXE_SUFFIX);
    let built = target.join("release").join(&exe);
    let out = root.join("dist");
    std::fs::create_dir_all(&out)?;
    std::fs::copy(&built, out.join(&exe))?;
    eprintln!("{}", out.join(&exe).display());
    if install {
        let dir = game_dir.ok_or("--install needs --game-dir")?;
        std::fs::copy(&built, dir.join(&exe))?;
        eprintln!("installed to {}", dir.join(&exe).display());
    }
    Ok(())
}

/// Minimal static file server for local testing. Binds to localhost only.
fn serve(dir: &Path, game_dir: Option<&Path>, port: u16) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    eprintln!("serving {} at http://127.0.0.1:{port}/", dir.display());
    if let Some(game) = game_dir {
        eprintln!("game archives from {} at http://127.0.0.1:{port}/?game=/game/", game.display());
    }
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
    // Only plain names are served: no traversal, no subdirectories.
    let plain = |name: &str| !name.is_empty() && !name.contains(['/', '\\']) && !name.contains("..");

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
