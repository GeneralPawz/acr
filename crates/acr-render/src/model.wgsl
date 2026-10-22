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
// fray); each material has its layer's windows; per corpse one `decay` value. Models
// without masks (the animals, `material.look.w`) rot by a procedural fallback: noise from
// the rest position for the discolour and the dissolve, and the collapse and sink applied
// after the palette (in model space), so physics poses sink too (docs/rendering-3d.md).
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
//
// Looks (#283, docs/rendering-3d.md "Individual looks"): 3 texels after the marks (skin
// tint and stubble, hair tint and age, pocks, scar side, eye height, has look). Hair and beard
// variant vertices (`body.w` = variant + 1) are dropped unless bit `body.w − 1` of
// `extra.w` is set; skin and hair materials (`material.look.z` 1, 2) take the tints; the
// `_FACE` zones (location 14) get stubble, wrinkles, pocks and a scar.

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
    // x: stump cap (wet, ragged outline from the uv), y: tissue kind, z: per-soldier tint
    // (1 skin, 2 hair), w: procedural decay (a model without masks).
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
    // _PART (−1 none), _LIMB level 0..1 (−1 none), _GORE.x, hair/beard variant + 1 (0 none)
    @location(8) body: vec4<f32>,
    // _FACE: beard, wrinkle, pock zones, face front (0 without)
    @location(14) face: vec4<f32>,
};

struct InstanceIn {
    @location(9) row0: vec4<f32>,
    @location(10) row1: vec4<f32>,
    @location(11) row2: vec4<f32>,
    // decay, deflate weight, lie_flat weight, half transparent (1)
    @location(12) params: vec4<f32>,
    // first texel of the pose, first texel of the instance data, player, hair and beard
    // variants shown (bit mask)
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
    // sprayed blood amount, its age, first texel of the instance data, wounds placed at the
    // hits (#277)
    @location(8) @interpolate(flat) spray_look: vec4<f32>,
    // kept distance past the cut (level units, < 0 discards), _GORE.x, sign of the ragged
    // offset, belly open (1)
    @location(9) cut: vec4<f32>,
    // _PART (−1: gear, not torn by wound volumes)
    @location(10) @interpolate(flat) part: f32,
    @location(11) face: vec4<f32>,
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
    // A shred of uniform is crumpled, not a flat sheet.
    if (i32(round(material.look.y)) == 3) {
        let k = noise3(v.position * 30.0) - 0.5;
        let fold = sin(v.position.x * 70.0 + v.position.z * 40.0) * 0.4;
        p = p + n * (k * 0.05 + fold * 0.012) + vec3<f32>(0.0, abs(k) * 0.03, 0.0);
        n = normalize(n + vec3<f32>(k * 1.5, 0.0, fold));
    }
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
    var local = apply(r0, r1, r2, vec4<f32>(p, 1.0));
    let local_n = apply(r0, r1, r2, vec4<f32>(n, 0.0));
    // Procedural decay: the carcass collapses towards the ground and sinks into it (model
    // space, metres, +y up from the ground point; after the palette, so physics poses too).
    let decay = inst.params.x;
    if (material.look.w > 0.5 && decay > 0.0) {
        let collapse = ramp(decay, 0.15, 0.6) * (0.3 + 0.12 * noise3(v.position * 3.0));
        local.y = local.y * (1.0 - collapse) - DECAY_SINK * ramp(decay, 0.7, 1.0);
    }
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
    out.spray_look = vec4<f32>(look.xy, f32(d), look.w);
    out.cut = vec4<f32>(clamp(keep, -1.0, 1.0), v.body.z, side, belly);
    out.part = v.body.x;
    out.face = v.face;
    // A hair or beard variant this soldier does not wear: off screen.
    let variant = u32(round(v.body.w));
    if (variant > 0u && ((inst.extra.w >> (variant - 1u)) & 1u) == 0u) {
        out.pos = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    }
    return out;
}

// How far a carcass without masks sinks into the ground at the end (metres).
const DECAY_SINK: f32 = 0.3;

// Procedural decay of a model without masks: mottled darkening towards grey-green and
// brown-black as it rots, a ragged dissolve at the end with a dark rim. `discard` is
// signalled by a negative red.
fn rot(rgb_in: vec3<f32>, rest: vec3<f32>, decay: f32, dissolve: bool) -> vec3<f32> {
    let mottle = 0.6 * noise3(rest * 7.0) + 0.4 * noise3(rest * 23.0 + vec3<f32>(5.3));
    let lum = dot(rgb_in, vec3<f32>(0.3, 0.55, 0.15));
    // Grey-green rot in the patches, brown-black elsewhere, keeping some of the pattern.
    let rotten = mix(vec3<f32>(0.10, 0.07, 0.045), vec3<f32>(0.21, 0.21, 0.13), smoothstep(0.45, 0.7, mottle))
        * (0.55 + 0.9 * lum);
    var rgb = mix(rgb_in, rotten, ramp(decay, 0.02, 0.55) * (0.5 + 0.5 * mottle));
    rgb = rgb * mix(1.0, 0.5, ramp(decay, 0.1, 0.85));
    if (dissolve) {
        let p = ramp(decay, 0.62, 1.0);
        let mask = 0.65 * noise3(rest * 5.0 + vec3<f32>(1.9)) + 0.35 * noise3(rest * 19.0);
        let th = p * 1.12;
        if (p > 0.0 && mask <= th) {
            return vec3<f32>(-1.0);
        }
        if (p > 0.0 && mask - th < 0.06) {
            rgb = vec3<f32>(0.09, 0.05, 0.035);
        }
    }
    return rgb;
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

// Wounds placed where hits struck (#277): `n` marks of two texels each after the instance's
// data (and its damage block): [centre (rest pose), radius (< 0: a ragged exit)], [blood run
// down (m), soak 0..1, age 0..1, seed]. A dark hole, torn flesh around an exit, the cloth
// soaked around it and blood running down from it.
fn marked(rgb_in: vec3<f32>, p: vec3<f32>, d: u32, n: u32) -> vec3<f32> {
    var rgb = rgb_in;
    let damaged = texel(d + 3u).z > 0.5;
    let start = d + 4u + select(0u, 28u, damaged);
    let grain = noise3(p * 22.0);
    var stain = 0.0;
    var dry = 1.0;
    var flesh = 0.0;
    var core = 0.0;
    for (var i = 0u; i < n; i = i + 1u) {
        let a = texel(start + 2u * i);
        let b = texel(start + 2u * i + 1u);
        let c = a.xyz;
        let r = max(abs(a.w), 0.005);
        let exit = a.w < 0.0;
        let dist = length(p - c);
        if (dist > r * 6.0 + b.x + 0.25) {
            continue;
        }
        let rough = noise3(p * 70.0 + vec3<f32>(b.w)) - 0.5;
        let hole = r * (1.0 + select(0.3, 0.8, exit) * rough);
        core = max(core, 1.0 - smoothstep(hole * 0.55, hole, dist));
        if (exit) {
            flesh = max(flesh, 1.0 - smoothstep(hole * 1.1, hole * 1.8, dist + rough * r));
        }
        // Soaked cloth around the wound, spreading as it bleeds.
        let soak = r * (1.6 + 4.5 * b.y);
        var s = 1.0 - smoothstep(soak * 0.45, soak, dist + (grain - 0.5) * soak * 0.6);
        // Blood running down from it (down in the rest pose), widening and thinning.
        let below = c.y - p.y;
        if (below > -r && below < b.x) {
            let f = clamp(below / max(b.x, 0.01), 0.0, 1.0);
            let side = length((p - c).xz) + (noise3(p * 9.0 + vec3<f32>(b.w)) - 0.5) * 0.05;
            let width = r * (1.1 + 1.6 * f) + 0.02 * b.y;
            s = max(s, (1.0 - smoothstep(width * 0.55, width, side)) * (1.0 - f * f) * 0.95);
        }
        if (s > stain) {
            stain = s;
            dry = b.z;
        }
    }
    // Dark and wet, so it reads on a red coat as well as on white breeches.
    let red = mix(vec3<f32>(0.19, 0.006, 0.005), vec3<f32>(0.07, 0.025, 0.015), smoothstep(0.0, 1.0, dry));
    rgb = mix(rgb, red, clamp(stain, 0.0, 1.0) * 0.95);
    let meat = mix(vec3<f32>(0.42, 0.05, 0.04), vec3<f32>(0.62, 0.30, 0.26), noise3(p * 90.0));
    rgb = mix(rgb, meat, flesh);
    rgb = mix(rgb, vec3<f32>(0.06, 0.0, 0.0), core * 0.95);
    return rgb;
}

// Fibres of torn muscle and cloth: noise stretched along the body's long axis (rest y).
fn strands(p: vec3<f32>, seed: f32) -> f32 {
    return noise3(vec3<f32>(p.x * 150.0, p.y * 26.0, p.z * 150.0) + vec3<f32>(seed));
}

// Tissue torn out (#280): the wound volumes of the damage block (texels d + 16.., see
// damage.rs `Volume::gpu`). Returns the nearest boundary as (signed distance / radius,
// < 0 inside; the fibre noise there; that volume's radius (m); 1 when any volume is near).
// The boundary is never round: low noise bites chunks, fibre noise frays it, `_GORE.x`
// shifts it per vertex region, and gashes (kind 1) are cleaner than tears.
fn holes(p: vec3<f32>, d: u32, gore: f32) -> vec4<f32> {
    var best = vec4<f32>(1e3, 0.0, 0.0, 0.0);
    if (texel(d + 3u).z < 0.5) {
        return best;
    }
    let n = u32(texel(d + 15u).w + 0.5);
    for (var i = 0u; i < n; i = i + 1u) {
        let a = texel(d + 16u + 2u * i);
        let b = texel(d + 17u + 2u * i);
        let r = max(a.w, 0.005);
        let ab = b.xyz - a.xyz;
        let t = clamp(dot(p - a.xyz, ab) / max(dot(ab, ab), 1e-8), 0.0, 1.0);
        let dist = length(p - a.xyz - ab * t);
        if (dist > r * 2.6) {
            continue;
        }
        let gash = b.w >= 1.0;
        let seed = fract(b.w) * 97.0;
        let chunk = noise3(p * (5.0 / r) + vec3<f32>(seed)) * 0.6 + noise3(p * (13.0 / r) + vec3<f32>(seed * 1.7)) * 0.4;
        let fib = strands(p, seed);
        let amp = select(0.75, 0.3, gash);
        let rr = r * (1.0 + amp * (chunk - 0.5) * 2.0 + select(0.22, 0.1, gash) * (fib - 0.5) + 0.25 * (gore - 0.5));
        let e = (dist - rr) / r;
        if (e < best.x) {
            best = vec4<f32>(e, fib, r, 1.0);
        }
    }
    return best;
}

// The inside of torn flesh: wet red meat, a yellow fat layer under the skin, dark clots and
// splinters of bone; `depth` is how far behind the torn boundary (radius units).
fn tissue(p: vec3<f32>, depth: f32, fib: f32) -> vec3<f32> {
    let grain = noise3(p * 40.0);
    var meat = mix(vec3<f32>(0.16, 0.006, 0.008), vec3<f32>(0.62, 0.06, 0.05), fib * 0.75 + grain * 0.25);
    let fat = 1.0 - smoothstep(0.0, 0.18, depth + (grain - 0.5) * 0.12);
    meat = mix(meat, vec3<f32>(0.72, 0.56, 0.30), fat * 0.85);
    let clot = smoothstep(0.62, 0.72, noise3(p * 22.0 + vec3<f32>(5.3)));
    meat = mix(meat, vec3<f32>(0.035, 0.0, 0.0), clot * 0.95);
    let bone = smoothstep(0.80, 0.84, noise3(p * 60.0 + vec3<f32>(11.0)));
    return mix(meat, vec3<f32>(0.80, 0.74, 0.62), bone * 0.8);
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

// Look texel `k` of an instance (after its marks): see the header.
fn look_texel(d: u32, marks: u32, k: u32) -> vec4<f32> {
    let damaged = texel(d + 3u).z > 0.5;
    return texel(d + 4u + select(0u, 28u, damaged) + 2u * marks + k);
}

// Stubble, wrinkles, pock marks and a scar from the `_FACE` zones (rest pose `p`, metres).
fn face_detail(rgb_in: vec3<f32>, p: vec3<f32>, zones: vec4<f32>, l0: vec4<f32>, l1: vec4<f32>, l2: vec4<f32>) -> vec3<f32> {
    var rgb = rgb_in;
    // Stubble: dark blue-grey dots of growing hair over the beard zone.
    let dots = smoothstep(0.35, 0.75, noise3(p * 700.0));
    let stubble = zones.x * l0.w * (0.55 + 0.45 * dots);
    rgb = mix(rgb, rgb * vec3<f32>(0.42, 0.42, 0.46), clamp(stubble, 0.0, 1.0) * 0.35);
    // Wrinkles: thin dark folds, horizontal across the forehead, deeper with age.
    let wave = abs(fract(p.y * 190.0 + noise3(p * 55.0) * 2.0) - 0.5) * 2.0;
    let fold = 1.0 - smoothstep(0.0, 0.3, wave);
    rgb = rgb * (1.0 - zones.y * l1.w * fold * 0.25);
    // Pock marks: pitted, reddish spots.
    let pit = smoothstep(0.70, 0.80, noise3(p * 520.0 + vec3<f32>(3.7)));
    rgb = mix(rgb, rgb * vec3<f32>(0.70, 0.55, 0.52), zones.z * l2.x * pit);
    // A scar: a pale line slanting down across one cheek.
    if (abs(l2.y) > 0.5) {
        let q = p.xy - vec2<f32>(0.034 * l2.y, l2.z - 0.04);
        let dir = normalize(vec2<f32>(0.5 * l2.y, -1.0));
        let along = dot(q, dir);
        let across = abs(q.x * dir.y - q.y * dir.x);
        let line = (1.0 - smoothstep(0.0012, 0.0022, across)) * (1.0 - smoothstep(0.018, 0.024, abs(along)));
        rgb = mix(rgb, vec3<f32>(0.78, 0.55, 0.50), line * smoothstep(0.3, 0.6, zones.w) * 0.9);
    }
    return rgb;
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

@fragment
fn fs_main(in: VsOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
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
    // Torn out (#280): nothing inside a wound volume but shreds of skin and cloth hanging
    // into the hole from its edge; a ragged band of holes in the cloth around it.
    // Gear (a knapsack in the way) is torn too, but only flesh shows tissue.
    let hole = holes(in.rest.xyz, u32(in.spray_look.z + 0.5), in.cut.y);
    let shred = noise3(in.rest.xyz * vec3<f32>(90.0, 30.0, 90.0) + vec3<f32>(hole.y * 3.0));
    if (hole.x < 0.0 && !(front && hole.x > -0.22 && shred > 0.68)) {
        discard;
    }
    if (front && hole.x >= 0.0 && hole.x < 0.1 && shred < 0.3 - hole.x * 2.0) {
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
    // The wall of a wound cavity (an inside-out sphere): only the far side, facing the
    // viewer, so it is seen through the hole and closes a hole straight through.
    let kind = i32(round(material.look.y));
    if (kind == 1 && dot(normalize(in.normal), globals.forward.xyz) > 0.0) {
        discard;
    }
    var rgb = c.rgb;
    // Each soldier's skin tone and hair colour, and the detail of his face (#283).
    if (material.look.z > 0.5) {
        let d = u32(in.spray_look.z + 0.5);
        let marks = u32(in.spray_look.w + 0.5);
        let l0 = look_texel(d, marks, 0u);
        let l1 = look_texel(d, marks, 1u);
        if (material.look.z < 1.5) {
            rgb = rgb * l0.rgb;
            if (in.face.x + in.face.y + in.face.z > 0.0) {
                rgb = face_detail(rgb, in.rest.xyz, in.face, l0, l1, look_texel(d, marks, 2u));
            }
        } else {
            rgb = rgb * l1.rgb;
        }
    }
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
    if (material.look.w > 0.5 && decay > 0.0) {
        rgb = rot(rgb, in.rest.xyz, decay, globals.params.y > 0.5);
        if (rgb.r < 0.0) {
            discard;
        }
    }
    if (material.flags.w < 0.5 && in.wounds0.z + in.wounds0.w + in.wounds1.x > 0.0) {
        rgb = wounded(rgb, in.rest, in.wounds0, in.wounds1, decay);
    }
    if (material.flags.w < 0.5 && in.spray_look.w > 0.5) {
        rgb = marked(rgb, in.rest.xyz, u32(in.spray_look.z + 0.5), u32(in.spray_look.w + 0.5));
    }
    if (material.flags.w < 0.5 && in.spray_look.x > 0.0) {
        rgb = sprayed(rgb, in.rest.xyz, in.spray, in.spray_look.xy);
    }
    // Lambert from the upper left (the sprites' light), both faces lit.
    var n = normalize(in.normal);
    if (dot(n, globals.forward.xyz) > 0.0) {
        n = -n;
    }
    // An open belly: a dark wet gash.
    var wet = material.look.x;
    if (hole.w > 0.5 && material.flags.w < 0.5) {
        if (!front && in.part > -0.5) {
            // The far wall seen through the hole (and the shell's inside): flesh, fat at
            // the torn edge, not an empty shell.
            rgb = tissue(in.rest.xyz, max(hole.x, 0.0), hole.y);
            wet = 1.0;
        } else if (hole.x < 0.35) {
            // Torn skin and cloth at the edge: soaked dark, raw red right at the tear.
            let k = 1.0 - smoothstep(0.0, 0.35, hole.x + (hole.y - 0.5) * 0.15);
            rgb = mix(rgb, rgb * 0.22 + vec3<f32>(0.10, 0.004, 0.003), k);
            let raw = 1.0 - smoothstep(0.0, 0.07, hole.x);
            rgb = mix(rgb, mix(vec3<f32>(0.45, 0.05, 0.04), vec3<f32>(0.66, 0.42, 0.30), shred * 0.6), raw);
            wet = max(wet, k * 0.6);
        }
    } else if (!front && in.cut.x < 0.12 && in.cut.x > -0.5 && material.flags.w < 0.5) {
        // The inside of a planar cut's sleeve where the cap does not cover it.
        rgb = tissue(in.rest.xyz, in.cut.x * 3.0, strands(in.rest.xyz, 0.0));
        wet = 1.0;
    }
    if (kind == 1) {
        // Wet meat darkening into the deep centre of the cavity.
        let q = in.rest.xyz * 0.07 + vec3<f32>(in.params.x * 3.0);
        rgb = tissue(q, 1.0, strands(q, 1.3));
        let deep = smoothstep(0.55, 1.0, -dot(normalize(in.normal), globals.forward.xyz));
        rgb = rgb * mix(0.85, 0.18, deep);
        // Lumpy, not a smooth bowl: the gloss breaks up.
        wet = 0.45 * smoothstep(0.3, 0.7, noise3(in.rest.xyz * 6.0 + vec3<f32>(2.0)));
    } else if (kind == 2) {
        // Bone: blood-smeared ivory, glossy.
        rgb = mix(rgb, vec3<f32>(0.30, 0.02, 0.015), smoothstep(0.35, 0.65, noise3(in.rest.xyz * 9.0 + vec3<f32>(in.params.x))) * 0.85);
        wet = 0.8;
    } else if (kind == 3 && in.spray.w < 0.0) {
        // A shred of uniform: the uniform's colour (in the spray texel), soaked in places.
        let lum = dot(c.rgb, vec3<f32>(0.3, 0.55, 0.15));
        rgb = in.spray.xyz * (0.55 + 0.9 * lum);
        let soak = smoothstep(0.45, 0.7, noise3(in.rest.xyz * 25.0));
        rgb = mix(rgb, vec3<f32>(0.12, 0.005, 0.004), soak * 0.7);
        wet = soak * 0.4;
    }
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
        let nh = max(dot(n, h), 0.0);
        rgb = rgb + vec3<f32>(0.6, 0.55, 0.5) * wet * pow(nh, 40.0) + vec3<f32>(0.08) * wet * pow(nh, 6.0);
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
