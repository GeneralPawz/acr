// Fog of war over the map view (see fog.rs): the darkness row at the corners of the screen
// cells, interpolated bilinearly per pixel (0x4845ae), applied as black of the row's opacity.

struct Globals {
    // Camera x, y (original pixels), output pixels per original pixel, 1 for an sRGB target.
    camera: vec4<f32>,
    // Original pixel of corner (0, 0), corner spacing.
    grid: vec4<f32>,
    // Corners per row, rows.
    size: vec4<u32>,
    // Opacity of black per darkness row, four per entry.
    dark: array<vec4<f32>, 40>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var rows: texture_2d<u32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var out: VsOut;
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    out.pos = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    return out;
}

fn row_at(c: vec2<i32>) -> f32 {
    let last = vec2<i32>(globals.size.xy) - vec2<i32>(1, 1);
    return f32(textureLoad(rows, clamp(c, vec2<i32>(0, 0), last), 0).r);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let p = floor(in.pos.xy / globals.camera.z + globals.camera.xy);
    let q = (p - globals.grid.xy) / globals.grid.zw;
    let c = vec2<i32>(floor(q));
    let f = q - floor(q);
    let top = mix(row_at(c), row_at(c + vec2<i32>(1, 0)), f.x);
    let bottom = mix(row_at(c + vec2<i32>(0, 1)), row_at(c + vec2<i32>(1, 1)), f.x);
    let row = u32(clamp(floor(mix(top, bottom, f.y)), 0.0, 159.0));
    // Component by mask, not by a runtime index: Adreno's Vulkan driver fails to link the
    // latter.
    let four = globals.dark[row >> 2u];
    var a = dot(four, select(vec4<f32>(0.0), vec4<f32>(1.0), vec4<u32>(0u, 1u, 2u, 3u) == vec4<u32>(row & 3u)));
    if (a <= 0.0) {
        discard;
    }
    // The table darkens the sRGB-encoded colour; blending happens in linear light.
    if (globals.camera.w > 0.5) {
        a = 1.0 - pow(1.0 - a, 2.2);
    }
    return vec4<f32>(0.0, 0.0, 0.0, a);
}
