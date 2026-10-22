// Blood pools under corpses (docs/rendering-3d.md): one quad per pool lying on the ground,
// a noisy round pool with a trail running out away from the blow, fresh red drying to brown,
// blended over the terrain and tested against its depth.

struct Globals {
    view_proj: mat4x4<f32>,
    light: vec4<f32>,
    forward: vec4<f32>,
    // x: encode sRGB here (the target is not sRGB).
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

struct PoolIn {
    // centre on the ground (world), radius (world pixels)
    @location(0) centre: vec4<f32>,
    // direction of the trail (world x, y), stretch, seed
    @location(1) shape: vec4<f32>,
    // age (0 fresh .. 1 dried), fade (1 .. 0)
    @location(2) look: vec4<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    // position relative to the centre: along the trail, across it (world pixels)
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) shape: vec4<f32>,
    @location(2) @interpolate(flat) look: vec4<f32>,
    @location(3) @interpolate(flat) radius: f32,
};

// Height above the ground against z-fighting (world units).
const LIFT: f32 = 1.5;

@vertex
fn vs_pool(@builtin(vertex_index) vi: u32, p: PoolIn) -> VsOut {
    let corner = vec2<f32>(f32(vi & 1u), f32((vi >> 1u) & 1u));
    let r = p.centre.w;
    var along = p.shape.xy;
    if (dot(along, along) < 1e-6) {
        along = vec2<f32>(1.0, 0.0);
    }
    let across = vec2<f32>(-along.y, along.x);
    let a = mix(-r, r * (1.0 + 2.0 * p.shape.z), corner.x);
    let b = mix(-r, r, corner.y);
    let xy = p.centre.xy + along * a + across * b;
    var out: VsOut;
    out.pos = globals.view_proj * vec4<f32>(xy, p.centre.z + LIFT, 1.0);
    out.local = vec2<f32>(a, b);
    out.shape = p.shape;
    out.look = p.look;
    out.radius = r;
    return out;
}

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(hash2(i), hash2(i + vec2<f32>(1.0, 0.0)), u.x);
    let b = mix(hash2(i + vec2<f32>(0.0, 1.0)), hash2(i + vec2<f32>(1.0, 1.0)), u.x);
    return mix(a, b, u.y);
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

@fragment
fn fs_pool(in: VsOut) -> @location(0) vec4<f32> {
    let r = max(in.radius, 1.0);
    let seed = in.shape.w;
    // Distance in pool radii: the round pool, and the trail narrowing away from it.
    var d = length(in.local) / r;
    let reach = 2.0 * in.shape.z * r;
    if (reach > 0.0 && in.local.x > 0.0 && in.local.x < reach) {
        let width = r * 0.45 * (1.0 - in.local.x / reach) + 0.5;
        d = min(d, abs(in.local.y) / width);
    }
    let q = in.local / r;
    d = d + (noise2(q * 3.0 + vec2<f32>(seed)) - 0.5) * 0.45 + (noise2(q * 9.0 + vec2<f32>(seed * 1.7)) - 0.5) * 0.15;
    if (d > 1.0) {
        discard;
    }
    let age = in.look.x;
    let fresh = mix(vec3<f32>(0.30, 0.012, 0.01), vec3<f32>(0.16, 0.0, 0.0), 1.0 - smoothstep(0.0, 0.7, d));
    let dried = vec3<f32>(0.085, 0.035, 0.022);
    var rgb = mix(fresh, dried, smoothstep(0.0, 1.0, age));
    // A wet sheen on fresh blood.
    rgb = rgb + vec3<f32>(0.08, 0.02, 0.02) * (1.0 - age) * smoothstep(0.6, 0.95, noise2(q * 5.0 + vec2<f32>(seed * 0.3)));
    let alpha = smoothstep(1.0, 0.75, d) * mix(0.9, 0.65, age) * in.look.y;
    if (globals.params.x > 0.5) {
        rgb = to_srgb(rgb);
    }
    return vec4<f32>(rgb, alpha);
}
