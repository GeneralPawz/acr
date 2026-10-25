//! GPU-compressed model textures (#297): KTX2 files in Basis Universal (`KHR_texture_basisu`,
//! ETC1S or UASTC, written by `assets/blender/lib/acr_assets/ktx2_pack.py`), transcoded at
//! load by the pure-Rust `basisu` crate (native and wasm) into what the device samples
//! directly: BC7 where it has BC (desktops; WebGL2 with `EXT_texture_compression_bptc`), else
//! ASTC 4x4, else ETC2 RGBA, else plain RGBA8. The file's own mips are kept.

use std::sync::atomic::{AtomicU8, Ordering};

/// What KTX2 textures transcode to on this device ([`Target::of`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Bc7,
    Astc,
    Etc2,
    Rgba,
}

impl Target {
    /// The best block format `features` can sample.
    pub fn of(features: wgpu::Features) -> Self {
        if features.contains(wgpu::Features::TEXTURE_COMPRESSION_BC) {
            Target::Bc7
        } else if features.contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC) {
            Target::Astc
        } else if features.contains(wgpu::Features::TEXTURE_COMPRESSION_ETC2) {
            Target::Etc2
        } else {
            Target::Rgba
        }
    }

    fn basis(self) -> basisu::TargetFormat {
        match self {
            Target::Bc7 => basisu::TargetFormat::Bc7Rgba,
            Target::Astc => basisu::TargetFormat::Astc4x4Rgba,
            Target::Etc2 => basisu::TargetFormat::Etc2Rgba,
            Target::Rgba => basisu::TargetFormat::Rgba32,
        }
    }

    pub fn format(self, srgb: bool) -> wgpu::TextureFormat {
        use wgpu::TextureFormat as F;
        match (self, srgb) {
            (Target::Bc7, true) => F::Bc7RgbaUnormSrgb,
            (Target::Bc7, false) => F::Bc7RgbaUnorm,
            (Target::Astc, s) => F::Astc {
                block: wgpu::AstcBlock::B4x4,
                channel: if s { wgpu::AstcChannel::UnormSrgb } else { wgpu::AstcChannel::Unorm },
            },
            (Target::Etc2, true) => F::Etc2Rgba8UnormSrgb,
            (Target::Etc2, false) => F::Etc2Rgba8Unorm,
            (Target::Rgba, true) => F::Rgba8UnormSrgb,
            (Target::Rgba, false) => F::Rgba8Unorm,
        }
    }

    fn code(self) -> u8 {
        self as u8
    }

    fn from_code(c: u8) -> Self {
        [Target::Bc7, Target::Astc, Target::Etc2, Target::Rgba].get(usize::from(c)).copied().unwrap_or(Target::Rgba)
    }
}

/// The device's target, set by the renderer before it uploads ([`set_target`]); RGBA8 until.
static TARGET: AtomicU8 = AtomicU8::new(3);

pub fn set_target(target: Target) {
    TARGET.store(target.code(), Ordering::Relaxed);
}

pub fn target() -> Target {
    Target::from_code(TARGET.load(Ordering::Relaxed))
}

const MAGIC: &[u8] = &[0xAB, b'K', b'T', b'X', b' ', b'2', b'0', 0xBB, 0x0D, 0x0A, 0x1A, 0x0A];

/// A KTX2 file (by its signature).
pub fn is_ktx2(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

/// A KTX2 file's base size from its header, without transcoding it.
pub fn size(bytes: &[u8]) -> Option<(u32, u32)> {
    if !is_ktx2(bytes) {
        return None;
    }
    let le = |at: usize| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    Some((le(20)?, le(24)?))
}

/// A transcoded texture: its levels (finest first), each as tightly packed blocks (or RGBA8
/// rows) of [`Target::format`].
#[derive(Debug, Clone, PartialEq)]
pub struct Blocks {
    pub width: u32,
    pub height: u32,
    pub target: Target,
    pub levels: Vec<Vec<u8>>,
}

impl Blocks {
    /// GPU bytes of the kept levels.
    pub fn bytes(&self) -> u64 {
        self.levels.iter().map(|l| l.len() as u64).sum()
    }

    /// A texture with these levels.
    pub fn upload(&self, device: &wgpu::Device, queue: &wgpu::Queue, label: &str, srgb: bool) -> wgpu::Texture {
        let format = self.target.format(srgb);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
            mip_level_count: self.levels.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let (bw, bh) = format.block_dimensions();
        let block_bytes = format.block_copy_size(None).unwrap_or(4);
        for (level, data) in self.levels.iter().enumerate() {
            let size = texture.size().mip_level_size(level as u32, wgpu::TextureDimension::D2);
            let physical = size.physical_size(format);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(physical.width / bw * block_bytes),
                    rows_per_image: Some(physical.height / bh),
                },
                physical,
            );
        }
        texture
    }
}

/// Transcodes a KTX2 file for `target`: every level when `mips`, else the finest only. Block
/// targets need a base size of whole 4x4 blocks; other sizes go to RGBA8.
pub fn transcode(bytes: &[u8], target: Target, mips: bool) -> Result<Blocks, String> {
    let tex = basisu::Transcoder::new(bytes).map_err(|e| format!("KTX2: {e:?}"))?;
    let (width, height) = tex.base_dimensions();
    let target = if target != Target::Rgba && (width % 4 != 0 || height % 4 != 0) { Target::Rgba } else { target };
    let count = if mips { tex.level_count().max(1) } else { 1 };
    let levels = (0..count)
        .map(|l| tex.transcode(l, target.basis(), basisu::DecodeFlags::HIGH_QUALITY))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("KTX2 transcode to {target:?}: {e:?}"))?;
    Ok(Blocks { width, height, target, levels })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_follow_features() {
        use wgpu::Features as F;
        assert_eq!(Target::of(F::TEXTURE_COMPRESSION_BC | F::TEXTURE_COMPRESSION_ETC2), Target::Bc7);
        assert_eq!(Target::of(F::TEXTURE_COMPRESSION_ETC2 | F::TEXTURE_COMPRESSION_ASTC), Target::Astc);
        assert_eq!(Target::of(F::TEXTURE_COMPRESSION_ETC2), Target::Etc2);
        assert_eq!(Target::of(F::empty()), Target::Rgba);
        for t in [Target::Bc7, Target::Astc, Target::Etc2, Target::Rgba] {
            assert_eq!(Target::from_code(t.code()), t);
        }
    }

    #[test]
    fn header_size_without_transcoding() {
        let mut head = MAGIC.to_vec();
        head.extend_from_slice(&[0; 8]);
        head.extend_from_slice(&512u32.to_le_bytes());
        head.extend_from_slice(&256u32.to_le_bytes());
        assert_eq!(size(&head), Some((512, 256)));
        assert_eq!(size(b"\x89PNG"), None);
        assert!(transcode(&head, Target::Bc7, true).is_err());
    }
}
