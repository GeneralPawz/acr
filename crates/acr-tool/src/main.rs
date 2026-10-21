//! `acr` - inspect and extract original American Conquest data.
//!
//! Every module lists its subcommands in a `COMMANDS` table; add new commands there and
//! register new modules in [`MODULES`].

mod archive;
mod audio;
mod defs;
mod fonts;
mod images;
mod maps;
mod scripts;
mod sim;
mod sprites;
mod video;

use std::io;
use std::path::Path;
use std::process::ExitCode;

use acr_formats::normalize_path;
use acr_formats::raster::Image;

/// A subcommand: `acr <name> <args..>`.
pub struct Command {
    pub name: &'static str,
    /// Arguments after the name, for the help text.
    pub usage: &'static str,
    /// Runs with the arguments after the name.
    pub run: fn(&[&str]) -> Result<()>,
}

/// Command tables of all modules, in help order.
const MODULES: &[&[Command]] = &[
    archive::COMMANDS,
    sprites::COMMANDS,
    maps::COMMANDS,
    defs::COMMANDS,
    images::COMMANDS,
    fonts::COMMANDS,
    audio::COMMANDS,
    video::COMMANDS,
    scripts::COMMANDS,
    sim::COMMANDS,
];

/// Returned by a command whose arguments do not fit; `main` then prints its usage.
pub const WRONG_ARGS: &str = "wrong arguments";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let commands = || MODULES.iter().flat_map(|m| m.iter());
    let Some(command) = args.first().and_then(|name| commands().find(|c| c.name == *name)) else {
        eprintln!("usage:");
        for c in commands() {
            eprintln!("  acr {} {}", c.name, c.usage);
        }
        return ExitCode::from(2);
    };
    match (command.run)(&args[1..]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.to_string() == WRONG_ARGS => {
            eprintln!("usage: acr {} {}", command.name, command.usage);
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Case-insensitive substring match on normalized paths.
pub fn matches(path: &str, filter: Option<&str>) -> bool {
    filter.is_none_or(|f| normalize_path(path).contains(&normalize_path(f)))
}

pub fn write_png(path: &Path, image: &Image) -> Result<()> {
    let file = io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&image.rgba)?;
    Ok(())
}
