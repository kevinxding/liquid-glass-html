//! GPU timestamp benchmark for the glass passes only. Run the full-scene benchmark as well.
use gpui::{BackdropFilter, Bounds, ContentMask, ScaledPixels, point, size};
use gpui_glass::{
    glass::{GlassDraw, GlassParams, GlassRenderer},
    shape::Shape,
};
use gpui_wgpu::{BackdropContext, WgpuBackdrop, WgpuContext, WgpuDeviceRequirements, wgpu};
use std::sync::{Arc, mpsc};
// Use pass-boundary markers and reject adapters which return unusable intervals.
fn marker(
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    query: &wgpu::QuerySet,
    index: u32,
) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("glass-timestamp-marker"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        timestamp_writes: Some(wgpu::RenderPassTimestampWrites {
            query_set: query,
            beginning_of_pass_write_index: if index == 0 { Some(0) } else { None },
            end_of_pass_write_index: if index == 1 { Some(1) } else { None },
        }),
        ..Default::default()
    });
}
fn main() -> anyhow::Result<()> {
    env_logger::init();
    std::fs::create_dir_all("artifacts")?;
    std::fs::write(
        "artifacts/gpu-cost.txt",
        "GPU timestamp measurement has not completed successfully. Use benchmark.txt for synchronized frame timings.\n",
    )?;
    let features = wgpu::Features::TIMESTAMP_QUERY;
    let context = WgpuContext::new_headless(Some(&WgpuDeviceRequirements {
        features,
        limits: None,
    }))?;
    let device = &context.device;
    let queue = &context.queue;
    let mut report = format!(
        "GPU timestamps, {} ({:?})\nFive lenses; includes crop capture, optional blur/fog, refraction, dispersion, AA and light.\nExcludes GPUI scene generation, final presentation blit, CPU encoding, and GPU readback.\n16 warmup + 80 measured samples per configuration; independent resource caches.\n",
        context.adapter.get_info().name,
        context.adapter.get_info().backend
    );
    print!("{report}");
    let query = device.create_query_set(&wgpu::QuerySetDescriptor {
        label: Some("glass-gpu-time"),
        ty: wgpu::QueryType::Timestamp,
        count: 2,
    });
    let resolve = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("timestamp-resolve"),
        size: 256,
        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("timestamp-readback"),
        size: 16,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    for scale in [1., 2.] {
        let viewport = [(1240. * scale) as u32, (860. * scale) as u32];
        let format = wgpu::TextureFormat::Bgra8Unorm;
        let desc = wgpu::TextureDescriptor {
            label: Some("benchmark-scene"),
            size: wgpu::Extent3d {
                width: viewport[0],
                height: viewport[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        };
        let original = device.create_texture(&desc);
        let scene = device.create_texture(&desc);
        let view = scene.create_view(&Default::default());
        let mut pixels = vec![0; (viewport[0] * viewport[1] * 4) as usize];
        for (i, p) in pixels.chunks_exact_mut(4).enumerate() {
            let x = i as u32 % viewport[0];
            let y = i as u32 / viewport[0];
            p.copy_from_slice(&[(x % 255) as u8, (y % 255) as u8, ((x ^ y) % 255) as u8, 255]);
        }
        queue.write_texture(
            original.as_image_copy(),
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(viewport[0] * 4),
                rows_per_image: Some(viewport[1]),
            },
            desc.size,
        );
        let default = GlassParams::default();
        let configs = [
            (
                "clear",
                GlassParams {
                    blur: 0.,
                    dispersion: 0.,
                    ..default
                },
            ),
            ("default", default),
            (
                "heavy frost",
                GlassParams {
                    blur: 20.,
                    distortion: 40.,
                    dispersion: 3.,
                    ..default
                },
            ),
            (
                "frost + fog",
                GlassParams {
                    blur: 6.,
                    fog: 12.,
                    ..default
                },
            ),
        ];
        let samples: Vec<_> = configs
            .iter()
            .map(|(_, p)| {
                let renderer = Arc::new(GlassRenderer::default());
                [
                    (30., 25., 152., 50.),
                    (450., 25., 124., 50.),
                    (600., 25., 96., 50.),
                    (740., 310., 170., 110.),
                    (44., 769., 848., 66.),
                ]
                .into_iter()
                .enumerate()
                .map(|(id, (x, y, w, h))| {
                    let filter = BackdropFilter {
                        bounds: Bounds::new(
                            point(ScaledPixels(x * scale), ScaledPixels(y * scale)),
                            size(ScaledPixels(w * scale), ScaledPixels(h * scale)),
                        ),
                        content_mask: ContentMask {
                            bounds: Bounds::new(
                                point(ScaledPixels(0.), ScaledPixels(0.)),
                                size(
                                    ScaledPixels(viewport[0] as f32),
                                    ScaledPixels(viewport[1] as f32),
                                ),
                            ),
                            ..Default::default()
                        },
                        opacity: 1.,
                        ..Default::default()
                    };
                    let draw = GlassDraw {
                        renderer: renderer.clone(),
                        id: id as u64,
                        params: *p,
                        shape: if id >= 3 {
                            Shape::Rounded
                        } else {
                            Shape::Capsule
                        },
                        scale,
                        contour: None,
                    };
                    (filter, draw)
                })
                .collect::<Vec<_>>()
            })
            .collect();
        let mut times = vec![Vec::new(); configs.len()];
        for iteration in 0..96 {
            for offset in 0..configs.len() {
                let i = (iteration + offset) % configs.len();
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("glass-timestamp-frame"),
                });
                encoder.copy_texture_to_texture(
                    original.as_image_copy(),
                    scene.as_image_copy(),
                    desc.size,
                );
                marker(&mut encoder, &view, &query, 0);
                for (filter, draw) in &samples[i] {
                    draw.paint(BackdropContext {
                        device,
                        queue,
                        encoder: &mut encoder,
                        scene: &view,
                        format,
                        viewport,
                        filter,
                    });
                }
                marker(&mut encoder, &view, &query, 1);
                encoder.resolve_query_set(&query, 0..2, &resolve, 0);
                encoder.copy_buffer_to_buffer(&resolve, 0, &readback, 0, 16);
                let submission = queue.submit([encoder.finish()]);
                let (send, recv) = mpsc::sync_channel(1);
                readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                    let _ = send.send(r);
                });
                device.poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: None,
                })?;
                recv.recv()??;
                let mapped = readback.slice(..).get_mapped_range();
                let stamps: &[u64] = bytemuck::cast_slice(&mapped);
                anyhow::ensure!(
                    stamps[1] > stamps[0],
                    "GPU timestamps returned no usable interval; use the synchronized complete-frame benchmark on this adapter"
                );
                let ms = stamps[1].saturating_sub(stamps[0]) as f64
                    * queue.get_timestamp_period() as f64
                    / 1e6;
                drop(mapped);
                readback.unmap();
                if iteration >= 16 {
                    times[i].push(ms);
                }
            }
        }
        for (i, (name, _)) in configs.iter().enumerate() {
            times[i].sort_by(f64::total_cmp);
            let median = times[i][40];
            let p95 = times[i][75];
            let line = format!("{scale}x {name}: median {median:.3} ms | p95 {p95:.3} ms\n");
            print!("{line}");
            report.push_str(&line);
        }
    }
    std::fs::write("artifacts/gpu-cost.txt", report)?;
    Ok(())
}
