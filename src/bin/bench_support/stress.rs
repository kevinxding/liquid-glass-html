//! Twenty-four independent materials exercise capture, reflection filtering and reuse.
use super::*;

fn make(
    scale: f32,
    engine: &Arc<GlassRenderer>,
    params: Option<GlassParams>,
    phase: usize,
    surface_count: usize,
) -> Scene {
    let full = bounds(0., 0., 1240., 860., scale);
    let mut scene = Scene::default();
    scene.push_layer(full);
    scene.insert_primitive(Quad {
        bounds: full,
        content_mask: ContentMask {
            bounds: full,
            ..Default::default()
        },
        background: solid_background(rgb(0xeeeeeb)),
        ..Default::default()
    });
    for row in 0..30 {
        for col in 0..40 {
            let color =
                [0xe7856e, 0x386a58, 0xf0c45f, 0x172d35, 0x9dbed0, 0xfaf7ed][(row + col) % 6];
            scene.insert_primitive(Quad {
                bounds: bounds(
                    col as f32 * 31. + (phase as f32 * 3.).sin() * 9.,
                    row as f32 * 29.,
                    26.,
                    22.,
                    scale,
                ),
                content_mask: ContentMask {
                    bounds: full,
                    ..Default::default()
                },
                background: solid_background(rgb(color)),
                corner_radii: Corners::all(ScaledPixels(3. * scale)),
                ..Default::default()
            });
        }
    }
    scene.pop_layer();
    if let Some(params) = params {
        for id in 0..surface_count {
            let columns = if surface_count > 24 { 8 } else { 6 };
            let rows = surface_count.div_ceil(columns);
            let cell_w = 1240. / columns as f32;
            let cell_h = 860. / rows as f32;
            let b = bounds(
                12. + (id % columns) as f32 * cell_w,
                12. + (id / columns) as f32 * cell_h,
                cell_w - 24.,
                cell_h - 32.,
                scale,
            );
            scene.insert_primitive(BackdropFilter {
                bounds: b,
                content_mask: ContentMask {
                    bounds: full,
                    ..Default::default()
                },
                opacity: 1.,
                custom: Some(
                    GlassDraw {
                        renderer: engine.clone(),
                        id: id as u64,
                        params,
                        shape: if id % 3 == 0 {
                            Shape::Capsule
                        } else {
                            Shape::Rounded
                        },
                        scale,
                        contour: None,
                    }
                    .effect(),
                ),
                ..Default::default()
            });
        }
    }
    scene.finish();
    scene
}

pub fn run(renderer: &mut WgpuHeadlessRenderer, report: &mut String) -> anyhow::Result<()> {
    let header = "\nStress: 1,200 detail quads +24 surfaces (64 in count stress), stable IDs;12 warmup,80 samples/case. Interleaved baseline. Scene preparation excluded; CPU submission+GPU completion included.\n";
    print!("{header}");
    report.push_str(header);
    let default = GlassParams::default();
    let isolated = GlassParams {
        light: 0.,
        lighting: LightParams::disabled(),
        blur: 0.,
        fog: 0.,
        fog_opacity: 0.,
        dispersion: 0.,
        distortion: 0.,
        ..GlassParams::default().reflection_only([0.2, 0.22, 0.21])
    };
    let cases = [
        (
            "optics only",
            GlassParams {
                light: 0.,
                reflection: ReflectionParams::disabled(),
                ..default
            },
            false,
            false,
        ),
        ("default", default, false, false),
        (
            "wide glow / small capture",
            GlassParams {
                reflection: ReflectionParams {
                    diffuse_radius: 1500.,
                    sampling_radius: 48.,
                    ..default.reflection
                },
                ..isolated
            },
            false,
            false,
        ),
        (
            "edge only",
            GlassParams {
                reflection: ReflectionParams {
                    diffuse_intensity: 0.,
                    ..default.reflection
                },
                ..isolated
            },
            false,
            false,
        ),
        (
            "diffuse only",
            GlassParams {
                reflection: ReflectionParams {
                    edge_intensity: 0.,
                    ..default.reflection
                },
                ..isolated
            },
            false,
            false,
        ),
        (
            "max reflection radius",
            GlassParams {
                reflection: ReflectionParams {
                    sampling_radius: 240.,
                    ..default.reflection
                },
                ..isolated
            },
            false,
            false,
        ),
        (
            "diffuse + shadow",
            GlassParams {
                light: 1.,
                lighting: LightParams {
                    outer_shadow_opacity: 0.25,
                    outer_shadow_radius: 14.,
                    ..LightParams::disabled()
                },
                reflection: ReflectionParams {
                    edge_intensity: 0.,
                    diffuse_intensity: 0.4,
                    diffuse_radius: 80.,
                    sampling_radius: 240.,
                    shadow_intensity: 0.8,
                    ..default.reflection
                },
                ..isolated
            },
            false,
            false,
        ),
        ("48 reflective tiles", isolated, true, false),
        ("moving exterior objects", isolated, true, false),
        ("radius changes", isolated, true, true),
        (
            "64 edge surfaces",
            GlassParams {
                reflection: ReflectionParams {
                    diffuse_intensity: 0.,
                    ..default.reflection
                },
                ..isolated
            },
            false,
            false,
        ),
    ];
    for scale in [1., 2.] {
        let extent = size(
            DevicePixels((1240. * scale) as i32),
            DevicePixels((860. * scale) as i32),
        );
        let baseline = make(scale, &Arc::new(GlassRenderer::default()), None, 0, 0);
        for (name, params, moving, varying_radius) in cases {
            if std::env::var("GLASS_BENCH_CASE").is_ok_and(|filter| !name.contains(&filter)) {
                continue;
            }
            let engine = Arc::new(GlassRenderer::default());
            let surface_count = match name {
                "64 edge surfaces" => 64,
                "48 reflective tiles" => 48,
                _ => 24,
            };
            let frames = if moving { 12 } else { 1 };
            let scenes: Vec<_> = (0..frames)
                .map(|phase| {
                    let p = if varying_radius {
                        GlassParams {
                            reflection: ReflectionParams {
                                sampling_radius: [16., 96., 240.][phase % 3],
                                ..params.reflection
                            },
                            ..params
                        }
                    } else {
                        params
                    };
                    make(scale, &engine, Some(p), phase, surface_count)
                })
                .collect();
            let cold = ms(renderer, &scenes[0], extent)?;
            for i in 1..12 {
                ms(renderer, &scenes[i % frames], extent)?;
            }
            let warmed_stats = engine.stats();
            let mut base_samples = Vec::with_capacity(80);
            let mut effect_samples = Vec::with_capacity(80);
            for i in 0..80 {
                // Alternating order avoids attributing monotonic scheduling/thermal drift
                // entirely to the effect. Every sample explicitly waits for completion.
                if i % 2 == 0 {
                    base_samples.push(ms(renderer, &baseline, extent)?);
                }
                effect_samples.push(ms(renderer, &scenes[i % frames], extent)?);
                if i % 2 != 0 {
                    base_samples.push(ms(renderer, &baseline, extent)?);
                }
            }
            let final_stats = engine.stats();
            anyhow::ensure!(
                final_stats.cached_surfaces == surface_count,
                "{name}: all stable surface IDs must remain cached"
            );
            anyhow::ensure!(
                final_stats.atlas_uploads == warmed_stats.atlas_uploads,
                "{name}: animated exterior objects or reflection radius must not regenerate shape atlases"
            );
            anyhow::ensure!(
                final_stats.slot_allocations == warmed_stats.slot_allocations,
                "{name}: frames and warmed radius changes must reuse GPU surface resources"
            );
            let base = percentile(&mut base_samples, 0.5);
            let median = percentile(&mut effect_samples, 0.5);
            let p95 = percentile(&mut effect_samples, 0.95);
            let maximum = effect_samples.iter().copied().fold(0., f64::max);
            let line = format!(
                "STRESS {scale}x {name}: median {median:.3} ms | p95 {p95:.3} ms | max {maximum:.3} ms | baseline {base:.3} ms | delta median {:+.3} ms | first frame {cold:.3} ms | timed slot allocations {} | timed atlas uploads {}\n",
                median - base,
                final_stats.slot_allocations - warmed_stats.slot_allocations,
                final_stats.atlas_uploads - warmed_stats.atlas_uploads
            );
            print!("{line}");
            report.push_str(&line);
            anyhow::ensure!(
                effect_samples.iter().all(|v| v.is_finite()),
                "non-finite stress timing"
            );
        }
    }
    Ok(())
}
