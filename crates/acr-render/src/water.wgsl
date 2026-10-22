// The water surface of the 3D pass (water.rs, docs/water.md): a grid at the water level
// (z = 0, where the original draws its depth tint) over the visible part of the map, moved by
// Gerstner waves scaled by the `SEA2` depth, shaded with depth colour and absorption (the
// lakebed shows through), a sky reflection by Fresnel, the sun's glint and foam at the shore.

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
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var depth_map: texture_2d<f32>;
@group(0) @binding(2) var depth_sampler: sampler;

const TAU: f32 = 6.2831853;
// Gravity in world pixels per s² (about 30 px to the metre).
const GRAVITY: f32 = 294.0;
// The original's depth tint saturates at 64 depth units above the shore value 127.
const DEPTH_FULL: f32 = 64.0;

// Depth units above the shore (0 at the waterline, < 0 on land) at world point `p`.
fn depth_units(p: vec2<f32>) -> f32 {
    let uv = (p / 32.0 + vec2<f32>(0.5)) / globals.grid.xy;
    return textureSampleLevel(depth_map, depth_sampler, uv, 0.0).r * 255.0 - 127.0;
}

// How much the waves move here: calm in shallows, full on open water.
fn wave_scale(units: f32) -> f32 {
    return mix(0.15, 1.0, smoothstep(0.0, 40.0, units)) * globals.sun.w;
}

// The big waves: direction, wavelength (px), amplitude (px), steepness.
fn wave(i: i32) -> vec4<f32> {
    switch i {
        case 0: { return vec4<f32>(0.96, 0.28, 260.0, 4.0); }
        case 1: { return vec4<f32>(0.71, 0.71, 150.0, 2.6); }
        case 2: { return vec4<f32>(0.20, 0.98, 90.0, 1.5); }
        default: { return vec4<f32>(-0.42, 0.91, 55.0, 0.8); }
    }
}

const STEEPNESS: f32 = 0.6;

struct Surface {
    offset: vec3<f32>,
    // d height / dx, d height / dy (the surface's slope).
    slope: vec2<f32>,
    // Height relative to the sum of amplitudes (-1..1), for crest foam.
    crest: f32,
};

fn gerstner(p: vec2<f32>, scale: f32) -> Surface {
    var s: Surface;
    s.offset = vec3<f32>(0.0);
    s.slope = vec2<f32>(0.0);
    s.crest = 0.0;
    var total = 0.0;
    let t = globals.eye.w;
    for (var i = 0; i < 4; i++) {
        let w = wave(i);
        let d = normalize(w.xy);
        let k = TAU / w.z;
        let a = w.w * scale;
        let phase = k * dot(d, p) - sqrt(GRAVITY * k) * t;
        let c = cos(phase);
        let sn = sin(phase);
        let q = STEEPNESS / (k * w.w * 4.0);
        s.offset += vec3<f32>(d * (q * a * c), a * sn);
        s.slope += d * (k * a * c);
        s.crest += w.w * sn;
        total += w.w;
    }
    s.crest /= total;
    return s;
}

// Small ripples for the shading only (normal detail below the grid's resolution).
fn ripples(p: vec2<f32>, scale: f32) -> vec2<f32> {
    let t = globals.eye.w;
    var slope = vec2<f32>(0.0);
    var dirs = array<vec2<f32>, 4>(
        vec2<f32>(0.8, 0.6),
        vec2<f32>(-0.6, 0.8),
        vec2<f32>(0.99, -0.14),
        vec2<f32>(0.3, -0.95),
    );
    var lengths = array<f32, 4>(31.0, 23.0, 17.0, 12.0);
    for (var i = 0; i < 4; i++) {
        let k = TAU / lengths[i];
        let a = 0.12 * lengths[i] / 31.0 * scale;
        let phase = k * dot(dirs[i], p) - sqrt(GRAVITY * k) * t;
        slope += dirs[i] * (k * a * cos(phase));
    }
    return slope;
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
    let units = depth_units(p);
    // The waterline: the original's tint starts where the depth passes 127.
    let edge = smoothstep(0.0, 5.0, units);
    if (edge <= 0.0) {
        discard;
    }
    let detailed = globals.sun.w;
    let scale = wave_scale(units);
    let s = gerstner(p, scale);
    let slope = s.slope + ripples(p, scale);
    let normal = normalize(vec3<f32>(-slope, 1.0));
    let v = globals.eye.xyz;
    let l = globals.sun.xyz;

    // Depth colour and absorption: green-blue shallows over the lakebed, dark open water.
    let t = clamp(units / DEPTH_FULL, 0.0, 1.0);
    let shallow = vec3<f32>(0.16, 0.36, 0.33);
    let deep = vec3<f32>(0.035, 0.12, 0.17);
    let body = mix(shallow, deep, sqrt(t)) * (0.75 + 0.35 * max(dot(normal, l), 0.0));
    let absorbed = mix(0.30, 0.94, sqrt(t));

    // Sky by Fresnel (Schlick, water F0 = 0.02); no sky box yet: a gradient by height.
    let cos_v = max(dot(normal, v), 0.0);
    let fresnel = (0.02 + 0.98 * pow(1.0 - cos_v, 5.0)) * mix(0.6, 1.0, detailed);
    let r = reflect(-v, normal);
    let sky = mix(vec3<f32>(0.72, 0.79, 0.84), vec3<f32>(0.36, 0.53, 0.74), sqrt(clamp(r.z, 0.0, 1.0)));
    let glint = pow(max(dot(r, l), 0.0), 400.0) * 1.5 * detailed;

    // Foam: a broken band along the shore and a little on the crests of open water.
    let time = globals.eye.w;
    let shore = 1.0 - smoothstep(1.0, 18.0, units);
    let f = noise(p * 0.07 + vec2<f32>(time * 0.35, time * 0.2)) * 0.6
        + noise(p * 0.19 - vec2<f32>(time * 0.25, -time * 0.3)) * 0.4;
    let crest = smoothstep(0.7, 0.98, s.crest) * smoothstep(30.0, 60.0, units) * 0.12;
    let foam = clamp(smoothstep(0.5, 0.85, f + shore * 0.45 - 0.2) * shore + crest * f, 0.0, 1.0)
        * 0.7 * detailed;

    let reflected = fresnel * sky + (1.0 - fresnel) * absorbed * body;
    let rgb = foam * vec3<f32>(0.92, 0.95, 0.95) + (1.0 - foam) * reflected + vec3<f32>(glint);
    let alpha = 1.0 - (1.0 - foam) * (1.0 - fresnel) * (1.0 - absorbed);
    // Premultiplied: the lakebed keeps (1 - alpha) of its colour.
    return vec4<f32>(rgb * edge, alpha * edge);
}
