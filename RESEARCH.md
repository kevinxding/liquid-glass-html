# Reflection research and implementation guidance

Research date: 2026-10-03. Local system inspected read-only: macOS 27.0,
build `26A428`. This document separates observed evidence from proposed
approximations. It does not claim a pixel-identical Apple implementation.

## What Apple documents publicly

- [Meet Liquid Glass, WWDC25](https://developer.apple.com/videos/play/wwdc2025/219/)
  describes an adaptive collection of layers. On large surfaces, nearby colorful
  content contributes light to the material and its shadow. Larger glass has a
  thicker appearance and softer scattering. This supports separate sharp edge,
  broad surface, and shadow responses rather than one universal blend operator.
- [What's new in SwiftUI, WWDC26](https://developer.apple.com/videos/play/wwdc2026/269/)
  confirms a revised material appearance in the 2027 releases and adaptation to
  the system tint slider. It does not publish reflection shader arithmetic.
- [NSGlassEffectContainerView](https://developer.apple.com/documentation/appkit/nsglasseffectcontainerview)
  explicitly connects grouping similar glass views to fewer rendering passes.
  [Applying Liquid Glass to custom views](https://developer.apple.com/documentation/swiftui/applying-liquid-glass-to-custom-views)
  recommends grouping effects and warns that excessive independent effects can
  degrade rendering performance.
- [Improving texture sampling quality and performance with mipmaps](https://developer.apple.com/documentation/metal/improving-texture-sampling-quality-and-performance-with-mipmaps)
  explains how appropriately sized mip levels reduce texture bandwidth and
  sampling artifacts. [Optimize Metal Performance for Apple silicon Macs](https://developer.apple.com/videos/play/wwdc2020/10632/)
  explains the bandwidth cost of writing intermediate attachments to memory and
  reading them back in later passes.

These sources explain the intended behavior and architecture, not the exact
recipe used by any particular macOS control.

## Local Core Animation evidence

The following are system resources, not dependencies of this project. No private
API calls are required by the renderer and no system files were modified.

### A readable CA archive

`/System/Library/PrivateFrameworks/ImagePlaygroundInternal.framework/Versions/A/Resources/LoadingBlobAsset.ca/main.caml`

- Lines 131–146 define a `CABackdropLayer` with a `glassBackground` filter.
  Its properties independently describe face, bleed, and shadow color matrices,
  including black/white points, saturation, and fill color. Bleed also exposes
  amount, height, blur radius, distance range, opacity, and a darken-blend input.
- Lines 137–145 contain key/fill highlight controls and a separate blur-fill
  lighten opacity. Lines 159–161 use a separate SDF highlight effect and a
  `plusL` compositing filter for specular light.
- Lines 242–270 change the face black point, shadow parameters, and specular
  intensity independently for the asset's dark state.
- The backdrop layer has `marginWidth=100`, supporting the existence of a
  bounded capture margin in this asset. It is not a universal reflection reach.

The archive is an animated Image Playground asset, not a standard toolbar or
sidebar. In its base state bleed is disabled. Its numbers must not be presented
as the canonical Liquid Glass reflection preset. In particular, its directional
highlight angles are not a reason to introduce diagonal highlights into this
project; the user's reference controls take precedence.

### Shader metadata

Readable names in
`/System/Library/Frameworks/QuartzCore.framework/Versions/A/Resources/default.metallib`
include:

- `glass_background_{minimal,e,r,re,c,ce,cr,all}` variants, with both SDF and
  non-SDF entries and float/half variants. This verifies that multiple compiled
  feature combinations exist. Names alone do not establish exactly which feature
  each letter enables.
- Separate `face_cm*`, `bleed_cm*`, and `shadow_cm*` fields; independent
  `edge_bleed_amount`, `edge_bleed_blur_radius`, `edge_bleed_opacity`, and
  `bleed_darken`; separate blur-fill lighten/darken/normal opacities.
- Dedicated downsample and downsample-blur kernels, a
  `fc_backdrop_downsample` field, variable-blur kernels, and luminance-reduction
  kernels.

The initial inspection above used metadata only. The follow-up below decodes
the installed 27.0 shader's actual arithmetic and supersedes that limitation for
the specific functions described there.

### Material recipes and reduced resolution

Under
`/System/Library/PrivateFrameworks/CoreMaterial.framework/Versions/A/Resources/`:

- `platformChromeLight.materialrecipe`, `platformChromeDark.materialrecipe`,
  `toolbarButtonBackground.materialrecipe`, and `dockLight.materialrecipe` /
  `dockDark.materialrecipe` explicitly specify `backdropScale=0.25` and
  `blurAtEnd=true`.
- `modulesSheer.descendantrecipe` explicitly specifies `backdropScale=0.05` and
  low blur input quality. This is evidence of aggressive downsampling for a
  different, very broad material, not a suitable scale for sharp glass edges.
- `platformContentGlass.materialrecipe` specifies `blurRadius=45` and a color
  matrix. It does **not** specify a backdrop scale. We therefore cannot say the
  current Liquid Glass reflection path always renders at quarter resolution.

The practical lesson is to separate bandwidth-heavy broad effects from sharp
geometry: broad diffuse reflection can be filtered on a smaller texture, while
the contour and narrow edge reflection remain at display resolution.

## Independent implementation research

[medfa12/liquid-glass-react-native](https://github.com/medfa12/liquid-glass-react-native)
and its [Metal shader](https://github.com/medfa12/liquid-glass-react-native/blob/main/ios/LiquidGlass.metal)
are primary reports by an independent implementer. The author describes
transcribing QuartzCore shaders and fitting against native output. Their code
uses separately graded face/bleed/shadow layers and independently weighted
lighten, darken, and normal operations. Their README also reports feature
specialization in macOS 27.

The local metadata and CA archive corroborate the existence of those kinds of
controls. We have not independently verified that project's entire transcription,
fidelity metrics, or defaults. Its README contains both older caveats and later
updates. For example, its description says several HDR fields disappeared in
27, but this installed build still exposes `clamp_limit`, `preserve_hue`, and
`sdr_white_value` in `GlassBackgroundUniformsExt`. Build differences and movement
between structures matter. No source from that implementation was copied into
this project.

## Proposed adaptive Radiance blend

This is an original, bounded approximation designed around the requested look,
not a recovered Apple formula. Retain the explicit legacy blend operators for
comparison, but use a dedicated reflection response by default.

Pure Screen or Add cannot produce a visible colored reflection on an SDR white
surface: every channel is already at its maximum. A small, hue-selective
reduction of non-dominant channels is necessary to show color there. It should
be limited independently from the glow so dark reflected content cannot make
the entire material muddy.

One inexpensive formulation, in the renderer's existing display-referred RGB:

```text
b = clamp(material RGB, 0, 1)
c = clamp(graded reflection RGB, 0, 1)
a = max(0, intensity * valid_support * reflection_signal * spatial_weight)
g = a / (1 + a)                     # smoothly saturating energy, no hard clip
Y(x) = dot(x, [0.2126, 0.7152, 0.0722])

glow = b + (1 - b) * c * g          # bounded screen-like response on dark material
peak = max(c)
hue = c / max(peak, epsilon)
chroma = (max(c) - min(c)) / max(peak, epsilon)
bright_surface = smoothstep(0.35, 0.85, Y(b))

loss = b * (1 - hue) * chroma       # color absorption, never neutral gray shading
loss *= min(1, permitted_luma_loss / max(Y(loss), epsilon))
result = clamp(glow - loss * g * bright_surface, 0, 1)
```

Suggested initial permitted luminance losses: roughly 0.08–0.12 for diffuse,
and a somewhat larger allowance for the narrow edge. These are tuning starting
points, not Apple constants. The narrower edge should also have a stronger
response than diffuse so silhouettes remain crisp. If zero reflection RGB must
always be a no-op, preserve zero through the brightness mapping or explicitly
gate source energy before grading.

The formula needs no additional texture reads, trigonometry, or per-pixel loop.
It works with the existing brightness/contrast/saturation/vibrance controls.
Grade only valid reflected radiance; apply its spatial/support weight after
grading and never process uncovered pixels as an opaque black or gray layer.

## Performance proposals

At the start of this research, `diffuse_sample` in `glass.wgsl` evaluated up to
three nine-tap mip kernels per fragment. That is up to 27 filtered reads across
large areas, before other material effects. Intensity also makes previously
negligible tails pass the shader's work threshold, increasing the active area.
This is a concrete reason to measure intense settings, not only defaults.

1. Precompute the combined broad reflection field at a blur-appropriate reduced
   resolution; reconstruct it with one or a few reads in the final glass shader.
   Keep premultiplied color and valid-support coverage together until the final
   normalization, so empty space does not inject neutral color.
2. Separate sharp edge sampling from diffuse filtering. Do not reduce final
   contour/SDF resolution or apply diffuse-resolution shortcuts to crisp edge
   refraction.
3. Keep the near-field kernel independent of sampling reach. Additional reach
   should add smoothly weighted wider support; it should not re-normalize the
   near layer downward. Use a fixed, small number of prefiltered levels, not a
   loop whose length grows with radius or intensity.
4. Bound source captures and output work. Reuse textures/bind groups and avoid
   rebuilding unchanged geometry. Group compatible sources when scene ordering
   permits it; do not silently make one glass layer see a later scene layer.
5. Specialize expensive feature combinations only when profiling justifies it.
   Uniformly disabled effects must issue no auxiliary passes. Their zero cases
   should also skip expensive per-fragment color work.
6. Benchmark end-to-end latency **and** GPU command duration. A frame below
   8.33 ms alone does not show low GPU duty cycle or low power. A paced 120 Hz
   realistic workload and a paced 60 Hz excessive-surface workload are more
   meaningful than an unlimited render loop for heat/power evaluation.

Skylake integrated graphics and a physical 5K/120 Hz display remain separate
hardware validation targets. An offscreen 5K timing on the test system is useful evidence
about pixel workload, not proof of those devices' behavior or battery life.

## Surface-size-dependent diffuse radius

Use the smaller logical dimension (or the local inradius for custom shapes),
not area or diagonal alone. Otherwise a long, thin toolbar gets a huge radius
merely because it is wide. Derive a smooth multiplier around a reference control
size, with an explicit maximum multiplier or maximum radius. For example:

```text
t = smoothstep(reference_short_side, large_short_side, min(width, height))
effective_radius = base_radius * mix(1, maximum_multiplier, size_amount * t)
```

Defaults such as a 60-point reference, a 300-point large surface, and a capped
3× multiplier are reasonable experiments. A zero size amount reproduces the
fixed-radius behavior. Sampling reach remains an independent control. Calculate
this once per surface/update and reuse the effective radius for filtering,
padding, and falloff; recompute it continuously as a real contour morphs.

## Required pixel and resource invariants

- No valid reflected support or zero intensity leaves the underlying color
  unchanged for every blend operator.
- Neutral or black surroundings do not create a diffuse gray wash in Radiance.
- All output channels stay finite and within [0, 1], including extreme grading.
- Saturated sources make a visible, hue-consistent edge on both dark and light
  material. Diffuse color can glow on dark surfaces and tint light ones without
  large luminance loss.
- Increasing reach does not substantially weaken an unchanged nearby source.
  Sources entering capture support appear continuously rather than popping in.
- Diffuse falloff has no visible box, mip seam, or abrupt inner cutoff across the
  full radius range and at both 1× and 2× scale.
- Increasing surface size changes the effective diffuse radius smoothly and
  eventually reaches its configured cap.
- Warm static surfaces allocate/upload no geometry resources. Zero effects skip
  their intermediate passes. Increasing intensity alone must not create more
  textures, more kernel layers, or more draw calls.

## Follow-up: decoded macOS 27.0 highlight and shadow arithmetic

This section comes from the installed `26A428` QuartzCore binary, not the macOS
26 implementation or a visual imitation. The complete library SHA-256 is
`ca0bfcd654daf489f8e9d2a916d5db80286d6a2b1361f2719f8b566471ed7de9`.

### Method and reproducibility

The library contains LLVM bitcode wrappers. Reading each wrapper's offset and
length locates the bitcode; the relevant shader's name is retained in the
module. Existing Apple Clang 21 successfully decoded these modules with
`clang -S -emit-llvm -x ir`. Clang warned that it replaced the target triple with
the host's CPU triple; only the emitted text was inspected, not executed as CPU
code. No system resource was modified, no toolchain was installed, and there
was no signing workaround.

The `glass_background_all_lpf` wrapper starts at byte 455790 and contains 20000
bytes of bitcode after its 20-byte wrapper header. Its SHA-256 is
`318a03b7dd51e187ca515876fbb27c8c7d951bd9461506cc2b355d5fbd3b184d`.
The raw modules and annotated IR remain under `/tmp/gpui-glass27-research`, not
in this repository. Only compact mathematical findings and independently
evaluated scalar fixtures are retained here.

The shader's retained type metadata maps its 320-byte uniform buffer directly
to names. Key/fill direction, height, spread, amount, offset, and color bias are
at byte offsets 264, 272, 276, 280, 284, and 288. Ring offset, stroke width,
radius, opacity, and mask are at 240, 248, 252, 256, and 260. Blur-fill radius and
lighten/darken/normal weights are at 292, 296, 300, and 304.

### Distance and angular highlight profiles

For the ordinary filled-shape branch, let `d` be signed distance, negative
inside; `z = -(d + effect_offset)`; `h` be highlight height; `fw` be
`max(fwidth(d), 0.0001)`; and `sat(x) = clamp(x, 0, 1)`.

```text
t = sat(z / h)
radial = mix(t < 1 ? 1 : 0, 1-t, 0.75)
radial *= sat(z/fw + 0.5) * sat((h-z)/fw + 0.5) * sdf_support_alpha

facing = dot(normal, direction)
lobes = sat((vec2(facing, -facing) - spread) / max(1-spread, 0.000001))
lobes *= radial
H = sum(lobes / max(1 + amount*(1-lobes), 0.000001))
```

The background shader therefore uses two opposing lobes, an AA distance band,
and a rational shoulder curve. It does not use a sine-wave diagonal stripe or
the previous simple power of a normal/light dot product. Choosing a vertical
direction produces top/bottom lighting; the geometry itself does not require a
diagonal highlight.

The separate `sdf_key_fill_highlight` function permits independently colored,
sized, directed, and shaped key/fill lobes. Its radial softness is an input
instead of the background shader's fixed 0.75. Its result is the sum of key
color times its rational lobe and fill color times its rational lobe.

`sdf_diffuse_kf_highlight` adds a second, broader lobe in each direction using
fully linear distance falloff, with separate height, spread, and rational
amount. The result is `keyColor*(key+diffuseKey) + fillColor*(fill+diffuseFill)`.
The CA archive described above explicitly composites its separate specular
layer with `plusL`; this does not mean every highlight in every control uses
that same outer layer.

Raw shader `amount` is a curve parameter, not opacity: higher positive values
suppress shoulders while preserving a unit peak. A user opacity must remain a
separate multiplier with a zero-work case. Native SDF helper functions normalize
their normals; this project deliberately retains its continuous interior
direction field to avoid the previously fixed wide-band medial-axis seams.

### Background highlight blend

After face/bleed, exterior shadow, and black ring-shadow composition, the 27.0
background shader performs this per-channel color operation:

```text
color *= 1 + color_bias * H * (3 - 2*color)
```

This is material-dependent polynomial bias, not white source-over, Screen, or
Overlay. It can brighten or darken according to the sign of the bias. When the
partially transparent branch extends highlights beyond the face, it also
reconstructs the backdrop contribution, applies the face color matrix with a
highlight weight, and optionally limits that correction with per-channel
`min`/`max` according to the bias sign. A fully opaque face does not need that
extra backdrop reconstruction.

The native extended-range path can retain negative values. One final clamp
branch permits RGB as low as -0.75; a separate branch scales over-range color
uniformly to preserve hue. Directly reproducing those ranges in this project's
SDR blend operators would reintroduce invalid blend inputs. Explicit SDR bounds
are an intentional adaptation, not evidence that Apple uses [0,1] at every step.

### Shadow and ring profiles

The installed shader uses this compact polynomial, with coefficients decoded
from its half-precision constants, instead of an exponential:

```text
x = clamp(normalized_distance, -2, 2)
P(x) = 0.5 + x*(-0.560546875
                  + x²*(0.168212890625
                  + x²*(-0.034454345703125
                  + x²*0.0029544830322265625)))
```

The ordinary shadow evaluates `P(shifted_distance * inverse_radius)` and
multiplies by shadow opacity and the shifted SDF's support alpha. Radius and
positional offset are independent. For a ring, with shifted signed distance
`dr`, radius `r`, and stroke width `w`:

```text
ring = sat(P(dr/(sqrt(2)*r)) - P((dr+w)/(sqrt(2)*r)))
ring *= ring_opacity * shifted_sdf_alpha * mix(1, face_coverage, ring_mask)
```

The resulting black ring is composited over the earlier face/shadow result
before the key/fill color bias. It is the difference of two blurred boundaries,
not a broad flat inset overlay. The shader skips this branch when ring opacity
is zero. The polynomial has a residual tail of 1/4096 at its positive clamp;
subtracting two equal clamped tails cancels in the ring.
The polynomial can slightly overshoot [0,1] near its ends. Clamp the ring
**after subtraction**, not each polynomial separately: at distance 10, radius
4, stroke 1, opacity 0.5, the reference ring is 0.00171197548; premature clamps
produce 0.00146421. A standalone shadow may clamp its final scalar separately.

### Blur-fill composition and filter cost

For reconstructed material color `base` and a separate blurred sample `fill`:

```text
mixed = darken_weight * min(base, fill)
      + lighten_weight * max(base, fill)
      + (1-darken_weight-lighten_weight) * base
result = mix(mixed, fill, normal_weight)
```

The 27.0 all-features shader takes two symmetrically offset mip samples for this
blur fill. Its LOD is `max(0, log2(radius < 2 ? 1+radius/2 : radius))`; the UV
offset is `LOD / (4 * source_texture_dimensions)`. It averages premultiplied RGBA
before unpremultiplying. This confirms a cheap mip-based broad fill rather than
a large per-pixel Gaussian kernel in this stage. Mip generation cost is separate.

### Bleed color composition

The shader first applies a distinct 3×3+bias matrix to the sampled bleed color.
Its blend weight is then adapted to the existing face's luminance:

```text
band = sat((d - edge_bleed_dist0) / (edge_bleed_dist1-edge_bleed_dist0))
luma = sat(dot(face_rgb, approximately_Rec709_weights))
weight = edge_bleed_opacity * band² * (bleed_darken.x*luma + bleed_darken.y)^4
result = mix(face_rgb, graded_bleed_rgb, weight)
```

This squared spatial ramp and fourth-power luminance response are actual 27.0
arithmetic. Which way the ramp runs depends on the two provided distances.
It is not generic Overlay. The CPU-side mapping was subsequently decoded below: darken uses `Y⁴`;
the other variant uses `(1-Y)⁴`.

### Exact archive settings versus unknown control defaults

Read-only runtime inspection of a temporary `CAFilter("glassBackground")`
confirmed that angle and spread are angle-typed inputs, bleed-darken is boolean,
and the opacity inputs have [0,1] editor ranges. Unspecified values on that bare
filter are nil, so it does not expose a universal control preset.

The installed Image Playground archive explicitly supplies:

| Setting | Archive value |
| --- | ---: |
| Key/fill angle | 0.795339 radians |
| Key/fill spread | 2.0944 radians |
| Key/fill height | 54 |
| Key/fill amount | 0.02 |
| Key/fill color bias | -0.8 |
| Ring shadow stroke / blur radius | 60 / 54 |
| Ring opacity, base / dark state | 0.12 / 0.8 |
| Outer shadow radius, base / dark state | 180 / 450 |
| Outer shadow offset, base / dark state | (0,0) / (0,60) |
| Outer shadow opacity, base / dark state | 0.12 / 0.3 |

Those dimensions belong to a 1086×1086 animated blob. They are not sensible
drop-in button defaults. The subsequent host-side investigation below resolves
angle, spread, and amount conversions and supplies a real AppKit control recipe.
Do not copy the blob's angles or treat its dimensions as a canonical theme.

The independently evaluated [equation fixtures](artifacts/apple27-equations.json)
cover scalar/vector cases for the derived radial band, rational lobe,
polynomial shadow, ring, color bias, and blur-fill blend. They are float64
reference values, not a native rendering capture; half rounding and GPU FMA can
produce small differences. No macOS 26 binary was compared in this follow-up,
so historical assertions about exactly when each primitive first appeared are
intentionally omitted.

## Native macOS 27 control recipe and host parameter conversion

This follow-up inspects an `NSGlassEffectView` created by a temporary research
process on the same `27.0 / 26A428` installation. The initial captures use an
**unordered** window at an offscreen origin. A later retry orders only that
research window behind other windows at `(-10000,-10000)` (the actual frame
was checked), without activation; it produces the same values and is closed
when the research process exits. No other application's UI is inspected or
changed. Both Aqua and Dark Aqua were measured at 320×72 with
radius 36 and at 520×300 with radius 24. Values below are actual layer-model
attributes, not measured screenshots. Offscreen construction leaves several
visibility-dependent opacities at zero, so those zeros cannot establish the
opacity of a visible system control. Overriding the temporary process/window's
active-status accessors did not remove this limitation.

The control creates a `glassBackground` filter and a **separate**
`CASDFKeyFillHighlightEffect` layer. The background key/fill settings are a dark
exterior rim; they are not the bright inner specular layer:

| Setting | Both measured themes and sizes |
| --- | --- |
| Background key/fill height | 1 point |
| Background angle / spread | π/2 / 2π/3 |
| Background CA amount | 0.5 |
| Background effect offset | -1 point |
| Background color bias | -0.1875 |
| Separate key / fill angles | 0 / π |
| Separate key / fill heights | 1 / 1 point |
| Separate key / fill CA amounts | 0.5 / 0.5 |
| Separate sharp curvature | 0.75 |
| Diffuse height / amount / spread scales | 8 / 0.15 / 0.65 |
| Height and spread scale / offset | 1 / 0 |
| Separate effect `global` | false |

The separate layer's sharp spread is 80° for the capsule and 90° for the larger
rectangle; other values in this table are unchanged. The background's `offset
= -1` with `height = 1` and `z = -(d+offset)` places its support outside the face
(`0 ≤ d ≤ 1`) before AA. Its negative color bias darkens the already reconstructed
backdrop/shadow through the polynomial, instead of painting a generic inside
stroke. The source-over bright layer follows the background filter.

### Native CPU conversion resolves the shader's parameter meanings

Existing `libLTO.dylib` provides an ARM64 disassembler. A temporary process used
it to read its own loaded QuartzCore text and symbol table; no process attach,
security change, shared-cache extraction, installation, or binary modification
was required. Relevant offsets from the QuartzCore image base:

| Function | Image-relative offset |
| --- | ---: |
| `CASDFKeyFillHighlightEffect configureLayer:transaction:` | `0x37d354` |
| `CA::OGL::SDFNode::apply` | `0x2cca50` |
| `CA::OGL::GlassBackgroundFilter::render` | `0x1ca4a0` |
| `CA::OGL::emit_sdf_key_fill_highlight_simple` | `0x317358` |

Getter names were resolved from the Objective-C dispatch stubs and matched to
the render structure offsets. This confirms these conversions for the separate
SDF effect:

```text
shader_curve_amount = 1 / CA_amount - 2
shader_spread_threshold = cos(CA_spread_in_radians)
shader_direction = (sin(CA_angle), -cos(CA_angle))

soft_height = sharp_height * diffuseHeightScale
soft_CA_amount = sharp_CA_amount * diffuseAmountScale
soft_spread_radians = sharp_spread_radians * diffuseSpreadScale
```

Thus native key angle 0 faces up and fill π faces down. No diagonal source is
needed. A CA amount of 0.5 becomes raw shader curve amount 0, while the diffuse
amount `0.5 × 0.15 = 0.075` becomes `11⅓`. The diffuse scale therefore changes
shoulder shape through this reciprocal curve, **not** merely output opacity.
The bright diffuse lobe uses linear spatial falloff; the sharp lobe uses 0.75
curvature. Both keep the explicit derivative-based AA terms from the shader.
A user opacity is a separate custom gain and should bypass work at zero.
A zero CA amount must be handled by disabling the lobe rather than evaluating
its reciprocal; it is not a useful literal GPU uniform.

The background host uses the same reciprocal amount conversion. Its input
spread interpolates between SDR/HDR spreads before `cosf`; the captured control
sets both spreads to 120°. The atom identities used in the host were checked
against `CAInternAtomWithCString` in the research process: 407 is amount, 408
angle, 411 height, 412 spread, 413 SDR spread, and 351 bleed-darken.

### The bright layer uses backdrop-dependent vibrancy

The separate bright layer has no explicit `compositingFilter`; it carries a
`vibrantColorMatrix` filter with `inputClamp = 1`. Its matrix is identical for
both measured themes and sizes. The RGB part and bias are approximately:

```text
M = [ 1.120199919  -0.1894000024  -0.01899999939
     -0.05629999936 0.9870999455 -0.01909999922
     -0.05629999936 -0.1893000007 1.157400012 ]
b = (0.1471000016, 0.1471000016, 0.1471000016)
```

The fourth matrix row preserves alpha. This is **not** a normal color matrix
applied to white highlight pixels. The decoded `vibrant_color_matrix_sover`
helper transforms unpremultiplied **backdrop** RGB, clamps the transformed
alpha, premultiplies, multiplies by the **source alpha mask**, and source-overs
that result onto the backdrop. On an opaque surface the result simplifies to:

```text
target = clamp(M * backdrop_rgb + b, -0.75, 1)
result = mix(backdrop_rgb, target, highlight_mask_alpha)
```

Its neutral response is approximately `0.9118*C + 0.1471`: a full-strength
black mask reaches 0.1471, gray 0.5 reaches approximately 0.603, and white stays
white after clamping. Therefore the highlight naturally lifts dark material
more than light material and slightly changes color saturation/contrast. This
explains why plain white Add or Screen alone misses the native response. This
matrix operation is inexpensive ALU, not an extra neighborhood blur.

The SDF AIR itself returns `keyRGBA*lobe + fillRGBA*lobe` without another
RGB-times-alpha multiplication. The offscreen effect reports both CGColors as
`(1,1,1,0)`; the subsequent host path color-matches their premultiplied values.
Consequently that **offscreen zero alpha is not evidence of additive lighting**.
A native visible-control opacity calibration has not been captured. Porting the
verified vibrancy response with an explicit adjustable lobe-mask gain is a
clearer statement of what this project implements than claiming pixel equality
with every native control. The standalone `plusL` archive blob is a different
recipe and must not be substituted for this layer's blend.

For SDR output this project can deliberately clamp the target to `[0,1]` to
keep older blend modes safe; the native helper's actual lower bound is -0.75
because it participates in an extended-range color pipeline. That lower-bound
adaptation must not be described as exact native HDR behavior.

### Native bleed gating and blur-fill appearance

`GlassBackgroundFilter::render` maps the bleed-darken boolean directly:

```text
inputBleedDarkenBlend true:  bleed_darken = (+1, 0) -> luminance_gate = Y^4
inputBleedDarkenBlend false: bleed_darken = (-1, 1) -> luminance_gate = (1-Y)^4
```

The full blend remains `opacity * band² * luminance_gate`, followed by a mix
with the separately graded bleed source. Aqua selects the former and Dark Aqua
the latter in the offscreen recipe. This is concrete support for treating
colorful reflections on a dark surface as a glow while adapting their strength
to the underlying material. Combining this native gate with the project's
independent edge/diffuse controls is an adaptation, not a claim that the
system exposes those same controls.

Both measured sizes use blur-fill radius 8. Aqua selects lighten weight 0.675
and darken weight 0; Dark Aqua selects darken 0.675 and lighten 0; normal weight
is zero in both. The outer-shadow offset is `(0,8)`, ring offset is 8, ring
stroke 4, ring blur radius 5; their visibility-dependent opacity values are zero
in the unordered capture and must not be used as visible-control defaults.

The [76 independent reference cases](artifacts/apple27-equations.json) now
include host amount/spread/direction conversion, opaque-surface vibrant matrix
composition on black/gray/white/saturated colors at several masks, and both
bleed luminance gates. These check the derived equations; they do not replace a
native GPU pixel comparison or calibration of a visible system control.

Run `python3 scripts/check-native27.py` for the independent CPU f32 model's 76
reference cases, 24 explicit SDR adaptation cases, and 2507 numerical invariant
checks. This includes zero-effect neutrality, amount conversion, combined-mask
composition, wide-band direction cancellation, AA support, ring-tail behavior,
and matrix channel ordering. It does not parse shader strings or run the GPU;
render integration requires the separate GPU verification. The initial run
caught the premature polynomial clamp described above.


## Follow-up: actual macOS 27 bleed sampling (October 3)

The earlier implementation adopted native color-composition math, but its
spatial diffuse sampling remained our own. A follow-up read of the same
`glass_background_all_lpf` AIR now establishes the **sampling operation** itself.
This describes the native `edge_bleed` stage, not the similarly named diffuse
key/fill highlight, and does not yet establish every upstream backdrop operation.

Let `d` be the SDF signed distance (negative inside), `n` the two direction
channels read from the SDF texture, `M` the native `displacement_mat` mapping
those directions into source UV coordinates, `A = edge_bleed_amount`, and
`Hinv = edge_bleed_inv_height`. The shader performs:

```text
t = saturate((-d) * Hinv)
shift = A * (1 - saturate(sqrt(t * (2 - t))))
uv_bleed = uv_source + shift * (M * n)
r = edge_bleed_blur_radius
lod = max(0, log2(r < 2 ? 1 + r/2 : r))
premultiplied = sampleLevel(source_texture, uv_bleed, lod)
bleed_rgb = premultiplied.rgb / max(premultiplied.a, 1e-6)
```

It then snaps RGB components with absolute magnitude below `1e-6` to zero,
applies the separate bleed color matrix, and composites through the previously
decoded squared distance band and fourth-power luminance gate. This stage has
**one explicitly mip-filtered texture lookup**. Its LOD is set by the uniform
blur radius; it is not varied by fragment depth in this branch. The curved
outward displacement is largest at the contour and reaches zero at depth `H`.
This differs materially from our current spatial convolution and its separate
near/far layers. Radiance blending is independent and has not been changed.

Evidence in the temporary annotated AIR: the base wrapper takes distance from
SDF `.r` and direction from `.gb`; base-function values `%13..20` apply the
2x2 displacement matrix, `%300` negates distance, `%622..637` compute the curved
displacement, and `%638..656` perform LOD selection, sampling and unpremultiplication.
The immutable module hash is recorded in the earlier binary-provenance section.
No proprietary IR is included in the repository.

A fresh read-only host disassembly also resolved
`CA::Render::get_glass_filter_bleed_blur_radius` (QuartzCore image offset
`0x12a14c` on the inspected build): it returns zero when `inputBleedOpacity <= 0`,
otherwise **half of `inputBleedBlurRadius`**. The property defaults in that helper
are opacity 1 and radius 100. Atom names were independently resolved in a temporary
process: 345 amount, 346 blur radius, 354 height, 355 opacity. This helper's result
is subsequently subject to renderer-coordinate packing; it is not yet a proven
points-to-source-texels conversion for every backdrop configuration.

The recorded offscreen AppKit recipes have amount/height 25.2 for a 320x72 control
and 105 for a 520x300 control, both equal to 0.35 times the smaller dimension.
That is an observation from two sizes, **not proof of the general sizing rule**.
Their bleed opacity and blur radius are zero offscreen, so these captures cannot
calibrate a visible control's blur strength.

Remaining boundary of this reconstruction: the source backdrop's exact mip
construction, capture extent, coordinate scaling and treatment of interior content
are not established by this fragment alone. It samples the same source texture
as the face/refraction branches, without a second explicit exterior-mask lookup
here. Therefore our strict outside-only masked Gaussian pyramid must not be called
Apple's exact source-preparation path. No replacement sampler has been shipped
on the basis of this partial pipeline reconstruction.

Independent scalar sampling fixtures are in
`artifacts/apple27-bleed-sampling.json`. These establish the curved profile and
LOD conversion, not GPU pixel equivalence or the upstream filtering kernel.

### Integrated sampling reconstruction and crease regression

The subsequent implementation now replaces the 9–27-tap custom near/far diffuse
kernel with the recovered curved outward displacement and one explicit mip lookup
per cached field pixel. The final diffuse band uses the recovered squared ramp.
Radiance color processing is unchanged. Continuous atlas directions retain their
magnitude, including at medial axes; they must not be normalized back into
opposing unit vectors.

Parameter adaptation is explicit: our resolved diffuse radius is the band height;
amount is capped at 0.35 times the surface's smaller dimension; source blur radius
is twice the band height (equivalent to passing four times height to the native
host's half-radius helper). These are demo-control mappings, **not recovered
Apple defaults**. An initial mapping with blur at half the height failed the
existing detached-band regression; the final mapping passes without weakening
that test. Native Apple controls these values separately. Our exterior-only
Gaussian source pyramid and source-coordinate scale are still custom.

Our former hard saturation of source-support gain was replaced by smooth
`1-exp(-support*gain)` compression to avoid a new contour at the clamp boundary.
The native fragment's unpremultiplication is retained; no-support pixels stay
neutral. Existing shadow tint continues to use the cached field. Native-27 inner
shadow already uses the recovered polynomial ring difference, offset and stroke
calculation above; it was not changed during this follow-up.

Validation: `scripts/check-bleed27.py` checks the 15 independent scalar fixtures.
`glass-bench --verify` passes the full GPU suite, including strongest-at-rim,
no detached band, outside-only capture, empty-support neutrality, radius sweeps,
and zero-effect resource skips. The square-corner red/blue medial-axis probe
has maximum two-pixel channel differences of 2/255 at 1x and 1/255 at 2x.
The rendered 1x image was visually inspected. These checks do not establish pixel
parity with native controls or reproduce the user's exact screenshot/preset.
Results: `artifacts/native27-bleed-checks.txt`.

## Backdrop preparation, sizing and ordered reuse (October 3 follow-up)

This supersedes the earlier **Integrated sampling reconstruction** mapping above.
That implementation retained an exterior-masked source and mapped the distance
band to diffuse radius. Both were mismatches, not established native behavior.
Radiance color composition is intentionally unchanged in this follow-up.

### Native evidence and implemented stages

Read-only inspection of QuartzCore and DesignLibrary on **macOS 27.0, 26A428**,
plus filter-setter probes in a separate process, established:

| Stage | Observation | Current implementation |
| --- | --- | --- |
| Regular glass size policy | Amount and height are both `0.35 * min(width, height)` across 24 combinations: heights 24–1000, width 1100, corners 0/12/36 | Both follow this size rule, including the current dimensions of morphing shapes |
| Source | Bleed samples the face/backdrop source; no exterior mask in this path | Full physical-resolution backdrop, separate from exterior-only edge capture |
| Base mip | Unfiltered half-precision pixels; first reduction averages four half texel reads, then applies a 13-point stencil; tiny RGB values are flushed only at this reduction | Same averaging order, half constants, stencil and cutoff |
| Subsequent mips | Metal compute variant shares a coarse sample grid, uses parent offsets 2/4 and half arithmetic | Shared workgroup grid with native f16 where supported; explicit half rounding in the f32 fallback |
| Source displacement | `A * (1 - sqrt(t*(2-t)))`, `t=sat(-d/H)` | Recovered expression with continuous shape directions; no contour mirroring |
| Blur radius | Host helper halves `inputBleedBlurRadius` | Resolved diffuse radius represents this input; half-radius is converted to physical source units |
| LOD | `log2(r < 2 ? 1+r/2 : r)`, lower bounded by zero | Explicit fractional mip lookup per final fragment |
| Distance gate | Observed Distance0=1, Distance1=0, giving `sat(1-d)^2` | Fully enabled inside; removed the erroneous inward fade |
| Color | Native unpremultiplication and near-zero channel cleanup precede grading | Preserved, followed by the existing customizable Radiance/legacy operators |

Sizing observations are in `artifacts/apple27-bleed-sizing.json`. Setter stacks
located the amount/height assignments in DesignLibrary at image offsets 0x110d64
and 0x110d88. The recorded cases include direct reads after layout, not merely
values inferred from screenshots. Neither those probes nor a separate visible
regular-glass probe enabled bleed opacity/blur: both remained zero. Therefore
this is **not a recovered canonical enabled-reflection preset**. Clear-style
probes also returned zero amount/height. Our sliders intentionally enable and
customize the reconstructed stage.

The two decoded mip kernels are not interchangeable: the floating fragment path
uses offsets approximately 1.960085/3.920676, while the Metal compute path uses
an integer coarse grid and half constants. `artifacts/apple27-mip-kernel.json`
records both. The compute base-copy path also exposes three source-boundary
modes: transparent, clamp, and reflect-once-then-clamp. These refer to the capture
rectangle, **not the glass contour**. Our in-window source uses clamp.

The blur planner was also invoked in-process for radii 1–512 and scales 1/2.
It uses a 1.6 radius multiplier for mip planning, 2.8 radii of crop margin per
side, and power-of-two grid alignment (128 default cap; another branch uses 64).
The previous per-object padded capture was replaced with a window-anchored shared
pyramid. This preserves mip phase while objects move and avoids repeated huge
captures for adjacent surfaces. It is our storage/dispatch strategy, not a claim
that Apple's compositor uses this exact sharing policy.

### Reuse and flicker boundary

The GPUI backdrop hook now provides a unique frame identity, backdrop sequence
number, and conservative damage bounds since the previous backdrop. Each native
pyramid starts with a complete refresh every frame. Consecutive surfaces on the
same target update the changed source area and all dependent mip footprints.
The kernel radius is propagated through every level. A missing sequence, changed
target, unknown effect, transformed glyph, external surface, or isolated filter
boundary forces a conservative refresh. Labels painted after one surface still
enter the next surface's backdrop. No previous-frame pixels are trusted.

This was checked against full independent pyramids for 24 frames with eight
overlapping surfaces, intervening colored primitives, odd dimensions, 1x/2x,
resizing and native/exterior mode switches: the final images matched **exactly**.
The motion fixture moves a one-sided source in 33 half-pixel steps across four
shape/size cases at 1x/2x. No opposite-rim echo appeared; maximum changes were
2/255 at 1x and 1/255 at 2x. These exercise implementation discontinuities, not
Apple's compositor's temporal behavior. Native tiny-value thresholds are retained;
no temporal smoothing, hysteresis or speculative Apple-flicker workaround was added.

### Fidelity limits

This now reconstructs the sizing, source, first/subsequent mip filters, blur/LOD,
curved sampling and distance gate instead of only the sampling expression.
It still does **not** establish end-to-end pixel identity with a live native 27
control. Native backdrop-group selection, source-edge-mode selection, occlusion,
adaptive activation, transform packing and EDR/color-management context have not
all been matched. Those can affect output and are not being declared fungible.
The Lisse/custom-contour direction atlas, GPUI scene source, and Radiance operator
remain deliberate adaptations. The native source mode is the default; the debug
menu can switch to exterior-only sampling for comparison. In native mode the
sampling-radius control applies to edges, while diffuse blur has its own radius.

## Native 27 splay, distance field and refraction transition (2026-10-03)

Inspected the installed macOS **27.0 / 26A428** QuartzCore ARM64 host code and
`glass_background_sdf_all_lpf` / `glass_background_all_lpf` AIR in its
`default.metallib`. This is direct evidence from this macOS build; it is not a
claim that every iOS 27 build or every native material selects the same variant.
Native binaries and disassembly remain in the temporary research directory.
`scripts/check-splay27.py` contains an independent scalar reconstruction and
numerical checks, including the fitted supercircle distance helper.

### Splay is gradient ovalization

The host trace links `CASDFElementLayer.gradientOvalization` through the render
layer's float at offset 28, `emit_sdf_bounds`'s `s6` argument, and
`emit_sdf_bounds_internal` to the fourth component of the SDF argument vector.
All three branches of `compute_sdf_with_mode` consume that component when
mixing directions. This establishes the property-to-shader connection rather
than inferring the effect from its name.

For center-relative position `p`, half-extents `h`, geometric direction `n`,
and ovalization `s`, the recovered calculation is:

```
r = normalize(vec2(p.x, p.y * h.x / h.y))
n_splayed = normalize(mix(n, r, s))
output_direction = transform_2x2 * n_splayed
```

Equivalently, `r = normalize(p / h)`. This is **not** the mathematical gradient
of an ellipse SDF (which would divide by squared radii). It also uses the current
fragment position, not its projection onto the contour. The previous GPUI
approximation used an uncorrected ray from that projected contour point, which
fans wide capsules too aggressively toward their ends.

The glass shader now uses the recovered aspect-corrected center ray and
normalizes the two directions before mixing. It retains the magnitude of the
continuous atlas direction after the blend. Native analytic directions are unit
length; retaining our magnitude is an intentional safety adaptation at opposing
boundaries and for user-configurable bands crossing the medial axis. Exact
center normalization is guarded, so it cannot generate a NaN. Splay remains
restricted to the refraction band, costs no additional textures/passes or atlas
rebuild, and skips its arithmetic at zero. Native stores ovalization in the SDF
field shared by consumers; we apply it to lens rays only, preserving the already
accepted reflection/highlight direction behavior. Thus this is the native splay
formula adapted to our renderer, not a claim of identical complete SDF routing.
The automatic policy deciding which native materials use splay was not ported.

### Native distance and direction are separate approximations

The analytic shader has three paths:

- Rectangle (`mode < 4`): `d = max(abs(p).x-h.x, abs(p).y-h.y)` and the dominant
  axis supplies the normal. This is the interior rectangle distance, not the
  usual Euclidean exterior corner distance.
- Uniform supercircle (`mode == 4`): a fitted scalar distance, plus a separately
  evaluated rounded-corner direction. The distance is not a simple power-norm
  superellipse; the direction is not computed by differentiating that fit.
- Independent corners: evaluates four signed/reflected corner helpers and picks
  the maximum distance with that helper's direction. Circularization weights
  depend on adjacent radii and available half-extents. This max selection can
  have direction discontinuities where candidates meet; it is not evidence that
  copying it would solve every wide-band crease.

For the uniform helper, with `r = abs(radius)`, `k = 1.5286649465560913`,
`z = max((abs(p)-h+k*r)/(k*r), 0)`, `rho = length(z)`, and
`t = min(z)/max(z)` (zero when both are zero), the fitted smooth term is:

```
P(t) = (((-0.9260540008544922*t + 3.1560099124908447)*t
         - 3.6412200927734375)*t + 1.268030047416687)*t
         + 0.2685309946537018
smooth = rho + 1 - 1/(1 - t*t*saturate(rho)*P(t))
circular = length(max(k*z - 0.5286650061607361, 0))
           * 0.6541655659675598 + 0.3458344340324402
```

Separate x/y circularization weights mix those terms; a direction-dependent
weight blends the two results. The shader rounds the resulting normalized
residual (`term-1`) to **half precision**, rescales by `k*r`, then adds the
interior straight-edge contribution. Its direction instead normalizes the
positive part of `abs(p)-h+mix(k*r,r,max(circularization))`, with a dominant-axis
fallback. The reference script includes these final steps. Independent-corner
weights use `saturate((k+h/adjacent_mean_radius)*1.891557216644287)` for negative
signed adjacent means, and one otherwise, before the four-way max selection.
Host radius sign/packing for all native corner configurations has not been
reconstructed end-to-end, so the helper is retained as a research reference,
not silently substituted for Lisse.

We intentionally retain the requested Lisse/custom contours and their continuous
interior direction atlas. Switching the entire app to this native analytic SDF
would change its silhouette and arbitrary-contour behavior. This work identifies
that fidelity difference explicitly rather than treating the two as identical.

### Circular lens profile and return to undistorted content

For signed distance `d` (negative inside), inner height `H`, signed amount `A`:

```
t = saturate(-d/H)
shift = A * (1 - sqrt(t*(2-t)))
lookup = uv + displacement_matrix * sdf_direction * shift
```

The field-derived normal is not gradually turned into a different "flat normal"
near the end of the band. The **displacement magnitude** goes to zero. Its value
and first derivative are zero at depth `H`, so it joins the undistorted interior
with a C1 transition. At the outer contour its slope is singular; sampling/AA
still matters. Our existing circular profile (`profile = 1`) is algebraically
this exact curve, and was left intact. Linear and superellipse options remain
explicit customizations. Our positive distortion uses a subtracting UV convention;
native AppKit examples use negative signed amounts with an adding convention.

The native shader also supports a separate outer-refraction amount/height pair
using the same circular law. It blends the separately sampled outer color into
the inner result with a distance threshold ramp times refraction opacity. This
is not a fade against an unfiltered backdrop. Previously probed native controls
had that outer contribution disabled; it has not been added as an unsolicited
new material control. Native coverage uses `saturate(.5-d/fwidth(d))` times SDF
coverage; our existing contour AA/filtering is another deliberate difference.

These equations and the host mapping are recovered; live native-vs-GPUI pixel
identity, all transform variants, native filtering footprints, and the full
corner-parameter packing are **not** established by the scalar checks.

Validation for this change: `python3 scripts/check-splay27.py` passed aspect,
transpose/scale invariance, circularized distance, monotonic displacement and
C1-endpoint checks. Release `glass-bench --verify` passed the full optical suite
(`artifacts/splay27-optical-check.txt`), including unchanged flat-center pixels
between splay zero/one and visible straight-rim fanning. The generated checkerboard
render was inspected. This validates our implementation's behavior, not a native
compositor screenshot comparison. No new performance numbers are claimed for
this change; texture/pass/allocation structure is unchanged.
