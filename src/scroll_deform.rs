//! Paint-only viewport deformation. Captures and warps the already painted list,
//! preserving logical scroll geometry and the fixed outer viewport.
use gpui_wgpu::{BackdropContext, WgpuBackdrop, WgpuBackdropEffect, wgpu};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug)]
pub struct ScrollDeformation {
    /// Positive compresses along the vertical bounce axis; negative stretches.
    pub along_axis: f32,
    /// Positive expands perpendicular to the bounce; negative narrows.
    pub cross_axis: f32,
    /// Maximum fractional scale change in either axis, below 1.
    pub max_deformation: f32,
    /// Fraction of ordinary bounce translation to retain.
    pub translation: f32,
    /// Curve exponent applied to displacement / viewport height.
    pub exponent: f32,
    /// Pin the edge opposite the pull; false scales around the center.
    pub edge_anchor: bool,
}
impl Default for ScrollDeformation {
    fn default() -> Self {
        Self {
            along_axis: 1.8,
            cross_axis: 0.25,
            max_deformation: 0.4,
            translation: 0.,
            exponent: 0.8,
            edge_anchor: true,
        }
    }
}
impl ScrollDeformation {
    pub fn scales(self, offset: f32, height: f32) -> [f32; 2] {
        let amount = (offset.abs() / height.max(1.)).min(1.).powf(self.exponent);
        [
            (1. + self.cross_axis * amount)
                .clamp(1. - self.max_deformation, 1. + self.max_deformation),
            (1. - self.along_axis * amount)
                .clamp(1. - self.max_deformation, 1. + self.max_deformation),
        ]
    }
}
#[derive(Default)]
pub struct DeformationRenderer(Mutex<Option<Gpu>>);

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    bounds: [f32; 4],
    scale_anchor: [f32; 4],
    motion: [f32; 4],
    background: [f32; 4],
}
struct Gpu {
    device: wgpu::Device,
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    capture: wgpu::RenderPipeline,
    warp: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    crop: Option<Crop>,
    source: Option<(wgpu::TextureView, wgpu::BindGroup)>,
}
struct Crop {
    dimensions: [u32; 2],
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind: wgpu::BindGroup,
}
impl Gpu {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scroll deformation layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scroll deformation pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scroll deformation shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scroll_deform.wgsl").into()),
        });
        let pipeline = |entry: &'static str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let capture = pipeline("capture");
        let warp = pipeline("warp");
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("scroll deformation sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scroll deformation uniform"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            device: device.clone(),
            format,
            layout,
            capture,
            warp,
            sampler,
            uniform,
            crop: None,
            source: None,
        }
    }
    fn bind(&self, view: &wgpu::TextureView) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scroll deformation bind"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }
}
struct Draw {
    renderer: Arc<DeformationRenderer>,
    params: ScrollDeformation,
    offset: f32,
    logical_height: f32,
    pixel_scale: f32,
    background: [f32; 4],
}
impl DeformationRenderer {
    pub fn paint(
        self: &Arc<Self>,
        window: &mut gpui::Window,
        bounds: gpui::Bounds<gpui::Pixels>,
        params: ScrollDeformation,
        offset: f32,
    ) {
        if offset == 0. {
            return;
        }
        window.paint_custom_backdrop(
            bounds,
            WgpuBackdropEffect(Arc::new(Draw {
                renderer: self.clone(),
                params,
                offset,
                logical_height: bounds.size.height.as_f32(),
                pixel_scale: window.scale_factor(),
                // Lab viewport's exposed paper color. Premultiplied, opaque.
                background: [227. / 255., 236. / 255., 230. / 255., 1.],
            }))
            .into_gpui(),
        );
    }
}
impl WgpuBackdrop for Draw {
    fn paint(&self, c: BackdropContext<'_>) {
        let b = c.filter.bounds;
        let m = c.filter.content_mask.bounds;
        let x = b.origin.x.0.max(m.origin.x.0).max(0.).ceil() as u32;
        let y = b.origin.y.0.max(m.origin.y.0).max(0.).ceil() as u32;
        let right = (b.origin.x.0 + b.size.width.0)
            .min(m.origin.x.0 + m.size.width.0)
            .min(c.viewport[0] as f32)
            .floor()
            .max(0.) as u32;
        let bottom = (b.origin.y.0 + b.size.height.0)
            .min(m.origin.y.0 + m.size.height.0)
            .min(c.viewport[1] as f32)
            .floor()
            .max(0.) as u32;
        if right <= x || bottom <= y {
            return;
        }
        let mut lock = self.renderer.0.lock().unwrap();
        if lock
            .as_ref()
            .is_none_or(|g| g.device != *c.device || g.format != c.format)
        {
            *lock = Some(Gpu::new(c.device, c.format));
        }
        let g = lock.as_mut().unwrap();
        let dimensions = [
            b.size.width.0.ceil().max(1.) as u32,
            b.size.height.0.ceil().max(1.) as u32,
        ];
        if g.crop
            .as_ref()
            .is_none_or(|crop| crop.dimensions != dimensions)
        {
            let texture = c.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("scroll deformation crop"),
                size: wgpu::Extent3d {
                    width: dimensions[0],
                    height: dimensions[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: c.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let bind = g.bind(&view);
            g.crop = Some(Crop {
                dimensions,
                _texture: texture,
                view,
                bind,
            });
        }
        if g.source.as_ref().is_none_or(|(view, _)| view != c.scene) {
            g.source = Some((c.scene.clone(), g.bind(c.scene)));
        }
        let scales = self.params.scales(self.offset, self.logical_height);
        c.queue.write_buffer(
            &g.uniform,
            0,
            bytemuck::bytes_of(&Uniform {
                bounds: [b.origin.x.0, b.origin.y.0, b.size.width.0, b.size.height.0],
                scale_anchor: [
                    scales[0],
                    scales[1],
                    0.5,
                    if self.params.edge_anchor {
                        if self.offset > 0. { 1. } else { 0. }
                    } else {
                        0.5
                    },
                ],
                motion: [
                    self.offset * self.params.translation * self.pixel_scale,
                    c.filter.opacity,
                    c.viewport[0] as f32,
                    c.viewport[1] as f32,
                ],
                background: self.background,
            }),
        );
        let crop = g.crop.as_ref().unwrap();
        {
            let mut pass = c.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("capture scroll viewport"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &crop.view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&g.capture);
            pass.set_bind_group(0, &g.source.as_ref().unwrap().1, &[]);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = c.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("deform scroll viewport"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: c.scene,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_scissor_rect(x, y, right - x, bottom - y);
            pass.set_pipeline(&g.warp);
            pass.set_bind_group(0, &crop.bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
