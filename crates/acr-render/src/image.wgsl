// Full-colour images (menu backgrounds, previews) drawn as scaled quads.
//
// The images are straight-alpha RGBA. Filtering them as they are would blend an edge texel
// towards the black of its transparent neighbours (dark fringes at non-integer scales), so
// the four texels of the bilinear footprint are premultiplied first and the result is
// blended with `PREMULTIPLIED_ALPHA_BLENDING`. The fetch returns what the texture format
// decodes to (linear for the sRGB texture, the stored value otherwise), so the product and
// the mix happen in the same space the framebuffer blends in.

struct Globals {
    screen: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var image: texture_2d<f32>;

struct Instance {
    @location(0) dst: vec4<f32>,   // screen px: x, y, w, h
    @location(1) src: vec4<f32>,   // uv: u, v, w, h
    @location(2) color: vec4<f32>, // multiplied with the texel
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Instance) -> VsOut {
    let corner = vec2<f32>(f32(vi & 1u), f32(vi >> 1u));
    let px = inst.dst.xy + corner * inst.dst.zw;
    var out: VsOut;
    out.pos = vec4<f32>(px.x / globals.screen.x * 2.0 - 1.0, 1.0 - px.y / globals.screen.y * 2.0, 0.0, 1.0);
    out.uv = inst.src.xy + corner * inst.src.zw;
    out.color = inst.color;
    return out;
}

fn texel(p: vec2<i32>, last: vec2<i32>) -> vec4<f32> {
    let t = textureLoad(image, clamp(p, vec2<i32>(0), last), 0);
    return vec4<f32>(t.rgb * t.a, t.a);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(image));
    let last = vec2<i32>(size) - vec2<i32>(1);
    // Bilinear filtering with texel centres at half pixels, edges clamped.
    let p = in.uv * size - vec2<f32>(0.5);
    let base = floor(p);
    let f = p - base;
    let i = vec2<i32>(base);
    let top = mix(texel(i, last), texel(i + vec2<i32>(1, 0), last), f.x);
    let bottom = mix(texel(i + vec2<i32>(0, 1), last), texel(i + vec2<i32>(1, 1), last), f.x);
    let c = mix(top, bottom, f.y);
    return vec4<f32>(c.rgb * in.color.rgb * in.color.a, c.a * in.color.a);
}
