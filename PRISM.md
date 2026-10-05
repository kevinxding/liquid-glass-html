# Prism — glass and reflection stress test

Prism deliberately puts an unreasonable number of reflective surfaces in a dense,
animated synthetic trading board. It is a workload generator, not an appearance
reference or a proposed desktop UI. The tiles retain the library's outside-only
reflection behavior: their own foreground content is not reflected by their edges.
Fieldnotes remains the appearance demo.

Run `cargo run --release --bin market-demo`, or package `artifacts/Prism.app` with
`./scripts/package-prism.sh`. The bundle display name is **Prism Stress Test**.
For repeatable app-level profiling, launch directly with e.g.
`cargo run --release --bin market-demo -- --workload=cranked --fps=60`.
Use `--workload=realistic --fps=120` for a standard control count, or `--paused`
for a static workload. `--help` lists the options without opening a window.

The quotes, charts, depth bars, heatmap and trades are generated locally. There
are no accounts, connections or orders.

## Workload controls

The header exposes three reproducible presets. Selecting one resets the material
parameters and enables effects; the inspector remains editable afterward, and the
header then marks the settings as custom. A workload reset or switching effects
off releases cached effect resources, including large scratch textures retained
from Cranked. The next enabled workload pays cold setup again. Slider changes and
animation frames do not clear the cache.

| Preset | Surfaces | Reflection parameters |
| --- | --- | --- |
| Realistic | Floating controls and navigation rail; ordinary flat content tiles | Library defaults |
| Prism | The same controls plus all 35 content panels and heatmap cells | Diffuse intensity 0.7, radius 110px |
| Cranked | The same excessive surface count as Prism | Edge and diffuse intensity 10×, edge width 12px, diffuse radius 512px, sampling reach 240px, shadow reflection 2× |

The complete board contains 50 effect-bearing surfaces before viewport clipping,
or 51 with a popover. Realistic removes the 35 tile effects. Window size, scrolling
and clipping change how many are actually painted. Increasing intensity does not
change resolution or reduce the number of surfaces. There is no automatic quality
fallback, so high-load comparisons stay meaningful.

**Target 60 / 120 fps** sets the animation pacing target; it does not claim the
machine or display can sustain that rate. The default is 60 to avoid running an
idle stress window at maximum refresh. Animation uses the native display callback,
with no view invalidation on refreshes skipped by the selected target. Pause stops
synthetic data updates; once interactive springs have settled, the app requests
no animation redraws and runs no animation timer. Reduce Motion freezes the data
and makes interactive springs settle immediately. Normal input remains responsive. GPUI may limit inactive windows or respond to
thermal pressure independently of the selected target.

The inspector reports observed **render cadence**, including CPU work and display
scheduling. It is not a GPU timestamp or a power measurement. No GPU utilization,
battery-life or low-end hardware claim should be inferred from that number.

## Interactive geometry workload

Click the three controls below the positions/heatmap row. Their springs change
width and height, and Lisse's capsule generator runs again at those dimensions.
They do not stretch an existing silhouette. `dynamic_shape: true` uses
`shape_scale` as dimensions relative to a fixed maximum drawing area. The CPU
simplifies the contour to a bounded 0.05 logical-pixel error and a compute pass
rebuilds the signed-distance atlas on the GPU. Stable buffers/textures are reused.
Continuous weighted boundary normals avoid nearest-edge flips in the dynamic
interior. Static shapes retain their cached harmonic field.

Market and order popovers keep a fixed 16px Lisse corner with no geometry zoom.
They remain mounted while closing. Blur, fog, refraction, edge width, dispersion,
tint, lighting, reflections and grading approach their neutral values. Only the
foreground content fades in opacity; the glass canvas stays at opacity 1. Zero
progress reproduces the unfiltered backdrop. Backing resources can change when
blur passes switch on or off, but atlas padding is retained.

Both light and dark modes provide the appropriate neutral material tint. Text
inherits OpenType `tnum`. The right inspector embeds reusable `GlassControls`,
including JSON preset save/load.

## Reproducible renderer checks

`cargo run --release --features bench --bin glass-bench -- --dynamic` exercises
nine simultaneously regenerated contours over 300 moving color quads, including
independent static-contour comparisons at 1x/2x and zero-progress popover checks.
The complete benchmark and optics commands are described in `PERFORMANCE.md`.
These measurements include scene construction, unlike a shader-only microbench.

Previous reports under `artifacts/prism-*.txt` are historical measurements of the
implementation at the time they were produced. They are not current performance
guarantees. Use the latest performance-pass report and its stated workload,
hardware, scale, synchronization and warm-up conditions when comparing results.
Native performance on the test system does not establish Skylake integrated-GPU
performance or 5K/120Hz output on another Mac.
