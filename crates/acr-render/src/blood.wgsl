// Blood pools under corpses and splatter behind hit units (docs/rendering-3d.md): one quad
// per pool lying on the ground, a noisy round pool with a trail running out away from the
// blow (or a droplet: a crisp head with a tail), fresh red drying to brown, blended over the
// terrain and tested against its depth. The same decal in earth colours marks where rounds
// struck the ground: a cannonball's crater, a musket ball's scuff. Each decal lies on the
// slope of the ground under it (`tilt`). A smear is one segment of the ribbon a body leaves
// when dragged through blood: its edges and streaks are noise in world space, so the segments
// of one trail join without seams.

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
    // age (0 fresh .. 1 dried), fade (1 .. 0), kind (0 pool, 1 droplet, 2 crater, 3 scuff,
    // 4 smear), a smear's density
    @location(2) look: vec4<f32>,
    // the ground's slope: world z per world x, per world y; a smear's start width
    @location(3) tilt: vec4<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    // position relative to the centre: along the trail, across it (world pixels)
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) shape: vec4<f32>,
    @location(2) @interpolate(flat) look: vec4<f32>,
    @location(3) @interpolate(flat) radius: f32,
    // world position (x, y)
    @location(4) world: vec2<f32>,
    @location(5) @interpolate(flat) start: f32,
};

// Height above the ground against z-fighting (world units).
const LIFT: f32 = 1.5;
const SMEAR_LIFT: f32 = 4.0;

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
    let off = xy - p.centre.xy;
    // A smear is narrow and long over bumpy ground: lifted more so the bumps do not cut it up.
    let lift = select(LIFT, SMEAR_LIFT, p.look.z > 3.5);
    let z = p.centre.z + lift + dot(p.tilt.xy, off);
    out.pos = globals.view_proj * vec4<f32>(xy, z, 1.0);
    out.local = vec2<f32>(a, b);
    out.shape = p.shape;
    out.look = p.look;
    out.radius = r;
    out.world = xy;
    out.start = p.tilt.z;
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

// Churned earth where a round struck: `crater` a dark pit in a ring of thrown earth, else a
// dusty scuff. Fades with `look.y`, the fresh earth greys with `look.x`.
fn dirt(in: VsOut, crater: bool) -> vec4<f32> {
    let r = max(in.radius, 1.0);
    let seed = in.shape.w;
    let q = in.local / r;
    var d = length(q);
    // Thrown earth runs out along the flight (the tail), ragged at the rim.
    let reach = 2.0 * in.shape.z;
    if (reach > 0.0 && q.x > 0.0 && q.x < reach) {
        let t = 1.0 - q.x / reach;
        d = min(d, abs(q.y) / (0.8 * t + 0.05));
    }
    d = d + (noise2(q * 4.0 + vec2<f32>(seed)) - 0.5) * 0.5 + (noise2(q * 11.0 + vec2<f32>(seed * 1.3)) - 0.5) * 0.25;
    if (d > 1.0) {
        discard;
    }
    let speck = noise2(q * 14.0 + vec2<f32>(seed * 0.7));
    var rgb: vec3<f32>;
    var alpha: f32;
    if (crater) {
        let pit = 1.0 - smoothstep(0.15, 0.45, d);
        let earth = mix(vec3<f32>(0.16, 0.11, 0.07), vec3<f32>(0.22, 0.16, 0.10), speck);
        rgb = mix(earth, vec3<f32>(0.05, 0.035, 0.025), pit);
        alpha = smoothstep(1.0, 0.7, d) * 0.92;
    } else {
        rgb = mix(vec3<f32>(0.20, 0.15, 0.10), vec3<f32>(0.30, 0.25, 0.18), speck);
        alpha = smoothstep(1.0, 0.4, d) * 0.7;
    }
    // Dust settles: the fresh earth greys towards the ground's colour.
    rgb = mix(rgb, vec3<f32>(0.24, 0.21, 0.16), 0.5 * in.look.x);
    alpha = alpha * mix(1.0, 0.6, in.look.x) * in.look.y;
    if (globals.params.x > 0.5) {
        rgb = to_srgb(rgb);
    }
    return vec4<f32>(rgb, alpha);
}

// A body dragged through blood: a ribbon from the start to `reach` along, widening from
// `start` to the full radius, with ragged edges, streaks along the drag and gaps where the
// blood runs thin (`look.w` low).
fn smear(in: VsOut) -> vec4<f32> {
    let r = max(in.radius, 1.0);
    let seed = in.shape.w;
    let len = 2.0 * in.shape.z * r;
    let a = in.local.x;
    let t = clamp(a / max(len, 1.0), 0.0, 1.0);
    // Along and across the drag in world space: the same pattern in every segment.
    var dir = in.shape.xy;
    if (dot(dir, dir) < 1e-6) {
        dir = vec2<f32>(1.0, 0.0);
    }
    let s = dot(in.world, dir);
    let c = dot(in.world, vec2<f32>(-dir.y, dir.x));
    let wp = in.world / 30.0;
    // The body's width wavers and the ribbon wanders a little from side to side.
    let w = r * mix(in.start, 1.0, t) * (0.8 + 0.45 * noise2(wp * 1.3 + vec2<f32>(seed * 0.5)));
    let y = in.local.y + (noise2(vec2<f32>(s / 40.0, seed)) - 0.5) * 0.6 * r;
    // A segment of no length (he lay still) is a round patch; the others end flat, so the
    // next one carries on without a doubled seam.
    var d: f32;
    if (len < 0.5) {
        d = length(vec2<f32>(a, y)) / w;
    } else {
        if (a < 0.0 || a >= len) {
            discard;
        }
        d = abs(y) / w;
    }
    // Ragged edges: blotchy at the metre scale, torn finer in.
    d = d + (noise2(wp * 3.0 + vec2<f32>(seed)) - 0.5) * 0.55 + (noise2(wp * 11.0) - 0.5) * 0.25;
    // Streaks pulled out along the drag: fine across, long along.
    let streak = noise2(vec2<f32>(s / 30.0, c / 4.0) + vec2<f32>(seed * 0.37));
    d = d + (0.5 - streak) * 0.5 * smoothstep(0.4, 1.0, d);
    if (d > 1.0) {
        discard;
    }
    let density = in.look.w;
    // Thin blood breaks up into dabs; heavy blood covers.
    let cover = noise2(vec2<f32>(s / 22.0, c / 9.0) + vec2<f32>(seed * 1.9));
    let body = smoothstep(0.45 - 0.5 * density, 0.85 - 0.4 * density, cover + 0.25 * streak);
    let age = in.look.x;
    let fresh = mix(vec3<f32>(0.24, 0.012, 0.01), vec3<f32>(0.11, 0.0, 0.0), streak * 0.7 + 0.3 * density);
    let dried = mix(vec3<f32>(0.10, 0.045, 0.028), vec3<f32>(0.055, 0.025, 0.018), streak);
    var rgb = mix(fresh, dried, smoothstep(0.0, 1.0, age));
    // Wet gloss on the fresh streaks.
    rgb = rgb + vec3<f32>(0.10, 0.03, 0.03) * (1.0 - age) * smoothstep(0.7, 0.95, streak) * density;
    let edge = smoothstep(1.0, 0.7, d);
    let alpha = edge * mix(0.45, 1.0, body) * mix(0.4, 0.95, streak) * mix(0.9, 0.65, age) * in.look.y;
    if (globals.params.x > 0.5) {
        rgb = to_srgb(rgb);
    }
    return vec4<f32>(rgb, alpha);
}

@fragment
fn fs_pool(in: VsOut) -> @location(0) vec4<f32> {
    if (in.look.z > 3.5) {
        return smear(in);
    }
    if (in.look.z > 1.5) {
        return dirt(in, in.look.z < 2.5);
    }
    let r = max(in.radius, 1.0);
    let seed = in.shape.w;
    // Distance in pool radii: the round pool, and the trail narrowing away from it.
    var d = length(in.local) / r;
    let reach = 2.0 * in.shape.z * r;
    let droplet = in.look.z > 0.5 && in.look.z < 1.5;
    // A droplet's tail is wider and tapers to a point; a pool's trail is a thin run.
    let tail = select(0.45, 0.7, droplet);
    if (reach > 0.0 && in.local.x > 0.0 && in.local.x < reach) {
        let t = 1.0 - in.local.x / reach;
        let width = r * tail * select(t, sqrt(t), droplet) + 0.5;
        d = min(d, abs(in.local.y) / width);
    }
    let q = in.local / r;
    // Droplets have crisp edges: less and finer noise.
    let rough = select(1.0, 0.35, droplet);
    d = d + ((noise2(q * 3.0 + vec2<f32>(seed)) - 0.5) * 0.45 + (noise2(q * 9.0 + vec2<f32>(seed * 1.7)) - 0.5) * 0.15) * rough;
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
