//! The wind field every swaying plant shares: `wind.wgsl` in Rust, for the readers outside
//! the shaders (weather sound, [`crate::weather`]) and the tests. Models (`model.wgsl`) and
//! ground cover (`ground_cover.wgsl`) sway by [`gust`] and [`trunk_bend`] alike, so plants of
//! one kind sway at one frequency, and a plant differs by its phase and stiffness only.

const WIND_LEAN: f32 = 0.06;
const WIND_ROCK: f32 = 0.07;
const WIND_ROCK_RATE: f32 = 1.1;

/// The gust strength (0..1) at world point `at` (px) and wind clock `clock` for wind
/// direction `dir` (unit): `gust` of `wind.wgsl`. The fronts travel along `dir`.
pub fn gust(at: [f32; 2], dir: [f32; 2], clock: f32) -> f32 {
    let along = at[0] * dir[0] + at[1] * dir[1];
    let across = -at[0] * dir[1] + at[1] * dir[0];
    let a = (along * 0.0052 - clock * 0.78 + 1.6 * (across * 0.0021 + clock * 0.05).sin()).sin();
    let b = (along * 0.003 - clock * 0.33 + across * 0.0013 + 1.7).sin();
    let g = (0.45 + 0.35 * a + 0.3 * b).clamp(0.0, 1.0);
    g * g * (3.0 - 2.0 * g)
}

/// The downwind lean (before stiffness and the wind's strength) of a plant of `tempo` with
/// per-plant `phase` in gust `g` at `clock`: `trunk_bend` of `wind.wgsl`.
pub fn trunk_bend(g: f32, clock: f32, tempo: f32, phase: f32) -> f32 {
    WIND_LEAN * g + WIND_ROCK * (clock * tempo * WIND_ROCK_RATE + phase).sin() * (0.3 + 0.7 * g)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: [f32; 2] = [0.94, 0.34];

    /// The rocking's dominant angular frequency (rad/s) of a plant in a steady gust, found
    /// by the spacing of the zero crossings of the bend about its mean.
    fn rock_frequency(tempo: f32, phase: f32, stiffness: f32) -> f32 {
        let dt = 0.002;
        let v: Vec<f32> = (0..20_000).map(|i| stiffness * trunk_bend(1.0, i as f32 * dt, tempo, phase)).collect();
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        let ups: Vec<f32> =
            v.windows(2).enumerate().filter(|(_, w)| w[0] < mean && w[1] >= mean).map(|(i, _)| i as f32 * dt).collect();
        std::f32::consts::TAU * (ups.len() - 1) as f32 / (ups[ups.len() - 1] - ups[0])
    }

    #[test]
    fn one_kind_rocks_at_one_frequency_whatever_phase_and_stiffness() {
        let base = rock_frequency(1.0, 0.0, 1.0);
        assert!((base - WIND_ROCK_RATE).abs() < 0.02, "{base}");
        for (phase, stiffness) in [(1.3, 0.8), (4.0, 1.2), (5.9, 1.0)] {
            let f = rock_frequency(1.0, phase, stiffness);
            assert!((f - base).abs() < 0.02, "{phase} {stiffness}: {f}");
        }
        // Only the kind's tempo changes the frequency.
        assert!((rock_frequency(2.0, 0.0, 1.0) - 2.0 * base).abs() < 0.04);
    }

    #[test]
    fn gust_phase_depends_on_the_position_along_the_wind() {
        // The same clock, 300 px apart along the wind: the gusts differ.
        let at = [3000.0, 2000.0];
        let ahead = [at[0] + DIR[0] * 300.0, at[1] + DIR[1] * 300.0];
        assert!((0..50).any(|i| (gust(at, DIR, i as f32 * 0.7) - gust(ahead, DIR, i as f32 * 0.7)).abs() > 0.05));
        // Positions with the same distance along the wind (and nearby across) agree.
        let side = [at[0] - DIR[1] * 10.0, at[1] + DIR[0] * 10.0];
        for i in 0..50 {
            let t = i as f32 * 0.7;
            assert!((gust(at, DIR, t) - gust(side, DIR, t)).abs() < 0.1);
        }
        // The front travels downwind: what `at` sees now, a point ahead sees later.
        let (speed, mut best) = (0.78 / 0.0052, (f32::MAX, 0.0));
        for i in 0..2000 {
            let late = i as f32 * 0.01;
            let d = (gust(at, DIR, 1.0) - gust(ahead, DIR, 1.0 + late)).abs();
            if d < best.0 {
                best = (d, late);
            }
        }
        assert!(best.1 > 0.0 && best.1 < 300.0 / speed * 3.0, "{best:?}");
        for i in 0..200 {
            assert!((0.0..=1.0).contains(&gust(at, DIR, i as f32 * 0.37)));
        }
    }
}
