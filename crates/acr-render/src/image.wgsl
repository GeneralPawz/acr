// Full-colour images (menu backgrounds, previews) drawn as scaled quads.

struct Globals {
    screen: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

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

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(image, image_sampler, in.uv) * in.color;
}
