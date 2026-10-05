//! Deterministic desktop/5K/Prism workload. Timing includes driver submission and
//! a GPU completion fence, but excludes scene preparation, readback and presentation.
use super::*;

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    width: f32,
    height: f32,
    scale: f32,
    stress: bool,
    strong: bool,
    intensity: f32,
}

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

fn make(case: Case, engine: &Arc<GlassRenderer>, phase: usize, effect: bool) -> Scene {
    let mut scene = Scene::default();
    let full = bounds(0., 0., case.width, case.height, case.scale);
    scene.push_layer(full);
    quad(&mut scene, full, full, 0x101010);
    let motion = std::env::var("GLASS_PERF_MOTION").unwrap_or_else(|_| "animated".into());
    let phase = if motion == "idle" { 0 } else { phase };
    let scrolling = motion == "scroll";
    for row in 0..34 {
        for col in 0..40 {
            let x = (col as f32
                + (if scrolling { 0. } else { phase as f32 * 0.35 } + row as f32).sin() * 0.25)
                * case.width
                / 40.;
            let y =
                row as f32 * case.height / 30. - if scrolling { phase as f32 * 7.5 } else { 0. };
            let color = [0xf84075, 0x24edaa, 0xffb02e, 0x04bcff, 0xbe51ff, 0xeeeeee]
                [(row + col + if scrolling { 0 } else { phase / 3 }) % 6];
            quad(
                &mut scene,
                full,
                bounds(x, y, case.width / 45., case.height / 36., case.scale),
                color,
            );
        }
    }
    scene.pop_layer();
    if effect {
        let reflection = if case.strong {
            ReflectionParams {
                edge_intensity: case.intensity,
                edge_width: 12.,
                diffuse_intensity: case.intensity,
                diffuse_radius: if case.stress { 1500. } else { 500. },
                sampling_radius: 240.,
                shadow_intensity: 2.,
                ..Default::default()
            }
        } else {
            ReflectionParams::default()
        };
        let params = GlassParams {
            reflection,
            tint_color: [0.035; 3],
            ..Default::default()
        };
        let mut surfaces: Vec<(f32, f32, f32, f32, bool, Shape)> = Vec::new();
        if case.stress {
            for i in 0..48 {
                let w = case.width / 8.;
                let h = case.height / 6.;
                surfaces.push((
                    6. + (i % 8) as f32 * w,
                    6. + (i / 8) as f32 * h,
                    w - 12.,
                    h - 12.,
                    i % 4 != 0,
                    Shape::Rounded,
                ));
            }
        } else {
            // Realistic complete-window composition: one opaque reflecting rail,
            // five toolbar controls, two cards, one popover, one floating input.
            surfaces.push((0., 0., 210., case.height, true, Shape::Rectangle));
            for i in 0..5 {
                surfaces.push((
                    232. + i as f32 * 132.,
                    18.,
                    116.,
                    40.,
                    false,
                    Shape::Capsule,
                ));
            }
            surfaces.extend([
                (250., 190., 180., 100., false, Shape::Rounded),
                (case.width - 238., 230., 206., 132., false, Shape::Rounded),
                (case.width - 354., 440., 310., 218., false, Shape::Rounded),
                (
                    244.,
                    case.height - 86.,
                    case.width.min(1200.) - 300.,
                    62.,
                    false,
                    Shape::Capsule,
                ),
            ]);
        }
        for (id, (x, y, w, h, opaque, shape)) in surfaces.into_iter().enumerate() {
            scene.insert_primitive(BackdropFilter {
                bounds: bounds(x, y, w, h, case.scale),
                content_mask: ContentMask {
                    bounds: full,
                    ..Default::default()
                },
                opacity: 1.,
                custom: Some(
                    GlassDraw {
                        renderer: engine.clone(),
                        id: id as u64,
                        params: if opaque {
                            params.reflection_only([0.065; 3])
                        } else {
                            params
                        },
                        shape,
                        scale: case.scale,
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

pub fn run(renderer: &mut WgpuHeadlessRenderer) -> anyhow::Result<()> {
    let samples: usize = std::env::var("GLASS_PERF_SAMPLES")
        .ok()
        .and_then(|x| x.parse().ok())
        .unwrap_or(80)
        .max(8);
    let label = std::env::var("GLASS_PERF_LABEL").unwrap_or_else(|_| "current".into());
    anyhow::ensure!(
        label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "invalid report label"
    );
    let mut report = format!(
        "Complete scene performance [{label}]. 12 warmups, {samples} samples, 12 moving content frames.\nSynchronized CPU submission + GPU completion wall time; NOT GPU timestamp duration.\nNo readback/presentation/scene-build time in samples. Interleaved same-content no-effect baseline.\n120 FPS budget=8.333 ms;60 FPS=16.667 ms;45 FPS=22.222 ms.\n"
    );
    report.push_str(&format!("Adapter: {}\nGPU timer at startup: {} (invalid/zero results are unavailable). Dedicated GPU sweep; wall sweep has no instrumentation.\n", renderer.adapter_description(), renderer.gpu_timing_source()));
    report.push_str(&format!(
        "Motion: {} (idle means forced redraw of unchanged content, not a sleeping app).\n",
        std::env::var("GLASS_PERF_MOTION").unwrap_or_else(|_| "animated".into())
    ));
    print!("{report}");
    let cases = [
        Case {
            name: "desktop-default-1x",
            width: 1440.,
            height: 940.,
            scale: 1.,
            stress: false,
            strong: false,
            intensity: 10.,
        },
        Case {
            name: "desktop-default-2x",
            width: 1440.,
            height: 940.,
            scale: 2.,
            stress: false,
            strong: false,
            intensity: 10.,
        },
        Case {
            name: "desktop-strong-1x",
            width: 1440.,
            height: 940.,
            scale: 1.,
            stress: false,
            strong: true,
            intensity: 10.,
        },
        Case {
            name: "desktop-strong-2x",
            width: 1440.,
            height: 940.,
            scale: 2.,
            stress: false,
            strong: true,
            intensity: 10.,
        },
        Case {
            name: "5k-default",
            width: 2560.,
            height: 1440.,
            scale: 2.,
            stress: false,
            strong: false,
            intensity: 10.,
        },
        Case {
            name: "5k-strong",
            width: 2560.,
            height: 1440.,
            scale: 2.,
            stress: false,
            strong: true,
            intensity: 10.,
        },
        Case {
            name: "prism-cranked-1x",
            width: 1440.,
            height: 940.,
            scale: 1.,
            stress: true,
            strong: true,
            intensity: 10.,
        },
        Case {
            name: "prism-cranked-2x",
            width: 1440.,
            height: 940.,
            scale: 2.,
            stress: true,
            strong: true,
            intensity: 10.,
        },
        Case {
            name: "prism-intensity-1-2x",
            width: 1440.,
            height: 940.,
            scale: 2.,
            stress: true,
            strong: true,
            intensity: 1.,
        },
        Case {
            name: "prism-intensity-10-2x",
            width: 1440.,
            height: 940.,
            scale: 2.,
            stress: true,
            strong: true,
            intensity: 10.,
        },
    ];
    let mut intensity_work: Option<(&str, [u64; 6])> = None;
    for case in cases {
        if std::env::var("GLASS_BENCH_CASE").is_ok_and(|f| !case.name.contains(&f)) {
            continue;
        }
        let engine = Arc::new(GlassRenderer::default());
        let extent = size(
            DevicePixels((case.width * case.scale) as i32),
            DevicePixels((case.height * case.scale) as i32),
        );
        let frames: Vec<_> = (0..12).map(|p| make(case, &engine, p, true)).collect();
        let baselines: Vec<_> = (0..12).map(|p| make(case, &engine, p, false)).collect();
        let cold = ms(renderer, &frames[0], extent)?;
        for i in 0..12 {
            ms(renderer, &frames[i], extent)?;
            ms(renderer, &baselines[i], extent)?;
        }
        let warm = engine.stats();
        let mut times = Vec::with_capacity(samples);
        let mut base = Vec::with_capacity(samples);
        for i in 0..samples {
            if i % 2 == 0 {
                base.push(ms(renderer, &baselines[i % 12], extent)?);
            }
            times.push(ms(renderer, &frames[i % 12], extent)?);
            if i % 2 != 0 {
                base.push(ms(renderer, &baselines[i % 12], extent)?);
            }
        }
        let end = engine.stats();
        let mut gpu_times = Vec::new();
        let mut gpu_base = Vec::new();
        if renderer.gpu_timing_enabled() {
            for i in 0..samples {
                let scene = &frames[i % 12];
                let baseline = &baselines[i % 12];
                let Some(base_time) = renderer.render_scene_timed_and_wait(baseline, extent)?
                else {
                    gpu_times.clear();
                    gpu_base.clear();
                    break;
                };
                let Some(effect_time) = renderer.render_scene_timed_and_wait(scene, extent)? else {
                    gpu_times.clear();
                    gpu_base.clear();
                    break;
                };
                gpu_times.push(effect_time);
                gpu_base.push(base_time);
            }
        }
        let gpu_summary = if gpu_times.is_empty() {
            " gpu=unavailable".into()
        } else {
            let gpu_median = percentile(&mut gpu_times, 0.5);
            let gpu_p95 = percentile(&mut gpu_times, 0.95);
            let gpu_baseline = percentile(&mut gpu_base, 0.5);
            format!(
                " gpu-interval-median={gpu_median:.3}ms gpu-interval-p95={gpu_p95:.3}ms gpu-interval-baseline={gpu_baseline:.3}ms gpu-interval-delta={:.3}ms",
                gpu_median - gpu_baseline
            )
        };
        anyhow::ensure!(
            end.slot_allocations == warm.slot_allocations,
            "{} warmed allocation regression",
            case.name
        );
        anyhow::ensure!(
            end.atlas_uploads == warm.atlas_uploads,
            "{} warmed atlas upload regression",
            case.name
        );
        let median = percentile(&mut times, 0.5);
        let p95 = percentile(&mut times, 0.95);
        let p99 = percentile(&mut times, 0.99);
        let baseline = percentile(&mut base, 0.5);
        let line = format!(
            "PERF {} output={}x{} surfaces={} median={median:.3}ms p95={p95:.3}ms p99={p99:.3}ms baseline={baseline:.3}ms delta={:.3}ms cold={cold:.3}ms captures/frame={:.1} blur-passes/frame={:.1} reflection-encodes/frame={:.1} warmed-allocations={} warmed-atlas-uploads={}{gpu_summary}\n",
            case.name,
            extent.width.0,
            extent.height.0,
            end.cached_surfaces,
            median - baseline,
            (end.backdrop_captures - warm.backdrop_captures) as f64 / samples as f64,
            (end.blur_passes - warm.blur_passes) as f64 / samples as f64,
            (end.reflection_passes - warm.reflection_passes) as f64 / samples as f64,
            end.slot_allocations - warm.slot_allocations,
            end.atlas_uploads - warm.atlas_uploads
        );
        report.push_str(&format!(
            "UNIFORMS {} uploads={} reuses={}\n",
            case.name,
            end.uniform_uploads - warm.uniform_uploads,
            end.uniform_reuses - warm.uniform_reuses
        ));
        print!("{line}");
        report.push_str(&line);
        let resources = format!(
            "RESOURCES {} effect-textures={:.3}MiB reflection-capture-texels/frame={:.0} diffuse-prefilter-texels/frame={:.0}\n",
            case.name,
            end.cached_texture_bytes as f64 / (1024. * 1024.),
            (end.reflection_capture_texels - warm.reflection_capture_texels) as f64
                / samples as f64,
            (end.diffuse_prefilter_texels - warm.diffuse_prefilter_texels) as f64 / samples as f64
        );
        print!("{resources}");
        report.push_str(&resources);
        if case.name.contains("prism-intensity-") {
            let work = [
                end.backdrop_captures - warm.backdrop_captures,
                end.blur_passes - warm.blur_passes,
                end.reflection_passes - warm.reflection_passes,
                end.reflection_capture_texels - warm.reflection_capture_texels,
                end.diffuse_prefilter_texels - warm.diffuse_prefilter_texels,
                end.cached_texture_bytes,
            ];
            if let Some((other, previous)) = intensity_work {
                anyhow::ensure!(
                    work == previous,
                    "intensity alone changed encoded work: {other} {previous:?}, {} {work:?}",
                    case.name
                );
                let line = "INTENSITY_WORK_IDENTICAL intensity1/intensity10: captures, blur/reflection encodes, capture/prefilter texels and cached texture bytes unchanged. Timing is descriptive, not a strict assertion.\n";
                print!("{line}");
                report.push_str(line);
            } else {
                intensity_work = Some((case.name, work));
            }
        }
        // Save after timing, so readback never contaminates a sample.
        if std::env::var_os("GLASS_PERF_IMAGES").is_some() {
            renderer
                .render_scene_to_image(&frames[0], extent)?
                .save(format!("artifacts/perf-{label}-{}.png", case.name))?;
        }
        std::fs::write(format!("artifacts/perf-{label}.txt"), &report)?;
    }
    report.push_str("Effect-texture bytes exclude native renderer targets, buffers and driver memory. Power unmeasured; no low-end, thermal or battery claim follows from these timings.\n");
    std::fs::write(format!("artifacts/perf-{label}.txt"), report)?;
    Ok(())
}
