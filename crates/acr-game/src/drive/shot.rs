//! `screenshot`: the current frame drawn into an offscreen texture and written as PNG.
//! Only with a GPU (`--drive-gpu`); the window stays hidden.

use crate::{Game, Mode};

/// Draws the frame once offscreen and throws it away, so that its missing files are asked
/// for.
pub fn prime(game: &mut Game) -> Result<(), String> {
    let g = game.graphics.as_ref().ok_or("no GPU")?;
    let texture = target(&g.gpu.device, g.gpu.config.format, g.gpu.config.width, g.gpu.config.height);
    game.draw_frame(&texture.create_view(&Default::default()));
    game.load_requested();
    Ok(())
}

fn target(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("screenshot"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Draws the frame offscreen and writes it to `path`, cut to `crop` (x, y, w, h) if given.
/// Returns the written size.
pub fn screenshot(game: &mut Game, path: &str, crop: Option<[u32; 4]>) -> Result<[u32; 2], String> {
    let (size, rgba) = capture(game, crop)?;
    write_png(path, size, &rgba)?;
    Ok(size)
}

/// Draws the frame offscreen and returns it as RGBA, cut to `crop` (x, y, w, h) if given.
pub fn capture(game: &mut Game, crop: Option<[u32; 4]>) -> Result<([u32; 2], Vec<u8>), String> {
    let g = game.graphics.as_ref().ok_or("no GPU: start the driver with --drive-gpu for screenshots")?;
    let (device, queue) = (g.gpu.device.clone(), g.gpu.queue.clone());
    let format = g.gpu.config.format;
    let (width, height) = (g.gpu.config.width, g.gpu.config.height);
    let texture = target(&device, format, width, height);
    let view = texture.create_view(&Default::default());
    game.draw_frame(&view);
    let row = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("screenshot"),
        size: u64::from(row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(height) },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.map_async(wgpu::MapMode::Read, .., move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(10)) })
        .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    let data = buffer.get_mapped_range(..).map_err(|e| e.to_string())?;
    let bgra = matches!(format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
    let [cx, cy, cw, ch] = crop.unwrap_or([0, 0, width, height]);
    let (cx, cy) = (cx.min(width), cy.min(height));
    let (cw, ch) = (cw.min(width - cx).max(1), ch.min(height - cy).max(1));
    let mut rgba = Vec::with_capacity((cw * ch * 4) as usize);
    for y in cy..cy + ch {
        let start = (y * row + cx * 4) as usize;
        for &[a, b, c, _] in data[start..start + (cw * 4) as usize].as_chunks::<4>().0 {
            rgba.extend_from_slice(&if bgra { [c, b, a, 255] } else { [a, b, c, 255] });
        }
    }
    drop(data);
    buffer.unmap();
    Ok(([cw, ch], rgba))
}

/// Writes RGBA pixels as a PNG, making its folder.
pub fn write_png(path: &str, [cw, ch]: [u32; 2], rgba: &[u8]) -> Result<(), String> {
    if let Some(dir) = std::path::Path::new(path).parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let file = std::fs::File::create(path).map_err(|e| format!("{path}: {e}"))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), cw, ch);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    Ok(())
}

/// The model textures' GPU memory in MB and the streaming counts (`textures`).
pub fn textures(game: &Game) -> serde_json::Value {
    let Mode::InGame(g) = &game.mode else { return serde_json::Value::Null };
    let Some(t) = g.texture_stats() else { return serde_json::Value::Null };
    let mb = |b: u64| (b as f64 / 1e5).round() / 10.0;
    serde_json::json!({
        "loaded_mb": mb(t.loaded_bytes),
        "streamed_mb": mb(t.streamed_bytes),
        "total_mb": mb(t.loaded_bytes + t.streamed_bytes),
        "pending": t.pending,
        "loads": t.loads,
        "evictions": t.evictions,
    })
}

/// `frametimes`: draws `frames` frames offscreen back to back, each waited for on the GPU,
/// and returns the mean and max wall milliseconds (hitches: texture uploads, `tex_cache`).
pub fn frametimes(game: &mut Game, frames: u32) -> Result<serde_json::Value, String> {
    let g = game.graphics.as_ref().ok_or("no GPU: start the driver with --drive-gpu")?;
    let device = g.gpu.device.clone();
    let texture = target(&device, g.gpu.config.format, g.gpu.config.width, g.gpu.config.height);
    let view = texture.create_view(&Default::default());
    let mut ms: Vec<f64> = Vec::with_capacity(frames as usize);
    for _ in 0..frames.max(1) {
        let start = std::time::Instant::now();
        game.draw_frame(&view);
        device
            .poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(10)) })
            .map_err(|e| e.to_string())?;
        ms.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let round = |v: f64| (v * 100.0).round() / 100.0;
    let mean = ms.iter().sum::<f64>() / ms.len() as f64;
    let max = ms.iter().copied().fold(0.0, f64::max);
    let worst = ms.iter().position(|&v| v == max).unwrap_or(0);
    let mut sorted = ms.clone();
    sorted.sort_by(f64::total_cmp);
    Ok(serde_json::json!({
        "frames": ms.len(),
        "mean_ms": round(mean),
        "median_ms": round(sorted[sorted.len() / 2]),
        "max_ms": round(max),
        "max_at": worst,
        "over_33ms": ms.iter().filter(|&&v| v > 33.3).count(),
        "textures": textures(game),
    }))
}

/// `gputime`: draws `frames` frames offscreen with the GPU timer on and returns the mean
/// milliseconds per segment (docs/ground.md), or why the device cannot time.
pub fn gputime(game: &mut Game, frames: u32) -> Result<serde_json::Value, String> {
    let g = game.graphics.as_ref().ok_or("no GPU: start the driver with --drive-gpu")?;
    let (device, queue) = (g.gpu.device.clone(), g.gpu.queue.clone());
    if let Err(why) = acr_render::gpu_timer::supported(&device) {
        return Ok(serde_json::json!({ "supported": false, "reason": why }));
    }
    let texture = target(&device, g.gpu.config.format, g.gpu.config.width, g.gpu.config.height);
    let view = texture.create_view(&Default::default());
    let mut sums: Vec<(&'static str, f64)> = Vec::new();
    let frames = frames.max(1);
    for _ in 0..frames {
        acr_render::gpu_timer::begin(&device, &queue)?;
        {
            acr_sim::prof_scope!("frame.render_cpu");
            game.draw_frame(&view);
        }
        for (label, ms) in acr_render::gpu_timer::finish(&device)? {
            match sums.iter_mut().find(|(l, _)| *l == label) {
                Some(s) => s.1 += ms,
                None => sums.push((label, ms)),
            }
        }
    }
    let n = f64::from(frames);
    let total: f64 = sums.iter().map(|(_, ms)| ms).sum::<f64>() / n;
    let ms: serde_json::Map<String, serde_json::Value> =
        sums.iter().map(|(l, s)| ((*l).to_owned(), serde_json::json!((s / n * 1000.0).round() / 1000.0))).collect();
    Ok(
        serde_json::json!({ "supported": true, "frames": frames, "ms": ms, "total_ms": (total * 1000.0).round() / 1000.0 }),
    )
}
