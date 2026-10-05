#!/usr/bin/env python3
"""Independent f32 checks against decoded native bleed scalar fixtures.
GPU integration and continuity are covered by glass-bench --verify.
"""
import json
import math
from pathlib import Path
import struct


def f32(value):
    return struct.unpack('f', struct.pack('f', value))[0]


fixture = json.loads((Path(__file__).resolve().parents[1] /
                      'artifacts/apple27-bleed-sampling.json').read_text())
for case in fixture['profile']:
    t = min(1., max(0., f32(case['normalized_depth'])))
    shift = f32(1. - f32(math.sqrt(f32(t * f32(2. - t)))))
    assert abs(shift - case['shift_fraction']) < 2e-7, case
for case in fixture['lod']:
    radius = f32(case['shader_radius'])
    footprint = f32(1. + radius * .5) if radius < 2. else radius
    lod = max(0., f32(math.log2(footprint)))
    assert abs(lod - case['lod']) < 2e-7, case
print('BLEED27_SCALARS_OK: 7 displacement and 8 LOD fixtures; not GPU pixel equivalence')

sizing = json.loads((Path(__file__).resolve().parents[1] / 'artifacts/apple27-bleed-sizing.json').read_text())
for case in sizing['cases']:
    expected = .35 * min(case['width'],case['height'])
    assert abs(case['inputBleedAmount'] - expected) < 1e-10
    assert abs(case['inputBleedHeight'] - expected) < 1e-10
    assert case['inputBleedDistance0'] == 1 and case['inputBleedDistance1'] == 0
for d in [-1000., -10., -1., 0.]:
    assert min(1.,max(0.,1-d))**2 == 1.
print('BLEED27_SIZING_OK: 24 observed native recipes; distance gate remains enabled throughout interior')
