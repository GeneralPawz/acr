
// ---- Shadow casters (model_shadow.wgsl, appended to model.wgsl + shadow.wgsl) ----
// The shadow-map pass draws the models with `vs_main` (the light's matrix in
// `globals.view_proj`), so they are skinned, broken and posed exactly as drawn; `fs_shadow`
// drops what `fs_main` drops: alpha cut-outs, parts cut away or torn out, dissolved rot,
// a stump cap's outline; the half transparent fade casts half a shadow.

@fragment
fn fs_shadow(in: VsOut) {
    let tex = textureSampleLevel(base_texture, base_sampler, in.uv, 0.0);
    var c = material.base * tex;
    if (material.flags.y > 0.5) {
        c = c * in.color;
    }
    if (material.flags.z > 0.0 && c.a < material.flags.z) {
        discard;
    }
    if (in.params.w > 0.5) {
        let p = vec2<i32>(floor(in.pos.xy));
        if (((p.x + p.y) & 1) == 1) {
            discard;
        }
    }
    if (in.cut.x - in.cut.z * ragged(in.cut.y, in.rest.xyz) < 0.0) {
        discard;
    }
    if (holes(in.rest.xyz, u32(in.spray_look.z + 0.5), in.cut.y).x < 0.0) {
        discard;
    }
    // Wound cavities sit inside the body.
    if (i32(round(material.look.y)) == 1) {
        discard;
    }
    if (material.look.x > 0.5) {
        let a = atan2(in.uv.y, in.uv.x);
        let edge = 0.9 + 0.1 * noise3(vec3<f32>(cos(a) * 3.0, sin(a) * 3.0, 1.7));
        if (length(in.uv) > edge) {
            discard;
        }
    }
    let decay = in.params.x;
    if (material.flags.x > 0.5 && decay > 0.0 && globals.params.y > 0.5 && material.windows.z <= 1.0) {
        let mask = 0.75 * in.color.r + 0.25 * in.color.g;
        let p = ramp(decay, material.windows.z, material.windows.w);
        if (p > 0.0 && mask <= p * (1.0 + material.edge.w)) {
            discard;
        }
    }
    if (material.look.w > 0.5 && decay > 0.0 && rot(vec3<f32>(0.5), in.rest.xyz, decay, globals.params.y > 0.5).r < 0.0) {
        discard;
    }
}
