// ---- The detailed ground (ground.wgsl, appended to terrain.wgsl; docs/ground.md) ----
// The terrain mesh once more, over the original ground already in the output: per triangle
// the material classes of its three vertices (ground.rs), blended by barycentric weight and
// the materials' height, each tinted by its vertex's original texture colour and shaded by
// the original light level, with the material's normal map under the sun. Alpha keeps the
// original where a vertex has class KEEP (paving, logs) and over water.

struct Detail {
    // Direction to the sun (world), w: 1 when the output is sRGB (linear maths), else 0.
    sun: vec4<f32>,
    // Brightness of light levels 0..31 relative to the texel (AGEW_1DR.GRD).
    shade: array<vec4<f32>, 8>,
    // Per class: the texture layer of its second set (ground.rs SECOND).
    second: array<vec4<f32>, 4>,
    // Per texture layer (class i is layer i): mean albedo (linear) and world pixels per repeat.
    layers: array<vec4<f32>, 32>,
    // Per original texture: mean colour (linear) and class (255: keep).
    tiles: array<vec4<f32>, 256>,
};

@group(2) @binding(0) var<uniform> detail: Detail;
@group(2) @binding(1) var albedo: texture_2d_array<f32>;
@group(2) @binding(2) var normals: texture_2d_array<f32>;
@group(2) @binding(3) var material_sampler: sampler;
// The ground state (ground_state.rs): R compression, G mud, B scorch, A displacement.
@group(3) @binding(0) var state_atlas: texture_2d<f32>;
@group(3) @binding(1) var state_pages: texture_2d<u32>;
@group(3) @binding(2) var state_sampler: sampler;

const STATE_TEXEL_PX: f32 = 5.0;
const STATE_TILE: f32 = 256.0;
const STATE_SLOTS: u32 = 8u;
const STATE_PAGES: i32 = 64;
// Metres of displacement at the channel's ends, and world pixels per metre.
const STATE_DISP_M: f32 = 0.25;
const PX_PER_M: f32 = 30.0;

// The ground state at world point `p` (level, untouched ground where no tile is).
fn ground_state(p: vec2<f32>) -> vec4<f32> {
    let tile = vec2<i32>(floor(p / (STATE_TEXEL_PX * STATE_TILE)));
    if (any(tile < vec2<i32>(0)) || any(tile >= vec2<i32>(STATE_PAGES))) {
        return vec4<f32>(0.0, 0.0, 0.0, 128.0 / 255.0);
    }
    let slot = textureLoad(state_pages, tile, 0).r;
    if (slot == 0u) {
        return vec4<f32>(0.0, 0.0, 0.0, 128.0 / 255.0);
    }
    let s = slot - 1u;
    let local = clamp(p / STATE_TEXEL_PX - vec2<f32>(tile) * STATE_TILE, vec2<f32>(0.5), vec2<f32>(STATE_TILE - 0.5));
    let uv = (vec2<f32>(f32(s % STATE_SLOTS), f32(s / STATE_SLOTS)) * STATE_TILE + local) / (f32(STATE_SLOTS) * STATE_TILE);
    return textureSampleLevel(state_atlas, state_sampler, uv, 0.0);
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// Calibration against the original on battle3 (docs/ground.md).
const TINT_SATURATION: f32 = 0.82;
const CHROMA_DETAIL: f32 = 0.35;

struct DetailOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) bary: vec3<f32>,
    @location(2) light: f32,
    @location(3) @interpolate(flat) classes: vec4<u32>,
    @location(4) @interpolate(flat) tint0: vec3<f32>,
    @location(5) @interpolate(flat) tint1: vec3<f32>,
    @location(6) @interpolate(flat) tint2: vec3<f32>,
};

@vertex
fn vs_detail(@builtin(vertex_index) vi: u32) -> DetailOut {
    var out: DetailOut;
    let tri = vi / 3u;
    let corner = vi % 3u;
    let cell = i32(tri / 4u);
    let kind = tri % 4u;
    let cp = globals.cells.x + cell % globals.cells.z;
    let row = globals.cells.y + cell / globals.cells.z;
    let w = globals.cells.w;
    if (row >= globals.map.x - 1) {
        out.pos = vec4<f32>(2.0, 2.0, 0.0, 1.0);
        return out;
    }
    let v = row * w + cp * 2;
    var corners: array<i32, 3>;
    switch kind {
        case 0u: { corners = array<i32, 3>(v, v + w, v + w + 1); }
        case 1u: { corners = array<i32, 3>(v, v + 1, v + w + 1); }
        case 2u: { corners = array<i32, 3>(v + 2, v + 1, v + w + 1); }
        default: { corners = array<i32, 3>(v + 2, v + w + 2, v + w + 1); }
    }
    let here = corners[corner];
    let world = vertex_pos(here);
    let data = textureLoad(vertices, texel_at(here), 0);
    let height = textureLoad(heights, texel_at(here), 0).r;
    // Placed as vs_ground places the original ground, but straight into the output.
    let sx = f32(world.x) - globals.camera.x;
    let sy = f32((world.y >> 1u) - height - i32(data.b)) - globals.camera.y;
    let px = vec2<f32>(sx, sy) * globals.camera.z;
    out.pos = vec4<f32>(px.x / globals.sizes.z * 2.0 - 1.0, 1.0 - px.y / globals.sizes.w * 2.0, 0.0, 1.0);
    let z = f32(height + i32(data.b)) / COS_E;
    if (globals.mode.x > 0.5) {
        let clip = globals.view * vec4<f32>(f32(world.x), f32(world.y), z, 1.0);
        out.pos = vec4<f32>(clip.xyz, 1.0);
    }
    out.world = vec3<f32>(f32(world.x), f32(world.y), z);
    out.bary = vec3<f32>(select(0.0, 1.0, corner == 0u), select(0.0, 1.0, corner == 1u), select(0.0, 1.0, corner == 2u));
    out.light = f32(data.g);
    let t0 = detail.tiles[textureLoad(vertices, texel_at(corners[0]), 0).r];
    let t1 = detail.tiles[textureLoad(vertices, texel_at(corners[1]), 0).r];
    let t2 = detail.tiles[textureLoad(vertices, texel_at(corners[2]), 0).r];
    out.classes = vec4<u32>(u32(t0.w), u32(t1.w), u32(t2.w), 0u);
    out.tint0 = t0.rgb;
    out.tint1 = t1.rgb;
    out.tint2 = t2.rgb;
    return out;
}

fn detail_hash(p: vec2<i32>) -> f32 {
    return f32(hash(p) & 0xFFFFu) / 65535.0;
}

// Smooth value noise, 0..1.
fn value_noise(p: vec2<f32>) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = detail_hash(i);
    let b = detail_hash(i + vec2<i32>(1, 0));
    let c = detail_hash(i + vec2<i32>(0, 1));
    let d = detail_hash(i + vec2<i32>(1, 1));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

struct Sample {
    // rgb: the albedo over the layer's mean albedo (the detail ratio), a: height.
    albedo: vec4<f32>,
    normal: vec4<f32>,
    // The luminance of the albedo over that of the layer's mean.
    lratio: f32,
};

// One layer sampled at `uv` with gradients `gx`, `gy`, as a detail ratio over its mean.
fn sample_layer(layer: u32, uv: vec2<f32>, gx: vec2<f32>, gy: vec2<f32>) -> Sample {
    let a = textureSampleGrad(albedo, material_sampler, uv, i32(layer), gx, gy);
    let mean = max(detail.layers[layer].rgb, vec3<f32>(0.02));
    var s: Sample;
    s.albedo = vec4<f32>(clamp(a.rgb / mean, vec3<f32>(0.25), vec3<f32>(2.2)), a.a);
    s.lratio = clamp(luminance(a.rgb) / max(luminance(mean), 0.01), 0.25, 2.2);
    s.normal = textureSampleGrad(normals, material_sampler, uv, i32(layer), gx, gy);
    return s;
}

// One class at a world point: its set at its repeat and its second set (or the same set) at
// a turned, larger repeat, mixed by low-frequency noise against tiling. The gradients of `xy`
// come from uniform control flow (the classes differ per triangle).
fn sample_class(kind: u32, xy: vec2<f32>, gx: vec2<f32>, gy: vec2<f32>, mixer: f32) -> Sample {
    let k = 1.0 / detail.layers[kind].w;
    let other = u32(detail.second[kind / 4u][kind % 4u]);
    // The second sample is turned and about 1.9 x larger than its layer's repeat.
    let k2 = 1.0 / detail.layers[other].w;
    let s1 = sample_layer(kind, xy * k, gx * k, gy * k);
    let s2 = sample_layer(other, rotate_detail(xy * k2) + vec2<f32>(0.37, 0.71), rotate_detail(gx * k2), rotate_detail(gy * k2));
    var s: Sample;
    s.albedo = mix(s1.albedo, s2.albedo, mixer);
    s.normal = mix(s1.normal, s2.normal, mixer);
    s.lratio = mix(s1.lratio, s2.lratio, mixer);
    return s;
}

// The second, larger and turned repeat.
fn rotate_detail(v: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(v.x * 0.8 - v.y * 0.6, v.x * 0.6 + v.y * 0.8) * 0.53;
}

fn shade_at(level: f32) -> f32 {
    let l = clamp(level, 0.0, 31.0);
    let i = u32(floor(l));
    let j = min(i + 1u, 31u);
    let a = detail.shade[i / 4u][i % 4u];
    let b = detail.shade[j / 4u][j % 4u];
    return mix(a, b, l - floor(l));
}

@fragment
fn fs_detail(in: DetailOut) -> @location(0) vec4<f32> {
    let xy = in.world.xy;
    let gx = dpdx(xy);
    let gy = dpdy(xy);
    var n = normalize(cross(dpdx(in.world), dpdy(in.world)));
    let mixer = smoothstep(0.3, 0.7, value_noise(xy / 160.0));
    var cls = array<u32, 3>(in.classes.x, in.classes.y, in.classes.z);
    var tints = array<vec3<f32>, 3>(in.tint0, in.tint1, in.tint2);
    var bary = array<f32, 3>(in.bary.x, in.bary.y, in.bary.z);
    var colour = vec3<f32>(0.0);
    var normal = vec3<f32>(0.0);
    var weights = array<f32, 3>(0.0, 0.0, 0.0);
    var samples: array<Sample, 3>;
    var top = -1.0;
    for (var i = 0u; i < 3u; i++) {
        if (cls[i] == 255u) {
            // The original texture: its weight is the plain barycentric one.
            weights[i] = bary[i] + 0.35;
            top = max(top, weights[i]);
            continue;
        }
        // A vertex sharing the class of an earlier one reuses its sample.
        var same = 3u;
        for (var j = 0u; j < i; j++) {
            if (cls[j] == cls[i] && same == 3u) {
                same = j;
            }
        }
        if (same < 3u) {
            samples[i] = samples[same];
        } else {
            samples[i] = sample_class(cls[i], xy, gx, gy, mixer);
        }
        // Height blend: the higher material wins near the border.
        weights[i] = bary[i] + samples[i].albedo.a * 0.5;
        top = max(top, weights[i]);
    }
    var total = 0.0;
    var kept = 0.0;
    for (var i = 0u; i < 3u; i++) {
        let wgt = max(weights[i] - top + 0.2, 0.0) * smoothstep(0.0, 0.2, bary[i]);
        total += wgt;
        if (cls[i] == 255u) {
            kept += wgt;
            continue;
        }
        // Detail mostly as luminance: a full per-channel ratio crushes the blue of grass
        // and over-saturates (docs/ground.md, calibration).
        let ratio = samples[i].albedo.rgb;
        let lratio = samples[i].lratio;
        let tint = mix(vec3<f32>(luminance(tints[i])), tints[i], TINT_SATURATION);
        colour += wgt * tint * mix(vec3<f32>(lratio), ratio, CHROMA_DETAIL);
        normal += wgt * vec3<f32>(samples[i].normal.xy * 2.0 - 1.0, 0.0);
    }
    total = max(total, 1e-4);
    let land = total - kept;
    if (land <= 1e-4) {
        discard;
    }
    colour /= land;
    var nt = normal.xy / land;

    // The ground state: trampled, churned, scorched, dug.
    let st = ground_state(xy);
    let disp = (st.a - 128.0 / 255.0) * (255.0 / 127.0) * STATE_DISP_M;
    if (any(st != vec4<f32>(0.0, 0.0, 0.0, 128.0 / 255.0))) {
        // The displacement's slope (metres per metre) from its neighbours, into the normal.
        let e = STATE_TEXEL_PX;
        let k = (255.0 / 127.0) * STATE_DISP_M * PX_PER_M / (2.0 * e);
        let hx = (ground_state(xy + vec2<f32>(e, 0.0)).a - ground_state(xy - vec2<f32>(e, 0.0)).a) * k;
        let hy = (ground_state(xy + vec2<f32>(0.0, e)).a - ground_state(xy - vec2<f32>(0.0, e)).a) * k;
        nt = nt + vec2<f32>(-hx, hy);
        let len = length(nt);
        if (len > 0.9) {
            nt = nt * (0.9 / len);
        }
        // Compressed: flattened grass, darker, duller, towards bare soil.
        let l = luminance(colour);
        colour = mix(colour, vec3<f32>(l) * vec3<f32>(1.15, 0.95, 0.7), 0.6 * st.r) * (1.0 - 0.3 * st.r);
        // Churned mud: dark and brown.
        colour = mix(colour, vec3<f32>(l) * vec3<f32>(1.1, 0.85, 0.6) * 0.45, 0.7 * st.g);
        // Dug ground (holes and their thrown rims) shows bare soil: brown, no grass.
        let dug = clamp(abs(disp) * 5.0, 0.0, 0.85);
        colour = mix(colour, vec3<f32>(l) * vec3<f32>(1.2, 0.9, 0.6) * 0.7, dug);
        // Holes are darker inside (soil in shade, moister).
        colour = colour * (1.0 - clamp(-disp * 2.5, 0.0, 0.45));
        // Scorch: black-brown.
        colour = mix(colour, vec3<f32>(0.025, 0.02, 0.015), 0.85 * st.b);
    }

    // The geometric normal from the surface; the material's normal map on top of it.
    if (n.z < 0.0) {
        n = -n;
    }
    let t = normalize(vec3<f32>(1.0, 0.0, 0.0) - n * n.x);
    let north = -cross(n, t);
    let nz = sqrt(max(1.0 - dot(nt, nt), 0.0));
    let np = normalize(t * nt.x + north * nt.y + n * nz);
    let sun = detail.sun.xyz;
    // The relief light is in the original level already; the normal map adds the small one.
    let micro = (0.5 + 0.5 * max(dot(np, sun), 0.0)) / (0.5 + 0.5 * max(dot(n, sun), 0.0));
    var rgb = colour * shade_at(in.light) * micro;
    // Wet mud glints under the sun (a view from the south above, as the original's).
    let half_v = normalize(sun + normalize(vec3<f32>(0.0, 0.6, 0.8)));
    rgb += vec3<f32>(0.35) * st.g * st.g * pow(max(dot(np, half_v), 0.0), 48.0) * shade_at(in.light);

    // Water with the original tint (original view, no water surface of docs/water.md):
    // the original's tinted lakebed shows through. The water surface covers this lakebed.
    var water = 0.0;
    if (globals.mode.x < 0.5 && globals.mode.z < 0.5) {
        let screen = in.pos.xy / globals.camera.z + globals.camera.xy;
        let gx = i32(floor(screen.x)) >> 5u;
        let gy = (i32(floor(screen.y)) >> 5u) * 2;
        let f = fract(screen / 32.0);
        let raw = mix(mix(f32(depth(gx, gy)), f32(depth(gx + 1, gy)), f.x), mix(f32(depth(gx, gy + 2)), f32(depth(gx + 1, gy + 2)), f.x), f.y);
        water = clamp((raw + 0.5) * 4.0, 0.0, 1.0);
    }
    if (detail.sun.w < 0.5) {
        rgb = pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
    }
    return vec4<f32>(rgb, (land / total) * (1.0 - water));
}
