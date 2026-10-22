// Palette-indexed sprite layers. Each instance draws one decoded GP layer.
//
// Atlas texels are (value, coverage); darkness levels are stored as their alpha. The
// palette lookup happens here so player colours and palette effects stay cheap to change
// at runtime.

struct Globals {
    screen: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var palette: texture_2d<f32>;
@group(1) @binding(0) var page: texture_2d<u32>;

// Must match `LayerKind` in lib.rs.
const KIND_OPAQUE: u32 = 0u;
const KIND_PLAYER: u32 = 1u;
const KIND_TRANSLUCENT: u32 = 2u;
const KIND_SHADOW_LEVELS: u32 = 3u;
const KIND_SHADOW: u32 = 4u;
const PLAYER_RAMP_BASE: u32 = 208u; // 0xD0
// Kind bit of a half transparent instance (`TRANSPARENT` in lib.rs).
const TRANSPARENT: u32 = 256u;
// Kind bit of darkness levels that lighten (`WHITE` in lib.rs): `AGEW_1W.GRD` row r moves a
// colour about r / 62 of the way to white, half as far as `AGEW_1D.GRD` darkens.
const WHITE: u32 = 512u;

struct Instance {
    @location(0) dst: vec4<f32>,  // screen px: x, y, w, h
    @location(1) src: vec4<u32>,  // atlas px: x, y, w, h
    @location(2) kind: u32,
    @location(3) param: u32,
    // Depth in the 3D pass (0 otherwise).
    @location(4) depth: f32,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) src: vec4<u32>,
    @location(2) @interpolate(flat) kind: u32,
    @location(3) @interpolate(flat) param: u32,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Instance) -> VsOut {
    let corner = vec2<f32>(f32(vi & 1u), f32(vi >> 1u));
    let px = inst.dst.xy + corner * inst.dst.zw;
    var out: VsOut;
    out.pos = vec4<f32>(px.x / globals.screen.x * 2.0 - 1.0, 1.0 - px.y / globals.screen.y * 2.0, inst.depth, 1.0);
    out.local = corner * vec2<f32>(inst.src.zw);
    out.src = inst.src;
    out.kind = inst.kind;
    out.param = inst.param;
    return out;
}

fn palette_color(index: u32) -> vec3<f32> {
    return textureLoad(palette, vec2<i32>(i32(index & 255u), 0), 0).rgb;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = layer_color(in);
    if ((in.kind & TRANSPARENT) != 0u) {
        return vec4<f32>(c.rgb, c.a * 0.5);
    }
    return c;
}

fn layer_color(in: VsOut) -> vec4<f32> {
    let texel = in.src.xy + min(vec2<u32>(in.local), in.src.zw - vec2<u32>(1u, 1u));
    let t = textureLoad(page, vec2<i32>(texel), 0);
    if (t.g == 0u) {
        discard;
    }
    let v = t.r;
    switch in.kind & 255u {
        case KIND_PLAYER: {
            return vec4<f32>(palette_color(PLAYER_RAMP_BASE + 4u * in.param + v), 1.0);
        }
        case KIND_TRANSLUCENT: {
            return vec4<f32>(palette_color(v), f32(in.param) / 255.0);
        }
        case KIND_SHADOW_LEVELS: {
            if ((in.kind & WHITE) != 0u) {
                return vec4<f32>(1.0, 1.0, 1.0, f32(v) / 510.0);
            }
            return vec4<f32>(0.0, 0.0, 0.0, f32(v) / 255.0);
        }
        case KIND_SHADOW: {
            return vec4<f32>(0.0, 0.0, 0.0, f32(in.param) / 255.0);
        }
        default: {
            return vec4<f32>(palette_color(v), 1.0);
        }
    }
}
