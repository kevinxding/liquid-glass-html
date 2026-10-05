//! Fixed-bounds contour animation: full scene construction + synchronized frame.
use super::*;
fn scene(
    scale: f32,
    engine: &Arc<GlassRenderer>,
    phase: f32,
    moving: bool,
    pop: Option<f32>,
) -> Scene {
    let full = bounds(0., 0., 1240., 860., scale);
    let mut s = Scene::default();
    s.push_layer(full);
    s.insert_primitive(Quad {
        bounds: full,
        content_mask: ContentMask {
            bounds: full,
            ..Default::default()
        },
        background: solid_background(rgb(0x101522)),
        ..Default::default()
    });
    for i in 0..300 {
        let x = (i % 30) as f32 * 42.;
        let y = (i / 30) as f32 * 85. + (phase + i as f32).sin() * 28.;
        s.insert_primitive(Quad {
            bounds: bounds(x, y, 30., 65., scale),
            content_mask: ContentMask {
                bounds: full,
                ..Default::default()
            },
            background: solid_background(rgb(
                [0x35e3b1, 0xa889ff, 0xff689d, 0xffc65c, 0x41cfff][i % 5]
            )),
            ..Default::default()
        });
    }
    s.pop_layer();
    for i in 0..9 {
        if i == 8 && pop == Some(-1.) {
            continue;
        }
        let mut p = GlassParams {
            dynamic_shape: true,
            ..Default::default()
        };
        if moving {
            p.shape_scale = [
                0.6 + 0.28 * (phase + i as f32).sin(),
                0.7 + 0.2 * (phase * 1.3 + i as f32).cos(),
            ];
        }
        if i == 8
            && let Some(progress) = pop
        {
            p = market_motion::materialise(p, progress);
        }
        s.insert_primitive(BackdropFilter {
            bounds: bounds(
                65. + (i % 3) as f32 * 380.,
                80. + (i / 3) as f32 * 235.,
                300.,
                150.,
                scale,
            ),
            content_mask: ContentMask {
                bounds: full,
                ..Default::default()
            },
            opacity: 1.,
            custom: Some(
                GlassDraw {
                    renderer: engine.clone(),
                    id: i as u64,
                    params: p,
                    shape: Shape::Capsule,
                    scale,
                    contour: None,
                }
                .effect(),
            ),
            ..Default::default()
        });
    }
    s.finish();
    s
}
fn verify_contours(renderer: &mut WgpuHeadlessRenderer) -> anyhow::Result<()> {
    for scale in [1., 2.] {
        let engine = Arc::new(GlassRenderer::default());
        for (case, [w, h]) in [[44., 60.], [176., 38.], [64., 64.]]
            .into_iter()
            .enumerate()
        {
            let make = |dynamic: bool| {
                let full = bounds(0., 0., 320., 180., scale);
                let mut scene = Scene::default();
                scene.push_layer(full);
                scene.insert_primitive(Quad {
                    bounds: full,
                    content_mask: ContentMask {
                        bounds: full,
                        ..Default::default()
                    },
                    background: solid_background(rgb(0x111111)),
                    ..Default::default()
                });
                scene.pop_layer();
                let b = if dynamic {
                    bounds(50., 58., 220., 64., scale)
                } else {
                    bounds(50. + (220. - w) * 0.5, 58. + (64. - h) * 0.5, w, h, scale)
                };
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
                            id: case as u64 * 2 + u64::from(dynamic),
                            params: GlassParams {
                                dynamic_shape: dynamic,
                                shape_scale: if dynamic {
                                    [w / 220., h / 64.]
                                } else {
                                    [1., 1.]
                                },
                                light: 0.,
                                surface: Some([0.95; 3]),
                                reflection: ReflectionParams::disabled(),
                                ..Default::default()
                            },
                            shape: Shape::Capsule,
                            scale,
                            contour: None,
                        }
                        .effect(),
                    ),
                    ..Default::default()
                });
                scene.finish();
                scene
            };
            let extent = size(
                DevicePixels((320. * scale) as i32),
                DevicePixels((180. * scale) as i32),
            );
            let a = renderer.render_scene_to_image(&make(true), extent)?;
            let b = renderer.render_scene_to_image(&make(false), extent)?;
            let max = a
                .as_raw()
                .iter()
                .zip(b.as_raw())
                .map(|(a, b)| (*a as i16 - *b as i16).abs())
                .max()
                .unwrap();
            assert!(
                max <= 36,
                "regenerated Lisse contour must match fresh static shape at {w}x{h}, {scale}x: {max}/255"
            );
        }
    }
    println!(
        "DYNAMIC_CONTOUR_OK: recalculated narrow, wide and square Lisse contours match fresh static silhouettes at 1x/2x"
    );
    Ok(())
}
pub fn run(renderer: &mut WgpuHeadlessRenderer) -> anyhow::Result<()> {
    verify_contours(renderer)?;
    let mut report = String::from(
        "Dynamic contour benchmark: 300 moving color quads + 9 simultaneous regenerated Lisse contour morphs. 120 samples; scene construction + CPU submission + GPU completion; no readback in timings.\n",
    );
    for scale in [1., 2.] {
        let engine = Arc::new(GlassRenderer::default());
        let extent = size(
            DevicePixels((1240. * scale) as i32),
            DevicePixels((860. * scale) as i32),
        );
        for i in 0..12 {
            renderer.render_scene_and_wait(
                &scene(scale, &engine, i as f32 * 0.1, true, None),
                extent,
            )?;
        }
        let mut spring = market_motion::Spring::new(0.);
        spring.toggle();
        let before = engine.stats();
        let mut times = Vec::new();
        for i in 0..120 {
            if i % 40 == 0 {
                spring.toggle();
            }
            spring.step(1. / 60., false);
            let start = Instant::now();
            renderer.render_scene_and_wait(
                &scene(scale, &engine, i as f32 * 0.05 + spring.value, true, None),
                extent,
            )?;
            times.push(start.elapsed().as_secs_f64() * 1000.);
        }
        let after = engine.stats();
        assert_eq!(
            before.atlas_uploads, after.atlas_uploads,
            "GPU morphs must not upload CPU atlases"
        );
        assert_eq!(
            before.slot_allocations, after.slot_allocations,
            "morphs must not reallocate surfaces"
        );
        assert!(
            after.dynamic_atlas_builds - before.dynamic_atlas_builds >= 9 * 119,
            "real contours must be regenerated each frame"
        );
        times.sort_by(f64::total_cmp);
        let line = format!(
            "{scale}x: median {:.3} ms, p95 {:.3} ms; 0 CPU atlas uploads, 0 slot allocations; GPU contour SDF regenerated every frame\n",
            times[60], times[114]
        );
        print!("{line}");
        report.push_str(&line);
        let a = renderer.render_scene_to_image(&scene(scale, &engine, 0., true, None), extent)?;
        let b = renderer.render_scene_to_image(&scene(scale, &engine, 2., true, None), extent)?;
        assert_ne!(a.as_raw(), b.as_raw());
        b.save(format!("artifacts/prism-morph-{scale}x.png"))?;
        // A zero-progress material must reproduce the unfiltered source. Compare
        // against a transparent zero-optics panel at the same location.
        let mut zero = scene(scale, &engine, 0., false, Some(0.));
        let empty = renderer.render_scene_to_image(&zero, extent)?;
        let absent =
            renderer.render_scene_to_image(&scene(scale, &engine, 0., false, Some(-1.)), extent)?;
        let error = empty
            .as_raw()
            .iter()
            .zip(absent.as_raw())
            .map(|(a, b)| (*a as i16 - *b as i16).abs())
            .max()
            .unwrap();
        assert!(
            error <= 1,
            "zero-progress optics must reproduce unfiltered backdrop, max error {error}"
        );
        zero = scene(scale, &engine, 0., false, Some(1.));
        let full = renderer.render_scene_to_image(&zero, extent)?;
        assert_ne!(
            empty.as_raw(),
            full.as_raw(),
            "materialisation must change optics"
        );
        // Exercise blur topology transitions too, without conflating them with
        // the zero-allocation contour animation guarantee.
        let start = Instant::now();
        let before = engine.stats();
        for i in 0..60 {
            renderer.render_scene_and_wait(
                &scene(
                    scale,
                    &engine,
                    0.,
                    false,
                    Some((i as f32 / 59. * std::f32::consts::PI).sin()),
                ),
                extent,
            )?;
        }
        let line = format!(
            "{scale}x materialise open/close: {:.3} ms/frame average, {} slot allocations, {} atlas uploads\n",
            start.elapsed().as_secs_f64() * 1000. / 60.,
            engine.stats().slot_allocations - before.slot_allocations,
            engine.stats().atlas_uploads - before.atlas_uploads
        );
        print!("{line}");
        report.push_str(&line);
    }
    std::fs::write("artifacts/prism-benchmark.txt", report)?;
    Ok(())
}
