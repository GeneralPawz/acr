// Ground cover of the 3D pass (ground_cover.rs): upright kinds are three crossed cards
// (18 vertices), flat kinds one card lying on the ground (6 vertices). The shapes are cut
// procedurally by alpha (blades, fronds, flower heads, leaves, pebbles); the wind bends the
// upright cards by the square of the height, the ground state's compression lays them down.

struct Globals {
    view: mat4x4<f32>,
    // xy direction, z strength, w clock (seconds).
    wind: vec4<f32>,
    // xyz the sun, w 1 when the shader encodes sRGB.
    sun: vec4<f32>,
    // x the zoom fade, yz the free camera's target, w its reach (0: fixed camera).
    fade: vec4<f32>,
    // x clip depth per world px, y the camera scale.
    misc: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

const KIND_GRASS: u32 = 0u;
const KIND_DRY: u32 = 1u;
const KIND_FLOWER: u32 = 2u;
const KIND_FERN: u32 = 3u;
const KIND_LITTER: u32 = 4u;
const KIND_PEBBLE: u32 = 5u;
const TAU: f32 = 6.2831853;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
    // kind, seed, flattening, card
    @location(2) @interpolate(flat) params: vec4<u32>,
    // uv x units per screen pixel (blades stay at least about a pixel wide).
    @location(3) @interpolate(flat) px: f32,
};

@vertex
fn vs_cover(
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec3<f32>,
    @location(1) size: f32,
    @location(2) color: vec4<f32>,
    @location(3) params: vec4<u32>,
) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let c = corners[vi % 6u];
    let card = vi / 6u;
    let kind = u32(color.a * 255.0 + 0.5);
    var out: VsOut;
    // Fade by rank: the zoom, and the distance from the free camera's target.
    var f = globals.fade.x;
    if globals.fade.w > 0.0 {
        let d = distance(pos.xy, globals.fade.yz);
        f = f * (1.0 - smoothstep(0.5 * globals.fade.w, globals.fade.w, d));
    }
    let rank = f32(params.w) / 255.0;
    let grow = smoothstep(rank * 0.85, rank * 0.85 + 0.15, f);
    if grow < 0.02 {
        out.clip = vec4<f32>(0.0, 0.0, -2.0, 1.0);
        return out;
    }
    let seed = f32(params.y);
    let yaw = f32(params.x) / 256.0 * TAU + f32(card) * 1.0472;
    let ax = vec3<f32>(cos(yaw), sin(yaw), 0.0);
    let flat_k = f32(params.z) / 255.0;
    let s = size * grow;
    var p = pos;
    var half = 0.5 * s;
    if kind >= KIND_LITTER {
        let ay = vec3<f32>(-ax.y, ax.x, 0.0);
        p = pos + (ax * c.x + ay * (c.y * 2.0 - 1.0)) * half + vec3<f32>(0.0, 0.0, 0.6);
    } else {
        if kind == KIND_FERN {
            half = 0.65 * s;
        }
        let h = s * (1.0 - 0.75 * flat_k);
        p = pos + ax * c.x * half + vec3<f32>(0.0, 0.0, c.y * h);
        // Trodden: laid over away from a hashed direction.
        let lean = seed / 255.0 * TAU;
        p = p + vec3<f32>(cos(lean), sin(lean), 0.0) * flat_k * s * 0.7 * c.y;
        // Wind: a slow sway rolling over the field and a quick flutter, by height squared.
        let w = globals.wind;
        let phase = w.w * 1.9 - dot(pos.xy, w.xy) * 0.006 + seed * 0.03;
        let gust = 0.55 + 0.45 * sin(phase) + 0.15 * sin(w.w * 6.7 + seed * 0.7);
        p = p + vec3<f32>(w.x, w.y, 0.0) * w.z * gust * s * 0.25 * c.y * c.y;
    }
    var clip = globals.view * vec4<f32>(p, 1.0);
    // Orthographic (w = 1): pulled 3 px towards the camera, so the slope behind the
    // card's foot does not cut it.
    clip.z = clamp(clip.z - globals.misc.x * 3.0, 0.0, 1.0);
    out.clip = clip;
    out.uv = c;
    out.color = pow(color.rgb, vec3<f32>(2.2)) * (1.0 - 0.3 * flat_k);
    out.params = vec4<u32>(kind, params.y, params.z, card);
    out.px = 1.0 / max(half * globals.misc.y, 0.01);
    return out;
}

fn hash11(n: f32) -> f32 {
    var p = fract(n * 0.1031);
    p = p * (p + 33.33);
    p = p * (p + p);
    return fract(p);
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn finish(rgb: vec3<f32>) -> vec4<f32> {
    var c = min(rgb, vec3<f32>(1.0));
    if globals.sun.w > 0.5 {
        c = to_srgb(c);
    }
    return vec4<f32>(c, 1.0);
}

// Blades of a tuft: `count` tapering, leaning strands; returns the hit blade's height
// fraction (0 at the root, 1 at the tip), or -1.
fn blades(uv: vec2<f32>, seed: f32, count: i32, width: f32, min_h: f32, px: f32) -> vec2<f32> {
    for (var i = 0; i < count; i = i + 1) {
        let k = seed * 7.13 + f32(i) * 3.71;
        let x0 = (hash11(k) - 0.5) * 1.1;
        let lean = (hash11(k + 1.3) - 0.5) * 1.1 + x0 * 0.6;
        let hb = min_h + (1.0 - min_h) * hash11(k + 2.7);
        let t = uv.y / hb;
        if t > 1.0 {
            continue;
        }
        let cx = x0 + lean * t * t;
        let w = max(width * (1.0 - t * 0.92), px * 0.6);
        if abs(uv.x - cx) < w {
            return vec2<f32>(t, f32(i));
        }
    }
    return vec2<f32>(-1.0, 0.0);
}

@fragment
fn fs_cover(in: VsOut) -> @location(0) vec4<f32> {
    let kind = in.params.x;
    let seed = f32(in.params.y) + f32(in.params.w) * 17.0;
    let uv = in.uv;
    let sun = globals.sun.xyz;
    let lit = 0.62 + 0.5 * max(sun.z, 0.0);
    if kind == KIND_LITTER {
        // Fallen leaves: five ellipses, each its own brown, with a darker vein.
        let q = vec2<f32>(uv.x, uv.y * 2.0 - 1.0);
        for (var i = 0; i < 5; i = i + 1) {
            let k = seed * 5.3 + f32(i) * 2.9;
            let c = vec2<f32>(hash11(k) - 0.5, hash11(k + 0.7) - 0.5) * 1.3;
            let a = hash11(k + 1.9) * TAU;
            let d = q - c;
            let r = vec2<f32>(d.x * cos(a) + d.y * sin(a), -d.x * sin(a) + d.y * cos(a));
            let e = (r.x * r.x) / 0.07 + (r.y * r.y) / 0.018;
            if e < 1.0 {
                let tone = vec3<f32>(0.75 + 0.5 * hash11(k + 3.1), 0.7 + 0.45 * hash11(k + 4.3), 0.6 + 0.3 * hash11(k + 5.7));
                let vein = select(1.0, 0.7, abs(r.y) < 0.012);
                return finish(in.color * tone * vein * lit * (0.85 + 0.15 * (1.0 - e)));
            }
        }
        discard;
    }
    if kind == KIND_PEBBLE {
        let q = vec2<f32>(uv.x, uv.y * 2.0 - 1.0);
        for (var i = 0; i < 3; i = i + 1) {
            let k = seed * 3.7 + f32(i) * 4.1;
            let c = vec2<f32>(hash11(k) - 0.5, hash11(k + 0.9) - 0.5) * 1.1;
            let rad = 0.22 + 0.2 * hash11(k + 1.7);
            let d = (q - c) / rad;
            let ang = atan2(d.y, d.x);
            let edge = 1.0 + 0.12 * sin(ang * 3.0 + k) + 0.06 * sin(ang * 5.0 + k * 2.0);
            let r = length(d) / edge;
            if r < 1.0 {
                let nz = sqrt(max(1.0 - r * r, 0.0));
                let n = normalize(vec3<f32>(d.x, d.y, nz * 1.3));
                let diffuse = max(dot(n, sun), 0.0);
                let tone = 0.8 + 0.35 * hash11(k + 2.3);
                return finish(in.color * tone * (0.35 + 0.8 * diffuse));
            }
        }
        discard;
    }
    var t = -1.0;
    var rgb = in.color;
    if kind == KIND_FERN {
        // Two fronds per card arching out to either side, with serrated pinnae.
        for (var i = 0; i < 2; i = i + 1) {
            let side = f32(i) * 2.0 - 1.0;
            let hmax = 0.8 + 0.2 * hash11(seed + f32(i) * 5.0);
            let y = uv.y / hmax;
            if y > 1.0 {
                continue;
            }
            let stem = side * 0.75 * y * y;
            let d = abs(uv.x - stem);
            // Pinnae: narrow leaflets sloping up from the stem, gaps between them.
            let along = y * 26.0 + d * 9.0 + seed;
            let leaflet = pow(abs(sin(along)), 3.0);
            let pinna = 0.32 * (1.0 - y) * leaflet * smoothstep(0.0, 0.15, y);
            let stem_w = max(0.018, in.px * 0.6);
            if d < max(pinna, stem_w) {
                t = y;
                rgb = in.color * (0.85 + 0.3 * leaflet);
                break;
            }
        }
    } else {
        var count = 7;
        var width = 0.08;
        var min_h = 0.55;
        if kind == KIND_DRY {
            count = 9;
            width = 0.055;
            min_h = 0.45;
        } else if kind == KIND_FLOWER {
            // Heads: two or three discs on thin stems.
            for (var i = 0; i < 3; i = i + 1) {
                let k = seed * 2.3 + f32(i) * 6.1;
                let hx = (hash11(k) - 0.5) * 1.2;
                let hy = 0.72 + 0.26 * hash11(k + 1.1);
                let d = vec2<f32>((uv.x - hx) * 0.5, uv.y - hy);
                let pick = hash11(seed * 0.37);
                var petal = vec3<f32>(0.85, 0.85, 0.8);
                if pick > 0.75 {
                    petal = vec3<f32>(0.85, 0.7, 0.12);
                } else if pick > 0.5 {
                    petal = vec3<f32>(0.45, 0.3, 0.7);
                } else if pick > 0.35 {
                    petal = vec3<f32>(0.75, 0.12, 0.08);
                }
                if length(d) < 0.075 {
                    let centre = select(1.0, 0.5, length(d) < 0.025);
                    return finish(petal * centre * lit);
                }
                if uv.y < hy && abs(uv.x - hx) < max(0.02, in.px * 0.6) {
                    return finish(in.color * 0.8 * lit);
                }
            }
            count = 5;
        }
        let b = blades(uv, seed, count, width, min_h, in.px);
        t = b.x;
        if kind == KIND_DRY {
            // Straw: paler towards the tips, some blades greyer.
            rgb = in.color * (0.85 + 0.25 * hash11(seed + b.y * 1.7)) * (0.9 + 0.15 * t);
        }
    }
    if t < 0.0 {
        discard;
    }
    // Dark at the root (in the tuft's own shade), lit towards the tip.
    let ao = 0.5 + 0.5 * t;
    return finish(rgb * ao * lit);
}
