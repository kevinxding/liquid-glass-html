# Fieldnotes · GPUI glass lab

A native GPUI-CE demo with an editorial canvas, bundled photographs, floating capsule
buttons, a text input, a shape lens, and live optical controls. The demo buttons have
no application action. Scroll the canvas underneath the glass to inspect distortion.
The canvas uses GPUI Kit's elastic edge scrolling: trackpad pulls stretch at the
top/bottom and spring back beneath the fixed glass controls. Ordinary mouse-wheel
line scrolling is unchanged. The macOS Reduce Motion preference is read at launch
and disables displacement when enabled.

Prism is now a [stress test](PRISM.md), with explicit realistic/cranked workloads,
60/120 Hz pacing and an idle-safe pause. For measured rendering costs and memory,
see [the performance report](REFLECTION_PERFORMANCE.md).

The default highlight path reconstructs macOS 27 QuartzCore's SDF key/fill masks,
vibrant backdrop transform and dark exterior rim. [Research notes](RESEARCH.md)
separate verified shader/host equations from uncalibrated native opacity and the
intentional SDR adaptations. The runtime remains a portable WGSL implementation;
it does not load Apple's private material APIs.

## Run

```sh
cargo run --release
```

Developed and verified on macOS / Apple Silicon, using GPUI-CE's WGPU-over-Metal backend.
Requires Rust 1.95+ and Xcode command line tools. No webview, dev server, runtime network
requests, or macOS private APIs. Other OS backends have not been validated.

A local app bundle can be built with:

```sh
./scripts/package-macos.sh
open artifacts/Fieldnotes.app
```

The four photos are embedded in the executable. The bundle can be moved independently
of the source folder. Packaging atomically replaces the executable; quit and reopen an
already-running instance to load an updated build. The script does not sign or notarize it.

## Controls and optical order

1. **Frost** uniformly blurs the captured backdrop **before** the refractive sampling.
   It never fades out at the rim. Zero skips its blur passes.
2. **Fog** is a separate interior blur. **Fog blur** sets its additional Gaussian
   sigma, **Fog edge width** sets the distance over which it fades in from the rim,
   and **Fog opacity** / **Fog edge opacity** set the interior and rim blend amounts. Its width is independent of refraction.
   Zero edge width removes the rim fade. Zero blur or both effective opacities zero omits the fog textures
   and blur passes. Fog defaults to off.
3. **Refraction** offsets the filtered backdrop using the shape's distance and normal.
   **Refraction edge width** is the depth of the lens band. **Edge profile** continuously
   blends linear, circular, and superellipse cross sections.
4. **Splay** blends normal-directed rays toward radial rays at the boundary, fanning
   the lens toward its ends. It does not scale or displace the flat center.
5. **Dispersion** integrates overlapping spectral bands rather than hard RGB offsets.
   It runs only in the refracting band and is completely bypassed at zero.
6. **Light** has independent dark exterior outline, sharp highlight, soft highlight, inner shadow,
   and outer shadow layers, each with an opacity and a width or radius. Zero opacity
   skips that layer's shader work. **Light master** at zero disables all five; it
   also removes the shadow/outline expanded draw area. The outline is outside the
   contour, above a layered contact/soft shadow whose downward offset grows with its
   radius. Highlights default to vertical top/bottom lighting. Angle, inset, angular
   focus, opposite-highlight strength, and blend controls tune each layer. The defaults interpret the
   supplied iOS/macOS reference images, rather than reproducing Apple's implementation.
7. **Reflections** have separate edge, diffuse and shadow intensity/blend controls.
   Edges sample already-painted content outside the shape, with distance-dependent
   filtering. The default **27 backdrop** diffuse path reconstructs the native
   backdrop bleed, including size-derived displacement and native mip filtering.
   **Diffuse blur radius** controls its blur; **Edge sampling radius** controls the
   exterior source reach. The source-mode button retains **exterior only** for
   comparison. Shadow reflections tint the existing shadow profile independently
   of black-shadow opacity. All three intensities at zero skip reflection work.
8. **Saturation** and **Tint opacity** adjust the transmitted backdrop.
   **Corner smoothing** changes the study lens and the toolbar's Lisse capsules.
   Zero gives circular capsules; one gives full cubic shoulders flowing into the
   circular end caps. The shoulder's reach is clamped to the available flat side.

The scrollable inspector groups optics, fog, light, and reflection settings. Its
fixed, square-cornered background uses the same reflections over a solid paper-colored surface,
without refraction. **Reflection targets** adds colored swatches next to the panel;
**Animate backdrop** moves them toward and away from its edge and animates the
canvas target. This makes changes in reflection reach and sharpness easier to see.
**Lighting preview** switches the study lens between its backdrop, light material,
and dark material to compare the finish. Angles use 0° for up and 90° for right;
insets move bands inward from the contour.

The shape study includes a smoothed rectangle, pebble, concave notch, and flower.
Clear, Frost, and Prism presets reset the material parameters. Glass effect provides
an opaque/translucent fallback for comparison. Animate backdrop is an optional
continuous-redraw stress case; normal operation redraws on interaction only (and
caret blink while the input is focused).

## Architecture

The demo consumes two reusable crates. See [REUSE.md](REUSE.md) for dependency
setup, styling support, and the GPUI patch/update contract.

- `src/main.rs`: native demo and inspector UI.
- `src/editor.rs`: small text input engine adapted from the pinned GPUI-CE example.
- `src/scroll_bounce.rs`: GPUI Kit's `ScrollBounce`, adapted to GPUI-CE. Its source
  revision and Apache-2.0 attribution are in `THIRD_PARTY_NOTICES.md`. This is demo
  behavior; neither reusable graphics crate depends on GPUI Kit.
- `crates/gpui-glass/src/glass.rs`: GPU resource ownership, cropped backdrop capture, filtered reductions,
  Gaussian passes, exterior reflection prefiltering, parameter upload, and the custom
  backdrop callback.
- `crates/gpui-glass/shaders/glass.wgsl`: backdrop blur, lens profile, pixel-footprint
  antialiasing, spectral integration, reflection sampling, layered lighting/shadows,
  and compositing.
- `crates/gpui-glass/shaders/reflection.wgsl`: exterior-only coarse capture,
  overlapping Gaussian mip reductions, and separate detailed edge/diffuse fields;
  dispatches share one compute pass per reflecting surface.
- `crates/gpui-glass/src/shape.rs`: contour construction and a cached distance/normal atlas. A segment
  BVH computes subpixel distances. A cached harmonic extension blends boundary
  directions through the interior, avoiding facets where nearest edges switch.
  Coverage and exterior distances remain unchanged.
- `crates/gpui-smooth`: Lisse capsules and uniform rounded rectangles, with an optional
  GPUI adapter for fills, gradients, and strokes. Shared by glass and fallback.
- `patches/gpui-backdrop.patch`: the complete, small GPUI extension.
- `vendor/UPSTREAM.md`: exact upstream revision and patch rationale.

GPUI's existing custom GPU controls can display an externally rendered texture, but
cannot access the already-painted UI. The local extension adds an opt-in callback
at the existing backdrop stage. It passes the current scene view and encoder to the
application; all optical code remains outside GPUI. There is no published fork.

The callback runs in scene order. Source and destination alias, so the application
first captures a separate cropped texture, then composites back. Foreground labels
and input text are drawn afterwards and stay sharp. The shader respects rectangular
content clipping and inherited opacity. GPUI's optional content-mask edge fades are
not used by this demo.

Edge reflections use a masked, prefiltered exterior-radiance texture. Native
diffuse uses a separate full-resolution backdrop pyramid, updated in scene order
from conservative damage. It is fully rebuilt on the first use of every frame.
Native sizing/filtering observations and remaining fidelity differences are
recorded in [RESEARCH.md](RESEARCH.md). Radiance blending remains customizable.

## Reuse with custom shapes

GPUI-CE provides `.rounded_full().rounded_smoothing(0.6)` and the shorthand
`.rounded_smoothing_ios()` for its own corner smoothing. Its crowded-corner
construction differs from [Lisse's continuous capsule ends](https://github.com/JaceThings/Lisse/blob/173846978ed806e14c78154ec914a9645a991932/packages/core/src/curves/capsule.ts).
For the reference capsule shape, this demo ports Lisse's end-cap algorithm into
`capsule::outline(width, height, smoothing)`, including horizontal/vertical forms
and the near-square smoothing clamp. The port's source revision and MIT notice
are recorded in `THIRD_PARTY_NOTICES.md` and `LICENSES/Lisse.txt`.

`Shape::Capsule` uses this outline automatically. Its cached distance/normal atlas
drives coverage, refraction, layered light/shadows, and reflection boundaries together.
Choosing a capsule does not add a shape-specific shader pass.
The toolbar fallback paints the same outline as a GPUI path. To use a capsule
elsewhere, call `capsule::outline` with logical-pixel dimensions; either paint that
path directly or divide each point by those dimensions to build a `GlassContour`.

`GlassContour::new(Vec<[f32; 2]>)` accepts a normalized, closed polygon (last point
need not repeat the first). Concave simple outlines work. Flatten Bézier curves to
points once; supply the resulting contour through `GlassDraw::contour`. Coordinates
must be finite and between 0 and 1. This API is for simple closed outlines, not
self-intersections or overlapping subpaths.

```rust,ignore
let draw = GlassDraw {
    renderer: shared_renderer.clone(),
    id: 7, // stable and unique per surface within a frame
    params,
    shape: Shape::Rounded, // used when contour is None
    scale: window.scale_factor(),
    contour: Some(custom_contour.clone()),
};
draw.paint_in(window, bounds); // inside an Element/canvas paint callback
```

`paint_in` reads the current window scale and registers the full shadow/outline
extent with GPUI while preserving the original shape coordinates. Prefer it over
manually inserting `GlassDraw::effect()`, whose caller must handle that registration.

Keep the `Arc<GlassRenderer>` alive. Pipelines, textures, bind groups, and shape atlases
are reused. Resources are recreated when the device changes. Most optical controls
reuse the atlas; resizing, contour changes, or a larger required shadow margin can
rebuild it. Capture extents grow when needed and retain their warmed maximum, so
moving the sampling radius down and back up within that extent does not reallocate
an otherwise unchanged surface.

128 IDs is a soft retention threshold: insertion may evict one least-recently-used
surface only if it has been idle for more than two seconds. Active working sets may
exceed 128, avoiding repeated rebuilds. `GlassRenderer::stats()` exposes allocation,
atlas upload, backdrop capture, blur-pass, and reflection-pass counters; these count
encoded work, not GPU time. `reflection_passes` counts compute passes, not individual
mip levels. `clear_cache()` explicitly releases cached surfaces when a view or document
closes. There is no fixed texture-memory budget. The native Metal GPUI backend does
not implement this callback, so use the configured `macos-wgpu` feature.

## Validation and performance

```sh
./scripts/check-crates.sh
cargo run --release --features bench --bin glass-bench
cargo run --release --features bench --bin glass-bench -- --verify-reflection
cargo run --release --features bench --bin glass-bench -- --verify-lighting
cargo run --release --features bench --bin glass-bench -- --verify-wide-bands
cargo run --release --features bench --bin glass-bench -- --stress
cargo run --release --bin gpu-cost
```

The first benchmark renders 1,290 backdrop quads and five glass surfaces at 1× and
2×. It alternates configurations, warms each independent cache, and waits for GPU
completion. These are **complete-frame wall times**, including driver/CPU overhead,
not FPS or isolated shader timing. It also writes shape snapshots and checks content
clipping, parameter response, exterior preservation, splay's unchanged center, and
uniform pre-refraction frost. `glass-bench --verify` runs just the optical regressions.
`--verify-reflection` isolates the light/fog/reflection checks, including exterior-only
sources, distance-dependent sharpness, and disabled-pass counters. `--stress` exercises
larger surface counts and repeated parameter changes; its report records tail latency
and cache work as well as median cost.
`--verify-lighting` writes a blend comparison in light/dark/blue materials and checks
outside outline ordering, angle/inset response, diffuse rim placement and seam
continuity at 1×/2×, rectangular corners, and reflection/shadow blend behavior.

The GPU-cost diagnostic attempts GPU timestamp queries around the five glass effects.
It includes capture, blur, refraction, AA, dispersion, light, and reflections; it
excludes GPUI's background drawing and final presentation blit. On the tested Metal
setup, timestamps returned no usable interval, so this diagnostic exits with an
error instead of reporting a zero cost. Use the complete-frame benchmark above.

The benchmark tools write reports under `artifacts/`. See `PERFORMANCE.md` for this
machine's measurements and the limits of those measurements. No low-end GPU or
software renderer is claimed to have been tested.

Performance choices: bounded cropped captures, quantized sampling halos, normalized
Gaussian bilinear pairs, prefiltered reductions for wide blur, cached contour atlases,
additional AA taps only in minifying regions, exterior-only shadow work, and skipped sampling or
passes for disabled fog/dispersion/blur/light/reflections. Exterior reflections use
prefiltered textures and bounded per-fragment sampling, rather than a source-search
loop for every rim pixel. No CPU framebuffer readback or full-screen CPU blur is
used in the application. Readback exists only in the validation binaries.

`--verify-wide-bands` covers wide highlights and inner shadows, including radii
larger than the shape, on rectangles, circular corners, capsules, and concave
contours at 1×/2×. The direction field is prepared only when geometry changes;
it uses the existing atlas lookup and adds no per-frame filtering pass.

## Reusable inspector controls and additional material settings

The sidebar embeds `gpui_glass::controls::GlassControls`; its layout, background,
and scroll view remain in the demo. The component can be placed in any GPUI layout.
It emits `ControlsChanged` and provides native **Save JSON… / Load JSON…** dialogs
for versioned material/shape presets. See the crate README for the embedding API.

Fog has separate edge and interior opacities, and Frost/Fog sliders reach 128 px.
Reflections default to Overlay with brightness minimum/maximum and dark-surface
bias controls. Diffuse reach extends to 2048 px independently of the exterior
sampling radius. Neutral/no-source regions remain unchanged across all blend modes.
Each highlight and inner shadow also has its own source-brightness slider.

The floating **Dark / Light** button changes the page and inspector theme without
resetting material settings. Two additional colorful photographs extend the scrollable
page; all four photos are bundled for offline use. Attribution is in `assets/SOURCES.md`.

`glass-bench --verify-customisation` checks neutral blending, a broad glow at fixed
source radius, reflection tone range, edge-only fog, large blur, and Burn brightness.

## Prism: an animated reflection demo

[Prism](PRISM.md) is a second, dark-first trading-style app with simulated animated
charts, a reflective left navigation rail, shared JSON-capable controls on the
right, spring-driven glass shapes, and materialising popovers. Run
`cargo run --release --bin market-demo` or build `artifacts/Prism.app` with
`./scripts/package-prism.sh`. It also supports light mode and Reduce Motion.


Native diffuse regressions: `glass-bench --verify-native-bleed` checks one-sided
moving sources at 1x/2x; `--verify-native-damage` compares shared incremental
pyramids against full rebuilds. Independent native mip CPU/GPU checks live in
`crates/gpui-glass/src/reflection_tests.rs` (explicit ignored GPU tests).

## License

Project code is licensed under BSD-3-Clause; see [LICENSE](LICENSE).
Third-party code and photographs retain their original licenses and attribution;
see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and [assets/SOURCES.md](assets/SOURCES.md).

