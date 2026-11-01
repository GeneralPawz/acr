//! The image pass filters premultiplied alpha: an edge texel next to a transparent one must
//! not darken (#464). Runs on whatever adapter the machine has (software included) and is
//! skipped where there is none.

use acr_render::{ImageDraw, ImageRenderer};

/// Draws `rgba` (`w` x `h`, straight alpha) stretched over a `out_w` x 1 target cleared to
/// `background`, and returns the target's pixels as RGBA bytes.
fn draw(
    format: wgpu::TextureFormat,
    (w, h): (u32, u32),
    rgba: &[u8],
    out_w: u32,
    background: wgpu::Color,
) -> Option<Vec<u8>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).ok()?;
    let mut images = ImageRenderer::new(&device, format);
    let texture = images.upload(&device, &queue, w, h, rgba);
    images.prepare(&device, &queue, [out_w as f32, 1.0], &[ImageDraw::full(&texture, [0.0, 0.0, out_w as f32, 1.0])]);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d { width: out_w, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let row = (out_w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(row),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let view = target.create_view(&Default::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("image"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(background), store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        images.render(&mut pass);
    }
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(1) },
        },
        wgpu::Extent3d { width: out_w, height: 1, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.map_async(wgpu::MapMode::Read, .., move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
    rx.recv().ok()?.ok()?;
    let data = buffer.get_mapped_range(..).ok()?;
    Some(data[..(out_w * 4) as usize].to_vec())
}

fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f64) -> f64 {
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// A 2x1 sprite (opaque grey, then a transparent black texel) stretched over 4 pixels: the
/// two middle pixels lie between the texels and get 3/4 and 1/4 of the grey.
const SPRITE: [u8; 8] = [200, 200, 200, 255, 0, 0, 0, 0];
const WEIGHTS: [f64; 4] = [1.0, 0.75, 0.25, 0.0];

#[test]
fn edge_pixels_blend_premultiplied_in_the_framebuffers_space() {
    // A target that blends the stored values (the browser's canvas): straight filtering
    // would give 0.75 * 200 = 150 at alpha 0.75 and so 137.5 over the background; the
    // premultiplied reference is 150 + 0.25 * 100 = 175.
    let background = wgpu::Color { r: 100.0 / 255.0, g: 100.0 / 255.0, b: 100.0 / 255.0, a: 1.0 };
    let Some(out) = draw(wgpu::TextureFormat::Rgba8Unorm, (2, 1), &SPRITE, 4, background) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    for (x, w) in WEIGHTS.iter().enumerate() {
        let expected = w * 200.0 + (1.0 - w) * 100.0;
        let got = f64::from(out[x * 4]);
        assert!((got - expected).abs() <= 1.0, "pixel {x}: {got} != {expected}");
    }
    assert!((i32::from(out[4]) - 175).abs() <= 1);
}

#[test]
fn srgb_targets_mix_in_linear_light() {
    // An sRGB target decodes the texture and blends in linear light, so the reference is
    // the premultiplied mix of the linear values, encoded again.
    let background = wgpu::Color { r: 0.5, g: 0.5, b: 0.5, a: 1.0 };
    let Some(out) = draw(wgpu::TextureFormat::Rgba8UnormSrgb, (2, 1), &SPRITE, 4, background) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (grey, back) = (srgb_to_linear(200.0 / 255.0), 0.5);
    for (x, w) in WEIGHTS.iter().enumerate() {
        let expected = linear_to_srgb(w * grey + (1.0 - w) * back) * 255.0;
        let got = f64::from(out[x * 4]);
        assert!((got - expected).abs() <= 1.5, "pixel {x}: {got} != {expected}");
    }
}
