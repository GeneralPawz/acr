// Terrain of a map, reproducing the software renderer of dmcr.exe (see docs/formats/terrain.md).
//
// Pass 1 (`vs_ground`/`fs_ground`) draws the triangle mesh into an R8Uint target of palette
// indices: textures composed with transition masks and the original blend tables, shaded
// by the per-vertex light through the original darkness table. Pass 2 (`vs_full`/
// `fs_present`) tints water by depth through WCOST.GRD and resolves the palette.

struct Globals {
    // Camera: top-left in original screen pixels, pixels per original pixel, target scale.
    camera: vec4<f32>,
    // Index target size in pixels, output size in pixels.
    sizes: vec4<f32>,
    // First column pair, first row, column pairs, vertices per row.
    cells: vec4<i32>,
    // Vertex rows, water grid width, water grid height, unused.
    map: vec4<i32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var heights: texture_2d<i32>;
// Per vertex: texture, light level, random lift.
@group(0) @binding(2) var vertices: texture_2d<u32>;
// Per cell: transition variants S0..S3 and S4..S6.
@group(0) @binding(3) var sections: texture_2d<u32>;
// 256 ground textures of 64 x 64, 8 per row.
@group(0) @binding(4) var tiles: texture_2d<u32>;
// 216 masks of 64 x 32, 8 per row: 108 layered (bits A=1 B=2 C=4 D=8), 108 hard (bit 1).
@group(0) @binding(5) var masks: texture_2d<u32>;
// Rows 0..255 AGEW_TR, 256..511 AGEW_TR4, 512..543 darkness levels, 544..799 WCOST.
@group(0) @binding(6) var tables: texture_2d<u32>;
@group(0) @binding(7) var water: texture_2d<u32>;
@group(0) @binding(8) var palette: texture_2d<f32>;
@group(1) @binding(0) var ground: texture_2d<u32>;

const NONE: u32 = 0xFFFFu;
const HARD: u32 = 108u;

struct GroundOut {
    @builtin(position) pos: vec4<f32>,
    // Texture-space coordinates (uniform triangles).
    @location(0) tex: vec2<f32>,
    // Composition-buffer coordinates (blended triangles).
    @location(1) local: vec2<f32>,
    @location(2) light: f32,
    // Base texture, overlay 1 texture, overlay 1 mask, overlay 2 texture, overlay 2 mask, flags.
    @location(3) @interpolate(flat) layers: vec4<u32>,
    @location(4) @interpolate(flat) layers2: vec2<u32>,
    // Composition origin in texture space.
    @location(5) @interpolate(flat) origin: vec2<i32>,
    // Original screen position for dithering.
    @location(6) screen: vec2<f32>,
};

fn vertex_pos(v: i32) -> vec2<i32> {
    let w = globals.cells.w;
    let col = v % w;
    let row = v / w;
    return vec2<i32>(col * 32, row * 32 - select(0, 16, (col & 1) == 1));
}

fn texel_at(v: i32) -> vec2<i32> {
    let w = globals.cells.w;
    return vec2<i32>(v % w, v / w);
}

// The texture-space position of a world point (0x44864B).
fn tex_coord(p: vec2<i32>) -> vec2<i32> {
    return vec2<i32>(-(p.x >> 1u) - p.y, (((p.x + p.x + p.x) >> 1u) - p.y) >> 1u);
}

fn section(cp: i32, row: i32, k: u32) -> u32 {
    let lo = textureLoad(sections, vec2<i32>(cp * 2, row), 0);
    let hi = textureLoad(sections, vec2<i32>(cp * 2 + 1, row), 0);
    switch k {
        case 0u: { return lo.r; }
        case 1u: { return lo.g; }
        case 2u: { return lo.b; }
        case 3u: { return lo.a; }
        case 4u: { return hi.r; }
        case 5u: { return hi.g; }
        default: { return hi.b; }
    }
}

// Mask index of vertex x given its own and the previous vertex's variant.
fn variant(prev: u32, own: u32) -> u32 {
    return (2u - min(prev, 2u)) * 3u + min(own, 2u);
}

@vertex
fn vs_ground(@builtin(vertex_index) vi: u32) -> GroundOut {
    var out: GroundOut;
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
    var p: i32;
    var q: i32;
    var r: i32;
    var lp: vec2<f32>;
    var lq: vec2<f32>;
    var lr: vec2<f32>;
    // Vertices P, Q, R, composer order (a, b, c) and variant bytes per triangle (0x44864B).
    var a: i32;
    var b: i32;
    var c: i32;
    var va: u32;
    var vb: u32;
    var vc: u32;
    var left = false;
    switch kind {
        case 0u: {
            p = v; q = v + w; r = v + w + 1;
            lp = vec2(30.0, 15.0); lq = vec2(0.0, 0.0); lr = vec2(0.0, 31.0);
            a = q; b = p; c = r;
            va = section(cp, row, 0u); vb = section(cp, row, 2u); vc = section(cp, row, 1u);
        }
        case 1u: {
            p = v; q = v + 1; r = v + w + 1;
            lp = vec2(30.0, 1.0); lq = vec2(30.0, 30.0); lr = vec2(1.0, 14.0);
            a = p; b = r; c = q;
            va = section(cp, row, 2u); vb = section(cp, row, 3u); vc = section(cp, row, 1u);
            left = true;
        }
        case 2u: {
            p = v + 2; q = v + 1; r = v + w + 1;
            lp = vec2(1.0, 30.0); lq = vec2(30.0, 14.0); lr = vec2(1.0, 1.0);
            a = r; b = q; c = p;
            va = section(cp, row, 3u); vb = section(cp, row, 4u); vc = section(cp, row, 5u);
        }
        default: {
            p = v + 2; q = v + w + 2; r = v + w + 1;
            lp = vec2(30.0, 30.0); lq = vec2(1.0, 14.0); lr = vec2(30.0, 1.0);
            a = r; b = q; c = p;
            va = section(cp, row, 4u); vb = section(cp, row, 6u); vc = section(cp, row, 5u);
            left = true;
        }
    }
    var here: i32;
    var local: vec2<f32>;
    switch corner {
        case 0u: { here = p; local = lp; }
        case 1u: { here = q; local = lq; }
        default: { here = r; local = lr; }
    }
    let world = vertex_pos(here);
    let data = textureLoad(vertices, texel_at(here), 0);
    let height = textureLoad(heights, texel_at(here), 0).r;
    let sx = f32(world.x) - globals.camera.x;
    let sy = f32((world.y >> 1u) - height - i32(data.b)) - globals.camera.y;
    let scale = globals.camera.w;
    let target_px = vec2<f32>(sx, sy) * scale;
    out.pos = vec4<f32>(target_px.x / globals.sizes.x * 2.0 - 1.0, 1.0 - target_px.y / globals.sizes.y * 2.0, 0.0, 1.0);
    out.screen = vec2<f32>(sx, sy);
    out.tex = vec2<f32>(tex_coord(world));
    out.local = local;
    out.light = f32(data.g);
    out.origin = tex_coord(vertex_pos(q));

    let ta = textureLoad(vertices, texel_at(a), 0).r;
    let tb = textureLoad(vertices, texel_at(b), 0).r;
    let tc = textureLoad(vertices, texel_at(c), 0).r;
    let ma = variant(vc, va);
    let mb = variant(va, vb);
    let mc = variant(vb, vc);
    // Composition rules of 0x49610B (right-pointing) and 0x49654D (left-pointing).
    var base = ta;
    var t1 = NONE;
    var m1 = NONE;
    var t2 = NONE;
    var m2 = NONE;
    let o = select(0u, 54u, left);
    if (ta == tb && tb == tc) {
        base = ta;
    } else if (ta == tb) {
        base = ta; t1 = tc;
        m1 = o + select(18u, 27u, tc < tb) + mc;
    } else if (tb == tc) {
        base = tc; t1 = ta;
        m1 = o + select(36u, 45u, ta < tc) + ma;
    } else if (ta == tc) {
        base = tc; t1 = tb;
        m1 = o + select(0u, 9u, tb < tc) + mb;
    } else if (ta < tb && ta < tc) {
        base = ta; t1 = tb; t2 = tc;
        m1 = select(HARD + mb, o + mb, left);
        m2 = o + 18u + mc;
    } else if (tb < ta && tb < tc) {
        base = tb; t1 = ta; t2 = tc;
        m1 = o + 36u + ma;
        m2 = o + 18u + mc;
    } else {
        base = tc; t1 = ta; t2 = tb;
        m1 = o + 36u + ma;
        m2 = o + mb;
    }
    out.layers = vec4<u32>(base, t1, m1, t2);
    out.layers2 = vec2<u32>(m2, select(0u, 1u, left));
    return out;
}

fn tile_texel(tile: u32, uv: vec2<i32>) -> u32 {
    let p = vec2<i32>(i32(tile % 8u) * 64 + (uv.x & 63), i32(tile / 8u) * 64 + (uv.y & 63));
    return textureLoad(tiles, p, 0).r;
}

fn table(row: u32, col: u32) -> u32 {
    return textureLoad(tables, vec2<i32>(i32(col), i32(row)), 0).r;
}

// Overlays texture `tile` through mask `mask` onto colour `c` (0x495DC1, 0x495C00).
fn overlay(c: u32, tile: u32, mask: u32, at: vec2<i32>, uv: vec2<i32>) -> u32 {
    if (mask == NONE) {
        return c;
    }
    let bits = textureLoad(masks, vec2<i32>(i32(mask % 8u) * 64 + at.x, i32(mask / 8u) * 32 + at.y), 0).r;
    let n = tile_texel(tile, uv);
    var out = c;
    if ((bits & 1u) != 0u) { out = n; }
    if ((bits & 2u) != 0u) { out = table(256u + n, out); }
    if ((bits & 4u) != 0u) { out = table(n, out); }
    if ((bits & 8u) != 0u) { out = table(256u + out, n); }
    return out;
}

@fragment
fn fs_ground(in: GroundOut) -> @location(0) u32 {
    var c: u32;
    if (in.layers.y == NONE) {
        c = tile_texel(in.layers.x, vec2<i32>(floor(in.tex)));
    } else {
        let at = clamp(vec2<i32>(floor(in.local)), vec2<i32>(0, 0), vec2<i32>(63, 31));
        let left = in.layers2.y == 1u;
        let base_uv = in.origin + at - vec2<i32>(0, select(0, 16, left));
        let over_uv = in.origin + at - vec2<i32>(0, select(0, 31, left));
        c = tile_texel(in.layers.x, base_uv);
        c = overlay(c, in.layers.y, in.layers.z, at, over_uv);
        c = overlay(c, in.layers.w, in.layers2.x, at, over_uv);
    }
    // Ordered dither of the light level (0x445368): +1/2 on odd pixels, +1/4 on odd rows.
    let px = vec2<i32>(floor(in.screen));
    let dither = select(0.0, 0.5, (px.x & 1) == 1) + select(0.0, 0.25, (px.y & 1) == 1);
    let level = u32(clamp(floor(in.light + dither), 0.0, 31.0));
    return table(512u + level, c);
}

struct FullOut {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> FullOut {
    var out: FullOut;
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    out.pos = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    return out;
}

fn depth(x: i32, y: i32) -> i32 {
    if (x < 0 || y < 0 || x >= globals.map.y || y >= globals.map.z) {
        return 0;
    }
    return i32(textureLoad(water, vec2<i32>(x, y), 0).r) - 127;
}

fn hash(p: vec2<i32>) -> u32 {
    var h = u32(p.x) * 374761393u + u32(p.y) * 668265263u;
    h = (h ^ (h >> 13u)) * 1274126177u;
    return h ^ (h >> 16u);
}

@fragment
fn fs_present(in: FullOut) -> @location(0) vec4<f32> {
    let out_px = in.pos.xy;
    let scale = globals.camera.z;
    let target_px = vec2<i32>(floor(out_px * globals.camera.w / scale));
    if (target_px.x >= i32(globals.sizes.x) || target_px.y >= i32(globals.sizes.y)) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    var c = textureLoad(ground, target_px, 0).r;
    // Water (0x5EF5A6): per 32 x 32 screen block, bilinear depth of grid rows 2j and 2j + 2.
    let screen = vec2<i32>(floor(out_px / scale + globals.camera.xy));
    let gx = screen.x >> 5u;
    let gy = (screen.y >> 5u) * 2;
    let d00 = depth(gx, gy);
    let d10 = depth(gx + 1, gy);
    let d01 = depth(gx, gy + 2);
    let d11 = depth(gx + 1, gy + 2);
    if (d00 >= 0 || d10 >= 0 || d01 >= 0) {
        let f = vec2<f32>(screen & vec2<i32>(31, 31)) / 32.0;
        let raw = mix(mix(f32(d00), f32(d10), f.x), mix(f32(d01), f32(d11), f.x), f.y) * 1024.0;
        var alpha = i32(clamp(raw, 0.0, 65000.0));
        if (alpha < 65000) {
            alpha = min(alpha + i32((hash(screen) & 0xFFFFu) >> 2u), 65000);
        }
        let level = u32(alpha >> 8u) & 0xF0u;
        c = table(544u + c, min(level + 1u, 255u));
    }
    let rgb = textureLoad(palette, vec2<i32>(i32(c), 0), 0).rgb;
    return vec4<f32>(rgb, 1.0);
}
