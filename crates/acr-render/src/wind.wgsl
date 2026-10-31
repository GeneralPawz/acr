// The wind field every swaying plant shares (wind.rs mirrors it): `gust` is the strength
// over the map, `trunk_bend` how far a plant of tempo `tempo` leans downwind in it. Models
// (model.wgsl) and ground cover (ground_cover.wgsl) both sway by these, so one kind sways at
// one frequency; a plant differs by its phase and stiffness only.
const WIND_LEAN: f32 = 0.06;
const WIND_ROCK: f32 = 0.07;
// Rad/s of the rocking at tempo 1.
const WIND_ROCK_RATE: f32 = 1.1;

fn gust(at: vec2<f32>, dir: vec2<f32>, t: f32) -> f32 {
    let along = dot(at, dir);
    let across = dot(at, vec2<f32>(-dir.y, dir.x));
    // About 40 m long at 5 m/s, and 70 m at 3.7 m/s (30 px/m).
    let a = sin(along * 0.0052 - t * 0.78 + 1.6 * sin(across * 0.0021 + t * 0.05));
    let b = sin(along * 0.003 - t * 0.33 + across * 0.0013 + 1.7);
    let g = clamp(0.45 + 0.35 * a + 0.3 * b, 0.0, 1.0);
    return g * g * (3.0 - 2.0 * g);
}

// Downwind lean (before the plant's stiffness and the wind's strength) in gust `g` at clock
// `t`: the gust leans it, and it rocks at `tempo` with the per-plant `phase`.
fn trunk_bend(g: f32, t: f32, tempo: f32, phase: f32) -> f32 {
    return WIND_LEAN * g + WIND_ROCK * sin(t * tempo * WIND_ROCK_RATE + phase) * (0.3 + 0.7 * g);
}
