# Reflection performance pass — October 2026

Hardware model and local machine identifiers are omitted for privacy. Numerical
results describe the original test system and are not portable performance guarantees.

On the test system, the 48-surface cranked workload's measured GPU stage coverage
falls from **9.047 to 6.297 ms median (30.4%)**. The final realistic 5K workload
finishes within **7.465–7.522 ms at p95**; cranked 2× stress finishes within
**14.459 ms at p95**. Warm allocations and shape uploads remain zero. Cached
effect textures fall **80.2% versus the intermediate full-resolution reflection
pipeline** (503.762 → 99.860 MiB); original texture-byte counters were unavailable.
Power and low-end hardware performance remain unverified. These are measured
workload results, not a guarantee of presentation frame rate or negligible load.

Prism is an intentionally excessive workload generator. Fieldnotes remains the
appearance demonstration. A fast measured frame on one Mac does not establish a
low GPU duty cycle, acceptable power use, or performance on another GPU.

## Reproduce

```sh
cargo build --release --features bench --bin glass-bench
GLASS_PERF_LABEL=current target/release/glass-bench --performance
```

Optional environment variables:

- `GLASS_BENCH_CASE=5k` selects case names containing that substring.
- `GLASS_PERF_SAMPLES=160` changes the sample count (default 80; minimum 8).
- `GLASS_PERF_IMAGES=1` saves representative frames **after** timing.
- `GLASS_PERF_LABEL=experiment-name` names `artifacts/perf-experiment-name.txt`.

The suite exercises 1,200 bright moving detail quads in 12 deterministic scene
phases. Each workload warms for 12 frames before taking 80 samples interleaved
with an identical no-effect scene. Stable IDs are reused. Every case asserts
zero warmed texture-slot allocations and zero warmed shape-atlas uploads. These
assertions do not count unrelated renderer/driver allocations.

Desktop scenes contain ten surfaces: a full-height opaque reflective sidebar,
five toolbar capsules, two floating cards, a popover, and a bottom input bar.
They run at 1440 × 940 logical pixels at 1× and 2×; 5K scenes use 2560 × 1440
logical pixels at 2×, producing exactly **5120 × 2880** device pixels. The stress
scene has 48 tiled materials (36 opaque reflective panels, 12 refracting glass
panels) at 1440 × 940 logical pixels at both scales.

Strong desktop settings use edge/diffuse intensity 10, edge width 12 px,
diffuse radius 500 px, sampling reach 240 px and shadow reflection intensity 2.
The cranked stress case raises diffuse radius to 1500 px. Intensity, radius and
surface count are independent. Two additional 2× cases hold every parameter
fixed and compare intensity 1 with intensity 10. Their capture/pass counts,
capture/prefilter texel counts and cached texture bytes must match exactly;
timing differences are reported without a noise-sensitive threshold.

This synthetic test does not include live GPUI
text layout, input events, spring construction, native presentation or the actual
Prism scene tree; run the native app to inspect those additional costs.

## Timing boundaries

The principal measurement is synchronized **CPU submission + GPU completion wall
time**, excluding scene construction, image readback and native presentation.
The comparison baseline renders the same detail geometry with no glass effects.
Median differences are illustrative; they are not isolated shader time.

The benchmark-only GPU timing path on Metal brackets the **already prepared**
frame on its own native command queue with two four-byte blit markers. The
reported interval is the first marker's documented `GPUEndTime` to the second
marker's `GPUStartTime`. Both markers and the complete WGPU frame are submitted
without a wait between them. CPU frame encoding occurs before that interval.
This measures the GPU queue interval, **including inter-command-buffer scheduling
gaps** and possible overlap of independent render/compute/blit stages. It is
a diagnostic proxy and must not be called isolated shader execution, a strict
upper bound, or GPU active duty cycle. In particular the approximately 0.04 ms
5K no-effect marker interval is too small to confidently attribute the full
pixel workload to that interval. Performance conclusions use the synchronized
wall measurement and resource/work counters, not this proxy. Normal applications allocate or encode none of this
instrumentation. Other supported backends can use optional WGPU timestamp
queries; invalid zero results are reported unavailable.

On the test system, advertised WGPU timestamp query support returned raw zero start
and end values with encoder writes, empty compute pass boundaries, and nonempty
blit boundary markers. Those zeros were rejected rather than treated as zero
cost. WGPU 29 prohibits inspecting its command encoder through mutable HAL after
WGPU encoding; the final implementation uses only the public queue handle and
new native marker buffers. It does not patch WGPU internals or intercept Metal
methods.

GPU timing is a separate sweep. Wall samples use the original uninstrumented
submission path. GPU timings are not power, energy, battery life or thermals.
Noninteractive `powermetrics` access was unavailable because `sudo -n` requires
a password on this account. No password prompt or privileged configuration
change was attempted. A subsequent public Instruments Power Profiler recording
returned “not supported on macOS; record on iOS or iPadOS instead.” Power remains
unmeasured.

Budgets: **8.333 ms at 120 Hz**, **16.667 ms at 60 Hz**, **22.222 ms at 45 Hz**.
An application needs spare time inside those budgets for its own work and the
window compositor.

## Before optimization

Measured on the test system using Metal and an optimized release binary. The
baseline executable was preserved before renderer changes at
`/tmp/glass-bench-perf-baseline` (SHA-256
`6f588c74b8acdea5383d9555c3b691a4904fcec492f27b8017f296d0a02b28af`).
Raw report: `artifacts/perf-baseline.txt`. This binary predates GPU timestamp
instrumentation, so that report contains completion wall times only. A later
public Instruments capture of this exact preserved binary provides actual GPU
stage intervals, without needing timestamp support in the binary.

| Workload | Output pixels | Median ms | p95 ms |
| --- | --- | ---: | ---: |
| Desktop default 1× | 1440 × 940 | 3.555 | 5.877 |
| Desktop default 2× | 2880 × 1880 | 4.770 | 6.109 |
| Desktop strong 1× | 1440 × 940 | 4.584 | 5.882 |
| Desktop strong 2× | 2880 × 1880 | 6.209 | 7.406 |
| 5K default | 5120 × 2880 | 6.048 | 7.435 |
| 5K strong | 5120 × 2880 | 6.072 | 7.576 |
| Prism cranked 1× | 1440 × 940 | 8.760 | 10.450 |
| Prism cranked 2× | 2880 × 1880 | 13.140 | 14.031 |

All cases reported zero warmed slot allocations and atlas uploads. Cold first
frames took 0.28–1.15 seconds, including pipeline compilation, shape-atlas
creation and allocations; the warm results do not characterize startup latency.

## Initial optimized architecture measurement

Raw report: `artifacts/perf-optimized.txt`, binary SHA-256
`35009ee63d3f5cfd8f24e33c021654db705e6d1c3a23b5eec6eed79e2e9b3491`.
This run includes the new bounded diffuse field, split capture path, automatic
radius and updated reflection default. It is an intermediate result, not an
assertion that every workload improved. Representative frames are saved as
`artifacts/perf-optimized-<case>.png` after timing.

| Workload | Wall median ms | Wall p95 ms | GPU queue median ms | GPU queue p95 ms | Effect textures MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| Desktop default 1× | 3.361 | 7.113 | 1.623 | 3.060 | 35.472 |
| Desktop default 2× | 4.839 | 7.358 | 2.015 | 3.839 | 69.944 |
| Desktop strong 1× | 3.431 | 7.296 | 1.527 | 3.944 | 53.508 |
| Desktop strong 2× | 4.886 | 7.508 | 2.029 | 3.961 | 133.560 |
| 5K default | 4.668 | 7.273 | 2.606 | 4.478 | 76.403 |
| 5K strong | 4.788 | 7.347 | 2.764 | 4.813 | 142.064 |
| Prism cranked 1× | 9.780 | 13.863 | 4.685 | 6.753 | 179.331 |
| Prism cranked 2× | 11.979 | 13.249 | 6.675 | 8.507 | 503.762 |

All measured realistic-case p95 wall times fit within 8.333 ms, and both cranked
synthetic cases fit within 16.667 ms in this run. This is not a 120/60 FPS native
presentation guarantee. The 1× cranked wall measurement regressed relative to
the earlier baseline; desktop default 2× was effectively flat. Effect-texture
memory excludes native framework render targets, buffers and driver memory.
The 2× cranked workload still captures 41.02 million reflection texels per frame
and builds 2.57 million diffuse field texels, identifying a remaining cost to
address. No warmed effect-slot allocations or atlas uploads occurred.

## Instruments execution validation of the intermediate workload

A separate **Metal System Trace** recording of the preserved intermediate
optimized binary validates actual GPU stage execution for the 48-surface,
cranked 2× case. Unlike the native marker proxy, these intervals come from
Instruments' `metal-gpu-intervals` table, filtered to the benchmark process.
The parser groups the known synthetic scene by `main_pass`, requires all 48
glass composites for a complete effect frame, discards incomplete frames, and
unions overlapping Vertex/Fragment/Compute intervals across **all depth lanes**
rather than adding them. Depth is timeline placement; valid commands may occur
only on a higher lane.
It excludes internal PendingWrites/Transit buffers and diagnostic markers.
The first 13 frames of each kind are excluded. Profiling and an active desktop
can change scheduling and timings.

| Observed GPU stage coverage | Complete frames | Median ms | p95 ms |
| --- | ---: | ---: | ---: |
| Cranked 48-surface frame | 578 | 6.701 | 8.664 |
| No-effect geometry baseline | 578 | 0.349 | 0.977 |

For effect frames, per-channel medians are Compute **4.911 ms**, Fragment
**2.544 ms**, and Vertex **0.459 ms**. These channels overlap and must not be
summed. The median first-to-last GPU stage span is 7.432 ms, including gaps.
This confirms a substantial compute cost in the intermediate architecture and
also confirms that the lightweight native marker baseline undercounts actual
rendering. It does **not** measure power or whole-device utilization.

The filtered evidence is retained in
`artifacts/reflections-metal-target-intervals.json.gz` (1.6 MiB), with a
per-frame report in `artifacts/reflections-metal-execution-summary.json`.
The 4.1 GiB full trace and unfiltered exports were deleted after extraction.
The parser is `src/bin/bench_support/analyse_metal_trace.py`.

Reproduce with the desired release binary and workload. The original recording
used 10 seconds and 500 requested samples; a short diagnostic can use fewer:

```sh
xcrun xctrace record --template 'Metal System Trace' --output run.trace \
  --time-limit 3s --no-prompt --env GLASS_BENCH_CASE=prism-cranked-2x \
  --env GLASS_PERF_SAMPLES=32 --launch -- target/release/glass-bench --performance
xcrun xctrace export --input run.trace --toc --output toc.xml
xcrun xctrace export --input run.trace \
  --xpath '/trace-toc/run[@number="1"]/data/table[@schema="metal-gpu-intervals"]' \
  --output intervals.xml
# Use the launched target PID from toc.xml:
python3 src/bin/bench_support/analyse_metal_trace.py intervals.xml \
  --pid TARGET_PID --output execution.json --retain-intervals target-intervals.json.gz
```

## Final bounded-field measurement

Raw report: `artifacts/perf-final.txt`; preserved release binary SHA-256
`662f39afe707d15646e0e9a6a2572d6ccfdbde5df0e7a8a6753dd9ba03e0f5b6`.
The final architecture uses a coarse exterior radiance field, a separate detailed
edge field, a smoothly overlapping Gaussian pyramid, bounded diffuse sampling,
and exact zero-parameter/flat-interior work skips. Representative frames are
`artifacts/perf-final-<case>.png`.

| Workload | Wall median ms | Wall p95 ms | Wall p99 ms | Effect textures MiB |
| --- | ---: | ---: | ---: | ---: |
| Desktop default 1× | 3.460 | 7.303 | 8.549 | 30.455 |
| Desktop default 2× | 4.980 | 7.570 | 7.604 | 38.877 |
| Desktop strong 1× | 4.818 | 7.327 | 8.405 | 33.792 |
| Desktop strong 2× | 4.863 | 6.242 | 6.407 | 42.214 |
| 5K default | 6.147 | 7.465 | 8.648 | 43.234 |
| 5K strong | 5.988 | 7.522 | 7.555 | 46.714 |
| Prism cranked 1× | 10.303 | 11.999 | 12.179 | 88.210 |
| Prism cranked 2× | 11.979 | 14.459 | 14.667 | 99.860 |
| Prism intensity 1, 2× | 10.698 | 13.117 | 13.669 | 99.860 |
| Prism intensity 10, 2× | 10.534 | 13.789 | 15.725 | 99.860 |

Every realistic-case p95 fits the 8.333 ms budget in this run; several p99
samples exceed it. The stress workload fits 16.667 ms at p95 and p99. This is
measured completion latency, **not a guarantee of uninterrupted 120/60 FPS** in
an application with presentation, layout and other work. Desktop default 2× and
5K default did not improve versus the original wall-time baseline; the cranked
1× case regressed. The preserved reports expose these results rather than
selecting only faster cases.

Against the intermediate pipeline, cranked 2× reflection capture drops from
41,019,264 to **1,346,400 texels per frame** (96.7% lower), and cached effect
textures drop from 503.762 to **99.860 MiB** (80.2% lower). Strong 5K textures
drop from 142.064 to **46.714 MiB** (67.1% lower). These counters exclude GPUI's
own render targets, buffers and driver memory. Capture and diffuse-field texel
counts are identical at 1× and 2×; high-resolution refraction/compositing still
scales with output resolution.

Increasing both reflection intensities from 1 to 10 at a fixed radius changes
**none** of the encoded capture/blur/reflection counts, capture/prefilter texels,
or cached effect texture bytes. The assertion passes. All ten cases have zero
warmed slot allocations and zero warmed atlas uploads. Cold first frames take
0.287–0.953 seconds and include pipeline/shape preparation; this remains separate
from steady-state performance.

The independent native dynamic verification also passes. Nine simultaneous
shape-changing surfaces at 2× measure 7.750 ms median / 8.912 ms p95, with no
CPU atlas uploads and no warmed allocations. This is a deliberately excessive
concurrent morph case; its p95 exceeds the 120 Hz budget. The materialise
animation averages 4.240 ms in that test. These results do not include native
presentation or establish smoothness on other hardware.

## Actual GPU execution after bounded-field changes

Short three-second target recordings use the same all-depth interval-union
analysis as the intermediate trace above. Warmed complete frames are retained;
profiling overhead, GPU scheduling and the active desktop apply.

| Workload | Complete effect frames | GPU coverage median ms | GPU coverage p95 ms | Baseline median ms |
| --- | ---: | ---: | ---: | ---: |
| Cranked 48 surfaces, 2× | 64 | 6.297 | 7.768 | 0.348 |
| Realistic default 5K | 64 | 3.269 | 4.152 | 0.893 |

The stress Compute channel median falls from **4.911 to 3.172 ms** (35.4% lower),
while whole-frame covered interval falls from 6.701 to 6.297 ms (6.0% lower).
The final recording has more Vertex-stage scheduling overlap/stalls than the
intermediate recording (1.103 vs 0.459 ms median), illustrating why short
trace comparisons cannot isolate shader cost perfectly. Fragment median is
2.257 ms; overlapping channel medians must not be added. The remaining largest
labeled stage is `glass-exterior-pyramid`; composite cost is still approximately
1.614 ms across 48 surfaces. The stage span, including gaps, is 7.846 ms median.

A matching capture of the **original pre-task binary** provides the true
before-optimization GPU comparison: 32 warmed complete cranked 2× frames cover
**9.047 ms median / 9.333 ms p95**, against a 0.348 ms geometry baseline.
Compute median is 6.301 ms, Fragment 4.853 ms and Vertex 0.499 ms. Comparing
with the bounded-field final result, stage coverage is **30.4% lower** and
Compute coverage **49.7% lower**. Original capture alone cost 1.472 ms versus
0.217 ms after the split path. Whole-stage coverage includes overlap and
scheduling effects, and is still not a power measurement. Evidence is in
`artifacts/reflections-metal-original-summary.json` and the corresponding
target-only compressed interval file.

For 5K, Compute median is 0.638 ms, Fragment 2.222 ms and Vertex 0.514 ms.
The 3.269 ms covered interval includes ordinary geometry and output rendering;
the identical no-effect scene covers 0.893 ms. This provides a measured realistic
pixel-workload result, but does not demonstrate negligible GPU load at 120 Hz
or establish energy consumption.

Reproducible summaries and target-only compressed intervals are retained as
`artifacts/reflections-metal-final-summary.json`,
`artifacts/reflections-metal-final-target-intervals.json.gz`,
`artifacts/reflections-metal-final-5k-summary.json`, and
`artifacts/reflections-metal-final-5k-target-intervals.json.gz`.
Raw full traces/unfiltered exports are removed after extraction.

## Paired-pyramid experiment

A two-level Gaussian dispatch prototype was validated against the reference
filter, then A/B tested before deciding whether to retain it. It does not change
the measured texture memory or capture/prefilter texel counts.

| Adjacent release run | Wall median ms | Wall p95 ms |
| --- | ---: | ---: |
| Paired cranked 2× | 10.209 | 12.509 |
| Reference cranked 2× | 10.437 | 13.379 |
| Reference strong 5K | 4.692 | 7.026 |
| Paired strong 5K | 4.889 | 8.498 |

Actual compute coverage was 3.715 ms median in the paired trace and 4.060 ms in
an adjacent reference trace, but their no-effect baselines also shifted from
0.411 to 0.587 ms. The earlier reference measured 3.172 ms compute with a
0.348 ms baseline. These changing conditions prevent attributing the apparent
trace difference to pairing. The small stress wall difference and worse 5K
p95 do not establish a repeatable gain. **The simpler unpaired implementation
is retained.** The prototype binary is preserved for audit at
`/tmp/glass-bench-perf-paired`, SHA-256
`de390f09cd644000db7a52854c52329e92fe69cee34df128b1d86ef7b70617de`.

Raw text reports are `artifacts/perf-paired-stress.txt`,
`artifacts/perf-unpaired-stress.txt`, `artifacts/perf-paired-5k.txt`, and
`artifacts/perf-unpaired-5k.txt`. Filtered trace summaries/intervals use the
`reflections-metal-paired` and `reflections-metal-unpaired-adjacent` prefixes.
No performance claim is based on selecting only this experiment's best run.

## Limits

The desktop was active during measurements. Scheduling and other applications
can affect both the baseline and the effect. No Skylake Intel i3, discrete GPU,
Windows, Linux, battery, acoustic, sustained thermal or energy measurement was
performed. A 5K offscreen framebuffer tests the exact pixel workload; it does
not verify a physical 5K monitor's refresh, compositor or presentation behavior.

Timing API references: [Apple gpuStartTime](https://developer.apple.com/documentation/metal/mtlcommandbuffer/gpustarttime), [Apple GPU scheduling and resource dependencies](https://developer.apple.com/videos/play/wwdc2022/10101/).

## Native-27 bleed sampling follow-up

Replaced the prior 9–27-tap cached diffuse convolution with one curved-displacement
mip sample per cached field pixel. The source Gaussian pyramid and final one-tap
field fetch remain. Radiance blending is unchanged. Additional SDF sampling and
scalar profile arithmetic replace those source taps.

A fresh 32-sample run (`artifacts/perf-native27-bleed.txt`) on the same test system
reported synchronized CPU-submit + GPU-completion times:

- Strong desktop, 2880×1880, 10 surfaces: median 4.665 ms, p95 5.022 ms.
- Cranked Prism, 2880×1880, 48 surfaces: median 10.283 ms, p95 11.714 ms.
- Warm allocations and atlas uploads remained zero.
- Reflection intensity 1 versus 10 retained identical encoded work and texture bytes.

These are headless completion timings, not presentation FPS or power measurements;
this shorter run is not a controlled speedup comparison with earlier runs. Native
GPU marker intervals remain diagnostic for the reasons documented above.

## Full-resolution native diffuse pipeline follow-up

This supersedes the timing interpretation of the earlier cached, exterior-only
bleed port. The new default includes a full physical-resolution backdrop and
Apple's decoded Metal mip filter. This is more work than the previous custom
coarse source; the earlier 4.7 ms strong-desktop result is not this pipeline.

An initial independent-pyramid implementation reached about 2.7 GiB of effect
textures in cranked Prism. Sharing scratch storage reduced this to 144.7 MiB,
but repeatedly rebuilding it still cost about 45 ms. Conservative incremental
updates between consecutive surfaces reduced capture work from approximately
261 million to 11.9 million texels/frame. The first use every frame still fully
refreshes the backdrop; source changes between surfaces are propagated through
all mips. Pixel comparisons against independent full rebuilds were identical.

Final release results on the test system (48 measured samples after 12 warmups;
interleaved no-effect baseline; `artifacts/perf-native27-final.txt`):

| Workload | Output pixels | Surfaces | Completion median | p95 | Cached effect textures |
| --- | --- | ---: | ---: | ---: | ---: |
| Default desktop | 2880×1880 | 10 | 6.502 ms | 7.975 ms | see raw report |
| Strong desktop | 2880×1880 | 10 | 6.991 ms | 7.877 ms | see raw report |
| Default 5K | 5120×2880 | 10 | 8.871 ms | 9.364 ms | 192.1 MiB |
| Strong 5K | 5120×2880 | 10 | 8.962 ms | 10.146 ms | 194.1 MiB |
| Cranked Prism | 1440×940 | 48 | 12.813 ms | 14.662 ms | 91.7 MiB |
| Cranked Prism Retina | 2880×1880 | 48 | 15.667 ms | 16.832 ms | 144.7 MiB |

These include CPU submission and GPU completion, not scene creation, presentation,
or readback. They are not displayed FPS. The 5K result is still outside an 8.333 ms
120 Hz budget; no claim of an always-120-FPS or negligible-power implementation
follows. Cranked Retina Prism is around the 60 Hz completion budget. Warm texture
allocations and atlas uploads were zero. Intensities 1 and 10 had identical
capture texels, passes and texture bytes, with medians 15.599 and 15.565 ms.

A 16×16 workgroup experiment produced exactly the same eight fixture images as
8×8, but did not establish a timing improvement: strong 5K measured 8.929 ms
with a lower 2.481 ms baseline, compared with 8.962/2.686 ms for 8×8. The tested
8×8 layout is retained. Results: `artifacts/perf-native27-16x16-5k.txt`.

Validation artifacts:

- `native27-damage-check.txt`: exact full/incremental image equality over 24 frames.
- `native27-bleed-motion.txt`: eight source-motion cases, no opposite-edge echo.
- `native27-mip-check.txt`: independent GPU/CPU mip-stage comparison, including
  odd and one-pixel axes, for both native and legacy kernels.
- `native27-pipeline-checks.txt`: full existing optical/resource-skip suite;
  exterior-only assertions explicitly select the legacy source mode.
- `native27-unit-tests.txt`: library tests, WGSL validation and documentation examples.

Apple Silicon native half arithmetic is enabled only where supported. The
f32 fallback explicitly rounds the same stages to half. Low-end hardware and
fallback-path speed remain unmeasured; no thermal, acoustic or power conclusion
is inferred from these tests.

## Scrolling and idle pass (2026-10-03)

Retained changes:

- Each surface remembers its complete uploaded uniform block. Byte-identical
  blocks reuse the existing GPU buffer contents, avoiding queue uploads and
  staging work while the content scrolls under stationary controls. Bounds,
  optical settings, source damage, viewport, scale and morph dimensions remain
  part of the comparison. Recreated slots/devices always upload again.
- Native diffuse-only surfaces skip the exterior capture/reduction pyramid,
  which is used by edge reflections and the legacy exterior-only diffuse mode.
  Native backdrop preparation and sampling remain unchanged.
- The performance harness now separates moving chart content, vertical scrolling
  (`GLASS_PERF_MOTION=scroll`) and forced redraws of unchanged content (`idle`).
  The latter is deliberately *not* described as a sleeping application's cost.
- The native damage fixture now includes nine radii from 0.1 through 2048 logical
  pixels, edge-reflection enable/disable transitions, eight overlapping surfaces,
  24 frames, 1x/2x, odd sizes, resizing and changing source content.

`python3 scripts/check-scroll-pyramid.py` runs the normal and reference-work paths
and compares all 24 images exactly. `GLASS_REFERENCE_PYRAMID=1` is a diagnostic
reference-work override: it also forces uniform uploads and exterior preparation.
It is not an application quality setting. The normal path keeps the complete
native mip chain and full precision; no resolution/quality scaling was introduced.

### Experiments that were rejected

A fused scene-to-first-mip implementation removed most full-resolution copy
traffic, but introduced small half-precision differences (up to 5/255 in the
stress fixtures). It was removed. A non-anisotropic compute sampler matched the
fixture pixels but showed no dependable speedup, so it was removed too.

Aggressive mip truncation initially appeared faster, then exposed a bad tradeoff:
heterogeneous surface sizes could request levels missing from the preceding
surface's pyramid and cause extra full-resolution refreshes. Per-level validity
and a conservative shared-mip floor were tested; neither established a reliable
5K benefit. Mip truncation was removed from the shipped implementation. Reports
with `fused`, `capped`, `retained`, `shared-floor`, or `sampler` in their names are
**intermediate experiments**, not measurements of the shipped build.

### Measurement interpretation

Earlier 48-sample sweeps ranged from roughly 4.7–6.6 ms for a Retina desktop and
12–16 ms for cranked Retina Prism. Some early comparisons suggested a substantial
Prism gain; adjacent runs did not reproduce it. 5K timings and unchanged-content
redraw timings also varied considerably. No large general speedup is claimed.
The retained optimizations remove demonstrable work; they do not establish a
negligible GPU load, 120 Hz presentation guarantee, or a power/thermal improvement.

The uniform counters showed zero repeated material uploads in steady scrolling
and unchanged-content fixtures (480 reuses for 10 surfaces over 48 frames;
2,304 for 48 surfaces). Warm texture allocations and atlas uploads remained zero.
The final reports listed below supersede intermediate timings.

A launched default/non-animated Fieldnotes instance showed CPU snapshots of
0.4–0.9%; its accumulated CPU time grew slowly. Both Fieldnotes and paused,
settled Prism already stop requesting animation frames. This pass does not
introduce a timer, readback, or polling loop. These observations are not a
controlled GPU-power or battery measurement. Live trackpad scrolling, presentation
cadence, low-end hardware and thermal/acoustic behavior remain unmeasured.

Reproduce after building `glass-bench` with `--release --features bench`:

```
python3 scripts/check-scroll-pyramid.py
GLASS_PERF_MOTION=scroll GLASS_PERF_LABEL=my-scroll glass-bench --performance
GLASS_PERF_MOTION=idle GLASS_PERF_LABEL=my-idle glass-bench --performance
```

Use `target/release/glass-bench` when it is not on PATH. Time builds and benchmarks
separately. The reported wall time is CPU submission plus GPU completion, excluding
scene construction, readback and display presentation. Native GPU marker intervals
retain the limitations documented earlier in this file.

Final retained-build measurements (48 samples, 12 warmups, test system; completion
wall times in milliseconds):

| Workload | Median | p95 |
| --- | ---: | ---: |
| Retina desktop, scrolling, default | 6.096 | 7.679 |
| Retina desktop, scrolling, strong | 5.511 | 7.487 |
| Retina Prism, scrolling, cranked | 13.501 | 14.566 |
| Retina desktop, forced unchanged-content redraw | 6.144 | 7.544 |
| 5K desktop, scrolling, default | 7.397 | 7.518 |
| 5K desktop, scrolling, strong | 8.780 | 9.077 |

These are a final snapshot, not a demonstrated percentage speedup. In particular,
strong 5K still exceeds the 8.333 ms budget before display presentation. The final
build retains the same native capture texel counts as the original when edges are
on (7,180,940 per default Retina desktop frame; 16,948,765 at default 5K), and zero
warm allocations/atlas uploads. Every steady fixture performed zero material
uniform uploads after warmup. Turning edge reflection off additionally removes
its exterior capture/reduction dispatches; the image-equivalence test covers
turning that path off and back on.

Final artifacts:

- `artifacts/perf-shipped-scrolling.txt`
- `artifacts/perf-shipped-idle-redraw.txt`
- `artifacts/perf-shipped-scrolling-5k.txt`
- `artifacts/scroll-pyramid-exact.txt` — original-work/optimized pixels identical
- `artifacts/scroll-idle-optical-check.txt` — complete optical/resource-skip suite
- `artifacts/scroll-idle-unit-tests.txt` — 18 unit and 3 documentation tests passed
- `artifacts/scroll-idle-clippy.txt` — crate all-target strict Clippy passed
