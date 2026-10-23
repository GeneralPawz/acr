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

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    // kind, turn, opacity, seed
    @location(2) @interpolate(flat) params: vec4<u32>,
    @location(3) @interpolate(flat) resting: u32,
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
    let cs = cos(turn);
    let sn = sin(turn);
    if kind != KIND_DUST && kind != KIND_SPRAY {
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
    return out;
}

fn hash11(n: f32) -> f32 {
    return fract(sin(n * 12.9898) * 43758.547);
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

@fragment
fn fs_particle(in: VsOut) -> @location(0) vec4<f32> {
    let kind = in.params.x;
    let opacity = f32(in.params.z) / 255.0;
    let seed = f32(in.params.w);
    let uv = in.uv;
    let r = length(uv);
    let sun = globals.sun.xyz;
    if kind == KIND_DUST || kind == KIND_SPRAY {
        // A soft puff with a ragged edge; lit from the sun's side a little.
        let a = atan2(uv.y, uv.x);
        let edge = 0.8 + 0.12 * sin(a * 4.0 + seed) + 0.08 * sin(a * 7.0 + seed * 1.7);
        let d = r / edge;
        var alpha = exp(-3.2 * d * d) * opacity;
        if kind == KIND_DUST {
            alpha = alpha * 0.55;
        } else {
            alpha = alpha * 0.8;
        }
        if alpha < 0.004 {
            discard;
        }
        let n = normalize(globals.right.xyz * uv.x + globals.up.xyz * uv.y + globals.toward.xyz * 0.8);
        let light = 0.75 + 0.35 * max(dot(n, sun), 0.0);
        return finish(in.color.rgb * light, alpha);
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
    if kind == KIND_MUD || kind == KIND_DROPLET {
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
