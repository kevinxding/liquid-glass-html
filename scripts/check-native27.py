#!/usr/bin/env python3
"""CPU equation checks for the native-27 shader adaptation; never starts a GPU.

The fixture values were independently derived from installed QuartzCore AIR and
host ARM64. These implementations use f32 arithmetic to approximate WGSL, not
substring checks or a GPU interpreter. They verify the numerical contract and
its intentional SDR differences; shader integration still needs its GPU tests.
"""
from collections import Counter
import json
import math
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "artifacts/apple27-equations.json"


def f32(x):
    return struct.unpack("f", struct.pack("f", x))[0]


def clamp(x, lo=0.0, hi=1.0):
    return min(hi, max(lo, x))


def mix(x, y, a):
    return f32(f32(x * f32(1.0 - a)) + f32(y * a))


def radial_band(z, height, fwidth, softness):
    z, height, fwidth, softness = map(f32, (z, height, fwidth, softness))
    ramp = f32(1.0 - clamp(f32(z / max(height, 0.0001))))
    core = mix(float(ramp > 0), ramp, softness)
    outer = clamp(f32(f32(z / fwidth) + 0.5))
    inner = clamp(f32(f32(f32(height - z) / fwidth) + 0.5))
    return f32(f32(core * inner) * outer)


def rational_lobe(mask, amount):
    mask, amount = map(f32, (mask, amount))
    denominator = f32(1.0 + f32(amount * f32(1.0 - mask)))
    return f32(mask / max(denominator, 0.000001))


def shadow_polynomial(normalized_distance):
    x = clamp(f32(normalized_distance), -2.0, 2.0)
    x2 = f32(x * x)
    coefficient = 0.0029544830322265625
    for c in (-0.034454345703125, 0.168212890625, -0.560546875):
        coefficient = f32(c + f32(x2 * coefficient))
    return f32(0.5 + f32(x * coefficient))


def ring_shadow(distance, radius, stroke, opacity):
    scale = f32(f32(radius) * f32(math.sqrt(2.0)))
    outside = shadow_polynomial(f32(f32(distance) / scale))
    inside = shadow_polynomial(f32(f32(distance + stroke) / scale))
    return f32(clamp(f32(outside - inside)) * f32(opacity))


def key_fill_color_bias(rgb, bias, highlight):
    return [f32(f32(c) * f32(1.0 + f32(f32(bias * highlight) * f32(3.0 - f32(2.0*c))))) for c in rgb]


def blur_fill(base, fill, lighten, darken, normal):
    # Native reference stage, not currently an additional pass in glass.wgsl.
    result = []
    for b, f in zip(base, fill):
        combined = f32(f32(darken*min(b,f)) + f32(lighten*max(b,f)))
        combined = f32(combined + f32(f32(1.0-darken-lighten)*b))
        result.append(mix(combined, f32(f), f32(normal)))
    return result


def ca_amount_to_curve(amount):
    return f32(f32(1.0/f32(amount))-2.0)


def ca_angle_to_direction(radians):
    angle = f32(radians)
    return [f32(math.sin(angle)), f32(-math.cos(angle))]


def ca_spread_to_threshold(radians):
    return f32(math.cos(f32(radians)))


# Rounded constants used by the WGSL helper, intentionally independent of the
# fixture's matrix values. Row/column transposition changes saturated cases.
MATRIX = ((1.1201999, -0.1894, -0.019),
          (-0.0563, 0.98709995, -0.0191),
          (-0.0563, -0.1893, 1.1574))


def vibrant_target(color, lower_bound):
    target = []
    for row in MATRIX:
        value = f32(f32(color[0])*f32(row[0]))
        value = f32(value + f32(f32(color[1])*f32(row[1])))
        value = f32(value + f32(f32(color[2])*f32(row[2])))
        target.append(clamp(f32(value+f32(0.1471)), lower_bound, 1.0))
    return target


def native_control_vibrancy(backdrop_rgb, mask):
    target = vibrant_target(backdrop_rgb, -0.75)
    return [mix(f32(c), t, f32(mask)) for c, t in zip(backdrop_rgb, target)]


def shader_vibrancy(color, mask):
    # Deliberate SDR adaptation: target clamp precedes interpolation.
    target = vibrant_target(color, 0.0)
    return [mix(f32(c), t, f32(clamp(mask))) for c, t in zip(color, target)]


def native_bleed_luma_gate(darken, luminance):
    luma = f32(clamp(luminance))
    value = luma if darken else f32(1.0-luma)
    square = f32(value*value)
    return f32(square*square)


def key_fill(normal, depth, width, direction, inset, curve, spread, aa, softness, opposite=1.0):
    radial = radial_band(depth-inset, width, aa, softness)
    facing = f32(f32(normal[0]*direction[0])+f32(normal[1]*direction[1]))
    lobes = [f32(clamp(f32((sign*facing-spread)/max(1.0-spread, 0.000001)))*radial) for sign in (1, -1)]
    return f32(rational_lobe(lobes[0], curve) + f32(opposite*rational_lobe(lobes[1],curve)))


FUNCTIONS = {f.__name__: f for f in (
    radial_band, rational_lobe, shadow_polynomial, ring_shadow,
    key_fill_color_bias, blur_fill, ca_amount_to_curve, ca_angle_to_direction,
    ca_spread_to_threshold, native_control_vibrancy, native_bleed_luma_gate)}


def assert_close(actual, expected, label, tolerance=3e-6):
    if isinstance(expected, list):
        assert len(actual) == len(expected), label
        for i, (a, b) in enumerate(zip(actual, expected)):
            assert_close(a, b, f"{label}[{i}]", tolerance)
    else:
        assert math.isfinite(actual), f"{label}: nonfinite {actual}"
        assert abs(actual-expected) <= tolerance*max(1.0,abs(expected)), f"{label}: actual={actual} expected={expected}"


def invariants():
    checks = 0
    # The native matrix is applied to a combined sharp+soft alpha mask once.
    # Exercise clipping as well as midtones, so double application cannot be
    # mistaken for equivalent additive highlighting.
    color = [.15, .4, .8]
    sharp, soft = .6, .35
    expected = shader_vibrancy(color, sharp+soft)
    sequential = shader_vibrancy(shader_vibrancy(color, soft), sharp)
    assert max(abs(a-b) for a,b in zip(expected,sequential)) > 0.001
    checks += 1

    # Native amount=.5 is the unmodified lobe; diffuse .075 suppresses shoulders
    # without reducing its unit peak. Multiplying by .15 would fail the peak.
    for mask in (0., .001, .1, .5, .9, 1.):
        assert_close(rational_lobe(mask, ca_amount_to_curve(.5)), mask, 'sharp amount')
        diffuse = rational_lobe(mask, ca_amount_to_curve(.075))
        assert -1e-6 <= diffuse <= mask+1e-6
        checks += 2
    assert_close(rational_lobe(1.,ca_amount_to_curve(.075)),1.,'diffuse peak')
    checks += 1

    # AA support and direction extremes, including wide bands meeting a medial
    # axis. The intentionally non-normalized zero vector must not produce a
    # diagonal seam, NaN, or spurious directional light at positive thresholds.
    up = ca_angle_to_direction(0)
    for width in (.125, 1., 8., 128., 1024.):
        for aa in (.25,.5,1.,2.):
            for softness, curve, spread in ((.75,0.,math.cos(math.radians(80))), (1.,ca_amount_to_curve(.075),math.cos(math.radians(52)))):
                for depth in (-2*aa, -.5*aa, 0., .5*aa, width*.5, width, width+aa):
                    for normal in ((0.,-1.),(0.,1.),(1.,0.),(0.,0.),(.00001,-.00001)):
                        value = key_fill(normal,depth,width,up,0.,curve,spread,aa,softness)
                        assert math.isfinite(value) and 0. <= value <= 1.000001
                        if depth < -.5*aa or depth > width or normal == (0.,0.):
                            assert_close(value,0.,'unsupported lobe')
                        checks += 1

    for c in ([0.,0.,0.],[.5,.5,.5],[1.,1.,1.],[1.,0.,0.],[0.,1.,0.],[.03,.7,.2]):
        assert_close(shader_vibrancy(c,0.),c,'zero highlight preserves material')
        assert_close(shader_vibrancy(c,3.),shader_vibrancy(c,1.),'bounded highlight gain')
        for mask in (0.,.1,.5,1.):
            assert all(0. <= v <= 1. for v in shader_vibrancy(c,mask))
            checks += 1
        checks += 2

    # Ring difference cancels both nonzero polynomial clamp tails, independent
    # of radius. Zero stroke/opacity cannot tint material or form a broad ring.
    for radius in (.25,1.,4.,64.,1024.):
        for d in (-10000.,-radius,0.,radius,10000.):
            assert_close(ring_shadow(d,radius,0.,1.),0.,'zero stroke')
            assert_close(ring_shadow(d,radius,8.,0.),0.,'zero ring opacity')
            checks += 2
        assert_close(ring_shadow(radius*4,radius,1.,1.),0.,'ring exterior tail')
        checks += 1

    # Polynomial parity and actual finite tail. Do not silently substitute an
    # exponential, or claim the standalone polynomial reaches exactly zero.
    for i in range(1001):
        x = i*.004
        assert_close(shadow_polynomial(x)+shadow_polynomial(-x),1.,'shadow symmetry')
        checks += 1
    assert_close(shadow_polynomial(100.),1/4096,'native residual tail')
    checks += 1
    return checks


def main():
    fixture = json.loads(FIXTURE.read_text())
    counts = Counter()
    sdr_checks = 0
    for i, case in enumerate(fixture['cases']):
        name, args = case['function'], case['input']
        assert name in FUNCTIONS, f"unsupported fixture function {name}"
        assert_close(FUNCTIONS[name](**args),case['output'],f"fixture {i} {name}")
        counts[name] += 1
        if name == 'native_control_vibrancy':
            # Derive the SDR oracle from independently frozen full-mask cases,
            # not this implementation's matrix constants.
            full = next(c['output'] for c in fixture['cases'] if c['function']==name and c['input']['backdrop_rgb']==args['backdrop_rgb'] and c['input']['mask']==1)
            expected = [base*(1-args['mask'])+clamp(target)*args['mask'] for base,target in zip(args['backdrop_rgb'],full)]
            assert_close(shader_vibrancy(args['backdrop_rgb'],args['mask']),expected,f"SDR fixture {i}")
            sdr_checks += 1
    checks = invariants()
    print(f"NATIVE27_EQUATIONS_OK: {sum(counts.values())} frozen native references, {sdr_checks} SDR adaptations, {checks} invariant checks")
    print("CPU f32 model only; does not claim native pixel matching or execute the WGSL/GPU.")


if __name__ == '__main__':
    main()
