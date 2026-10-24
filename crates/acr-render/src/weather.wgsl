// Weather of the 3D pass (weather.rs, docs/rendering-3d.md "Weather (#224)"): rain streaks
// and snow flakes as instanced quads in a box around the view (no vertex buffer: each
// instance hashes its own place and falls by the clock, wrapped into the box, so the drops
// stay fixed in the world as the camera moves); the ground mist where no model drew; and the
// light grade and distance fog over the whole 3D frame. Render only. WebGL2 safe.

struct Globals {
    view_proj: mat4x4<f32>,
    // xyz screen right (world), w output px per world px.
    right: vec4<f32>,
    // xyz screen down (world), w 1 when the shader encodes sRGB.
    down: vec4<f32>,
    // xyz into the screen (world), w the clock (s).
    forward: vec4<f32>,
    // xyz the ground point at the window centre, w the kind (1 rain, 2 snow, 3 fog).
    centre: vec4<f32>,
    // xyz the box's size (world px) around `box_at`, w the fall speed (world px/s).
    box_size: vec4<f32>,
    // xy the box's centre (world), z the streak length (world px), w the drops' opacity.
    box_at: vec4<f32>,
    // xy drift with the wind (world px/s), zw the output size (px).
    wind: vec4<f32>,
    // rgb the fog colour (output space), w the distance fog's strength (0..1).
    fog: vec4<f32>,
    // x the ground mist (0..1), y unused, z distance where the fog starts (world px from
    // the centre, negative: before it), w the fog's distance scale (world px).
    mist: vec4<f32>,
    // rgb lifted onto the light grade (output space); the grade's factor is the blend
    // constant (weather.rs).
    grade: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

fn hash(n: u32) -> u32 {
    var x = n * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return (x >> 22u) ^ x;
}

fn unit(n: u32) -> f32 {
    return f32(hash(n) & 0xffffu) / 65536.0;
}

struct DropOut {
    @builtin(position) clip: vec4<f32>,
    // x across the streak or flake (-1..1), y along it (0 head .. 1 tail).
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
};

@vertex
fn vs_drop(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> DropOut {
    let seed = hash(ii * 3u + 17u);
    let h = vec4<f32>(unit(seed), unit(seed + 1u), unit(seed + 2u), unit(seed + 3u));
    let t = globals.forward.w;
    let size = globals.box_size.xyz;
    let snow = globals.centre.w > 1.5;
    let speed = globals.box_size.w * (0.8 + 0.4 * h.w);
    // Falling and drifting from a place of its own, wrapped into the box (world-fixed tiles).
    var p = vec3<f32>(h.x * size.x, h.y * size.y, h.z * size.z);
    p.z = p.z - t * speed;
    p = vec3<f32>(p.xy + globals.wind.xy * t * (0.85 + 0.3 * h.w), p.z);
    if (snow) {
        // Flakes sway as they fall.
        let phase = t * (0.9 + 0.6 * h.x) + h.y * 6.2832;
        p = p + vec3<f32>(sin(phase) * 9.0, cos(phase * 0.7) * 9.0, 0.0);
    }
    let at = globals.box_at.xy;
    let xy = at + (fract((p.xy - at) / size.xy + 0.5) - 0.5) * size.xy;
    let z = fract(p.z / size.z) * size.z;
    let world = vec3<f32>(xy, z);
    let corner = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    )[vi];
    let px_to_clip = 2.0 / globals.wind.zw;
    let scale = globals.right.w;
    var out: DropOut;
    // Fades in at the top of the box and out near the ground (no pop at the wrap).
    var alpha = globals.box_at.w * smoothstep(0.0, 0.08, z / size.z) * (1.0 - smoothstep(0.85, 1.0, z / size.z));
    if (snow) {
        // A camera-facing flake (1.5 to 3.5 cm at most), at least two pixels across.
        let c = globals.view_proj * vec4<f32>(world, 1.0);
        let r = max(1.0, (0.45 + 0.6 * h.x) * scale);
        let q = vec2<f32>(corner.x, corner.y * 2.0 - 1.0);
        out.clip = vec4<f32>(c.xy + q * r * px_to_clip * vec2<f32>(1.0, -1.0), c.z, 1.0);
        out.uv = q;
        alpha = alpha * (0.6 + 0.4 * h.z);
    } else {
        // A streak along the fall (motion blur), at least a pixel wide.
        let vel = normalize(vec3<f32>(globals.wind.xy, -globals.box_size.w));
        let c0 = globals.view_proj * vec4<f32>(world, 1.0);
        let c1 = globals.view_proj * vec4<f32>(world - vel * globals.box_at.z * (0.7 + 0.6 * h.x), 1.0);
        let d = (c1.xy - c0.xy) / px_to_clip;
        let len = max(length(d), 1.0);
        let side = vec2<f32>(-d.y, d.x) / len;
        let width = max(1.0, 0.45 * scale);
        let c = mix(c0, c1, corner.y);
        out.clip = vec4<f32>(c.xy + side * corner.x * width * 0.5 * px_to_clip, c.z, 1.0);
        out.uv = corner;
        // Thin streaks would flicker as sub-pixel lines: their opacity scales with the width.
        alpha = alpha * (0.5 + 0.5 * h.z);
    }
    out.alpha = alpha;
    return out;
}

@fragment
fn fs_drop(in: DropOut) -> @location(0) vec4<f32> {
    var a = in.alpha;
    var rgb = vec3<f32>(0.0);
    if (globals.centre.w > 1.5) {
        let r = length(in.uv);
        a = a * (1.0 - smoothstep(0.45, 1.0, r));
        rgb = vec3<f32>(0.95, 0.96, 1.0);
    } else {
        // Bright at the head, fading along the tail.
        a = a * (1.0 - in.uv.y * 0.7) * (1.0 - abs(in.uv.x) * 0.4);
        rgb = vec3<f32>(0.78, 0.82, 0.9);
    }
    return vec4<f32>(rgb * a, a);
}

struct FullOut {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> FullOut {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    var out: FullOut;
    out.pos = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    return out;
}

// The ground mist: where no model drew (stencil 0), before the sprites. Models mist their
// own feet by height (model.wgsl), so a man's legs fade into it and his head stays clear.
@fragment
fn fs_mist(in: FullOut) -> @location(0) vec4<f32> {
    let f = globals.mist.x;
    return vec4<f32>(globals.fog.rgb * f, f);
}

// The distance fog over everything: the ground point under the pixel (the view ray meets
// z = 0), its distance ahead of the window centre. The fixed camera's far rows (the top of
// the window) fade more; the free camera's horizon goes into the fog.
@fragment
fn fs_fog(in: FullOut) -> @location(0) vec4<f32> {
    let s = globals.right.w;
    let o = globals.centre.xyz + globals.right.xyz * (in.pos.x - globals.wind.z * 0.5) / s
        + globals.down.xyz * (in.pos.y - globals.wind.w * 0.5) / s;
    let dir = globals.forward.xyz;
    let t = -o.z / min(dir.z, -1e-3);
    let p = o + dir * t;
    let ahead = normalize(vec2<f32>(dir.x, dir.y) + vec2<f32>(0.0, -1e-6));
    let d = dot(p.xy - globals.centre.xy, ahead);
    let f = globals.fog.w * (1.0 - exp(-max(d - globals.mist.z, 0.0) / globals.mist.w));
    return vec4<f32>(globals.fog.rgb * f, f);
}

// The light grade: the frame times the blend constant (darker, cooler for rain, brighter
// and colder for snow) plus this lift.
@fragment
fn fs_grade(in: FullOut) -> @location(0) vec4<f32> {
    return vec4<f32>(globals.grade.rgb, 0.0);
}
