// 3D models (glTF) skinned on the GPU, instanced per model type.
//
// Every vertex is skinned to up to four matrices of its model's palette (static meshes to
// their node's matrix, see model.rs). The palettes of all poses drawn this frame lie in the
// float texture `bones` (3 texels of affine rows per matrix, `globals.params.z` texels per
// row), so the skinning fits WebGL2 (no storage buffers, no large uniform arrays).
// Instances carry their model → world transform (3 rows), the decay and morph weights and
// the first texel of their pose.
//
// Decay (docs/assets-humans.md): `COLOR_0` of a decaying material is the mask (R noise, G
// fray); each material has its layer's windows; per corpse one `decay` value.

struct Globals {
    view_proj: mat4x4<f32>,
    // Direction to the light (world), ambient share in w.
    light: vec4<f32>,
    // View direction (world, into the screen).
    forward: vec4<f32>,
    // x: encode sRGB here (the target is not sRGB), y: dissolve allowed, z: bone texture width.
    params: vec4<f32>,
};

struct Material {
    base: vec4<f32>,
    // Discolour tint (linear RGB) and its strength.
    tint: vec4<f32>,
    // Discolour window d0, d1; dissolve window s0, s1 (s0 > 1: never).
    windows: vec4<f32>,
    // Colour of the rotten edge, edge width.
    edge: vec4<f32>,
    // x: decaying layer (COLOR_0 is the mask), y: COLOR_0 is a vertex colour, z: alpha cutoff
    // (0: none), w: unused.
    flags: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var bones: texture_2d<f32>;
@group(1) @binding(0) var<uniform> material: Material;
@group(1) @binding(1) var base_texture: texture_2d<f32>;
@group(1) @binding(2) var base_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) joints: vec4<u32>,
    @location(5) weights: vec4<f32>,
    @location(6) morph0: vec3<f32>,
    @location(7) morph1: vec3<f32>,
};

struct InstanceIn {
    @location(8) row0: vec4<f32>,
    @location(9) row1: vec4<f32>,
    @location(10) row2: vec4<f32>,
    // decay, deflate weight, lie_flat weight, half transparent (1)
    @location(11) params: vec4<f32>,
    // first texel of the pose, player, unused, unused
    @location(12) extra: vec4<u32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) @interpolate(flat) params: vec4<f32>,
};

fn bone_texel(base: u32, joint: u32, row: u32) -> vec4<f32> {
    let i = base + joint * 3u + row;
    let w = u32(globals.params.z);
    return textureLoad(bones, vec2<i32>(i32(i % w), i32(i / w)), 0);
}

fn apply(r0: vec4<f32>, r1: vec4<f32>, r2: vec4<f32>, p: vec4<f32>) -> vec3<f32> {
    return vec3<f32>(dot(r0, p), dot(r1, p), dot(r2, p));
}

@vertex
fn vs_main(v: VertexIn, inst: InstanceIn) -> VsOut {
    let base = inst.extra.x;
    var r0 = vec4<f32>(0.0);
    var r1 = vec4<f32>(0.0);
    var r2 = vec4<f32>(0.0);
    for (var k = 0u; k < 4u; k = k + 1u) {
        let w = v.weights[k];
        if (w > 0.0) {
            let j = v.joints[k];
            r0 = r0 + w * bone_texel(base, j, 0u);
            r1 = r1 + w * bone_texel(base, j, 1u);
            r2 = r2 + w * bone_texel(base, j, 2u);
        }
    }
    let p = v.position + v.morph0 * inst.params.y + v.morph1 * inst.params.z;
    let local = apply(r0, r1, r2, vec4<f32>(p, 1.0));
    let local_n = apply(r0, r1, r2, vec4<f32>(v.normal, 0.0));
    let world = apply(inst.row0, inst.row1, inst.row2, vec4<f32>(local, 1.0));
    let world_n = apply(inst.row0, inst.row1, inst.row2, vec4<f32>(local_n, 0.0));
    var out: VsOut;
    out.pos = globals.view_proj * vec4<f32>(world, 1.0);
    out.normal = world_n;
    out.uv = v.uv;
    out.color = v.color;
    out.params = inst.params;
    return out;
}

fn ramp(x: f32, a: f32, b: f32) -> f32 {
    return clamp((x - a) / max(b - a, 1e-5), 0.0, 1.0);
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Sampled first: derivatives need uniform control flow.
    let texel = textureSample(base_texture, base_sampler, in.uv);
    var c = material.base * texel;
    if (material.flags.y > 0.5) {
        c = c * in.color;
    }
    if (material.flags.z > 0.0 && c.a < material.flags.z) {
        discard;
    }
    // Half transparent like the original's fading corpses: every other pixel (no sorting).
    if (in.params.w > 0.5) {
        let p = vec2<i32>(floor(in.pos.xy));
        if (((p.x + p.y) & 1) == 1) {
            discard;
        }
    }
    var rgb = c.rgb;
    let decay = in.params.x;
    if (material.flags.x > 0.5 && decay > 0.0) {
        rgb = mix(rgb, material.tint.rgb, material.tint.w * ramp(decay, material.windows.x, material.windows.y));
        if (globals.params.y > 0.5 && material.windows.z <= 1.0) {
            let mask = 0.75 * in.color.r + 0.25 * in.color.g;
            let p = ramp(decay, material.windows.z, material.windows.w);
            let th = p * (1.0 + material.edge.w);
            if (p > 0.0 && mask <= th) {
                discard;
            }
            if (p > 0.0 && mask - th < material.edge.w) {
                rgb = material.edge.rgb;
            }
        }
    }
    // Lambert from the upper left (the sprites' light), both faces lit.
    var n = normalize(in.normal);
    if (dot(n, globals.forward.xyz) > 0.0) {
        n = -n;
    }
    let lit = globals.light.w + (1.0 - globals.light.w) * max(dot(n, globals.light.xyz), 0.0);
    rgb = rgb * lit;
    if (globals.params.x > 0.5) {
        rgb = to_srgb(rgb);
    }
    return vec4<f32>(rgb, 1.0);
}

// The depth reset after the models (see DepthTarget::reset_ground): a full-screen triangle at
// the far plane where no model drew (stencil 0), so sprites draw over the terrain as in the
// original but test against the models.
@vertex
fn vs_far(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 1.0, 1.0);
}

@fragment
fn fs_none() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0);
}
