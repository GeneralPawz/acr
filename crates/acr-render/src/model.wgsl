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
//
// Wounds (docs/rendering-3d.md): per corpse the direction of the blow, blood, dirt, burns
// and the number and size of the wounds. They are placed in the rest pose (the bind
// position, metres, +y up, facing +z), so they stay on the body in any pose.
//
// Instance data (wounds, spray, damage) lies in the bone texture after the palettes, from
// texel `extra.y` (the vertex slots are full on WebGL2): 4 texels, plus the damage block of
// `damage.rs` (`pack`) when damaged. Damage (docs/damage-model.md): vertices past a cut are
// discarded per fragment along `_LIMB` with a ragged edge from `_GORE.x`, vertices past a
// fracture turn about it in the rest pose before skinning, a piece keeps only its span.

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
    // (0: none), w: bone layer (no wounds).
    flags: vec4<f32>,
    // x: stump cap (wet, ragged outline from the uv).
    look: vec4<f32>,
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
    // _PART (−1 none), _LIMB level 0..1 (−1 none), _GORE.x, 0
    @location(8) body: vec4<f32>,
};

struct InstanceIn {
    @location(9) row0: vec4<f32>,
    @location(10) row1: vec4<f32>,
    @location(11) row2: vec4<f32>,
    // decay, deflate weight, lie_flat weight, half transparent (1)
    @location(12) params: vec4<f32>,
    // first texel of the pose, first texel of the instance data, player, 0
    @location(13) extra: vec4<u32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) @interpolate(flat) params: vec4<f32>,
    // rest-pose position (m), height above the model's ground point in the pose (m)
    @location(4) rest: vec4<f32>,
    @location(5) @interpolate(flat) wounds0: vec4<f32>,
    @location(6) @interpolate(flat) wounds1: vec4<f32>,
    @location(7) @interpolate(flat) spray: vec4<f32>,
    // sprayed blood amount, its age
    @location(8) @interpolate(flat) spray_look: vec2<f32>,
    // kept distance past the cut (level units, < 0 discards), _GORE.x, sign of the ragged
    // offset, belly open (1)
    @location(9) cut: vec4<f32>,
};

fn texel(i: u32) -> vec4<f32> {
    let w = u32(globals.params.z);
    return textureLoad(bones, vec2<i32>(i32(i % w), i32(i / w)), 0);
}

fn bone_texel(base: u32, joint: u32, row: u32) -> vec4<f32> {
    return texel(base + joint * 3u + row);
}

// Rodrigues: `v` turned by `angle` about the unit `axis`.
fn rotate(v: vec3<f32>, axis: vec3<f32>, angle: f32) -> vec3<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return v * c + cross(axis, v) * s + axis * dot(axis, v) * (1.0 - c);
}

// Limb of a body part (rig.PARTS): 0 arm_l, 1 arm_r, 2 leg_l, 3 leg_r, −1 torso/head/none.
fn limb_of(part: i32) -> i32 {
    if (part >= 2 && part <= 9) {
        return (part - 2) / 2;
    }
    return -1;
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
    var p = v.position + v.morph0 * inst.params.y + v.morph1 * inst.params.z;
    var n = v.normal;
    let d = inst.extra.y;
    let look = texel(d + 3u);
    let part = i32(round(v.body.x));
    let limb = limb_of(part);
    let t = v.body.y;
    var keep = 1.0;
    var side = 1.0;
    var belly = 0.0;
    if (look.z > 0.5) {
        let cuts = texel(d + 4u);
        let fracs = texel(d + 5u);
        let tr = texel(d + 14u);
        let pc = texel(d + 15u);
        let mode = i32(round(tr.w));
        belly = tr.z;
        if (limb >= 0 && t >= 0.0) {
            let fl = fracs[limb];
            if (fl < 1.5) {
                let pivot = texel(d + 6u + u32(limb));
                let axis = texel(d + 10u + u32(limb)).xyz;
                let a = pivot.w * smoothstep(fl - 0.012, fl + 0.012, t);
                p = pivot.xyz + rotate(p - pivot.xyz, axis, a);
                n = rotate(n, axis, a);
            }
        }
        // Trunk height in level-like units (a trunk is ~0.5 m).
        let above = (v.position.y - tr.x) * 2.0;
        let arm = limb == 0 || limb == 1;
        if (mode < 0) {
            if (limb >= 0 && t >= 0.0) {
                keep = cuts[limb] - t;
            }
            if (tr.x < 1e5) {
                if (part == 0) {
                    keep = -above;
                } else if (part == 1 || arm) {
                    keep = -1.0;
                }
            }
            if (part == 1 && tr.y > 0.5) {
                keep = -1.0;
            }
        } else if (mode < 4) {
            keep = -1.0;
            if (limb == mode && t >= 0.0) {
                let lo = t - pc.x;
                let hi = pc.y - t;
                keep = min(lo, hi);
                side = select(1.0, -1.0, lo < hi);
            }
        } else {
            keep = -1.0;
            if (part == 0) {
                keep = above;
                side = -1.0;
            } else if (part == 1 && tr.y < 0.5) {
                keep = 1.0;
            } else if (arm && t >= 0.0) {
                keep = cuts[limb] - t;
            } else if (arm) {
                keep = 1.0;
            }
        }
        // Belly wound on the front of the torso: x within 0.12 m, y around the belly.
        let b = (v.position - vec3<f32>(0.0, pc.z, 0.12)) / vec3<f32>(0.13, 0.1, 0.12);
        belly = select(0.0, belly * clamp(1.0 - length(b), 0.0, 1.0), part == 0);
    }
    let local = apply(r0, r1, r2, vec4<f32>(p, 1.0));
    let local_n = apply(r0, r1, r2, vec4<f32>(n, 0.0));
    let world = apply(inst.row0, inst.row1, inst.row2, vec4<f32>(local, 1.0));
    let world_n = apply(inst.row0, inst.row1, inst.row2, vec4<f32>(local_n, 0.0));
    var out: VsOut;
    out.pos = globals.view_proj * vec4<f32>(world, 1.0);
    out.normal = world_n;
    out.uv = v.uv;
    out.color = v.color;
    out.params = inst.params;
    out.rest = vec4<f32>(v.position, local.y);
    out.wounds0 = texel(d);
    out.wounds1 = texel(d + 1u);
    out.spray = texel(d + 2u);
    out.spray_look = look.xy;
    out.cut = vec4<f32>(clamp(keep, -1.0, 1.0), v.body.z, side, belly);
    return out;
}

fn ramp(x: f32, a: f32, b: f32) -> f32 {
    return clamp((x - a) / max(b - a, 1e-5), 0.0, 1.0);
}

// Ragged outline of a cut in level units: the asset's wound noise plus fine noise.
fn ragged(gore: f32, rest: vec3<f32>) -> f32 {
    return (gore - 0.5) * 0.06 + (noise3(rest * 45.0) - 0.5) * 0.035;
}

fn hash1(n: f32) -> f32 {
    return fract(sin(n) * 43758.5453);
}

fn hash3(p: vec3<f32>) -> f32 {
    return hash1(dot(p, vec3<f32>(127.1, 311.7, 74.7)));
}

// Value noise, 0..1.
fn noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(hash3(i), hash3(i + vec3<f32>(1.0, 0.0, 0.0)), u.x);
    let b = mix(hash3(i + vec3<f32>(0.0, 1.0, 0.0)), hash3(i + vec3<f32>(1.0, 1.0, 0.0)), u.x);
    let c = mix(hash3(i + vec3<f32>(0.0, 0.0, 1.0)), hash3(i + vec3<f32>(1.0, 0.0, 1.0)), u.x);
    let d = mix(hash3(i + vec3<f32>(0.0, 1.0, 1.0)), hash3(i + vec3<f32>(1.0, 1.0, 1.0)), u.x);
    return mix(mix(a, b, u.y), mix(c, d, u.y), u.z);
}

// Centre of wound `i` on the rest pose: the first faces the blow at the chest, with two
// wounds the second is the exit on the far side, further ones spread over the front.
fn wound_centre(i: f32, count: f32, blow: vec2<f32>, seed: f32) -> vec3<f32> {
    var dir = vec3<f32>(0.0, 0.0, 1.0);
    if (dot(blow, blow) > 1e-4) {
        dir = normalize(vec3<f32>(blow.x, 0.0, blow.y));
    }
    var height = 1.25;
    if (i > 0.5) {
        if (count < 2.5 && i < 1.5) {
            dir = -dir;
            height = 1.18;
        } else {
            let a = (hash1(seed + i * 3.1) - 0.5) * 2.2;
            let s = sin(a);
            let c = cos(a);
            dir = vec3<f32>(dir.x * c + dir.z * s, 0.0, dir.z * c - dir.x * s);
            height = 0.75 + 0.7 * hash1(seed + i * 7.3);
        }
    }
    return vec3<f32>(dir.x * 0.15, height, dir.z * 0.15);
}

// Blood, dirt and burns over the colour `rgb` of a body layer.
fn wounded(rgb_in: vec3<f32>, rest: vec4<f32>, w0: vec4<f32>, w1: vec4<f32>, decay: f32) -> vec3<f32> {
    var rgb = rgb_in;
    let p = rest.xyz;
    let grain = noise3(p * 22.0);
    // Wounds and the blood running from them (down in the rest pose).
    let count = w1.y;
    let r = max(w1.z, 0.01);
    var stain = 0.0;
    var core = 0.0;
    for (var i = 0.0; i < count; i = i + 1.0) {
        let c = wound_centre(i, count, w0.xy, w1.w);
        let d = length(p - c) + (grain - 0.5) * r;
        stain = max(stain, 1.0 - smoothstep(r * 0.7, r * 1.8, d));
        core = max(core, 1.0 - smoothstep(r * 0.15, r * 0.55, d));
        let below = c.y - p.y;
        if (below > 0.0 && below < 0.7) {
            let side = length((p - c).xz) + (noise3(p * 9.0) - 0.5) * 0.06;
            let run = (1.0 - smoothstep(r * 0.5, r * 1.3, side)) * (1.0 - below / 0.7);
            stain = max(stain, run * 0.85);
        }
    }
    let blood = clamp(stain * w0.z, 0.0, 1.0);
    // Fresh red darkens to dried brown as the corpse decays.
    let dried = smoothstep(0.05, 0.5, decay);
    let red = mix(vec3<f32>(0.36, 0.015, 0.012), vec3<f32>(0.10, 0.03, 0.018), dried);
    rgb = mix(rgb, red, blood * 0.92);
    rgb = mix(rgb, vec3<f32>(0.05, 0.0, 0.0), core * w0.z * 0.85);
    // Dirt: from the ground up, in patches.
    let low = 1.0 - smoothstep(0.05, 0.6, rest.w);
    let patches = smoothstep(0.35, 0.7, noise3(p * 6.0) + 0.25 * low);
    rgb = mix(rgb, vec3<f32>(0.17, 0.13, 0.08), clamp(w0.w * patches * (0.45 + 0.55 * low), 0.0, 0.85));
    // Burns: charred patches.
    let charred = smoothstep(0.3, 0.6, noise3(p * 8.0) * 0.7 + w1.x * 0.6) * w1.x;
    rgb = mix(rgb, vec3<f32>(0.025, 0.02, 0.018), clamp(charred, 0.0, 1.0));
    return rgb;
}

// Blood sprayed onto a body or a wall: speckles of several sizes around `spray.xyz` (rest
// pose, metres) within `spray.w`, denser towards the centre.
fn sprayed(rgb: vec3<f32>, rest: vec3<f32>, spray: vec4<f32>, look: vec2<f32>) -> vec3<f32> {
    let r = max(spray.w, 0.05);
    let d = length(rest - spray.xyz) / r;
    let fall = 1.0 - smoothstep(0.25, 1.0, d);
    let big = smoothstep(0.70, 0.78, noise3(rest * 34.0 + vec3<f32>(3.1)));
    let small = smoothstep(0.74, 0.80, noise3(rest * 85.0 + vec3<f32>(7.7)));
    let mist = smoothstep(0.45, 0.75, noise3(rest * 12.0)) * 0.35 * (1.0 - smoothstep(0.0, 0.6, d));
    let a = clamp(look.x * fall * max(max(big, small * 0.9), mist), 0.0, 0.95);
    let red = mix(vec3<f32>(0.33, 0.012, 0.01), vec3<f32>(0.11, 0.035, 0.02), smoothstep(0.0, 1.0, look.y));
    return mix(rgb, red, a);
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
    if (in.cut.x - in.cut.z * ragged(in.cut.y, in.rest.xyz) < 0.0) {
        discard;
    }
    // A stump cap's ragged outline (uv: the unit disc).
    if (material.look.x > 0.5) {
        let a = atan2(in.uv.y, in.uv.x);
        let edge = 0.9 + 0.1 * noise3(vec3<f32>(cos(a) * 3.0, sin(a) * 3.0, in.params.x * 0.0 + 1.7));
        if (length(in.uv) > edge) {
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
    if (material.flags.w < 0.5 && in.wounds0.z + in.wounds0.w + in.wounds1.x > 0.0) {
        rgb = wounded(rgb, in.rest, in.wounds0, in.wounds1, decay);
    }
    if (material.flags.w < 0.5 && in.spray_look.x > 0.0) {
        rgb = sprayed(rgb, in.rest.xyz, in.spray, in.spray_look);
    }
    // Lambert from the upper left (the sprites' light), both faces lit.
    var n = normalize(in.normal);
    if (dot(n, globals.forward.xyz) > 0.0) {
        n = -n;
    }
    // An open belly: a dark wet gash.
    var wet = material.look.x;
    if (in.cut.w > 0.05) {
        let gash = smoothstep(0.05, 0.4, in.cut.w + (noise3(in.rest.xyz * 30.0) - 0.5) * 0.3);
        rgb = mix(rgb, mix(vec3<f32>(0.30, 0.02, 0.02), vec3<f32>(0.55, 0.25, 0.22), noise3(in.rest.xyz * 60.0)), gash);
        wet = max(wet, gash);
    }
    let lit = globals.light.w + (1.0 - globals.light.w) * max(dot(n, globals.light.xyz), 0.0);
    rgb = rgb * lit;
    // Wet flesh: a tight highlight.
    if (wet > 0.0) {
        let h = normalize(globals.light.xyz - globals.forward.xyz);
        rgb = rgb + vec3<f32>(0.35) * wet * pow(max(dot(n, h), 0.0), 28.0);
    }
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
