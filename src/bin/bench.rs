//! Reproducible complete-scene benchmark. Each sample waits for GPU completion (no readback).
//! Reports submission + driver + GPU wall time, NOT pure GPU timestamp duration.
use gpui::{
    BackdropFilter, Bounds, ContentMask, Corners, DevicePixels, PlatformHeadlessRenderer, Quad,
    ScaledPixels, Scene, point, rgb, size, solid_background,
};
use gpui_glass::{
    glass::{GlassDraw, GlassParams, GlassRenderer, LightParams, ReflectionParams},
    shape::Shape,
};
use gpui_wgpu::{WgpuContext, WgpuHeadlessRenderer};
use std::{sync::Arc, time::Instant};
#[path = "bench_support/dynamic.rs"]
mod dynamic;
#[path = "market/motion.rs"]
mod market_motion;
#[path = "bench_support/optics.rs"]
mod optics;
#[path = "bench_support/performance.rs"]
mod performance;
#[path = "bench_support/stress.rs"]
mod stress;
fn bounds(x: f32, y: f32, w: f32, h: f32, s: f32) -> Bounds<ScaledPixels> {
    Bounds::new(
        point(ScaledPixels(x * s), ScaledPixels(y * s)),
        size(ScaledPixels(w * s), ScaledPixels(h * s)),
    )
}
fn scene(
    scale: f32,
    renderer: &Arc<GlassRenderer>,
    params: Option<GlassParams>,
    shape: Shape,
    clip: bool,
) -> Scene {
    scene_with_sidebar(scale, renderer, params, shape, clip, false)
}
fn scene_with_sidebar(
    scale: f32,
    renderer: &Arc<GlassRenderer>,
    params: Option<GlassParams>,
    shape: Shape,
    clip: bool,
    sidebar: bool,
) -> Scene {
    let mut scene = Scene::default();
    let full = bounds(0., 0., 1240., 860., scale);
    scene.push_layer(full);
    scene.insert_primitive(Quad {
        bounds: full,
        content_mask: ContentMask {
            bounds: full,
            ..Default::default()
        },
        background: solid_background(rgb(0xf5f6f0)),
        ..Default::default()
    });
    for row in 0..30 {
        for col in 0..40 {
            let color =
                [0xe7856e, 0x386a58, 0xf0c45f, 0x172d35, 0x9dbed0, 0xfaf7ed][(row + col) % 6];
            let b = bounds(col as f32 * 31., row as f32 * 29., 26., 22., scale);
            scene.insert_primitive(Quad {
                bounds: b,
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
    // Close-packed high-contrast strokes expose refraction and chromatic separation.
    for i in 0..90 {
        let b = bounds(i as f32 * 13.8, 400., 4., 300., scale);
        scene.insert_primitive(Quad {
            bounds: b,
            content_mask: ContentMask {
                bounds: full,
                ..Default::default()
            },
            background: solid_background(rgb(0x101610)),
            ..Default::default()
        });
    }
    scene.pop_layer();
    if let Some(params) = params {
        for (id, (x, y, w, h)) in [
            (30., 25., 152., 50.),
            (450., 25., 124., 50.),
            (600., 25., 96., 50.),
            (740., 310., 170., 110.),
            (44., 769., 848., 66.),
        ]
        .into_iter()
        .enumerate()
        {
            let b = bounds(x, y, w, h, scale);
            scene.insert_primitive(BackdropFilter {
                bounds: b,
                content_mask: ContentMask {
                    bounds: if clip {
                        bounds(x, y, w * 0.5, h, scale)
                    } else {
                        full
                    },
                    ..Default::default()
                },
                opacity: 1.,
                custom: Some(
                    GlassDraw {
                        contour: None,
                        renderer: renderer.clone(),
                        id: id as u64,
                        params,
                        shape: if id == 3 || id == 4 {
                            shape
                        } else {
                            Shape::Capsule
                        },
                        scale,
                    }
                    .effect(),
                ),
                ..Default::default()
            });
        }
        if sidebar {
            scene.insert_primitive(BackdropFilter {
                bounds: bounds(936., 0., 304., 860., scale),
                content_mask: ContentMask {
                    bounds: full,
                    ..Default::default()
                },
                opacity: 1.,
                custom: Some(
                    GlassDraw {
                        renderer: renderer.clone(),
                        id: 6,
                        params: params.reflection_only([250. / 255., 251. / 255., 247. / 255.]),
                        shape: Shape::Rectangle,
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
fn ms(
    renderer: &mut WgpuHeadlessRenderer,
    scene: &Scene,
    extent: gpui::Size<DevicePixels>,
) -> anyhow::Result<f64> {
    let t = Instant::now();
    renderer.render_scene_and_wait(scene, extent)?;
    Ok(t.elapsed().as_secs_f64() * 1000.)
}
fn percentile(a: &mut [f64], p: f64) -> f64 {
    a.sort_by(f64::total_cmp);
    a[((a.len() - 1) as f64 * p).round() as usize]
}
fn verify_splay(renderer: &mut WgpuHeadlessRenderer) -> anyhow::Result<()> {
    let full = bounds(0., 0., 1000., 400., 1.);
    let extent = size(DevicePixels(1000), DevicePixels(400));
    let make = |splay: f32, frost: f32, fog: f32, distortion: f32| {
        let mut scene = Scene::default();
        scene.push_layer(full);
        for row in 0..13 {
            for col in 0..32 {
                let b = bounds(col as f32 * 32., row as f32 * 32., 32., 32., 1.);
                scene.insert_primitive(Quad {
                    bounds: b,
                    content_mask: ContentMask {
                        bounds: b,
                        ..Default::default()
                    },
                    background: solid_background(rgb(if (row + col) % 2 == 0 {
                        0xffffff
                    } else {
                        0xd0d0d0
                    })),
                    ..Default::default()
                });
            }
        }
        scene.pop_layer();
        scene.insert_primitive(BackdropFilter {
            bounds: bounds(150., 88., 700., 224., 1.),
            content_mask: ContentMask {
                bounds: full,
                ..Default::default()
            },
            opacity: 1.,
            custom: Some(
                GlassDraw {
                    renderer: Arc::new(GlassRenderer::default()),
                    id: 90,
                    params: GlassParams {
                        blur: frost,
                        fog,
                        opacity: 0.,
                        saturation: 1.,
                        light: 0.,
                        reflection: ReflectionParams::disabled(),
                        dispersion: 0.,
                        edge: 56.,
                        distortion,
                        splay,
                        ..Default::default()
                    },
                    shape: Shape::Capsule,
                    scale: 1.,
                    contour: None,
                }
                .effect(),
            ),
            ..Default::default()
        });
        scene.finish();
        scene
    };
    let a = renderer.render_scene_to_image(&make(0., 0., 0., 40.), extent)?;
    let b = renderer.render_scene_to_image(&make(1., 0., 0., 40.), extent)?;
    assert_ne!(
        a.get_pixel(16, 16),
        a.get_pixel(48, 16),
        "checkerboard background must have contrast"
    );
    a.save("artifacts/splay-0.png")?;
    b.save("artifacts/splay-1.png")?;
    for y in 155..245 {
        for x in 300..700 {
            assert_eq!(
                a.get_pixel(x, y),
                b.get_pixel(x, y),
                "splay must preserve the flat center at ({x},{y})"
            );
        }
    }
    let changed = (95..135)
        .flat_map(|y| (300..700).map(move |x| (x, y)))
        .filter(|&(x, y)| a.get_pixel(x, y) != b.get_pixel(x, y))
        .count();
    assert!(changed > 1000, "splay must fan the rays on a straight edge");
    println!("SPLAY_CHECK_OK: flat center identical, rim rays fan outward");
    let frost = renderer.render_scene_to_image(&make(0., 6., 0., 0.), extent)?;
    frost.save("artifacts/frost-uniform.png")?;
    for y in 100..110 {
        for x in 350..650 {
            let edge = frost.get_pixel(x, y);
            let center = frost.get_pixel(x, y + 64);
            for c in 0..3 {
                assert!(
                    (edge[c] as i16 - center[c] as i16).abs() <= 2,
                    "frost must be uniform before refraction: ({x},{y}) {edge:?} {center:?}"
                );
            }
        }
    }
    let fog = renderer.render_scene_to_image(&make(0., 0., 6., 0.), extent)?;
    fog.save("artifacts/fog-masked.png")?;
    assert!(
        frost.as_raw() != fog.as_raw(),
        "fog has a mask, frost does not"
    );
    println!("FROST_CHECK_OK: equal pre-refraction blur at center and edge, separate fog mask");
    Ok(())
}
fn verify_capsules(renderer: &mut WgpuHeadlessRenderer) -> anyhow::Result<()> {
    // Left to right: circular, default smoothing, full Lisse smoothing.
    // Top: opaque silhouette. Bottom: the same outlines with live glass optics.
    let scale = 2.;
    let full = bounds(0., 0., 940., 340., scale);
    let mut scene = Scene::default();
    scene.push_layer(full);
    for row in 0..17 {
        for col in 0..47 {
            let b = bounds(col as f32 * 20., row as f32 * 20., 20., 20., scale);
            scene.insert_primitive(Quad {
                bounds: b,
                content_mask: ContentMask {
                    bounds: b,
                    ..Default::default()
                },
                background: solid_background(rgb(if (row + col) % 2 == 0 {
                    0x66796c
                } else {
                    0xadc1ae
                })),
                ..Default::default()
            });
        }
    }
    scene.pop_layer();
    let engine = Arc::new(GlassRenderer::default());
    for (i, smoothing) in [0., 0.65, 1.].into_iter().enumerate() {
        for row in 0..2 {
            let params = if row == 0 {
                GlassParams {
                    smoothing,
                    opacity: 1.,
                    light: 0.,
                    reflection: ReflectionParams::disabled(),
                    blur: 0.,
                    dispersion: 0.,
                    distortion: 0.,
                    ..Default::default()
                }
            } else {
                GlassParams {
                    smoothing,
                    reflection: ReflectionParams::disabled(),
                    ..Default::default()
                }
            };
            scene.insert_primitive(BackdropFilter {
                bounds: bounds(
                    20. + i as f32 * 310.,
                    30. + row as f32 * 170.,
                    280.,
                    100.,
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
                        id: (i * 2 + row) as u64,
                        params,
                        shape: Shape::Capsule,
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
    let image =
        renderer.render_scene_to_image(&scene, size(DevicePixels(1880), DevicePixels(680)))?;
    image.save("artifacts/capsule-comparison.png")?;
    // Compare silhouettes after removing the column offset. The center stays
    // unchanged, while smoothing must visibly change the end's outline.
    let changed = (60..260)
        .flat_map(|y| (40..240).map(move |x| (x, y)))
        .filter(|&(x, y)| image.get_pixel(x, y) != image.get_pixel(x + 1240, y))
        .count();
    assert!(
        changed > 500,
        "capsule smoothing must change its silhouette"
    );
    assert_eq!(image.get_pixel(320, 160), image.get_pixel(1560, 160));
    println!(
        "CAPSULE_CHECK_OK: Lisse silhouettes and glass rendered at 2x; saved capsule-comparison.png"
    );
    Ok(())
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    std::fs::create_dir_all("artifacts")?;
    if std::env::args().any(|a| a == "--performance") {
        return performance::run(&mut WgpuHeadlessRenderer::new_with_timestamps()?);
    }
    if std::env::args().any(|a| a == "--dynamic") {
        return dynamic::run(&mut WgpuHeadlessRenderer::new()?);
    }
    if std::env::args().any(|a| a == "--stress") {
        let mut renderer = WgpuHeadlessRenderer::new()?;
        let mut report = String::new();
        optics::verify_customisation(&mut renderer)?;
        stress::run(&mut renderer, &mut report)?;
        std::fs::write("artifacts/benchmark-stress.txt", report)?;
        return Ok(());
    }
    if std::env::args().any(|a| a == "--verify-customisation") {
        return optics::verify_customisation(&mut WgpuHeadlessRenderer::new()?);
    }
    if std::env::args().any(|a| a == "--verify-wide-bands") {
        return optics::verify_wide_bands(&mut WgpuHeadlessRenderer::new()?);
    }
    if std::env::args().any(|a| a == "--verify-lighting") {
        return optics::verify_lighting(&mut WgpuHeadlessRenderer::new()?);
    }
    if std::env::args().any(|a| a == "--verify-reflection") {
        return optics::verify(&mut WgpuHeadlessRenderer::new()?);
    }
    if std::env::args().any(|a| a == "--verify-native-damage") {
        return optics::verify_native_damage(&mut WgpuHeadlessRenderer::new()?);
    }
    if std::env::args().any(|a| a == "--verify-native-bleed") {
        return optics::verify_native_bleed(&mut WgpuHeadlessRenderer::new()?);
    }
    if std::env::args().any(|a| a == "--verify") {
        let mut renderer = WgpuHeadlessRenderer::new()?;
        verify_splay(&mut renderer)?;
        verify_capsules(&mut renderer)?;
        optics::verify_customisation(&mut renderer)?;
        optics::verify_wide_bands(&mut renderer)?;
        optics::verify_lighting(&mut renderer)?;
        return optics::verify(&mut renderer);
    }
    let info = WgpuContext::new_headless(None)?.adapter.get_info();
    println!(
        "Adapter: {} ({:?} / {:?})",
        info.name, info.backend, info.device_type
    );
    println!(
        "Complete scene: 1,290 quads + 5 glass surfaces. Warm cache. Serial submit + GPU completion wall time."
    );
    let engine = Arc::new(GlassRenderer::default());
    let mut renderer = WgpuHeadlessRenderer::new()?;
    let mut report = format!(
        "Adapter: {} ({:?})\nProfile: {}\nSynchronized complete-frame wall time (not GPU timestamps); 1290 quads, 5 lenses; 20 warmup, 120 samples each.\n",
        info.name,
        info.backend,
        if cfg!(debug_assertions) {
            "dev, dependencies optimized"
        } else {
            "release"
        }
    );
    for scale in [1., 2.] {
        let extent = size(
            DevicePixels((1240. * scale) as i32),
            DevicePixels((860. * scale) as i32),
        );
        let default = GlassParams::default();
        let cases = [
            ("disabled", None),
            (
                "clear",
                Some(GlassParams {
                    blur: 0.,
                    dispersion: 0.,
                    ..default
                }),
            ),
            ("default", Some(default)),
            ("default + sidebar", Some(default)),
            (
                "light/reflection off",
                Some(GlassParams {
                    light: 0.,
                    lighting: LightParams::disabled(),
                    reflection: ReflectionParams::disabled(),
                    ..default
                }),
            ),
            (
                "lights only",
                Some(GlassParams {
                    reflection: ReflectionParams::disabled(),
                    ..default
                }),
            ),
            (
                "edge reflection only",
                Some(GlassParams {
                    light: 0.,
                    reflection: ReflectionParams {
                        diffuse_intensity: 0.,
                        ..default.reflection
                    },
                    ..default
                }),
            ),
            (
                "diffuse reflection only",
                Some(GlassParams {
                    light: 0.,
                    reflection: ReflectionParams {
                        edge_intensity: 0.,
                        ..default.reflection
                    },
                    ..default
                }),
            ),
            (
                "reflection max radius",
                Some(GlassParams {
                    reflection: ReflectionParams {
                        sampling_radius: 240.,
                        ..default.reflection
                    },
                    ..default
                }),
            ),
            (
                "heavy frost",
                Some(GlassParams {
                    blur: 20.,
                    dispersion: 3.,
                    distortion: 40.,
                    ..default
                }),
            ),
        ];
        let scenes: Vec<_> = cases
            .iter()
            .map(|(name, p)| {
                scene_with_sidebar(
                    scale,
                    &Arc::new(GlassRenderer::default()),
                    *p,
                    Shape::Rounded,
                    false,
                    *name == "default + sidebar",
                )
            })
            .collect();
        for _ in 0..20 {
            for s in &scenes {
                ms(&mut renderer, s, extent)?;
            }
        }
        let mut samples = vec![Vec::new(); cases.len()];
        for i in 0..120 {
            for j in 0..cases.len() {
                let j = (j + i) % cases.len();
                samples[j].push(ms(&mut renderer, &scenes[j], extent)?);
            }
        }
        let baseline = percentile(&mut samples[0], 0.5);
        for (i, (name, _)) in cases.iter().enumerate() {
            let median = percentile(&mut samples[i], 0.5);
            let p95 = percentile(&mut samples[i], 0.95);
            let line = format!(
                "{}x {}: median {:.3} ms | p95 {:.3} ms | delta median {:+.3} ms\n",
                scale,
                name,
                median,
                p95,
                median - baseline
            );
            print!("{line}");
            report.push_str(&line);
        }
        if scale == 1. {
            let baseline = renderer.render_scene_to_image(&scenes[0], extent)?;
            baseline.save("artifacts/baseline.png")?;
            let clear = renderer.render_scene_to_image(&scenes[1], extent)?;
            let glass = renderer.render_scene_to_image(&scenes[2], extent)?;
            glass.save("artifacts/glass.png")?;
            assert_eq!(
                baseline.get_pixel(5, 200),
                glass.get_pixel(5, 200),
                "outside pixels must remain identical"
            );
            assert!(
                baseline.as_raw() != glass.as_raw(),
                "glass must alter backdrop"
            );
            assert!(
                clear.as_raw() != glass.as_raw(),
                "optical parameters must alter output"
            );
            let clipped = renderer.render_scene_to_image(
                &scene(scale, &engine, Some(default), Shape::Rounded, true),
                extent,
            )?;
            assert_eq!(
                baseline.get_pixel(850, 365),
                clipped.get_pixel(850, 365),
                "content mask must clip custom shader"
            );
            for shape in Shape::ALL {
                renderer
                    .render_scene_to_image(
                        &scene(scale, &engine, Some(default), shape, false),
                        extent,
                    )?
                    .save(format!("artifacts/shape-{}.png", shape.name()))?;
            }
            println!(
                "VISUAL_CHECKS_OK: backdrop changed, parameter response, exterior preservation, clipping, all shape renders"
            );
            report.push_str("Visual assertions: passed. Shape PNGs saved in artifacts/.\n");
        }
    }
    verify_splay(&mut renderer)?;
    verify_capsules(&mut renderer)?;
    optics::verify_lighting(&mut renderer)?;
    optics::verify(&mut renderer)?;
    optics::verify_customisation(&mut renderer)?;
    stress::run(&mut renderer, &mut report)?;
    std::fs::write("artifacts/benchmark.txt", report)?;
    Ok(())
}
