// The water surface of the 3D pass (water.rs, docs/water.md): a grid at the water level
// (z = 0, where the original draws its depth tint) over the visible part of the map, moved by
// a seeded spectrum of Gerstner waves scaled by the `SEA2` depth, with a procedural detail
// normal, shaded with depth colour and absorption (the lakebed shows through), a sky
// reflection by Fresnel, the sun's glint and foam at the shore. Ships under way add a Kelvin
// wake and a bow wave, landing shot a ring and a burst of foam.

const WAVES: i32 = 10;
const MAX_WAKES: i32 = 8;
const MAX_RINGS: i32 = 16;

struct Globals {
    // World (x east, y south, z true height) to output clip space (view3d.rs).
    view: mat4x4<f32>,
    // World rectangle the grid covers: x0, y0, x1, y1.
    rect: vec4<f32>,
    // xyz: unit vector from the surface to the viewer (orthographic); w: seconds.
    eye: vec4<f32>,
    // xyz: unit vector to the sun; w: waves and foam (1, `water=detailed`) or a flat surface (0).
    sun: vec4<f32>,
    // Water grid width and height (cells of 32 px), grid cells per side, unused.
    grid: vec4<f32>,
    // Per wave: direction xy, wave number (rad/px), amplitude (px).
    waves: array<vec4<f32>, 10>,
    // The waves' phase offsets, four to a vector.
    phases: array<vec4<f32>, 3>,
    // Per ship: centre xy, heading xy; then speed (px/s), half length (px), unused, unused.
    wakes: array<vec4<f32>, 16>,
    // Per ring: centre xy, landing time (s, the clock of `eye.w`), strength.
    rings: array<vec4<f32>, 16>,
    // Wakes in use, rings in use, unused, unused.
    counts: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var depth_map: texture_2d<f32>;
@group(0) @binding(2) var depth_sampler: sampler;

const TAU: f32 = 6.2831853;
// Gravity in world pixels per s² (about 30 px to the metre).
const GRAVITY: f32 = 294.0;
// The original's depth tint saturates at 64 depth units above the shore value 127.
const DEPTH_FULL: f32 = 64.0;
// Overall Gerstner steepness, shared out over the waves.
const STEEPNESS: f32 = 0.5;
// sin of Kelvin's half angle (19.47°): the wake's edge, |lateral| = 0.354 × behind.
const KELVIN: f32 = 0.354;

// Depth units above the shore (0 at the waterline, < 0 on land) at world point `p`.
fn depth_units(p: vec2<f32>) -> f32 {
    let uv = (p / 32.0 + vec2<f32>(0.5)) / globals.grid.xy;
    return textureSampleLevel(depth_map, depth_sampler, uv, 0.0).r * 255.0 - 127.0;
}

// How much the waves move here: calm in shallows, full on open water (water.rs `wave_scale`).
fn wave_scale(units: f32) -> f32 {
    return mix(0.15, 1.0, smoothstep(0.0, 40.0, units)) * globals.sun.w * globals.grid.w;
}

// 0 for the calm sea, 1 for the storm (`grid.w` is the waves' gain, water.rs `SeaState`).
fn storm_level() -> f32 {
    return clamp((globals.grid.w - 1.0) / 3.0, 0.0, 1.0);
}

struct Surface {
    offset: vec3<f32>,
    // d height / dx, d height / dy (the surface's slope).
    slope: vec2<f32>,
};

// The Gerstner sum (water.rs `Waves::sample` without the horizontal shift).
fn gerstner(p: vec2<f32>, scale: f32) -> Surface {
    var s: Surface;
    s.offset = vec3<f32>(0.0);
    s.slope = vec2<f32>(0.0);
    let t = globals.eye.w;
    // The storm's waves are choppier, but not 4 times: the gain is divided out of the
    // steepness (the crests would loop), and the steepness rises by half.
    let steep = STEEPNESS * mix(1.0, 1.5, storm_level()) / max(globals.grid.w, 1.0);
    for (var i = 0; i < WAVES; i++) {
        let w = globals.waves[i];
        let k = w.z;
        let a = w.w * scale;
        let phase = k * dot(w.xy, p) - sqrt(GRAVITY * k) * t + globals.phases[i / 4][i % 4];
        let c = cos(phase);
        let sn = sin(phase);
        let q = steep / (k * max(w.w, 1e-3) * f32(WAVES));
        s.offset += vec3<f32>(w.xy * (q * a * c), a * sn);
        s.slope += w.xy * (k * a * c);
    }
    return s;
}

fn hash2(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(0.1031, 0.1030));
    let r = q + dot(q, q.yx + 33.33);
    return fract((r.x + r.y) * r.x);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(hash2(i), hash2(i + vec2<f32>(1.0, 0.0)), u.x),
        mix(hash2(i + vec2<f32>(0.0, 1.0)), hash2(i + vec2<f32>(1.0, 1.0)), u.x),
        u.y,
    );
}

// The gradient of value noise at `p` (analytic: the same four corners as `noise`).
fn noise_grad(p: vec2<f32>) -> vec2<f32> {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let du = 6.0 * f * (1.0 - f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    let e = a - b - c + d;
    return du * vec2<f32>(b - a + e * u.y, c - a + e * u.x);
}

// The fine detail normal: two layers of value-noise octaves drifting in different directions,
// rotated per octave so no grid shows, faded where a noise cell would be smaller than a few
// screen pixels (`pixel`: world px per pixel), so nothing shimmers zoomed out. Returns a slope.
fn detail(p: vec2<f32>, pixel: f32, scale: f32) -> vec2<f32> {
    let t = globals.eye.w;
    var slope = vec2<f32>(0.0);
    var freq = 1.0 / 22.0;
    var amp = 0.075;
    let rot = mat2x2<f32>(0.8, 0.6, -0.6, 0.8);
    var q = p;
    for (var o = 0; o < 4; o++) {
        let fade = 1.0 - smoothstep(0.25, 0.6, pixel * freq);
        let drift = vec2<f32>(t * 9.0, t * 4.0) * (1.0 + 0.4 * f32(o));
        let g1 = noise_grad((q + drift) * freq);
        let g2 = noise_grad(vec2<f32>(q.y, -q.x) * (freq * 1.37) - drift.yx * (freq * 1.1) + vec2<f32>(17.0, 5.0));
        slope += (g1 + vec2<f32>(g2.y, -g2.x)) * (amp * fade);
        q = rot * q;
        freq *= 2.03;
        amp *= 0.62;
    }
    return slope * mix(0.45, 1.0, min(scale, 1.0)) * mix(1.0, 1.8, storm_level());
}

// Ships and rings: extra height (px) and foam at world point `p`.
struct Disturbed {
    height: f32,
    foam: f32,
};

fn disturbed(p: vec2<f32>) -> Disturbed {
    var out: Disturbed;
    out.height = 0.0;
    out.foam = 0.0;
    let t = globals.eye.w;
    let n_wakes = i32(globals.counts.x);
    for (var i = 0; i < MAX_WAKES; i++) {
        if (i >= n_wakes) {
            break;
        }
        let a = globals.wakes[2 * i];
        let b = globals.wakes[2 * i + 1];
        let speed = b.x;
        let half = b.y;
        let d = p - a.xy;
        // Ship frame: `ahead` along the heading, `side` across it.
        let ahead = dot(d, a.zw);
        let side = dot(d, vec2<f32>(-a.w, a.z));
        let behind = -ahead - half * 0.8;
        let reach = 900.0;
        if (behind > reach || ahead > half * 1.6 || abs(side) > reach * 0.5) {
            continue;
        }
        let strength = clamp(speed / 45.0, 0.0, 1.5);
        // The waterline: a thin, broken line of foam and darker water hugging the hull (an
        // ellipse in the ship's frame), at anchor too.
        let e = length(vec2<f32>(ahead / (half * 0.92), side / (half * 0.24)));
        out.foam += exp(-pow((e - 1.0) / 0.07, 2.0)) * (0.35 + 0.4 * min(strength, 1.0));
        // Kelvin: transverse waves of the ship's speed (λ = 2π V²/g) inside the wedge, the
        // divergent ones strongest along its edge; both fade with distance.
        // (λ at least 60 px: shorter lines alias into moiré seen from afar.)
        let k = clamp(GRAVITY / max(speed * speed, 1.0), TAU / 160.0, TAU / 60.0);
        let u = max(behind, 0.0);
        let edge = KELVIN * u + half * 0.18;
        let inside = 1.0 - smoothstep(edge * 0.85, edge * 1.05, abs(side));
        let fade = exp(-u / 420.0) * smoothstep(-half * 0.2, half * 0.3, behind);
        let transverse = cos(k * u - 0.5 * t) * inside * 0.55;
        let cusp = exp(-pow((abs(side) - edge) / (8.0 + 0.06 * u), 2.0));
        let divergent = cos(k * (abs(side) * 1.8 + u * 0.5)) * cusp;
        out.height += (transverse + 1.3 * divergent) * fade * 2.2 * strength;
        // Churned water: a band along the track (the hull's width, spreading), the cusp
        // lines, and the bow wave folding over at the stem.
        let band = 1.0 - smoothstep(half * 0.16 + 0.05 * u, half * 0.3 + 0.09 * u, abs(side));
        let track = band * exp(-u / 300.0) * smoothstep(-half * 0.3, 0.0, behind);
        let bow_d = vec2<f32>(ahead - half * 0.95, abs(side) - (half * 0.95 - ahead) * 0.32);
        let bow = exp(-dot(bow_d, bow_d) / (half * half * 0.012)) * step(ahead, half * 1.05);
        out.foam += (track * 0.7 + cusp * fade * 0.35 + bow) * min(strength, 1.0);
        out.height += bow * 3.0 * strength;
    }
    let n_rings = i32(globals.counts.y);
    for (var i = 0; i < MAX_RINGS; i++) {
        if (i >= n_rings) {
            break;
        }
        let r = globals.rings[i];
        let age = t - r.z;
        if (age < 0.0 || age > 6.0) {
            continue;
        }
        let dist = length(p - r.xy);
        // A ring spreading at about 2 m/s, a packet of short waves that flattens out.
        let front = 18.0 + 60.0 * age;
        let envelope = exp(-pow((dist - front) / (10.0 + 6.0 * age), 2.0)) * exp(-age * 0.6);
        out.height += sin(0.35 * (dist - front)) * envelope * 3.5 * r.w;
        // The splash: a white boil at the centre for the first second or two.
        out.foam += (1.0 - smoothstep(4.0, 26.0 + 14.0 * age, dist)) * exp(-age * 1.6) * r.w
            + envelope * 0.25 * r.w;
    }
    return out;
}

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    // The grid point before the waves moved it, and its depth units.
    @location(0) ground: vec3<f32>,
};

@vertex
fn vs_water(@builtin(vertex_index) vi: u32) -> VsOut {
    let n = u32(globals.grid.z);
    let cell = vi / 6u;
    var corners = array<vec2<u32>, 6>(
        vec2<u32>(0u, 0u),
        vec2<u32>(1u, 0u),
        vec2<u32>(0u, 1u),
        vec2<u32>(1u, 0u),
        vec2<u32>(1u, 1u),
        vec2<u32>(0u, 1u),
    );
    let corner = corners[vi % 6u];
    let ij = vec2<f32>(vec2<u32>(cell % n, cell / n) + corner) / f32(n);
    let p = mix(globals.rect.xy, globals.rect.zw, ij);
    let units = depth_units(p);
    let s = gerstner(p, wave_scale(units));
    var out: VsOut;
    let moved = p + s.offset.xy;
    out.pos = globals.view * vec4<f32>(moved, s.offset.z, 1.0);
    // A trough takes the depth of the point where its view ray crosses the water level: the
    // terrain's depth (only pushed back a little, terrain.wgsl `vs_depth`) would otherwise
    // cut holes into the troughs at low angles. Same ray, so the same pixel (orthographic).
    let lifted = vec3<f32>(moved, s.offset.z) - globals.eye.xyz * (min(s.offset.z, 0.0) / globals.eye.z);
    out.pos.z = (globals.view * vec4<f32>(lifted, 1.0)).z;
    out.ground = vec3<f32>(p, units);
    return out;
}

@fragment
fn fs_water(in: VsOut) -> @location(0) vec4<f32> {
    let p = in.ground.xy;
    // World px per screen pixel, for fading detail (before any non-uniform branch).
    let pixel = max(length(dpdx(p)), length(dpdy(p)));
    let units = depth_units(p);
    // The waterline: the original's tint starts where the depth passes 127.
    let edge = smoothstep(0.0, 5.0, units);
    if (edge <= 0.0) {
        discard;
    }
    let detailed = globals.sun.w;
    let scale = wave_scale(units);
    let s = gerstner(p, scale);
    var slope = s.slope + detail(p, pixel, scale) * detailed;
    var churn = 0.0;
    if (detailed > 0.0 && globals.counts.x + globals.counts.y > 0.0) {
        // Wakes and rings: their slope by central differences (only near them).
        let e = 1.5;
        let c = disturbed(p);
        let hx = disturbed(p + vec2<f32>(e, 0.0)).height - disturbed(p - vec2<f32>(e, 0.0)).height;
        let hy = disturbed(p + vec2<f32>(0.0, e)).height - disturbed(p - vec2<f32>(0.0, e)).height;
        slope += vec2<f32>(hx, hy) / (2.0 * e);
        churn = c.foam;
    }
    let normal = normalize(vec3<f32>(-slope, 1.0));
    let v = globals.eye.xyz;
    let l = globals.sun.xyz;

    // Depth colour and absorption: olive-grey shallows over the lakebed, dark slate open
    // water (lakes and bays seen from 30° above are dark and little saturated).
    let t = clamp(units / DEPTH_FULL, 0.0, 1.0);
    let shallow = vec3<f32>(0.115, 0.17, 0.15);
    let deep = vec3<f32>(0.024, 0.050, 0.062);
    let storm = storm_level();
    // Under a storm sky the water is darker and greyer, the sun's light dim.
    // The storm's steep faces show: lit on the side of the sun, almost black in the lee.
    let lit = mix(0.85 + 0.2 * max(dot(normal, l), 0.0), 0.3 + 1.6 * max(dot(normal, l) - 0.2, 0.0), storm);
    let body = mix(shallow, deep, sqrt(t)) * lit * mix(1.0, 0.8, storm);
    let absorbed = mix(0.35, 0.95, sqrt(t));

    // Sky by Fresnel (Schlick, water F0 = 0.02); no sky box yet: a hazy gradient by height.
    let cos_v = max(dot(normal, v), 0.0);
    let fresnel = (0.02 + 0.98 * pow(1.0 - cos_v, 5.0)) * mix(0.6, 1.0, detailed);
    let r = reflect(-v, normal);
    var sky = mix(vec3<f32>(0.60, 0.65, 0.68), vec3<f32>(0.32, 0.40, 0.48), sqrt(clamp(r.z, 0.0, 1.0)));
    sky = mix(sky, vec3<f32>(0.20, 0.22, 0.25), 0.7 * storm);
    // The glint: a soft sheen plus sparse sparkles on the facets that face the sun just so.
    let rl = max(dot(r, l), 0.0);
    let glint = (pow(rl, 60.0) * 0.05 + pow(rl, 900.0) * 0.5) * detailed * (1.0 - 0.8 * storm);

    // Foam: a broken band along the shore, and the churn of wakes and splashes.
    let time = globals.eye.w;
    let shore = 1.0 - smoothstep(1.0, 16.0, units);
    let f = noise(p * 0.07 + vec2<f32>(time * 0.35, time * 0.2)) * 0.6
        + noise(p * 0.19 - vec2<f32>(time * 0.25, -time * 0.3)) * 0.4;
    let shore_foam = smoothstep(0.55, 0.9, f + shore * 0.45 - 0.2) * shore * 0.55;
    let churned = smoothstep(0.25, 0.9, churn * (0.55 + 0.9 * f)) * 0.85;
    // Whitecaps (storm only): foam on the high crests and the steep faces, torn into streaks
    // along the wind (the long waves' direction) by noise stretched that way.
    let wind_dir = globals.waves[0].xy;
    let along = vec2<f32>(dot(p, wind_dir), dot(p, vec2<f32>(-wind_dir.y, wind_dir.x)));
    let streak = noise(vec2<f32>(along.x * 0.03 - time * 2.5, along.y * 0.2))
        * (0.55 + 0.45 * noise(p * 0.35 + vec2<f32>(time * 0.6, 0.0)));
    let crest = s.offset.z / max(scale, 1e-3) / 4.5 + length(s.slope) * 1.2;
    let caps = smoothstep(1.0, 1.7, crest * (0.45 + streak * 1.1)) * storm * edge;
    let foam = clamp(shore_foam + churned + caps * 0.9, 0.0, 1.0) * detailed;

    let reflected = fresnel * sky + (1.0 - fresnel) * absorbed * body;
    let rgb = foam * mix(vec3<f32>(0.80, 0.83, 0.82), vec3<f32>(0.66, 0.69, 0.70), storm) + (1.0 - foam) * reflected
        + vec3<f32>(glint);
    let alpha = 1.0 - (1.0 - foam) * (1.0 - fresnel) * (1.0 - absorbed);
    // Premultiplied: the lakebed keeps (1 - alpha) of its colour.
    return vec4<f32>(rgb * edge, alpha * edge);
}
