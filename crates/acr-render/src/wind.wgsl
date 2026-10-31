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

// A flag's cloth (docs/rendering-3d.md "Wind"). `u` is the distance from the pole (0 at the
// hoist, 1 at the fly), `row` 0..1 a phase down the cloth, `t` the wind clock. The ripple
// grows from nothing at the pole, runs from the pole to the fly at a steady rate (a rate that
// followed the strength would jump the phase as it changes) and gets bigger with the
// `strength` (wind and the bearer's walk).
const FLAG_RIPPLE: f32 = 0.16;
// Rad of the wave per unit of `u` (about one and a half wavelengths over the cloth).
const FLAG_WAVE_K: f32 = 9.0;
// Rad/s of the wave.
const FLAG_RATE: f32 = 5.0;
// Strength per m/s of the bearer's pace, and the wind's share of it in a lull.
const FLAG_PACE: f32 = 0.45;
const FLAG_LULL: f32 = 0.4;
// Metres the free edge trails per unit of drift.
const FLAG_TRAIL: f32 = 0.12;

// The ripple's strength: the wind `s` (in gust `g`) plus the bearer's `pace` (m/s).
fn flag_strength(s: f32, g: f32, pace: f32) -> f32 {
    return min(s * (FLAG_LULL + (1.0 - FLAG_LULL) * g) + FLAG_PACE * pace, 1.5);
}

// The cloth's displacement (m) along its normal: zero at the pole whatever the strength.
fn flag_wave(u: f32, row: f32, t: f32, strength: f32) -> f32 {
    let envelope = u * (0.35 + 0.65 * u);
    return FLAG_RIPPLE * strength * envelope * sin(FLAG_WAVE_K * u - FLAG_RATE * t + 1.3 * row);
}

// Where the free edge streams (m, world xy) before the weight `u²`: downwind with the wind
// `s` along `dir`, opposite to the bearer's `pace` along the unit `heading`.
fn flag_drift(dir: vec2<f32>, s: f32, heading: vec2<f32>, pace: f32) -> vec2<f32> {
    return (dir * s - heading * (FLAG_PACE * pace * 2.0)) * FLAG_TRAIL;
}
