//! The shaders must validate and translate to GLSL ES 3.00, which the browser build runs on
//! WebGL2.

use naga::back::glsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

const SHADERS: [(&str, &str); 6] = [
    ("model.wgsl", include_str!("../src/model.wgsl")),
    ("blood.wgsl", include_str!("../src/blood.wgsl")),
    ("terrain.wgsl", include_str!("../src/terrain.wgsl")),
    ("fog.wgsl", include_str!("../src/fog.wgsl")),
    ("image.wgsl", include_str!("../src/image.wgsl")),
    ("sprite.wgsl", include_str!("../src/sprite.wgsl")),
];

#[test]
fn shaders_translate_to_webgl2() {
    for (name, source) in SHADERS {
        let module =
            naga::front::wgsl::parse_str(source).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
        let options =
            glsl::Options { version: glsl::Version::Embedded { version: 300, is_webgl: true }, ..Default::default() };
        for entry in &module.entry_points {
            let pipeline =
                glsl::PipelineOptions { shader_stage: entry.stage, entry_point: entry.name.clone(), multiview: None };
            let mut out = String::new();
            glsl::Writer::new(&mut out, &module, &info, &options, &pipeline, Default::default())
                .and_then(|mut w| w.write())
                .unwrap_or_else(|e| panic!("{name} {}: {e}", entry.name));
        }
    }
}
