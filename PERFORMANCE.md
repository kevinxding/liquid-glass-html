# Performance and validation

Hardware model and local machine identifiers are omitted for privacy. Numerical
results describe the original test system and are not portable performance guarantees.

For the current default native diffuse pipeline, see the final section of
[REFLECTION_PERFORMANCE.md](REFLECTION_PERFORMANCE.md). The measurements below
cover earlier exterior-only implementations.

Measured on 2026-10-03 with a test system, WGPU's Metal backend, and a release
build. This report covers the updated controls and sampling falloff (including independent chroma-only shadow tint; before the final material grading controls), along with the Lisse contours, five independent lighting
layers with directional blend controls, independent Fog width/opacity, and outside-only
edge/diffuse/shadow reflections. The outline is exterior, the sidebar rectangular,
and diffuse reflection uses spatial filtering to remove medial-axis facets.
Directional lighting, refraction, and edge reflection now use a cached harmonic
interior direction field, eliminating nearest-edge flips in oversized bands.

These are synchronized **complete-frame wall times**, including CPU submission,
driver overhead, and GPU completion. They are not isolated GPU shader timings or
interactive FPS. Scene construction and image readback are excluded from timed
frames. Runs occurred on an active desktop; scheduling noise affects both medians
and tails. Low-end hardware, software rendering, Intel Macs, Windows, and Linux
have not been measured. These numbers do not establish performance on a “potato.”

## Method and demo-sized workload

```sh
cargo run --release --features bench --bin glass-bench
```

A 1240 × 860 logical-pixel scene contains 1,290 backdrop detail quads and five
independently cached glass surfaces. The additional `default + sidebar` case adds
the demo's 304 × 860 opaque reflection-only sidebar. Each material uses an
independent cache, warms for 20 frames, and contributes 120 timed frames in
rotating/interleaved order at each display scale. Each sample waits for GPU
completion. All values below are milliseconds.

| Material | 1× median | 1× p95 | 2× median | 2× p95 |
| --- | ---: | ---: | ---: | ---: |
| disabled | 1.391 | 2.696 | 1.410 | 2.689 |
| clear | 2.888 | 5.463 | 3.015 | 4.392 |
| default | 1.943 | 5.651 | 3.358 | 5.992 |
| default + sidebar | 3.074 | 6.810 | 4.548 | 7.323 |
| light/reflection off | 1.787 | 5.424 | 3.170 | 4.623 |
| lights only | 1.821 | 5.523 | 3.131 | 4.608 |
| edge reflection only | 2.997 | 5.674 | 3.284 | 5.999 |
| diffuse reflection only | 1.865 | 5.672 | 3.325 | 6.006 |
| reflection max radius | 5.377 | 6.877 | 5.725 | 9.780 |
| heavy frost | 4.588 | 6.082 | 4.777 | 6.380 |

`light/reflection off` retains default transmission optics, so it isolates the
added finish from Frost, refraction, and dispersion. The edge-only and diffuse-only
cases also retain transmission but disable lighting and the other reflection
layer. Clear disables Frost/dispersion; heavy Frost uses Frost 20, dispersion 3,
and distortion 40. Max reflection radius is 240 logical pixels. Fog is disabled
in these timing cases; enabling its nonzero-opacity blur introduces a separate
blur chain. It is covered by optical and pass-skipping tests.

At 2×, the complete default demo-sized workload including the sidebar takes
**4.548 ms median / 7.323 ms p95**, versus 1.410 ms median without any glass.
That is a +3.138 ms complete-frame median difference for this synthetic workload.

## Stress and update behavior

The stress scene contains 1,200 detail quads and 24 tiled surfaces; the final row
uses 64 smaller surfaces in the same viewport. Each case warms for 12 frames and
then alternates 80 effect frames with 80 no-effect baseline frames. Cases run
sequentially so unused material caches do not inflate memory pressure. Reflection-
only cases use opaque panels with transmission/lighting disabled. The additional
`diffuse + shadow` case uses 24 opaque surfaces, a 14 px two-layer shadow, an 80 px
inward diffuse fade, and a 240 px exterior sampling radius. Moving-object
cases reuse 12 scene phases; radius changes cycle through 16, 96, and 240 logical
pixels after warming the largest retained capture region.

| Material | 1× median | 1× p95 | 2× median | 2× p95 |
| --- | ---: | ---: | ---: | ---: |
| optics only | 6.816 | 7.079 | 10.219 | 12.836 |
| default | 7.657 | 12.234 | 12.841 | 16.705 |
| edge only | 4.739 | 5.884 | 6.359 | 8.637 |
| diffuse only | 4.808 | 6.258 | 6.337 | 8.825 |
| max reflection radius | 4.745 | 5.746 | 10.031 | 11.431 |
| diffuse + shadow | 4.725 | 4.971 | 10.383 | 13.812 |
| moving exterior objects | 3.390 | 4.804 | 6.393 | 8.760 |
| radius changes | 4.699 | 5.128 | 9.877 | 12.490 |
| 64 edge surfaces | 10.251 | 14.247 | 14.976 | 18.056 |

Every stress case asserts that the live surface count remains cached and that
**all 80 timed frames allocate zero surface slots and upload zero shape atlases**.
This includes 64 simultaneous surfaces, moving outside objects, and repeated
radius changes within the warmed allocation. The live-source regression also
checks that changing an exterior object's color updates its reflection while
reusing the same resources.

Warm-frame cost is distinct from setup: first stress frames took
**560–1134 ms**, including pipeline creation, shape-atlas construction, and
GPU allocation. This demo is interaction-driven after setup. A first encounter
with a larger capture extent can still grow its textures; warm radius decreases
and increases within the retained maximum reuse them. The raw report records
first-frame, median, p95, maximum, baseline, and allocation counters per case.

## Earlier compute optimization comparison (2026-10-02)

The first implementation built reflection mips using a render pass per level.
That implementation generated up to eight levels with two compute dispatches
inside one pass per surface. The current implementation extends the same four-level
chunks to the complete mip chain for wide diffuse glows. It also retains capture capacity across radius
changes and skips all lens math for opaque reflection-only panels. The same
workloads, hardware, and timing method were used before and after the pass:

| 2× workload | Before compute optimization | After compute optimization (Oct 2) | Reduction |
| --- | ---: | ---: | ---: |
| 5 surfaces: default | 5.230 | 3.367 | 35.6% |
| 24 surfaces: edge only | 12.591 | 4.928 | 60.9% |
| 24 surfaces: radius changes | 17.250 | 6.352 | 63.2% |
| 64 edge surfaces | 10.251 | 14.247 | 14.976 | 18.056 |

These historical results precede the current lighting/diffuse fixes; current timings
are in the tables above. These sequential active-desktop runs are directional measurements rather than
precise isolated costs for a particular shader instruction. Edge-only stress has
unchanged material parameters; the small default lighting adjustments affect the
default case. The 64-surface gain reflects reduced per-surface mip-pass submission overhead.
Separate cache assertions confirm that its live working set remains cached.

## Work avoided

- Build an outside-only, alpha-weighted radiance pyramid at half physical
  resolution. Interior pixels are masked **before** building any mip levels,
  including concave outlines, preventing interior color from leaking into mips.
- Edge reflection uses one anisotropic/trilinear texture sample. Diffuse reflection
  uses five fixed coarse-mip taps at the current pixel, only within its inward band.
  This removes nearest-normal projection seams; the fade is strongest at the rim.
  Shadow color uses the same five-tap radiance filter only on visible shadow pixels.
  Sampling radius never introduces a ray march or an unbounded per-pixel loop.
  Farther edge sources select softer filtering; nearby sources retain tangent detail.
- Generate the radiance pyramid in one compute pass. Shared-memory reductions
  produce four levels per dispatch, with explicit odd/skinny mip-tail handling.
- Extend boundary directions continuously into the interior during atlas creation.
  Opposing directions cancel without being renormalized into a discontinuity.
  This adds geometry-setup work but no atlas reads or passes per frame.
- Reuse pipelines, textures, bind groups, and shape distance/direction atlases.
  Captures grow with the needed sampling halo but retain capacity when radius
  decreases. Above a soft 128-slot threshold, evict only slots idle for two seconds;
  never clear the active set each frame.
- At zero edge, diffuse, and shadow reflection intensities, allocate no radiance pyramid and issue no
  reflection pass. At zero effective Fog opacities or blur, allocate no Fog chain and issue no
  Fog blur passes. Disabled values also do not enlarge captures.
- Gate all five lighting layers independently by opacity; a zero master disables
  them all. Shadow/outline calculations are skipped inside fully covered material.
  Shadow reflection alone also skips its prefilter pass when the shadow is disabled.
  Opaque panels without reflections skip backdrop capture entirely;
  opaque reflective panels skip transmission/refraction math and blur passes.
- Keep normalized Gaussian blur, filtered reductions, adaptive refraction
  footprint filtering, and spectral dispersion's zero-value fast path.
- Keep readback out of the interactive app. Continuous animation remains opt-in;
  ordinary redraws follow interaction and focused-input caret blinking.

## Correctness checks

The release headless renderer passed all of the following on the measured backend:

- Every lighting layer contributes separately. Zero component opacities or a zero
  light master restore identical pixels, regardless of width/radius settings.
- Dark outline changes the exterior, leaves fully covered interior pixels alone,
  and covers the shadow. The shadow is offset downward. Disabled shadow reflection
  adds no pass, while enabled reflection picks up exterior color.
- Angles and insets reposition glints and inner shadows. Six offered reflection
  blend modes produce distinct pixels in both the inner diffuse and outer shadow.
- Diffuse color is strongest immediately at the rim, with no detached inner peak.
  Across the former diagonal seam, maximum two-pixel channel differences are 4/255
  at 1× and 2/255 at 2×. Capsule and concave visual fixtures also render smoothly.
- Wide-band checks cover square corners, circular corners, capsules, notches, and
  flowers with radii up to 1000 px on 130 px-high shapes. Maximum interior adjacent
  pixel steps are 9/255 at 1× and 5/255 at 2×, without the former wedge boundaries.
  Geometry tests also cover 1 px-wide/tall rectangles and padding invariance.
- Rectangular panel corners are fully filled. Light/dark/blue blend comparison
  images were inspected; defaults use vertical Screen glints and SoftLight shoulders.
- Both effective Fog opacities zero produce identical pixels and no passes. Fog width varies
  independently of refraction width; uniform Frost remains pre-refraction blur.
- Edge and diffuse reflections reject interior colored objects exactly. The
  outside-only check also covers the concave Flower and Notched shapes.
- Near exterior stripes retain more normalized tangent contrast than distant
  stripes: 0.8672 versus 0.5439 at 1×, and 0.9018 versus 0.5440 at 2×.
- Live exterior color updates appear using existing resources. Disabled effects
  schedule no unnecessary blur/reflection passes; an inactive opaque panel skips
  backdrop capture.
- Splay preserves the flat center, smooth capsules render at 2×, all demo outlines
  render, clipping is respected, and pixels far outside glass/shadows are preserved.

The explicit native GPU mip test compares every generated level against an
independent CPU reduction for 10 odd/skinny dimensions (including 1-pixel axes),
with RGBA16Float tolerance 0.001. It passed. `scripts/check-crates.sh` also covers
geometry, normalized blur kernels, WGSL validation, scrolling, doctests, formatting,
and Clippy. The earlier independent-consumer smoke test lives in
`scripts/check-reuse.py`; that checks unpatched smoothing and explicit glass-patch
application/idempotence/source-drift rejection.

Run visual/optical checks without timing:

```sh
cargo run --release --features bench --bin glass-bench -- --verify
```

`--verify-wide-bands` selects the oversized directional-band checks;
`--verify-lighting` selects the rim/outline/shadow/blend checks;
`--verify-reflection` selects the light/Fog/reflection checks, and `--stress`
selects just the stress cases. `GLASS_BENCH_CASE='64 edge'` optionally narrows a
stress run. The native mip test is intentionally ignored by default because it
requires a usable GPU; run it explicitly through the glass crate's test manifest.

Current raw timing and verification logs: `artifacts/benchmark.txt`,
`artifacts/wide-band-benchmark.txt`, `artifacts/wide-band-checks.txt`, and
`artifacts/wide-band-crate-checks.txt`.
The previous complete report is `artifacts/benchmark-before-wide-bands.txt`.
Before compute optimization:
`artifacts/benchmark-reflection-before-optimization.txt`. Representative output:
`artifacts/light-reflection-reference-sheet.png`, `reflection-distance-filtering.png`,
`reflection-distance-filtering-2x.png`, `lighting-blend-study.png`, `diffuse-rim-*.png`,
`layered-shadow-reflection.png`, `wide-bands-1x.png`, `wide-bands-2x.png`, and
individual `light-*.png` images.

## GPU timestamp limitation

The existing `gpu-cost` diagnostic could not obtain usable encoder or pass-boundary
GPU intervals on the test system / WGPU 29 / Metal setup. It reports failure instead of
presenting zero-duration samples as measurements. Only synchronized complete-frame
wall times support the performance claims above.

## Customisation validation (October 3)

The full-radius quintic sampling fade and gentle contrast response passed a
42-position approach sweep (excluding overlap with the outside-only mask): the
maximum 4 px response step was 6.807% of peak. Seven diffuse blend modes leave a
neutral surround pixel-identical. JSON roundtrip/validation, independent fog
endpoints, 96 px blur, source brightness, and 800 px glow with 48 px sampling pass.

A 24-surface, 1,500 px diffuse glow with only 48 px sampling took 6.318 ms median /
8.304 ms p95 at 2x, with zero warmed slot allocations or atlas uploads.
See `artifacts/customisation-benchmark.txt` for the complete run. These measurements
were made with the demo open on an active desktop.

Final native optical validation also passes bounded-input comparisons for six
blend operators, independent shadow-tint chroma at 0%/70% black-shadow opacity,
and material brightness/contrast/vibrance. Fully saturated red remains unchanged
under vibrance while a muted color gains chroma. See
`artifacts/customisation-final-optics.txt`. The final grading controls were added
after the timed run; no extra textures or rendering passes were introduced.

## Prism and subsequent shared-material changes

The regenerated-contour implementation and its independent motion benchmark are
recorded in [PRISM.md](PRISM.md) and `artifacts/prism-rebuilt-shapes.txt`. They
supersede the initial affine demo. Nine simultaneous contours rebuild their GPU
SDFs each frame without warmed texture allocations or CPU-atlas uploads.

Shared changes after the older timing tables above include the nine-tap diffuse
filter with a Gaussian depth tail, host-selected tint color, and independent
edge/diffuse brightness ranges with 0–10x controls. The final full optical suite
passed (`artifacts/prism-final-optics.txt`); maximum 4px reflection approach step
was 6.494% of peak. The older complete-demo timing tables have not been rerun for
this final filter footprint and should not be treated as final-build timings.
