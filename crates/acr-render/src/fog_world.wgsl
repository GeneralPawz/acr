// Fog of war of the free camera (fog_world.rs): the world point under each pixel comes from
// the 3D pass's depth; the grid (already moved onto the ground, three_d.rs `ground_fog`) is
// interpolated bilinearly at its x, y and turned into the darkness row of 0x484e63, then
// applied as black of the row's opacity like fog.wgsl.

struct Globals {
    // Clip → world (column-major).
    clip_to_world: mat4x4<f32>,
    // Output width, height (px), 1 for an sRGB target, unused.
    screen: vec4<f32>,
    // Grid cells per row, rows.
    size: vec4<u32>,
    // Opacity of black per darkness row, four per entry.
    dark: array<vec4<f32>, 40>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var cells: texture_2d<u32>;
@group(0) @binding(2) var depth: texture_2d<f32>;

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

fn cell_at(c: vec2<i32>) -> f32 {
    let last = vec2<i32>(globals.size.xy) - vec2<i32>(1, 1);
    if (any(c < vec2<i32>(0, 0)) || any(c > last)) {
        return 0.0;
    }
    return f32(textureLoad(cells, c, 0).r);
}

// 0x484e63 (acr_sim::fog::Fog::shade) on an interpolated value.
fn shade(v: f32) -> f32 {
    let i = 1500.0 - v;
    if (i <= 0.0) {
        return 5.0;
    }
    if (i <= 300.0) {
        return 5.0 + i * 153.0 / 300.0;
    }
    return 158.0;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.pos.xy);
    let z = textureLoad(depth, px, 0).r;
    if (z >= 1.0) {
        // Nothing drawn (off the map): no fog.
        discard;
    }
    let ndc = vec2<f32>(in.pos.x / globals.screen.x * 2.0 - 1.0, 1.0 - in.pos.y / globals.screen.y * 2.0);
    let w = globals.clip_to_world * vec4<f32>(ndc, z, 1.0);
    let p = w.xyz / w.w;
    // Cell (cx, cy) is centred on world (128·(cx − 3), 128·(cy − 3)).
    let q = p.xy / 128.0 + vec2<f32>(3.0, 3.0);
    let c = vec2<i32>(floor(q));
    let f = q - floor(q);
    let top = mix(cell_at(c), cell_at(c + vec2<i32>(1, 0)), f.x);
    let bottom = mix(cell_at(c + vec2<i32>(0, 1)), cell_at(c + vec2<i32>(1, 1)), f.x);
    let row = u32(clamp(floor(shade(mix(top, bottom, f.y))), 0.0, 159.0));
    var a = globals.dark[row >> 2u][row & 3u];
    if (a <= 0.0) {
        discard;
    }
    if (globals.screen.z > 0.5) {
        a = 1.0 - pow(1.0 - a, 2.2);
    }
    return vec4<f32>(0.0, 0.0, 0.0, a);
}
