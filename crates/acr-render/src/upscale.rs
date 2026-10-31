//! Render scale (#440): the scene is drawn into an offscreen target smaller than the window
//! (`render_scale=100|85|75|50`) and stretched over the swapchain by [`Upscaler::blit`], linear
//! (nearest at 50 %, where a linear stretch looks blurred and a pixel-doubling grid is cleaner).
//! The whole frame is scaled, the interface included: the game lays it out for the smaller
//! size, so the HUD loses sharpness with the world.

/// `render_scale=100|85|75|50`: the share of the window's size the scene is drawn at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderScale {
    #[default]
    P100,
    P85,
    P75,
    P50,
}

impl RenderScale {
    pub fn name(self) -> &'static str {
        match self {
            RenderScale::P100 => "100",
            RenderScale::P85 => "85",
            RenderScale::P75 => "75",
            RenderScale::P50 => "50",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [RenderScale::P100, RenderScale::P85, RenderScale::P75, RenderScale::P50]
            .into_iter()
            .find(|s| s.name() == value)
    }

    /// The scale as a factor (1.0 for 100 %).
    pub fn factor(self) -> f32 {
        match self {
            RenderScale::P100 => 1.0,
            RenderScale::P85 => 0.85,
            RenderScale::P75 => 0.75,
            RenderScale::P50 => 0.5,
        }
    }

    /// The upscale filter: nearest at 50 %, linear otherwise.
    pub fn filter(self) -> wgpu::FilterMode {
        if self == RenderScale::P50 { wgpu::FilterMode::Nearest } else { wgpu::FilterMode::Linear }
    }
}

struct Target {
    size: [u32; 2],
    view: wgpu::TextureView,
    linear: wgpu::BindGroup,
    nearest: wgpu::BindGroup,
}

/// The offscreen scene target and the pipeline that stretches it over the swapchain.
pub struct Upscaler {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    samplers: [wgpu::Sampler; 2],
    format: wgpu::TextureFormat,
    target: Option<Target>,
}

impl Upscaler {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("upscale.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("upscale"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("upscale"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("upscale"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = |filter| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("upscale"),
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            })
        };
        let samplers = [sampler(wgpu::FilterMode::Linear), sampler(wgpu::FilterMode::Nearest)];
        Self { pipeline, layout, samplers, format, target: None }
    }

    /// The offscreen view to draw the scene into, `size` px, kept between frames.
    pub fn target(&mut self, device: &wgpu::Device, size: [u32; 2]) -> wgpu::TextureView {
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("scene"),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let group = |sampler: &wgpu::Sampler| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("upscale"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                    ],
                })
            };
            let (linear, nearest) = (group(&self.samplers[0]), group(&self.samplers[1]));
            self.target = Some(Target { size, view, linear, nearest });
        }
        self.target.as_ref().expect("just made").view.clone()
    }

    /// Stretches the last [`Upscaler::target`] over `dst`, the swapchain's view.
    pub fn blit(&self, encoder: &mut wgpu::CommandEncoder, dst: &wgpu::TextureView, filter: wgpu::FilterMode) {
        let Some(target) = &self.target else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("upscale"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        let group = if filter == wgpu::FilterMode::Nearest { &target.nearest } else { &target.linear };
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for s in [RenderScale::P100, RenderScale::P85, RenderScale::P75, RenderScale::P50] {
            assert_eq!(RenderScale::parse(s.name()), Some(s));
        }
        assert_eq!(RenderScale::parse("60"), None);
    }

    #[test]
    fn default_is_100() {
        // Below 100 % sprites keep their pixel size, so the view shows less of the world: an
        // opt-in until the camera compensates (#440).
        assert_eq!(RenderScale::default(), RenderScale::P100);
    }

    #[test]
    fn nearest_only_at_half() {
        assert_eq!(RenderScale::P50.filter(), wgpu::FilterMode::Nearest);
        assert_eq!(RenderScale::P75.filter(), wgpu::FilterMode::Linear);
        assert_eq!(RenderScale::P100.filter(), wgpu::FilterMode::Linear);
    }
}
