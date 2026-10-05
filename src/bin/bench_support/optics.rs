//! Pixel regressions for independent material controls and outside-only reflections.
use super::*;
use image::RgbaImage;

// Optical fixtures isolate the layer being measured from the demo's tone preset.
fn neutral_reflections() -> ReflectionParams {
    ReflectionParams {
        native_diffuse: false,
        diffuse_size_scaling: 0.,
        edge_blend: gpui_glass::LightBlend::Normal,
        diffuse_blend: gpui_glass::LightBlend::Normal,
        shadow_blend: gpui_glass::LightBlend::Normal,
        edge_brightness_min: 0.,
        edge_brightness_max: 1.,
        brightness_min: 0.,
        brightness_max: 1.,
        dark_bias: 0.,
        ..ReflectionParams::disabled()
    }
}

const WIDTH: i32 = 960;
const HEIGHT: i32 = 500;
fn quad(scene: &mut Scene, full: Bounds<ScaledPixels>, b: Bounds<ScaledPixels>, color: u32) {
    scene.insert_primitive(Quad {
        bounds: b,
        content_mask: ContentMask {
            bounds: full,
            ..Default::default()
        },
        background: solid_background(rgb(color)),
        ..Default::default()
    });
}
fn surface(
    scene: &mut Scene,
    full: Bounds<ScaledPixels>,
    b: Bounds<ScaledPixels>,
    engine: &Arc<GlassRenderer>,
    id: u64,
    params: GlassParams,
    shape: Shape,
) {
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
                id,
                params,
                shape,
                scale: 1.,
                contour: None,
            }
            .effect(),
        ),
        ..Default::default()
    });
}
fn panel(
    params: GlassParams,
    interior: bool,
    exterior: bool,
    checker: bool,
    shape: Shape,
) -> Scene {
    let mut scene = Scene::default();
    let full = bounds(0., 0., WIDTH as f32, HEIGHT as f32, 1.);
    scene.push_layer(full);
    quad(&mut scene, full, full, 0xf1f1f1);
    if checker {
        for row in 0..32 {
            for col in 0..60 {
                if (row + col) % 2 == 0 {
                    quad(
                        &mut scene,
                        full,
                        bounds(col as f32 * 16., row as f32 * 16., 16., 16., 1.),
                        0x383838,
                    );
                }
            }
        }
    }
    if interior {
        quad(
            &mut scene,
            full,
            bounds(300., 200., 260., 100., 1.),
            0xff00ff,
        );
    }
    if exterior {
        // Identical tangent frequency at every outside distance: reductions should
        // attenuate this signal progressively as the sample moves away from the rim.
        for x in (180..800).step_by(12) {
            quad(
                &mut scene,
                full,
                bounds(x as f32, 0., 6., 100., 1.),
                0xf00035,
            );
        }
    }
    scene.pop_layer();
    surface(
        &mut scene,
        full,
        bounds(160., 100., 640., 300., 1.),
        &Arc::new(GlassRenderer::default()),
        900,
        params,
        shape,
    );
    scene.finish();
    scene
}
fn image(
    renderer: &mut WgpuHeadlessRenderer,
    params: GlassParams,
    interior: bool,
    exterior: bool,
    checker: bool,
) -> anyhow::Result<RgbaImage> {
    renderer.render_scene_to_image(
        &panel(params, interior, exterior, checker, Shape::Rounded),
        size(DevicePixels(WIDTH), DevicePixels(HEIGHT)),
    )
}
fn changed(a: &RgbaImage, b: &RgbaImage) -> usize {
    a.pixels().zip(b.pixels()).filter(|(a, b)| a != b).count()
}
fn tangent_contrast(image: &RgbaImage, y: u32, scale: u32) -> f64 {
    let signal: Vec<_> = (320 * scale..640 * scale)
        .map(|x| {
            let p = image.get_pixel(x, y);
            (f64::from(p[0]) - f64::from(p[1])).max(0.)
        })
        .collect();
    let mean = signal.iter().sum::<f64>() / signal.len() as f64;
    let variance = signal.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / signal.len() as f64;
    variance.sqrt() / mean.max(1.)
}

pub fn verify(renderer: &mut WgpuHeadlessRenderer) -> anyhow::Result<()> {
    let off = GlassParams {
        blur: 0.,
        fog: 0.,
        fog_opacity: 0.,
        opacity: 0.,
        saturation: 1.,
        distortion: 0.,
        dispersion: 0.,
        splay: 0.,
        light: 0.,
        lighting: LightParams::disabled(),
        reflection: neutral_reflections(),
        ..Default::default()
    };
    let clean = image(renderer, off, false, false, true)?;
    let zero_fog = image(
        renderer,
        GlassParams {
            fog: 30.,
            fog_edge: 80.,
            ..off
        },
        false,
        false,
        true,
    )?;
    assert_eq!(
        clean.as_raw(),
        zero_fog.as_raw(),
        "zero-opacity fog must be optically absent"
    );
    let fog = GlassParams {
        fog: 8.,
        fog_opacity: 0.8,
        fog_edge: 40.,
        ..off
    };
    let fog_a = image(
        renderer,
        GlassParams { edge: 4., ..fog },
        false,
        false,
        true,
    )?;
    let fog_b = image(
        renderer,
        GlassParams { edge: 60., ..fog },
        false,
        false,
        true,
    )?;
    assert_eq!(
        fog_a.as_raw(),
        fog_b.as_raw(),
        "fog mask must not depend on refraction edge width"
    );
    let fog_c = image(
        renderer,
        GlassParams {
            fog_edge: 4.,
            ..fog
        },
        false,
        false,
        true,
    )?;
    assert!(
        changed(&fog_a, &fog_c) > 1000,
        "fog width must independently change the mask"
    );
    fog_a.save("artifacts/fog-independent-width.png")?;
    println!("FOG_CONTROL_CHECK_OK: independent width, opacity-zero identical to disabled");

    let zero_light = image(
        renderer,
        GlassParams {
            light: 1.,
            lighting: LightParams {
                outline_width: 8.,
                sharp_width: 20.,
                soft_width: 40.,
                inner_shadow_radius: 40.,
                outer_shadow_radius: 30.,
                ..LightParams::disabled()
            },
            ..off
        },
        false,
        false,
        true,
    )?;
    assert_eq!(
        clean.as_raw(),
        zero_light.as_raw(),
        "zero-opacity light components must disappear regardless of width"
    );
    let master_off = image(
        renderer,
        GlassParams {
            lighting: LightParams::default(),
            ..off
        },
        false,
        false,
        true,
    )?;
    assert_eq!(
        clean.as_raw(),
        master_off.as_raw(),
        "master light zero must disable all light components"
    );
    for (name, lighting) in [
        (
            "outline",
            LightParams {
                outline_opacity: 0.7,
                ..LightParams::disabled()
            },
        ),
        (
            "sharp",
            LightParams {
                sharp_opacity: 0.7,
                ..LightParams::disabled()
            },
        ),
        (
            "soft",
            LightParams {
                soft_opacity: 0.7,
                ..LightParams::disabled()
            },
        ),
        (
            "inner-shadow",
            LightParams {
                inner_shadow_opacity: 0.7,
                ..LightParams::disabled()
            },
        ),
        (
            "outer-shadow",
            LightParams {
                outer_shadow_opacity: 0.7,
                ..LightParams::disabled()
            },
        ),
    ] {
        let enabled = image(
            renderer,
            GlassParams {
                light: 1.,
                lighting,
                ..off
            },
            false,
            false,
            true,
        )?;
        assert!(
            changed(&clean, &enabled) > 30,
            "{name} must visibly contribute when enabled"
        );
        enabled.save(format!("artifacts/light-{name}.png"))?;
    }
    println!(
        "LIGHT_CONTROL_CHECK_OK: five independent layers, zero opacities and zero master restore source"
    );

    let reflective = GlassParams {
        reflection: ReflectionParams {
            native_diffuse: false,
            edge_intensity: 1.,
            edge_width: 40.,
            diffuse_intensity: 0.,
            sampling_radius: 80.,
            sharpness: 1.,
            ..ReflectionParams::default()
        },
        ..GlassParams::default().reflection_only([0.18, 0.18, 0.18])
    };
    let empty = image(renderer, reflective, false, false, false)?;
    let inside = image(renderer, reflective, true, false, false)?;
    assert_eq!(
        empty.as_raw(),
        inside.as_raw(),
        "interior objects must never enter reflection, including blurred mips"
    );
    for shape in [Shape::Notched, Shape::Flower] {
        let empty = renderer.render_scene_to_image(
            &panel(reflective, false, false, false, shape),
            size(DevicePixels(WIDTH), DevicePixels(HEIGHT)),
        )?;
        let inside = renderer.render_scene_to_image(
            &panel(reflective, true, false, false, shape),
            size(DevicePixels(WIDTH), DevicePixels(HEIGHT)),
        )?;
        assert_eq!(
            empty.as_raw(),
            inside.as_raw(),
            "concave {shape:?} reflections must reject interior color before mip filtering"
        );
    }
    let outside = image(renderer, reflective, false, true, false)?;
    assert!(
        (101..135)
            .flat_map(|y| (320..640).map(move |x| (x, y)))
            .filter(|&(x, y)| empty.get_pixel(x, y) != outside.get_pixel(x, y))
            .count()
            > 1000,
        "outside stripes must contribute reflection"
    );
    let near = tangent_contrast(&outside, 104, 1);
    let far = tangent_contrast(&outside, 122, 1);
    assert!(
        near > far * 1.2 && near > 0.05,
        "nearby reflection must preserve more tangent contrast than distant reflection: near={near:.4}, far={far:.4}"
    );
    outside.save("artifacts/reflection-distance-filtering.png")?;
    let diffuse = GlassParams {
        reflection: ReflectionParams {
            native_diffuse: false,
            edge_intensity: 0.,
            diffuse_intensity: 0.8,
            diffuse_radius: 80.,
            ..reflective.reflection
        },
        ..reflective
    };
    let diffuse_empty = image(renderer, diffuse, false, false, false)?;
    let diffuse_inside = image(renderer, diffuse, true, false, false)?;
    assert_eq!(
        diffuse_empty.as_raw(),
        diffuse_inside.as_raw(),
        "diffuse reflection must also reject interior colors before filtering"
    );
    let diffuse_outside = image(renderer, diffuse, false, true, false)?;
    assert!(
        (108..155)
            .flat_map(|y| (320..640).map(move |x| (x, y)))
            .filter(|&(x, y)| diffuse_empty.get_pixel(x, y) != diffuse_outside.get_pixel(x, y))
            .count()
            > 1000,
        "diffuse reflection must affect the opaque interior"
    );
    diffuse_outside.save("artifacts/reflection-diffuse-outside-only.png")?;
    let no_reflection = GlassParams {
        reflection: neutral_reflections(),
        ..reflective
    };
    let dormant = GlassParams {
        reflection: ReflectionParams {
            native_diffuse: false,
            edge_width: 64.,
            diffuse_radius: 160.,
            sampling_radius: 240.,
            sharpness: 1.,
            ..neutral_reflections()
        },
        ..reflective
    };
    assert_eq!(
        image(renderer, no_reflection, false, true, false)?.as_raw(),
        image(renderer, dormant, false, true, false)?.as_raw(),
        "disabled reflections must ignore all radius/sharpness values"
    );
    println!(
        "REFLECTION_CHECK_OK: outside-only, zero intensity disabled; normalized tangent contrast near={near:.4}, far={far:.4}"
    );
    verify_retina(renderer, reflective)?;
    verify_resources(renderer, off)?;
    verify_live_source(renderer)?;
    reference_sheet(renderer)?;
    Ok(())
}

fn reference_sheet(renderer: &mut WgpuHeadlessRenderer) -> anyhow::Result<()> {
    let full = bounds(0., 0., 1200., 800., 1.);
    let mut scene = Scene::default();
    let engine = Arc::new(GlassRenderer::default());
    scene.push_layer(full);
    quad(&mut scene, full, full, 0xeef0ec);
    quad(&mut scene, full, bounds(0., 250., 560., 220., 1.), 0x99cbed);
    quad(&mut scene, full, bounds(600., 0., 600., 800., 1.), 0x262927);
    // Objects all lie outside the panel. Their reflections cross its left boundary.
    for (i, c) in [
        0xf62c4c, 0xfb8b36, 0xfad53f, 0x76e531, 0x22d1a8, 0x686bee, 0xd43b92,
    ]
    .into_iter()
    .enumerate()
    {
        quad(
            &mut scene,
            full,
            bounds(635., 80. + i as f32 * 90., 85., 62., 1.),
            c,
        );
    }
    scene.pop_layer();
    let lights = GlassParams {
        blur: 4.,
        opacity: 0.12,
        reflection: neutral_reflections(),
        ..Default::default()
    };
    for (id, b, shape) in [
        (1, bounds(50., 70., 450., 62., 1.), Shape::Capsule),
        (2, bounds(55., 300., 116., 116., 1.), Shape::Capsule),
        (3, bounds(220., 322., 275., 76., 1.), Shape::Capsule),
        (4, bounds(40., 555., 490., 155., 1.), Shape::Rounded),
    ] {
        surface(&mut scene, full, b, &engine, id, lights, shape);
    }
    surface(
        &mut scene,
        full,
        bounds(755., 45., 400., 700., 1.),
        &engine,
        5,
        GlassParams {
            reflection: ReflectionParams {
                native_diffuse: false,
                edge_intensity: 0.9,
                edge_width: 4.,
                diffuse_intensity: 0.45,
                diffuse_radius: 70.,
                sampling_radius: 150.,
                sharpness: 0.8,
                ..Default::default()
            },
            ..GlassParams::default().reflection_only([0.13, 0.14, 0.14])
        },
        Shape::Rectangle,
    );
    scene.finish();
    renderer
        .render_scene_to_image(&scene, size(DevicePixels(1200), DevicePixels(800)))?
        .save("artifacts/light-reflection-reference-sheet.png")?;
    println!(
        "REFERENCE_SHEET_OK: light-only shapes at left; opaque reflection-only panel at right"
    );
    Ok(())
}

fn verify_resources(renderer: &mut WgpuHeadlessRenderer, off: GlassParams) -> anyhow::Result<()> {
    let engine = Arc::new(GlassRenderer::default());
    let full = bounds(0., 0., 600., 300., 1.);
    let mut render = |params: GlassParams| -> anyhow::Result<()> {
        let mut scene = Scene::default();
        scene.push_layer(full);
        quad(&mut scene, full, full, 0xe0e0e0);
        scene.pop_layer();
        surface(
            &mut scene,
            full,
            bounds(80., 60., 440., 180., 1.),
            &engine,
            3000,
            params,
            Shape::Rounded,
        );
        scene.finish();
        renderer.render_scene_and_wait(&scene, size(DevicePixels(600), DevicePixels(300)))?;
        Ok(())
    };
    render(off)?;
    let clean = engine.stats();
    assert_eq!(clean.blur_passes, 0);
    assert_eq!(clean.reflection_passes, 0);
    render(GlassParams {
        fog: 32.,
        fog_opacity: 0.,
        fog_edge: 80.,
        reflection: ReflectionParams {
            native_diffuse: false,
            edge_width: 40.,
            diffuse_radius: 80.,
            sampling_radius: 240.,
            ..neutral_reflections()
        },
        ..off
    })?;
    let dormant = engine.stats();
    assert_eq!(
        clean.blur_passes, dormant.blur_passes,
        "zero-opacity fog must schedule no blur passes"
    );
    assert_eq!(
        clean.reflection_passes, dormant.reflection_passes,
        "zero-intensity reflection must schedule no reflection passes"
    );
    assert_eq!(
        clean.slot_allocations, dormant.slot_allocations,
        "disabled effect radius changes must not allocate textures"
    );
    render(off.reflection_only([0.2, 0.2, 0.2]))?;
    let solid = engine.stats();
    assert_eq!(
        dormant.backdrop_captures, solid.backdrop_captures,
        "an opaque surface with reflections disabled must not capture the backdrop"
    );
    assert_eq!(dormant.blur_passes, solid.blur_passes);
    assert_eq!(dormant.reflection_passes, solid.reflection_passes);
    println!(
        "RESOURCE_SKIP_CHECK_OK: zero-opacity fog/reflections have no passes or radius allocations; solid inactive panel skips backdrop capture"
    );
    Ok(())
}

fn verify_live_source(renderer: &mut WgpuHeadlessRenderer) -> anyhow::Result<()> {
    let engine = Arc::new(GlassRenderer::default());
    let full = bounds(0., 0., 600., 300., 1.);
    let material = GlassParams {
        reflection: ReflectionParams {
            native_diffuse: false,
            edge_intensity: 1.,
            edge_width: 12.,
            diffuse_intensity: 0.5,
            sampling_radius: 100.,
            ..Default::default()
        },
        ..GlassParams::default().reflection_only([0.15, 0.15, 0.15])
    };
    let make = |color| {
        let mut scene = Scene::default();
        scene.push_layer(full);
        quad(&mut scene, full, full, 0x383838);
        quad(&mut scene, full, bounds(12., 90., 60., 120., 1.), color);
        scene.pop_layer();
        surface(
            &mut scene,
            full,
            bounds(80., 60., 440., 180., 1.),
            &engine,
            4000,
            material,
            Shape::Rounded,
        );
        scene.finish();
        scene
    };
    let red = renderer
        .render_scene_to_image(&make(0xff0033), size(DevicePixels(600), DevicePixels(300)))?;
    let warmed = engine.stats();
    let blue = renderer
        .render_scene_to_image(&make(0x0033ff), size(DevicePixels(600), DevicePixels(300)))?;
    let updated = engine.stats();
    assert!(
        (130..170)
            .flat_map(|y| (82..104).map(move |x| (x, y)))
            .filter(|&(x, y)| red.get_pixel(x, y) != blue.get_pixel(x, y))
            .count()
            > 100,
        "reusing the surface must still capture changed exterior objects"
    );
    assert_eq!(warmed.slot_allocations, updated.slot_allocations);
    assert_eq!(warmed.atlas_uploads, updated.atlas_uploads);
    assert!(updated.reflection_passes > warmed.reflection_passes);
    println!(
        "LIVE_REFLECTION_CHECK_OK: cached resources reflect new exterior color without allocation or atlas upload"
    );
    Ok(())
}

fn verify_retina(
    renderer: &mut WgpuHeadlessRenderer,
    reflective: GlassParams,
) -> anyhow::Result<()> {
    let scale = 2.;
    let mut scene = Scene::default();
    let full = bounds(0., 0., WIDTH as f32, HEIGHT as f32, scale);
    scene.push_layer(full);
    quad(&mut scene, full, full, 0xf1f1f1);
    for x in (180..800).step_by(12) {
        quad(
            &mut scene,
            full,
            bounds(x as f32, 0., 6., 100., scale),
            0xf00035,
        );
    }
    scene.pop_layer();
    scene.insert_primitive(BackdropFilter {
        bounds: bounds(160., 100., 640., 300., scale),
        content_mask: ContentMask {
            bounds: full,
            ..Default::default()
        },
        opacity: 1.,
        custom: Some(
            GlassDraw {
                renderer: Arc::new(GlassRenderer::default()),
                id: 910,
                params: reflective,
                shape: Shape::Rounded,
                scale,
                contour: None,
            }
            .effect(),
        ),
        ..Default::default()
    });
    scene.finish();
    let result = renderer.render_scene_to_image(
        &scene,
        size(DevicePixels(WIDTH * 2), DevicePixels(HEIGHT * 2)),
    )?;
    let near = tangent_contrast(&result, 208, 2);
    let far = tangent_contrast(&result, 244, 2);
    assert!(
        near > far * 1.2 && near > 0.05,
        "2x reflection must progressively filter distance: near={near:.4},far={far:.4}"
    );
    result.save("artifacts/reflection-distance-filtering-2x.png")?;
    println!(
        "RETINA_REFLECTION_CHECK_OK: 2x normalized tangent contrast near={near:.4}, far={far:.4}"
    );
    Ok(())
}

/// Compare the same directional finish on light, dark, and blue surfaces.
/// Columns: Screen/Screen, SoftLight/SoftLight, Add/Add, chosen Screen/SoftLight.
pub fn verify_lighting(renderer: &mut WgpuHeadlessRenderer) -> anyhow::Result<()> {
    use gpui_glass::LightBlend;
    let full = bounds(0., 0., 1440., 600., 1.);
    let mut scene = Scene::default();
    let engine = Arc::new(GlassRenderer::default());
    let rows = [
        (0xececea, [0.96; 3]),
        (0x202020, [0.22; 3]),
        (0x95bfdf, [0.62, 0.79, 0.92]),
    ];
    scene.push_layer(full);
    for (row, (bg, _)) in rows.iter().enumerate() {
        quad(
            &mut scene,
            full,
            bounds(0., row as f32 * 200., 1440., 200., 1.),
            *bg,
        );
    }
    scene.pop_layer();
    for (row, (_, color)) in rows.iter().enumerate() {
        for (col, (sharp, soft)) in [
            (LightBlend::Screen, LightBlend::Screen),
            (LightBlend::SoftLight, LightBlend::SoftLight),
            (LightBlend::LinearDodge, LightBlend::LinearDodge),
            (LightBlend::Native27, LightBlend::Native27),
        ]
        .into_iter()
        .enumerate()
        {
            surface(
                &mut scene,
                full,
                bounds(
                    col as f32 * 360. + 20.,
                    row as f32 * 200. + 45.,
                    320.,
                    110.,
                    1.,
                ),
                &engine,
                (row * 4 + col) as u64,
                GlassParams {
                    surface: Some(*color),
                    reflection: neutral_reflections(),
                    lighting: LightParams {
                        sharp_blend: sharp,
                        soft_blend: soft,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Shape::Rounded,
            );
        }
    }
    scene.finish();
    renderer
        .render_scene_to_image(&scene, size(DevicePixels(1440), DevicePixels(600)))?
        .save("artifacts/lighting-blend-study.png")?;

    let off = GlassParams {
        surface: Some([0.22; 3]),
        reflection: neutral_reflections(),
        lighting: LightParams::disabled(),
        light: 1.,
        ..Default::default()
    };
    let clean = image(renderer, off, false, false, false)?;
    let outline = image(
        renderer,
        GlassParams {
            lighting: LightParams {
                outline_opacity: 0.8,
                outline_width: 2.,
                ..LightParams::disabled()
            },
            ..off
        },
        false,
        false,
        false,
    )?;
    assert!(
        outline.get_pixel(480, 99)[0] < clean.get_pixel(480, 99)[0],
        "outline must DARKEN the exterior"
    );
    for y in 100..400 {
        for x in 160..800 {
            for c in 0..3 {
                assert!(
                    outline.get_pixel(x, y)[c] <= clean.get_pixel(x, y)[c],
                    "isolated dark outline cannot brighten any pixel"
                );
            }
        }
    }
    let sharp = LightParams {
        sharp_opacity: 0.8,
        sharp_angle: 0.,
        sharp_inset: 3.,
        sharp_focus: 2.,
        opposite: 0.,
        ..LightParams::disabled()
    };
    let top = image(
        renderer,
        GlassParams {
            lighting: sharp,
            ..off
        },
        false,
        false,
        false,
    )?;
    let bottom = image(
        renderer,
        GlassParams {
            lighting: LightParams {
                sharp_angle: 180.,
                ..sharp
            },
            ..off
        },
        false,
        false,
        false,
    )?;
    assert!(
        top.get_pixel(480, 103)[0] > bottom.get_pixel(480, 103)[0],
        "angle must move glint from top to bottom"
    );
    assert!(bottom.get_pixel(480, 396)[0] > top.get_pixel(480, 396)[0]);
    let inset = image(
        renderer,
        GlassParams {
            lighting: LightParams {
                sharp_inset: 12.,
                ..sharp
            },
            ..off
        },
        false,
        false,
        false,
    )?;
    assert!(
        inset.get_pixel(480, 112)[0] > top.get_pixel(480, 112)[0],
        "inset must move band into surface"
    );
    assert!(top.get_pixel(480, 103)[0] > inset.get_pixel(480, 103)[0]);
    for mode in [
        LightBlend::Screen,
        LightBlend::SoftLight,
        LightBlend::LinearDodge,
    ] {
        let dormant = image(
            renderer,
            GlassParams {
                lighting: LightParams {
                    sharp_blend: mode,
                    soft_blend: mode,
                    sharp_angle: 130.,
                    sharp_inset: 12.,
                    ..LightParams::disabled()
                },
                ..off
            },
            false,
            false,
            false,
        )?;
        assert_eq!(
            dormant.as_raw(),
            clean.as_raw(),
            "zero opacity must skip every blend/position variant"
        );
    }
    let shadow = LightParams {
        inner_shadow_opacity: 0.6,
        inner_shadow_angle: 0.,
        inner_shadow_inset: 4.,
        ..LightParams::disabled()
    };
    let shadow_top = image(
        renderer,
        GlassParams {
            lighting: shadow,
            ..off
        },
        false,
        false,
        false,
    )?;
    let shadow_bottom = image(
        renderer,
        GlassParams {
            lighting: LightParams {
                inner_shadow_angle: 180.,
                ..shadow
            },
            ..off
        },
        false,
        false,
        false,
    )?;
    assert!(
        shadow_top.get_pixel(480, 105)[0] < shadow_bottom.get_pixel(480, 105)[0],
        "inner-shadow angle must move the penumbra"
    );
    println!(
        "LIGHTING_STUDY_OK: dark outline, directional glints/shadows, inset positioning, blend comparison, zero-opacity invariants"
    );
    verify_rim_and_shadow(renderer)?;
    Ok(())
}

fn verify_rim_and_shadow(renderer: &mut WgpuHeadlessRenderer) -> anyhow::Result<()> {
    use gpui_glass::LightBlend;
    let engine = Arc::new(GlassRenderer::default());
    let make = |params: GlassParams, shape, scale| {
        let mut scene = rim_scene_at_scale(params, shape, scale, &engine);
        scene.finish();
        scene
    };
    let off = GlassParams {
        surface: Some([0.22; 3]),
        light: 1.,
        lighting: LightParams::disabled(),
        reflection: neutral_reflections(),
        ..Default::default()
    };
    let diffuse = GlassParams {
        reflection: ReflectionParams {
            native_diffuse: false,
            diffuse_intensity: 0.85,
            diffuse_radius: 120.,
            sampling_radius: 240.,
            ..neutral_reflections()
        },
        ..off
    };
    for scale in [1u32, 2] {
        let extent = size(
            DevicePixels(WIDTH * scale as i32),
            DevicePixels(HEIGHT * scale as i32),
        );
        let pixels = renderer
            .render_scene_to_image(&make(diffuse, Shape::Rectangle, scale as f32), extent)?;
        pixels.save(format!("artifacts/diffuse-rim-{scale}x.png"))?;
        let red = |y| {
            let p = pixels.get_pixel(480 * scale, y * scale);
            i32::from(p[0]) - i32::from(p[2])
        };
        let rim = red(101);
        assert!(
            rim > 30,
            "diffuse color must be visible immediately at the rim"
        );
        assert!(red(170) < rim / 2, "diffuse color must fade inward");
        assert!(
            red(250).abs() <= 1,
            "distant flat center must remain untinted"
        );
        for y in 102..175 {
            assert!(
                red(y) <= rim + 3,
                "diffuse must not peak in a detached inner band"
            );
        }
        // The nearest side changes on x-160 == y-100. A projected reflection
        // abruptly switches between blue and red there; a spatial blur is smooth.
        let mut max_jump = 0;
        for depth in 12..100 {
            let y = (100 + depth) * scale;
            let x = (160 + depth) * scale;
            let a = pixels.get_pixel(x - 1, y);
            let b = pixels.get_pixel(x + 1, y);
            for c in 0..3 {
                max_jump = max_jump.max((i32::from(a[c]) - i32::from(b[c])).abs());
            }
        }
        assert!(
            max_jump <= 8,
            "diffuse medial-axis seam at {scale}x: {max_jump}"
        );
        println!(
            "DIFFUSE_RIM_CHECK_OK {scale}x: strongest at rim; medial-axis maximum two-pixel jump {max_jump}/255"
        );
    }
    let extent = size(DevicePixels(WIDTH), DevicePixels(HEIGHT));
    for shape in [Shape::Capsule, Shape::Notched] {
        renderer
            .render_scene_to_image(&make(diffuse, shape, 1.), extent)?
            .save(format!("artifacts/diffuse-rim-{}.png", shape.name()))?;
    }
    let clean = renderer.render_scene_to_image(&make(off, Shape::Rectangle, 1.), extent)?;
    assert_eq!(
        clean.get_pixel(161, 101),
        clean.get_pixel(480, 250),
        "rectangular sidebar must fill its corners"
    );
    let outline = GlassParams {
        lighting: LightParams {
            outline_opacity: 1.,
            outline_width: 2.,
            ..LightParams::disabled()
        },
        ..off
    };
    let outlined = renderer.render_scene_to_image(&make(outline, Shape::Rectangle, 1.), extent)?;
    for y in 102..398 {
        assert_eq!(
            clean.get_pixel(480, y),
            outlined.get_pixel(480, y),
            "outside outline cannot shade the interior"
        );
    }
    let shadow = GlassParams {
        lighting: LightParams {
            outer_shadow_opacity: 0.7,
            outer_shadow_radius: 14.,
            ..LightParams::disabled()
        },
        ..off
    };
    let shaded = renderer.render_scene_to_image(&make(shadow, Shape::Rectangle, 1.), extent)?;
    assert!(
        shaded.get_pixel(480, 412)[0] < shaded.get_pixel(480, 87)[0],
        "default shadow must be offset downward"
    );
    let outlined_shadow = GlassParams {
        lighting: LightParams {
            outline_opacity: 1.,
            outline_width: 2.,
            ..shadow.lighting
        },
        ..off
    };
    let layered =
        renderer.render_scene_to_image(&make(outlined_shadow, Shape::Rectangle, 1.), extent)?;
    assert_eq!(
        layered.get_pixel(480, 400),
        outlined.get_pixel(480, 400),
        "opaque exterior outline must cover the shadow"
    );

    let shadow_reflection = ReflectionParams {
        native_diffuse: false,
        shadow_intensity: 1.,
        sampling_radius: 120.,
        ..neutral_reflections()
    };
    let reflected = renderer.render_scene_to_image(
        &make(
            GlassParams {
                reflection: shadow_reflection,
                ..shadow
            },
            Shape::Rectangle,
            1.,
        ),
        extent,
    )?;
    reflected.save("artifacts/layered-shadow-reflection.png")?;
    assert!(
        (reflected.get_pixel(480, 410)[0] as i32 - reflected.get_pixel(480, 410)[1] as i32)
            > (shaded.get_pixel(480, 410)[0] as i32 - shaded.get_pixel(480, 410)[1] as i32) + 2,
        "exterior red source must tint the shadow"
    );
    let tint_only = renderer.render_scene_to_image(
        &make(
            GlassParams {
                lighting: LightParams {
                    outer_shadow_opacity: 0.,
                    ..shadow.lighting
                },
                reflection: shadow_reflection,
                ..shadow
            },
            Shape::Rectangle,
            1.,
        ),
        extent,
    )?;
    let chroma = |image: &RgbaImage| {
        let p = image.get_pixel(480, 410);
        p[0] as i32 - p[1] as i32
    };
    assert!(
        chroma(&tint_only) > 2,
        "tint must survive zero black-shadow opacity"
    );
    assert!(
        (chroma(&tint_only) - chroma(&reflected)).abs() <= 1,
        "shadow opacity must not modulate reflected chroma"
    );
    let stats = engine.stats();
    let dormant = renderer.render_scene_to_image(
        &make(
            GlassParams {
                light: 0.,
                reflection: shadow_reflection,
                ..off
            },
            Shape::Rectangle,
            1.,
        ),
        extent,
    )?;
    assert_eq!(
        clean.as_raw(),
        dormant.as_raw(),
        "shadow reflection with light master disabled must be inert"
    );
    assert_eq!(
        engine.stats().reflection_passes,
        stats.reflection_passes,
        "disabled master means no shadow reflection prefilter pass"
    );

    let mut reflection_outputs = Vec::new();
    let mut shadow_outputs = Vec::new();
    for mode in [
        LightBlend::Normal,
        LightBlend::Screen,
        LightBlend::SoftLight,
        LightBlend::Overlay,
        LightBlend::Multiply,
        LightBlend::LinearDodge,
    ] {
        let pixels = renderer.render_scene_to_image(
            &make(
                GlassParams {
                    reflection: ReflectionParams {
                        native_diffuse: false,
                        diffuse_blend: mode,
                        ..diffuse.reflection
                    },
                    ..off
                },
                Shape::Rectangle,
                1.,
            ),
            extent,
        )?;
        reflection_outputs.push(*pixels.get_pixel(480, 101));
        let pixels = renderer.render_scene_to_image(
            &make(
                GlassParams {
                    reflection: ReflectionParams {
                        native_diffuse: false,
                        shadow_blend: mode,
                        ..shadow_reflection
                    },
                    ..shadow
                },
                Shape::Rectangle,
                1.,
            ),
            extent,
        )?;
        shadow_outputs.push(*pixels.get_pixel(480, 410));
    }
    for outputs in [reflection_outputs] {
        for (i, a) in outputs.iter().enumerate() {
            for b in &outputs[i + 1..] {
                assert_ne!(
                    a, b,
                    "each offered reflection blend must produce a distinct result"
                );
            }
        }
    }
    for pixel in shadow_outputs {
        assert!(
            pixel[0] >= pixel[1],
            "red surroundings should tint, rather than brighten, the shadow"
        );
    }
    println!(
        "EXTERIOR_FINISH_CHECK_OK: square sidebar corners, outside-only dark outline above offset shadow, shadow tint and disabled pass, six distinct reflection blends"
    );
    Ok(())
}

fn rim_scene_at_scale(
    params: GlassParams,
    shape: Shape,
    scale: f32,
    engine: &Arc<GlassRenderer>,
) -> Scene {
    let full = bounds(0., 0., WIDTH as f32, HEIGHT as f32, scale);
    let mut scene = Scene::default();
    scene.push_layer(full);
    quad(&mut scene, full, full, 0xeeeeee);
    quad(
        &mut scene,
        full,
        bounds(0., 0., 960., 100., scale),
        0xf02040,
    );
    quad(
        &mut scene,
        full,
        bounds(0., 100., 160., 300., scale),
        0x2040f0,
    );
    quad(
        &mut scene,
        full,
        bounds(0., 418., 960., 82., scale),
        0xf02040,
    );
    scene.pop_layer();
    scene.insert_primitive(BackdropFilter {
        bounds: bounds(160., 100., 640., 300., scale),
        content_mask: ContentMask {
            bounds: full,
            ..Default::default()
        },
        opacity: 1.,
        custom: Some(
            GlassDraw {
                renderer: engine.clone(),
                id: 1900,
                params,
                shape,
                scale,
                contour: None,
            }
            .effect(),
        ),
        ..Default::default()
    });
    scene
}

/// Wide directional bands must remain continuous where the closest edge changes.
pub fn verify_wide_bands(renderer: &mut WgpuHeadlessRenderer) -> anyhow::Result<()> {
    let engine = Arc::new(GlassRenderer::default());
    let shapes = [
        Shape::Rectangle,
        Shape::Rounded,
        Shape::Capsule,
        Shape::Notched,
        Shape::Flower,
    ];
    let make = |enabled: bool, scale: f32| {
        let full = bounds(0., 0., 1100., 680., scale);
        let mut scene = Scene::default();
        scene.push_layer(full);
        quad(&mut scene, full, full, 0xeeeeee);
        scene.pop_layer();
        for row in 0..4 {
            for (col, shape) in shapes.into_iter().enumerate() {
                let mut lighting = LightParams::disabled();
                if enabled {
                    match row {
                        0 => {
                            lighting.inner_shadow_opacity = 1.;
                            lighting.inner_shadow_radius = 96.;
                            lighting.inner_shadow_inset = 0.;
                        }
                        1 => {
                            lighting.soft_opacity = 1.;
                            lighting.soft_width = 96.;
                            lighting.soft_inset = 32.;
                        }
                        2 => {
                            lighting.sharp_opacity = 1.;
                            lighting.sharp_width = 64.;
                            lighting.sharp_inset = 24.;
                        }
                        _ => {
                            lighting.inner_shadow_opacity = 1.;
                            lighting.inner_shadow_radius = 1000.;
                            lighting.inner_shadow_angle = 135.;
                            lighting.inner_shadow_inset = 0.;
                        }
                    }
                }
                scene.insert_primitive(BackdropFilter {
                    bounds: bounds(
                        20. + col as f32 * 220.,
                        20. + row as f32 * 170.,
                        180.,
                        130.,
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
                            id: (row * 5 + col) as u64,
                            scale,
                            shape,
                            contour: None,
                            params: GlassParams {
                                surface: Some([0.45; 3]),
                                light: 1.,
                                lighting,
                                smoothing: 0.,
                                reflection: neutral_reflections(),
                                ..Default::default()
                            },
                        }
                        .effect(),
                    ),
                    ..Default::default()
                });
            }
        }
        scene.finish();
        scene
    };
    for scale in [1u32, 2] {
        let extent = size(
            DevicePixels(1100 * scale as i32),
            DevicePixels(680 * scale as i32),
        );
        let clean = renderer.render_scene_to_image(&make(false, scale as f32), extent)?;
        let rendered = renderer.render_scene_to_image(&make(true, scale as f32), extent)?;
        rendered.save(format!("artifacts/wide-bands-{scale}x.png"))?;
        let material = *clean.get_pixel(80 * scale, 80 * scale);
        let mut maximum = 0u8;
        let mut location = (0, 0);
        for y in 6 * scale..674 * scale {
            for x in 6 * scale..1094 * scale {
                // Ignore actual contour coverage and the deliberately crisp rim.
                if (-4..=4).any(|dy| {
                    (-4..=4).any(|dx| {
                        *clean.get_pixel(
                            (x as i32 + dx * scale as i32) as u32,
                            (y as i32 + dy * scale as i32) as u32,
                        ) != material
                    })
                }) {
                    continue;
                }
                let p = rendered.get_pixel(x, y)[0];
                let jump = p
                    .abs_diff(rendered.get_pixel(x + 1, y)[0])
                    .max(p.abs_diff(rendered.get_pixel(x, y + 1)[0]));
                if jump > maximum {
                    maximum = jump;
                    location = (x, y);
                }
            }
        }
        assert!(
            maximum <= 24,
            "wide bands must have no interior jumps, {scale}x maximum={maximum}/255 at {location:?}"
        );
        println!(
            "WIDE_BAND_CHECK_OK {scale}x: five shapes, square corners, bands beyond shape size; maximum interior adjacent-pixel step {maximum}/255"
        );
    }
    Ok(())
}

pub fn verify_customisation(renderer: &mut WgpuHeadlessRenderer) -> anyhow::Result<()> {
    use gpui_glass::LightBlend;
    let extent = size(DevicePixels(WIDTH), DevicePixels(HEIGHT));
    let make = |params: GlassParams, colored: bool| {
        let full = bounds(0., 0., WIDTH as f32, HEIGHT as f32, 1.);
        let mut scene = Scene::default();
        scene.push_layer(full);
        quad(&mut scene, full, full, 0x383838);
        if colored {
            quad(&mut scene, full, bounds(120., 0., 720., 100., 1.), 0xff3050);
        }
        scene.pop_layer();
        surface(
            &mut scene,
            full,
            bounds(160., 100., 640., 300., 1.),
            &Arc::new(GlassRenderer::default()),
            4400,
            params,
            Shape::Rectangle,
        );
        scene.finish();
        scene
    };
    let off = GlassParams {
        surface: Some([56. / 255.; 3]),
        light: 0.,
        reflection: neutral_reflections(),
        ..Default::default()
    };
    let clean = renderer.render_scene_to_image(&make(off, false), extent)?;
    // Sweep diffuse reach across mip transitions and the old finite band end.
    for radius in [16., 31., 32., 33., 63., 64., 65., 128., 256.] {
        let image = renderer.render_scene_to_image(
            &make(
                GlassParams {
                    reflection: ReflectionParams {
                        native_diffuse: false,
                        diffuse_intensity: 1.,
                        diffuse_radius: radius,
                        sampling_radius: 100.,
                        ..neutral_reflections()
                    },
                    ..off
                },
                true,
            ),
            extent,
        )?;
        let center = (100. + radius).min(397.) as u32;
        let max = (center.saturating_sub(3)..(center + 3).min(399))
            .map(|y| {
                let a = image.get_pixel(480, y)[0] as i16;
                let b = image.get_pixel(480, y + 1)[0] as i16;
                (a - b).abs()
            })
            .max()
            .unwrap();
        assert!(
            max <= 6,
            "diffuse radius {radius} must not produce a visible hard cutoff: {max}/255"
        );
    }
    println!("DIFFUSE_TAIL_OK: smooth radius endpoints across 9 radii and mip boundaries");
    let dark_tint = GlassParams {
        surface: None,
        blur: 0.,
        distortion: 0.,
        dispersion: 0.,
        opacity: 0.6,
        tint_color: [0.04; 3],
        ..off
    };
    let light_tint = GlassParams {
        tint_color: [0.98; 3],
        ..dark_tint
    };
    let dark_image = renderer.render_scene_to_image(&make(dark_tint, false), extent)?;
    let light_image = renderer.render_scene_to_image(&make(light_tint, false), extent)?;
    assert!(
        light_image.get_pixel(480, 230)[0] > dark_image.get_pixel(480, 230)[0] + 100,
        "equal tint opacity must use the supplied theme color"
    );

    let grade = |params: GlassParams| {
        let full = bounds(0., 0., WIDTH as f32, HEIGHT as f32, 1.);
        let mut scene = Scene::default();
        scene.push_layer(full);
        quad(&mut scene, full, full, 0x998877);
        quad(&mut scene, full, bounds(0., 0., 480., 500., 1.), 0xff0000);
        scene.pop_layer();
        surface(
            &mut scene,
            full,
            bounds(160., 100., 640., 300., 1.),
            &Arc::new(GlassRenderer::default()),
            4500,
            params,
            Shape::Rectangle,
        );
        scene.finish();
        scene
    };
    let grade_off = GlassParams {
        surface: None,
        blur: 0.,
        fog: 0.,
        opacity: 0.,
        saturation: 1.,
        distortion: 0.,
        dispersion: 0.,
        ..off
    };
    let original = renderer.render_scene_to_image(&grade(grade_off), extent)?;
    let vibrant = renderer.render_scene_to_image(
        &grade(GlassParams {
            vibrance: 1.,
            ..grade_off
        }),
        extent,
    )?;
    assert_eq!(
        original.get_pixel(250, 200),
        vibrant.get_pixel(250, 200),
        "vibrance must leave fully saturated red unchanged"
    );
    let span = |p: &image::Rgba<u8>| {
        *p.0[..3].iter().max().unwrap() as i32 - *p.0[..3].iter().min().unwrap() as i32
    };
    assert!(
        span(vibrant.get_pixel(700, 200)) > span(original.get_pixel(700, 200)),
        "vibrance must boost muted colors"
    );
    let bright = renderer.render_scene_to_image(
        &grade(GlassParams {
            brightness: 0.15,
            ..grade_off
        }),
        extent,
    )?;
    assert!(bright.get_pixel(700, 200)[1] > original.get_pixel(700, 200)[1] + 25);
    let flat = renderer.render_scene_to_image(
        &grade(GlassParams {
            contrast: 0.,
            ..grade_off
        }),
        extent,
    )?;
    assert_eq!(
        flat.get_pixel(250, 200),
        flat.get_pixel(700, 200),
        "zero contrast must remove tonal differences"
    );
    println!(
        "MATERIAL_GRADE_OK: vibrance preserves saturated colors and boosts muted colors; brightness and contrast respond"
    );

    for mode in [
        LightBlend::Normal,
        LightBlend::Screen,
        LightBlend::SoftLight,
        LightBlend::Overlay,
        LightBlend::Multiply,
        LightBlend::LinearDodge,
        LightBlend::LinearBurn,
        LightBlend::Radiance,
    ] {
        let params = GlassParams {
            reflection: ReflectionParams {
                native_diffuse: false,
                diffuse_intensity: 2.,
                diffuse_radius: 800.,
                sampling_radius: 48.,
                diffuse_blend: mode,
                ..Default::default()
            },
            ..off
        };
        let empty = renderer.render_scene_to_image(&make(params, false), extent)?;
        assert_eq!(
            clean.as_raw(),
            empty.as_raw(),
            "uniform neutral surround must leave the surface EXACTLY unchanged for {mode:?}"
        );
    }
    let params = GlassParams {
        reflection: ReflectionParams {
            native_diffuse: false,
            edge_intensity: 0.,
            diffuse_intensity: 1.,
            diffuse_radius: 800.,
            sampling_radius: 48.,
            diffuse_size_scaling: 0.,
            diffuse_blend: LightBlend::Overlay,
            brightness_min: 0.5,
            ..Default::default()
        },
        ..off
    };
    let glow = renderer.render_scene_to_image(&make(params, true), extent)?;
    let narrow = renderer.render_scene_to_image(
        &make(
            GlassParams {
                reflection: ReflectionParams {
                    native_diffuse: false,
                    diffuse_radius: 32.,
                    ..params.reflection
                },
                ..params
            },
            true,
        ),
        extent,
    )?;
    assert!(
        glow.get_pixel(480, 230)[0] > narrow.get_pixel(480, 230)[0] + 2,
        "diffuse radius must reach deep inside with unchanged 48px source radius"
    );
    glow.save("artifacts/reflection-wide-glow.png")?;
    let edge_range_changed = renderer.render_scene_to_image(
        &make(
            GlassParams {
                reflection: ReflectionParams {
                    native_diffuse: false,
                    edge_brightness_min: 0.,
                    edge_brightness_max: 0.1,
                    ..params.reflection
                },
                ..params
            },
            true,
        ),
        extent,
    )?;
    assert_eq!(
        glow.as_raw(),
        edge_range_changed.as_raw(),
        "edge brightness must not change diffuse-only output"
    );
    let edge_params = GlassParams {
        reflection: ReflectionParams {
            native_diffuse: false,
            edge_intensity: 2.,
            diffuse_intensity: 0.,
            ..params.reflection
        },
        ..params
    };
    let edge_image = renderer.render_scene_to_image(&make(edge_params, true), extent)?;
    let diffuse_range_changed = renderer.render_scene_to_image(
        &make(
            GlassParams {
                reflection: ReflectionParams {
                    native_diffuse: false,
                    brightness_min: 0.,
                    brightness_max: 0.1,
                    ..edge_params.reflection
                },
                ..edge_params
            },
            true,
        ),
        extent,
    )?;
    assert_eq!(
        edge_image.as_raw(),
        diffuse_range_changed.as_raw(),
        "diffuse brightness must not change edge-only output"
    );
    println!("REFLECTION_RANGES_OK: edge and diffuse brightness ranges are independent");

    // Saturated transmission can leave display gamut. Every contrast operator
    // must behave like its bounded input, rather than reverse slope or overflow.
    for mode in [
        LightBlend::SoftLight,
        LightBlend::Overlay,
        LightBlend::Screen,
        LightBlend::Multiply,
        LightBlend::LinearDodge,
        LightBlend::LinearBurn,
    ] {
        let unbounded = GlassParams {
            surface: Some([-0.2, 0.3, 1.3]),
            reflection: ReflectionParams {
                native_diffuse: false,
                diffuse_blend: mode,
                ..params.reflection
            },
            ..params
        };
        let bounded = GlassParams {
            surface: Some([0., 0.3, 1.]),
            ..unbounded
        };
        let a = renderer.render_scene_to_image(&make(unbounded, true), extent)?;
        let b = renderer.render_scene_to_image(&make(bounded, true), extent)?;
        assert_eq!(
            a.as_raw(),
            b.as_raw(),
            "{mode:?} must bound input before applying contrast"
        );
    }
    let desaturated = renderer.render_scene_to_image(
        &make(
            GlassParams {
                reflection: ReflectionParams {
                    native_diffuse: false,
                    saturation: 0.,
                    ..params.reflection
                },
                ..params
            },
            true,
        ),
        extent,
    )?;
    assert_ne!(
        glow.as_raw(),
        desaturated.as_raw(),
        "reflection saturation must change colored radiance"
    );
    println!("REFLECTION_GAMUT_OK: six bounded blend operators and independent saturation");

    // Increasing reach must retain nearby energy, rather than widening and
    // renormalizing the only convolution until the near source disappears.
    let mut reach_energy = Vec::new();
    for reach in [32., 64., 128., 240.] {
        let full = bounds(0., 0., WIDTH as f32, HEIGHT as f32, 1.);
        let mut scene = Scene::default();
        scene.push_layer(full);
        quad(&mut scene, full, full, 0x383838);
        quad(
            &mut scene,
            full,
            bounds(200., 220., 560., 12., 1.),
            0xff3050,
        );
        scene.pop_layer();
        surface(
            &mut scene,
            full,
            bounds(160., 240., 640., 240., 1.),
            &Arc::new(GlassRenderer::default()),
            4500,
            GlassParams {
                reflection: ReflectionParams {
                    native_diffuse: false,
                    sampling_radius: reach,
                    diffuse_radius: 34.,
                    diffuse_intensity: 1.,
                    ..params.reflection
                },
                ..off
            },
            Shape::Rectangle,
        );
        scene.finish();
        let frame = renderer.render_scene_to_image(&scene, extent)?;
        let mut energy = 0_f64;
        for y in 242..285 {
            for x in 220..740 {
                energy += (frame.get_pixel(x, y)[0] as f64 - 56.).max(0.);
            }
        }
        reach_energy.push(energy);
    }
    assert!(reach_energy[0] > 100.);
    for pair in reach_energy.windows(2) {
        assert!(
            pair[1] >= pair[0] * 0.98,
            "reach diluted nearby reflection: {reach_energy:?}"
        );
    }
    for (brightness, contrast) in [(0.25, 1.), (0., 1.8)] {
        let graded = renderer.render_scene_to_image(
            &make(
                GlassParams {
                    reflection: ReflectionParams {
                        native_diffuse: false,
                        brightness,
                        contrast,
                        ..params.reflection
                    },
                    ..params
                },
                true,
            ),
            extent,
        )?;
        assert_ne!(
            glow.as_raw(),
            graded.as_raw(),
            "reflection grading must affect visible radiance"
        );
        let empty = renderer.render_scene_to_image(
            &make(
                GlassParams {
                    reflection: ReflectionParams {
                        native_diffuse: false,
                        brightness,
                        contrast,
                        ..params.reflection
                    },
                    ..params
                },
                false,
            ),
            extent,
        )?;
        assert_eq!(
            clean.as_raw(),
            empty.as_raw(),
            "grading must not introduce reflection where support is zero"
        );
    }
    println!(
        "REFLECTION_REACH_OK: nearby energy {reach_energy:?}; brightness/contrast preserve empty support"
    );

    // Sweep a thin bright source through the capture boundary. Measure integrated
    // panel response so this detects onset popping rather than texel movement.
    let engine = Arc::new(GlassRenderer::default());
    let mut responses = Vec::new();
    // Stop before contact: the outside-only mask intentionally excludes overlapping texels.
    for distance in (4..=168).rev().step_by(4) {
        let full = bounds(0., 0., WIDTH as f32, HEIGHT as f32, 1.);
        let mut scene = Scene::default();
        scene.push_layer(full);
        quad(&mut scene, full, full, 0x383838);
        quad(
            &mut scene,
            full,
            bounds(200., 240. - distance as f32 - 12., 560., 12., 1.),
            0xff3050,
        );
        scene.pop_layer();
        surface(
            &mut scene,
            full,
            bounds(160., 240., 640., 240., 1.),
            &engine,
            4499,
            GlassParams {
                reflection: ReflectionParams {
                    native_diffuse: false,
                    sampling_radius: 160.,
                    diffuse_radius: 180.,
                    diffuse_intensity: 1.,
                    ..params.reflection
                },
                ..off
            },
            Shape::Rectangle,
        );
        scene.finish();
        let frame = renderer.render_scene_to_image(&scene, extent)?;
        let mut response = 0.;
        for y in 245..460 {
            for x in 200..760 {
                response += (frame.get_pixel(x, y)[0] as f64 - 56.).max(0.);
            }
        }
        responses.push(response);
    }
    let peak = responses.iter().copied().fold(0_f64, f64::max);
    let jump = responses
        .windows(2)
        .map(|v| (v[1] - v[0]).abs())
        .fold(0_f64, f64::max);
    assert!(peak > 100., "approaching source must produce a reflection");
    assert!(
        responses[0] == 0.,
        "source beyond sampling radius must contribute nothing"
    );
    assert!(
        jump / peak < 0.12,
        "reflection onset must be gradual: max 4px step = {:.3}, responses={responses:?}",
        jump / peak
    );
    println!(
        "REFLECTION_APPROACH_OK: maximum 4px response step {:.3}% of peak",
        jump / peak * 100.
    );

    let low = renderer.render_scene_to_image(
        &make(
            GlassParams {
                reflection: ReflectionParams {
                    native_diffuse: false,
                    brightness_min: 0.1,
                    brightness_max: 0.4,
                    ..params.reflection
                },
                ..params
            },
            true,
        ),
        extent,
    )?;
    assert_ne!(
        glow.as_raw(),
        low.as_raw(),
        "reflection brightness range must change output"
    );
    let fog = GlassParams {
        surface: None,
        blur: 0.,
        fog: 96.,
        fog_opacity: 0.,
        fog_edge: 60.,
        fog_edge_opacity: 0.,
        light: 0.,
        opacity: 0.,
        distortion: 0.,
        dispersion: 0.,
        saturation: 1.,
        reflection: neutral_reflections(),
        ..Default::default()
    };
    let no_fog = image(renderer, fog, false, false, true)?;
    let edge_fog = image(
        renderer,
        GlassParams {
            fog_edge_opacity: 1.,
            ..fog
        },
        false,
        false,
        true,
    )?;
    assert_ne!(
        no_fog.get_pixel(480, 104),
        edge_fog.get_pixel(480, 104),
        "edge-only fog must run with interior opacity zero"
    );
    assert_eq!(
        no_fog.get_pixel(480, 250),
        edge_fog.get_pixel(480, 250),
        "edge fog cannot change the zero-opacity center"
    );
    let uniform_fog = image(
        renderer,
        GlassParams {
            fog_edge_opacity: 1.,
            fog_opacity: 1.,
            ..fog
        },
        false,
        false,
        true,
    )?;
    assert_ne!(
        edge_fog.get_pixel(480, 250),
        uniform_fog.get_pixel(480, 250)
    );
    uniform_fog.save("artifacts/fog-96px.png")?;
    let lit = GlassParams {
        light: 1.,
        lighting: LightParams {
            inner_shadow_opacity: 1.,
            inner_shadow_blend: LightBlend::LinearBurn,
            inner_shadow_brightness: 0.85,
            ..LightParams::disabled()
        },
        ..off
    };
    let a = renderer.render_scene_to_image(&make(lit, false), extent)?;
    let b = renderer.render_scene_to_image(
        &make(
            GlassParams {
                lighting: LightParams {
                    inner_shadow_brightness: 1.,
                    ..lit.lighting
                },
                ..lit
            },
            false,
        ),
        extent,
    )?;
    assert_ne!(
        a.as_raw(),
        b.as_raw(),
        "burn source brightness must tune the shadow"
    );
    println!(
        "CUSTOMISATION_CHECK_OK: eight neutral blend invariants, independent 800px glow/48px sampling, tone range, edge-only fog and 96px blur, burn source brightness"
    );
    Ok(())
}

/// One-sided source catches opposite-edge echoes; temporal sweep catches
/// our own sampling discontinuities independently of native compositor behavior.
pub fn verify_native_bleed(renderer: &mut WgpuHeadlessRenderer) -> anyhow::Result<()> {
    let engine = Arc::new(GlassRenderer::default());
    for scale in [1u32, 2] {
        for (width, height, shape) in [
            (640u32, 300u32, Shape::Rectangle),
            (320, 400, Shape::Rectangle),
            (800, 72, Shape::Capsule),
            (640, 300, Shape::Rounded),
        ] {
            let mut last: Option<RgbaImage> = None;
            let mut largest_step = 0;
            for frame in 0..33 {
                let full = bounds(0., 0., 960., 500., scale as f32);
                let mut scene = Scene::default();
                scene.push_layer(full);
                quad(&mut scene, full, full, 0x383838);
                quad(
                    &mut scene,
                    full,
                    bounds(88. + frame as f32 * 0.5, 0., 24., 500., scale as f32),
                    0xff2040,
                );
                scene.pop_layer();
                scaled_surface(
                    &mut scene,
                    full,
                    bounds(160., 100., width as f32, height as f32, scale as f32),
                    &engine,
                    9010,
                    GlassParams {
                        surface: Some([56. / 255.; 3]),
                        light: 0.,
                        reflection: ReflectionParams {
                            native_diffuse: true,
                            diffuse_intensity: 10.,
                            diffuse_radius: 45.2,
                            diffuse_size_scaling: 0.39,
                            sampling_radius: 100.,
                            diffuse_blend: gpui_glass::LightBlend::Radiance,
                            ..neutral_reflections()
                        },
                        ..Default::default()
                    },
                    shape,
                    scale as f32,
                );
                scene.finish();
                let pixels = renderer.render_scene_to_image(
                    &scene,
                    size(
                        DevicePixels(960 * scale as i32),
                        DevicePixels(500 * scale as i32),
                    ),
                )?;
                let color = |x, y| {
                    let p = pixels.get_pixel(x * scale, y * scale);
                    i32::from(p[0]) - i32::from(p[1])
                };
                assert!(
                    color(162, 100 + height / 2) > 4,
                    "native bleed must respond to exterior source"
                );
                assert!(
                    color(160 + width - 20, 100 + height / 2).abs() <= 2,
                    "native bleed must not echo on opposite rim: {}",
                    color(160 + width - 20, 100 + height / 2)
                );
                if let Some(previous) = &last {
                    for y in 110 * scale..(90 + height) * scale {
                        for x in 162 * scale..(150 + width) * scale {
                            let a = pixels.get_pixel(x, y);
                            let b = previous.get_pixel(x, y);
                            for c in 0..3 {
                                largest_step =
                                    largest_step.max((i32::from(a[c]) - i32::from(b[c])).abs());
                            }
                        }
                    }
                }
                if frame == 16 {
                    pixels.save(format!(
                        "artifacts/native-bleed-{}-{width}-{height}-{scale}x.png",
                        shape.name()
                    ))?;
                }
                last = Some(pixels);
            }
            assert!(
                largest_step <= 12,
                "native bleed source motion flickered: {largest_step}/255"
            );
            println!(
                "NATIVE_BLEED_MOTION_OK {} {width}x{height} {scale}x: no opposite-edge echo, 33 half-pixel steps, maximum {largest_step}/255",
                shape.name()
            );
        }
    }
    Ok(())
}

// Force a full native rebuild by giving each surface its own renderer. Compare
// against shared incremental storage without changing source content or order.
pub fn verify_native_damage(renderer: &mut WgpuHeadlessRenderer) -> anyhow::Result<()> {
    let shared = Arc::new(GlassRenderer::default());
    let isolated: Vec<_> = (0..8).map(|_| Arc::new(GlassRenderer::default())).collect();
    let mut max_delta = 0u8;
    for frame in 0..24 {
        let scale = if frame < 12 { 1. } else { 2. };
        // Resize, odd physical dimensions, changing targets and sequence breaks.
        let width = 639. + (frame % 3) as f32;
        let height = 383. + (frame % 2) as f32;
        let make = |incremental: bool| {
            let mut scene = Scene::default();
            let full = bounds(0., 0., width, height, scale);
            scene.push_layer(full);
            quad(
                &mut scene,
                full,
                full,
                if frame % 4 == 0 { 0xededed } else { 0x202020 },
            );
            for i in 0..28 {
                quad(
                    &mut scene,
                    full,
                    bounds(
                        (i * 37 + frame * 11) as f32 % width,
                        (i * 23) as f32 % height,
                        17.,
                        height / 3.,
                        scale,
                    ),
                    [0xff2060, 0x08dc7a, 0x10aaff][i % 3],
                );
            }
            scene.pop_layer();
            for (i, private) in isolated.iter().enumerate() {
                // Intervening primitives must update later filters' source.
                scene.push_layer(full);
                quad(
                    &mut scene,
                    full,
                    bounds(
                        (i * 73 + frame * 7) as f32 % width,
                        (i * 39) as f32,
                        39.,
                        21.,
                        scale,
                    ),
                    0xffbc10,
                );
                scene.pop_layer();
                let params = GlassParams {
                    surface: (i % 3 != 0).then_some([0.08; 3]),
                    blur: 2.,
                    light: 1.,
                    reflection: ReflectionParams {
                        native_diffuse: !(frame % 5 == 0 && i == 3),
                        diffuse_intensity: 7.,
                        diffuse_radius: [0.1, 1., 2., 4., 8., 20., 80., 300., 2048.]
                            [(i + frame) % 9],
                        diffuse_size_scaling: 0.,
                        edge_intensity: if (i + frame) % 4 == 0 { 0. } else { 3. },
                        shadow_intensity: 1.,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                scaled_surface(
                    &mut scene,
                    full,
                    bounds(
                        10. + i as f32 * 64.,
                        12. + (i % 3) as f32 * 89.,
                        170.,
                        118.,
                        scale,
                    ),
                    if incremental { &shared } else { private },
                    9200 + i as u64,
                    params,
                    if i % 2 == 0 {
                        Shape::Capsule
                    } else {
                        Shape::Rectangle
                    },
                    scale,
                );
            }
            scene.finish();
            scene
        };
        let size = size(
            DevicePixels((width * scale) as i32),
            DevicePixels((height * scale) as i32),
        );
        let a = renderer.render_scene_to_image(&make(true), size)?;
        let b = renderer.render_scene_to_image(&make(false), size)?;
        if let Ok(directory) = std::env::var("GLASS_VERIFY_OUTPUT") {
            std::fs::create_dir_all(&directory)?;
            a.save(format!("{directory}/frame-{frame:02}.png"))?;
        }
        for (a, b) in a.as_raw().iter().zip(b.as_raw()) {
            max_delta = max_delta.max(a.abs_diff(*b));
        }
        assert!(
            a == b,
            "incremental native backdrop diverged from full rebuild at frame {frame}; maximum channel delta {max_delta}"
        );
    }
    println!(
        "NATIVE_BLEED_DAMAGE_OK: 24 frames, 8 overlapping surfaces, 1x/2x, odd sizes, resize, scene mutations, engine changes; max delta {max_delta}/255"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn scaled_surface(
    scene: &mut Scene,
    full: Bounds<ScaledPixels>,
    b: Bounds<ScaledPixels>,
    engine: &Arc<GlassRenderer>,
    id: u64,
    params: GlassParams,
    shape: Shape,
    scale: f32,
) {
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
                id,
                params,
                shape,
                scale,
                contour: None,
            }
            .effect(),
        ),
        ..Default::default()
    });
}
