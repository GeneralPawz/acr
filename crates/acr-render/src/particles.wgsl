// Particles of the 3D pass (particles.rs): camera-facing quads, six vertices per instance.
// Hard kinds (clods, mud, grass, splinters, droplets) are lit lumpy impostors cut by alpha;
// soft kinds (dust, spray) are soft puffs. The quad keeps its centre's depth, pulled towards
// the camera by its radius, so a clod on the ground is not cut by the slope behind it.

struct Globals {
    view: mat4x4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    toward: vec4<f32>,
    // xyz the sun, w the clip depth per world px.
    sun: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

const KIND_CLOD: u32 = 0u;
const KIND_MUD: u32 = 1u;
const KIND_DUST: u32 = 2u;
const KIND_GRASS: u32 = 3u;
const KIND_SPRAY: u32 = 4u;
const KIND_DROPLET: u32 = 5u;
const KIND_SPLINTER: u32 = 6u;
const KIND_SOD: u32 = 7u;
const KIND_GORE: u32 = 8u;
const KIND_FLAME: u32 = 9u;
const KIND_EMBER: u32 = 10u;
const KIND_SMOKE: u32 = 11u;
const KIND_GLOW: u32 = 12u;
// The grass on a sod's top (linear).
const SOD_GRASS: vec3<f32> = vec3<f32>(0.045, 0.085, 0.02);

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    // kind, turn, opacity, seed
    @location(2) @interpolate(flat) params: vec4<u32>,
    @location(3) @interpolate(flat) resting: u32,
    // Soft kinds: the fragment's height above the ground and the radius (world px).
    @location(4) soft: vec2<f32>,
};

@vertex
fn vs_particle(
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec3<f32>,
    @location(1) size: f32,
    @location(2) color: vec4<f32>,
    @location(3) params: vec4<u32>,
) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let c = corners[vi];
    let kind = u32(color.a * 255.0 + 0.5);
    let turn = f32(params.x) / 256.0 * 6.2831853;
    var q = c;
    // Blades and splinters are long and thin.
    if kind == KIND_GRASS || kind == KIND_SPLINTER {
        q = vec2<f32>(c.x * 0.28, c.y * 1.6);
    }
    // Sods are slabs: wider than thick.
    if kind == KIND_SOD {
        q = vec2<f32>(c.x * 1.15, c.y * 0.7);
    }
    // A tongue of flame stands upright and tall, its root at the centre's lower half.
    if kind == KIND_FLAME {
        q = vec2<f32>(c.x * 0.7, c.y * 1.5 + 0.5);
    }
    let cs = cos(turn);
    let sn = sin(turn);
    if kind < KIND_DUST || kind == KIND_GRASS || kind == KIND_SPLINTER || kind == KIND_SOD || kind == KIND_GORE || kind == KIND_DROPLET {
        q = vec2<f32>(q.x * cs - q.y * sn, q.x * sn + q.y * cs);
    }
    let world = pos + (globals.right.xyz * q.x + globals.up.xyz * q.y) * size;
    let centre = globals.view * vec4<f32>(pos, 1.0);
    var clip = globals.view * vec4<f32>(world, 1.0);
    // Orthographic: the quad takes its centre's depth, pulled towards the camera.
    clip.z = clamp(centre.z - size * 0.8 * globals.sun.w, 0.0, 1.0);
    var out: VsOut;
    out.clip = clip;
    out.uv = c;
    // Colours come as sRGB: light them in linear.
    out.color = vec4<f32>(pow(color.rgb, vec3<f32>(2.2)), f32(kind));
    out.params = vec4<u32>(kind, params.x, params.y, params.z);
    out.resting = params.w;
    // The ground under a soft puff is its clearance below the centre; the quad faces the
    // camera, so a corner's height is the centre's plus the up vector's z.
    out.soft = vec2<f32>(f32(params.w) + globals.up.z * q.y * size, size);
    return out;
}

fn hash11(n: f32) -> f32 {
    return fract(sin(n * 12.9898) * 43758.547);
}

fn hash21(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.547);
}

// Value noise, 0..1.
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Three octaves of value noise, 0..1.
fn fbm(p: vec2<f32>) -> f32 {
    return 0.55 * vnoise(p) + 0.3 * vnoise(p * 2.13 + vec2<f32>(5.2, 1.3)) + 0.15 * vnoise(p * 4.37 + vec2<f32>(1.7, 9.2));
}

// The fire kinds' noise: an arithmetic hash (no `sin` of large arguments, whose low
// precision on some GPUs shows as blocks and checkers on the dark smoke).
fn hash21f(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn vnoisef(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21f(i);
    let b = hash21f(i + vec2<f32>(1.0, 0.0));
    let c = hash21f(i + vec2<f32>(0.0, 1.0));
    let d = hash21f(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbmf(p: vec2<f32>) -> f32 {
    return 0.55 * vnoisef(p) + 0.3 * vnoisef(p * 2.13 + vec2<f32>(5.2, 1.3)) + 0.15 * vnoisef(p * 4.37 + vec2<f32>(1.7, 9.2));
}

// A lumpy outline: the radius (0..1) of the shape at angle `a` for seed `s`.
fn lump(a: f32, s: f32) -> f32 {
    let r = 0.78 + 0.1 * sin(a * 3.0 + s * 6.3) + 0.07 * sin(a * 5.0 + s * 2.1) + 0.05 * sin(a * 7.0 + s * 9.7);
    return r;
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

// Premultiplied output, encoded when the target does not.
fn finish(rgb: vec3<f32>, alpha: f32) -> vec4<f32> {
    var c = min(rgb, vec3<f32>(1.0));
    if globals.toward.w > 0.5 {
        c = to_srgb(c);
    }
    return vec4<f32>(c * alpha, alpha);
}

// Fire light: `rgb` is already weighted by its coverage and added as it is (premultiplied
// with a small `alpha` that dims what is behind).
fn emit(rgb: vec3<f32>, alpha: f32) -> vec4<f32> {
    var c = min(rgb, vec3<f32>(1.0));
    if globals.toward.w > 0.5 {
        c = to_srgb(c);
    }
    return vec4<f32>(c, alpha);
}

@fragment
fn fs_particle(in: VsOut) -> @location(0) vec4<f32> {
    let kind = in.params.x;
    let opacity = f32(in.params.z) / 255.0;
    let seed = f32(in.params.w);
    let uv = in.uv;
    let r = length(uv);
    let sun = globals.sun.xyz;
    if kind == KIND_FLAME {
        // Emissive: white-yellow at the root, orange, then dull red at the licking tip,
        // its edge torn by noise rolling upward. Mostly added to what is behind (low alpha).
        let t = f32(in.params.y) / 256.0;
        let o = vec2<f32>(seed * 0.071, seed * 0.113 - t * 6.0);
        let n = fbmf(vec2<f32>(uv.x * 2.2, uv.y * 1.6) + o);
        // 0 at the root (uv.y -1), 1 at the tip.
        let up = clamp((uv.y + 1.0) * 0.5, 0.0, 1.0);
        // A teardrop: round at the root, a wavering tip.
        let width = sqrt(max(1.0 - up, 0.0)) * smoothstep(0.0, 0.25, up) * (0.45 + 0.6 * n) * 0.9;
        let x = abs(uv.x + (n - 0.5) * 0.5 * up);
        let body = 1.0 - smoothstep(width * 0.4, width, x);
        let heat = body * (1.0 - up * (0.6 + 0.5 * n)) * opacity;
        if heat < 0.01 {
            discard;
        }
        let core = smoothstep(0.55, 1.0, heat);
        let rgb = mix(in.color.rgb, vec3<f32>(1.0, 0.82, 0.45) * 1.5, core) * heat * 1.3;
        return emit(rgb, heat * 0.15);
    }
    if kind == KIND_EMBER {
        let g = exp(-5.0 * r * r) * opacity;
        if g < 0.01 {
            discard;
        }
        return emit(in.color.rgb * 2.5 * g, g * 0.1);
    }
    if kind == KIND_GLOW {
        // A faint warm light; no alpha, purely added.
        let g = exp(-3.2 * r * r) * opacity * 0.1;
        if g < 0.003 {
            discard;
        }
        return emit(in.color.rgb * g, 0.0);
    }
    if kind == KIND_DUST || kind == KIND_SPRAY || kind == KIND_SMOKE {
        // A billowing puff: a gaussian broken up by turning noise, lit as a ball whose
        // surface the noise dents, and faded where it meets the ground (no hard line).
        let turn = f32(in.params.y) / 256.0 * 6.2831853;
        let cs = cos(turn);
        let sn = sin(turn);
        let ruv = vec2<f32>(uv.x * cs - uv.y * sn, uv.x * sn + uv.y * cs);
        let o = vec2<f32>(seed * 0.37, seed * 0.61);
        var billow = 0.0;
        var fine = 0.0;
        var bump = vec2<f32>(0.0);
        if kind == KIND_SMOKE {
            let os = o * 0.2;
            billow = fbmf(ruv * 1.9 + os);
            fine = vnoisef(ruv * 5.3 + os * 1.7);
            bump = vec2<f32>(fbmf(ruv * 1.9 + os + vec2<f32>(0.15, 0.0)) - billow, fbmf(ruv * 1.9 + os + vec2<f32>(0.0, 0.15)) - billow);
        } else {
            billow = fbm(ruv * 1.9 + o);
            fine = vnoise(ruv * 5.3 + o * 1.7);
            bump = vec2<f32>(fbm(ruv * 1.9 + o + vec2<f32>(0.15, 0.0)) - billow, fbm(ruv * 1.9 + o + vec2<f32>(0.0, 0.15)) - billow);
        }
        let d = r / (0.75 + 0.35 * billow);
        var alpha = exp(-2.6 * d * d) * (0.35 + 0.9 * billow) * (0.8 + 0.3 * fine) * opacity;
        if kind == KIND_DUST {
            alpha = alpha * 0.62;
        } else if kind == KIND_SMOKE {
            alpha = alpha * 0.7;
        } else {
            alpha = alpha * 0.8;
        }
        alpha = alpha * smoothstep(0.0, max(in.soft.y * 0.45, 1.0), in.soft.x);
        if alpha < 0.004 {
            discard;
        }
        let n = normalize(
            globals.right.xyz * (uv.x - bump.x * 4.0) + globals.up.xyz * (uv.y - bump.y * 4.0) + globals.toward.xyz * 0.7,
        );
        // Dense parts are darker inside (self-shadowed), the sun side lighter.
        let light = (0.5 + 0.55 * max(dot(n, sun), 0.0)) * (0.8 + 0.25 * billow);
        return finish(in.color.rgb * light, alpha);
    }
    if kind == KIND_SOD {
        // A slab: a lumpy outline over the squashed quad, grass on the top edge (in its own
        // turning frame), earth below, lit as a flat-ish block.
        let s = seed / 255.0;
        let a = atan2(uv.y, uv.x);
        let edge_s = 0.85 + 0.08 * sin(a * 3.0 + s * 6.3) + 0.06 * sin(a * 6.0 + s * 2.1);
        if max(abs(uv.x), abs(uv.y) * 1.1) > edge_s || r > edge_s * 1.12 {
            discard;
        }
        let top = uv.y + 0.08 * sin(uv.x * 23.0 + s * 40.0) + 0.05 * sin(uv.x * 51.0 + s * 13.0);
        let n = normalize(globals.right.xyz * uv.x * 0.6 + globals.up.xyz * uv.y * 0.9 + globals.toward.xyz);
        let diffuse = max(dot(n, sun), 0.0);
        let grain = 0.8 + 0.4 * hash11(floor(uv.x * 6.0 + 7.0) * 13.0 + floor(uv.y * 6.0 + 7.0) * 7.0 + seed);
        var base = in.color.rgb * grain;
        if top > 0.3 {
            base = SOD_GRASS * (0.75 + 0.5 * hash11(floor(uv.x * 14.0) + seed));
        } else if top > 0.18 {
            // Roots: dark, matted.
            base = in.color.rgb * 0.55;
        }
        return finish(base * (0.35 + 0.85 * diffuse), opacity);
    }
    var edge = 1.0;
    if kind == KIND_GRASS || kind == KIND_SPLINTER {
        // Thin: the quad itself is the blade; taper the ends.
        if abs(uv.x) > 1.0 - abs(uv.y) * 0.6 {
            discard;
        }
        let n = normalize(globals.toward.xyz + globals.right.xyz * uv.x * 0.6);
        let light = 0.55 + 0.6 * max(dot(n, sun), 0.0);
        return finish(in.color.rgb * light * (0.9 + 0.2 * uv.y), opacity);
    }
    if kind == KIND_DROPLET {
        edge = 0.9;
    } else {
        edge = lump(atan2(uv.y, uv.x), seed / 255.0);
    }
    if r > edge {
        discard;
    }
    // A bulging normal over the impostor; resting mud lies flat (splat).
    let d = r / edge;
    var nz = sqrt(max(1.0 - d * d, 0.0));
    if kind == KIND_MUD && in.resting == 1u {
        nz = nz + 1.5;
    }
    let n = normalize(globals.right.xyz * uv.x / edge + globals.up.xyz * uv.y / edge + globals.toward.xyz * nz);
    let diffuse = max(dot(n, sun), 0.0);
    // Crumbs: a little grain inside the clod.
    let grain = 0.85 + 0.3 * hash11(floor(uv.x * 4.0 + 7.0) * 13.0 + floor(uv.y * 4.0 + 7.0) * 7.0 + seed);
    var rgb = in.color.rgb * (0.35 + 0.85 * diffuse) * grain;
    if kind == KIND_MUD || kind == KIND_DROPLET || kind == KIND_GORE {
        // Wet: a sharp glint of the sun.
        let h = normalize(sun + globals.toward.xyz);
        let spec = pow(max(dot(n, h), 0.0), 40.0);
        var k = 0.35;
        if kind == KIND_DROPLET {
            k = 0.9;
        }
        rgb = rgb + vec3<f32>(spec * k);
    }
    return finish(rgb, opacity);
}
