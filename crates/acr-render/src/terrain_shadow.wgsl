
// ---- Shadows on the ground (terrain_shadow.wgsl, appended to terrain.wgsl + shadow.wgsl) ----
// The terrain's triangles once more in the 3D pass, as `vs_depth` places them (so the depth
// test against the terrain passes and models in front cover it), darkening the drawn ground
// where the models' shadows fall.

struct GroundShadowOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
};

@vertex
fn vs_ground_shadow(@builtin(vertex_index) vi: u32) -> GroundShadowOut {
    var out: GroundShadowOut;
    let tri = vi / 3u;
    let corner = vi % 3u;
    let cell = i32(tri / 4u);
    let kind = tri % 4u;
    let cp = globals.cells.x + cell % globals.cells.z;
    let row = globals.cells.y + cell / globals.cells.z;
    let w = globals.cells.w;
    if (row >= globals.map.x - 1) {
        out.pos = vec4<f32>(2.0, 2.0, 0.0, 1.0);
        out.world = vec3<f32>(0.0);
        return out;
    }
    let v = row * w + cp * 2;
    var corners: array<i32, 3>;
    switch kind {
        case 0u: { corners = array<i32, 3>(v, v + w, v + w + 1); }
        case 1u: { corners = array<i32, 3>(v, v + 1, v + w + 1); }
        case 2u: { corners = array<i32, 3>(v + 2, v + 1, v + w + 1); }
        default: { corners = array<i32, 3>(v + 2, v + w + 2, v + w + 1); }
    }
    let here = corners[corner];
    let world = vertex_pos(here);
    let height = textureLoad(heights, texel_at(here), 0).r;
    let p = vec3<f32>(f32(world.x), f32(world.y), f32(height) / COS_E);
    var clip = globals.view * vec4<f32>(p, 1.0);
    clip.z = clip.z + globals.mode.y * clip.w;
    out.pos = clip;
    out.world = p;
    return out;
}

@fragment
fn fs_ground_shadow(in: GroundShadowOut) -> @location(0) vec4<f32> {
    let lit = sun_visibility(in.world, vec3<f32>(0.0, 0.0, 1.0));
    return vec4<f32>(0.0, 0.0, 0.0, (1.0 - lit) * shadow_params.params.w);
}
