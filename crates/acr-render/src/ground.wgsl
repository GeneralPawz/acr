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
    // Per class: mean albedo (linear) and world pixels per texture repeat.
    classes: array<vec4<f32>, 16>,
    // Per original texture: mean colour (linear) and class (255: keep).
    tiles: array<vec4<f32>, 256>,
};

@group(2) @binding(0) var<uniform> detail: Detail;
@group(2) @binding(1) var albedo: texture_2d_array<f32>;
@group(2) @binding(2) var normals: texture_2d_array<f32>;
@group(2) @binding(3) var material_sampler: sampler;

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
    albedo: vec4<f32>,
    normal: vec4<f32>,
};

// One class at a world point: two scales mixed by low-frequency noise against tiling. The
// gradients of `xy` come from uniform control flow (the classes differ per triangle).
fn sample_class(kind: u32, xy: vec2<f32>, gx: vec2<f32>, gy: vec2<f32>, mixer: f32) -> Sample {
    let k = 1.0 / detail.classes[kind].w;
    let uv = xy * k;
    let uv2 = rotate_detail(uv) + vec2<f32>(0.37, 0.71);
    let layer = i32(kind);
    var s: Sample;
    let a1 = textureSampleGrad(albedo, material_sampler, uv, layer, gx * k, gy * k);
    let a2 = textureSampleGrad(albedo, material_sampler, uv2, layer, rotate_detail(gx * k), rotate_detail(gy * k));
    let n1 = textureSampleGrad(normals, material_sampler, uv, layer, gx * k, gy * k);
    let n2 = textureSampleGrad(normals, material_sampler, uv2, layer, rotate_detail(gx * k), rotate_detail(gy * k));
    s.albedo = mix(a1, a2, mixer);
    s.normal = mix(n1, n2, mixer);
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
        let mean = max(detail.classes[cls[i]].rgb, vec3<f32>(0.02));
        let ratio = clamp(samples[i].albedo.rgb / mean, vec3<f32>(0.25), vec3<f32>(2.2));
        colour += wgt * tints[i] * ratio;
        normal += wgt * vec3<f32>(samples[i].normal.xy * 2.0 - 1.0, 0.0);
    }
    total = max(total, 1e-4);
    let land = total - kept;
    if (land <= 1e-4) {
        discard;
    }
    colour /= land;
    let nt = normal.xy / land;

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
