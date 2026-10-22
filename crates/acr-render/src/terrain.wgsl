// Terrain of a map, reproducing the software renderer of dmcr.exe (see docs/formats/terrain.md).
//
// Pass 1 (`vs_ground`/`fs_ground`) draws the triangle mesh into an R8Uint target of palette
// indices: textures composed with transition masks and the original blend tables, shaded
// by the per-vertex light through the original darkness table. Pass 2 (`vs_full`/
// `fs_present`) tints water by depth through WCOST.GRD and resolves the palette.
// The zoomed-out view (key L) replaces pass 1 by `vs_zoom`/`fs_zoom` (MiniMap4X.cpp).

struct Globals {
    // Camera: top-left in original screen pixels, pixels per original pixel, target scale.
    camera: vec4<f32>,
    // Index target size in pixels, output size in pixels.
    sizes: vec4<f32>,
    // First column pair, first row, column pairs, vertices per row.
    cells: vec4<i32>,
    // Vertex rows, water grid width, water grid height, zoomed-out view (1) or not (0).
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
// Per cell and zoomed-view triangle (4 per cell in a row): base texture, two overlays.
@group(0) @binding(9) var zoom_layers: texture_2d<u32>;
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

// The zoomed-out view's ground (MiniMap4X.cpp, 0x5fdddd), into a target with twice the
// rows: every cell is four 8 x 8-pixel triangles at a quarter of the world size (vertex
// (col, row) at (8 col, 8 row - 4 if col is odd - max(height, 0) / 2)), drawn in order so
// later ones cover earlier ones. Each texture is an unscaled 8 x 8 patch of its tile at a
// random offset (0x5f7ff0): the base texture, then the overlays blended by weights
// interpolated from the vertices, then the light (0x5f960b).

struct ZoomOut {
    @builtin(position) pos: vec4<f32>,
    // Position in the triangle's texture patch (before the height shift), weights of
    // overlay 1 and 2.
    @location(0) spot: vec4<f32>,
    @location(1) light: f32,
    // Base texture, overlay 1, overlay 2 (= base when absent), patch offsets (4 bits each:
    // u and v of the base, overlay 1, overlay 2).
    @location(2) @interpolate(flat) layers: vec4<u32>,
};

fn zoom_hash(x: u32) -> u32 {
    var h = x * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    return (h >> 22u) ^ h;
}

@vertex
fn vs_zoom(@builtin(vertex_index) vi: u32) -> ZoomOut {
    var out: ZoomOut;
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
    // P1 and P2 (the vertical edge), P3 (the apex) and the vertex whose light each corner
    // takes: ZOOM_TRIANGLES in terrain.rs.
    var p = array<i32, 3>(v, v + w, v + w + 1);
    var l = p;
    var corners = array<vec2<f32>, 3>(vec2<f32>(0.0, 0.0), vec2<f32>(0.0, 8.0), vec2<f32>(8.0, 4.0));
    switch kind {
        case 1u: {
            p = array<i32, 3>(v + 1, v + w + 1, v + 2);
            l = p;
        }
        case 2u: {
            p = array<i32, 3>(v + 1, v + w + 1, v);
            l = p;
        }
        case 3u: {
            p = array<i32, 3>(v + 2, v + w + 2, v + w + 1);
            l = array<i32, 3>(v + 2, v + w + 1, v + w + 1);
        }
        default: {}
    }
    if (kind >= 2u) {
        corners = array<vec2<f32>, 3>(vec2<f32>(8.0, 0.0), vec2<f32>(8.0, 8.0), vec2<f32>(0.0, 4.0));
    }
    let here = p[corner];
    let world = vertex_pos(here);
    let h = max(textureLoad(heights, texel_at(here), 0).r, 0) >> 1u;
    let sx = f32(world.x) - globals.camera.x;
    let sy = f32((world.y >> 1u) - (h << 1u)) - globals.camera.y;
    let target_px = vec2<f32>(sx, sy * 2.0) * globals.camera.w;
    out.pos = vec4<f32>(target_px.x / globals.sizes.x * 2.0 - 1.0, 1.0 - target_px.y / globals.sizes.y * 2.0, 0.0, 1.0);

    let layers = textureLoad(zoom_layers, vec2<i32>(cp * 4 + i32(kind), row), 0);
    let data = textureLoad(vertices, texel_at(here), 0);
    // An overlay weighs 255 - r at the vertices with its texture and r at the others.
    let r = f32(data.a);
    let w1 = select(r, 255.0 - r, data.r == layers.g);
    let w2 = select(r, 255.0 - r, data.r == layers.b);
    out.spot = vec4<f32>(corners[corner], w1, w2);
    out.light = f32(textureLoad(vertices, texel_at(l[corner]), 0).g);
    // The original takes `rand() & 15` for u and v per texture and triangle.
    out.layers = vec4<u32>(layers.r, layers.g, layers.b, zoom_hash(u32(v) * 4u + kind) & 0xFFFFFFu);
    return out;
}

// One overlay pass (0x5f8154): the interpolated weight picks the new texel, 3/4, 1/2 or
// 1/4 of it through AGEW_TR4/AGEW_TR, or keeps the old pixel. Between 51 and 65 the
// original writes the value left from the previous pixel; 1/4 is used there.
fn zoom_blend(old: u32, texel: u32, w: f32) -> u32 {
    if (w > 190.0) {
        return texel;
    }
    if (w > 153.0) {
        return table(256u + texel, old);
    }
    if (w > 102.0) {
        return table(old, texel);
    }
    if (w > 51.0) {
        return table(256u + old, texel);
    }
    return old;
}

fn patch_offset(seed: u32, layer: u32) -> vec2<i32> {
    let s = seed >> (layer * 8u);
    return vec2<i32>(i32(s & 15u), i32((s >> 4u) & 15u));
}

@fragment
fn fs_zoom(in: ZoomOut) -> @location(0) u32 {
    let at = clamp(vec2<i32>(floor(in.spot.xy)), vec2<i32>(0, 0), vec2<i32>(7, 7));
    let seed = in.layers.w;
    var c = tile_texel(in.layers.x, at + patch_offset(seed, 0u));
    if (in.layers.y != in.layers.x) {
        c = zoom_blend(c, tile_texel(in.layers.y, at + patch_offset(seed, 1u)), in.spot.z);
    }
    if (in.layers.z != in.layers.x) {
        c = zoom_blend(c, tile_texel(in.layers.z, at + patch_offset(seed, 2u)), in.spot.w);
    }
    // Light: row 63 + level + a random bit of AGEW_1DR.GRD; row 63 is the identity.
    let px = vec2<u32>(in.pos.xy);
    let level = u32(clamp(floor(in.light), 0.0, 31.0)) + (zoom_hash(px.x * 7919u + px.y * 104729u) & 1u);
    if (level == 0u) {
        return c;
    }
    return table(512u + min(level - 1u, 31u), c);
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
    let zoomed = globals.map.w == 1;
    var target_px = vec2<i32>(floor(out_px * globals.camera.w / scale));
    if (zoomed) {
        // The zoomed-out view keeps every other row of its ground (0x5fe62f).
        target_px.y *= 2;
    }
    if (target_px.x >= i32(globals.sizes.x) || target_px.y >= i32(globals.sizes.y)) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    var c = textureLoad(ground, target_px, 0).r;
    // Water (0x5EF5A6): per 32 x 32 screen block, bilinear depth of grid rows 2j and 2j + 2.
    // The zoomed-out view (0x5f2a89) does the same on its 8 x 8-pixel blocks, without noise.
    var screen = vec2<i32>(floor(out_px / scale + globals.camera.xy));
    if (zoomed) {
        screen = (screen >> vec2<u32>(2u, 2u)) << vec2<u32>(2u, 2u);
    }
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
        if (alpha < 65000 && !zoomed) {
            alpha = min(alpha + i32((hash(screen) & 0xFFFFu) >> 2u), 65000);
        }
        let level = u32(alpha >> 8u) & 0xF0u;
        c = table(544u + c, min(level + 1u, 255u));
    }
    let rgb = textureLoad(palette, vec2<i32>(i32(c), 0), 0).rgb;
    return vec4<f32>(rgb, 1.0);
}
