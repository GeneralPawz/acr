//! The same icon and version info as the game (see `acr-game/build.rs`).

#[path = "../acr-game/build.rs"]
mod game;

fn main() {
    game::main();
}
