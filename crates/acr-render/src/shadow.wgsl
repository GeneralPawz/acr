
// ---- Sun shadows (shadow.wgsl, appended to model.wgsl and terrain.wgsl; see shadow.rs) ----
// One shadow map seen from the sun, fitted to the models drawn. Receivers look their world
// point up through a comparison sampler (WebGL2: sampler2DShadow), 3×3 bilinear taps.

struct ShadowParams {
    // World → shadow clip space.
    matrix: mat4x4<f32>,
    // x: on (1), y: PCF step (uv), z: depth bias (light depth units), w: ground darkening.
    params: vec4<f32>,
    // Fade centre (world) and reach (world pixels): shadows fade out over its last fifth.
    fade: vec4<f32>,
    // x: normal offset (world pixels), y: one texel (uv).
    extra: vec4<f32>,
};

@group(2) @binding(0) var<uniform> shadow_params: ShadowParams;
@group(2) @binding(1) var shadow_map: texture_depth_2d;
@group(2) @binding(2) var shadow_compare: sampler_comparison;

// 1 in the sun .. 0 in full shadow, at world point `world` with unit normal `normal`
// (offset along it against acne).
fn sun_visibility(world: vec3<f32>, normal: vec3<f32>) -> f32 {
    if (shadow_params.params.x < 0.5) {
        return 1.0;
    }
    let c = shadow_params.matrix * vec4<f32>(world + normal * shadow_params.extra.x, 1.0);
    let uv = vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
    // Behind the far plane counts as at it: still behind every caster drawn there.
    let z = min(c.z, 1.0) - shadow_params.params.z;
    let s = shadow_params.params.y;
    var sum = 0.0;
    for (var i = -1; i <= 1; i = i + 1) {
        for (var j = -1; j <= 1; j = j + 1) {
            let o = vec2<f32>(f32(i), f32(j)) * s;
            sum = sum + textureSampleCompareLevel(shadow_map, shadow_compare, uv + o, z);
        }
    }
    let lit = sum / 9.0;
    // Fade out at the map's edge and towards the end of its reach (the free camera's far
    // ground).
    let edge = min(min(uv.x, uv.y), min(1.0 - uv.x, 1.0 - uv.y));
    var fade = smoothstep(0.0, 0.02, edge);
    let reach = shadow_params.fade.w;
    fade = fade * (1.0 - smoothstep(reach * 0.8, reach, length(world.xy - shadow_params.fade.xy)));
    if (c.z < 0.0) {
        fade = 0.0;
    }
    return mix(1.0, lit, fade);
}
