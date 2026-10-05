#!/usr/bin/env python3
"""Compare optimized/reference native pyramids on the GPU, without timing claims."""
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
bench = root / 'target/release/glass-bench'
with tempfile.TemporaryDirectory(prefix='glass-pyramid-') as temp:
    outputs = []
    for reference in (True, False):
        directory = Path(temp) / ('reference' if reference else 'optimized')
        env = os.environ.copy()
        env.pop('GLASS_REFERENCE_PYRAMID', None)
        if reference:
            env['GLASS_REFERENCE_PYRAMID'] = '1'
        env['GLASS_VERIFY_OUTPUT'] = str(directory)
        subprocess.run([str(bench), '--verify-native-damage'], cwd=root, env=env, check=True)
        outputs.append(directory)
    names = sorted(p.name for p in outputs[0].glob('*.png'))
    assert len(names) == 24
    for name in names:
        assert (outputs[0]/name).read_bytes() == (outputs[1]/name).read_bytes(), name
    print('SCROLL_PYRAMID_EXACT: 24 frames, 8 overlapping surfaces, 9 radii, 1x/2x; PNG pixels identical')
