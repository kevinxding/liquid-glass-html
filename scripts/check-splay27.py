#!/usr/bin/env python3
"""Independent scalar reconstruction of macOS 27 (26A428) SDF/lens equations.

No native code or private runtime dependency. This is a numerical research
reference, not a pixel-identity test against Apple's compositor. The application
keeps Lisse contours and continuous interior directions; see RESEARCH.md.
"""
import math
import struct


def sat(x):
    return min(1., max(0., x))


def unit(v):
    n = math.hypot(*v)
    return tuple(x / n for x in v) if n else (0., 0.)


def ovalize(p, half_size, normal, amount):
    radial = unit((p[0], p[1] * half_size[0] / half_size[1]))
    return unit(tuple((1. - amount) * n + amount * r for n, r in zip(normal, radial)))


def refraction(depth, height, amount):
    t = sat(depth / height)
    return amount * (1. - math.sqrt(t * (2. - t)))


def supercircle(p, half_size, radius, circularity):
    """Recovered uniform-corner distance/direction helper, positive quadrant.

    circularity contains separate x/y circularization weights. Host packing of
    all corner configurations is not reproduced here. Zero-radius uses rect mode.
    Native rounds the normalized distance residual to half before rescaling.
    """
    r = abs(radius)
    k = 1.5286649465560913
    reach = k * r
    nreach = reach + (r - reach) * max(circularity)
    q = tuple(p[i] - half_size[i] + nreach for i in range(2))
    z = tuple(max(0., (p[i] - half_size[i] + reach) / reach) for i in range(2))
    rho = math.hypot(*z)
    ratio = min(z) / max(z) if max(z) else 0.
    polynomial = (((-0.9260540008544922 * ratio + 3.1560099124908447)
                   * ratio - 3.6412200927734375) * ratio + 1.268030047416687)
    polynomial = polynomial * ratio + 0.2685309946537018
    smooth = rho + 1. - 1. / (1. - ratio * ratio * sat(rho) * polynomial)
    round_z = tuple(max(0., k * x - 0.5286650061607361) for x in z)
    circular = math.hypot(*round_z) * 0.6541655659675598 + 0.3458344340324402
    dx = smooth + (circular - smooth) * circularity[0]
    dy = smooth + (circular - smooth) * circularity[1]
    side = 1. if z[1] > z[0] else -1.
    w = sat(0.5 - side + side * ratio)
    residual = struct.unpack('e', struct.pack('e', dx + (dy - dx) * w - 1.))[0]
    d = min(max(q), 0.) + reach * residual
    normal = unit(tuple(max(x, 0.) for x in q))
    if normal == (0., 0.):
        normal = (1., 0.) if q[0] > q[1] else (0., 1.)
    return d, normal


def check():
    # Known aspect-corrected ray: normalized half-extents imply a 45-degree ray.
    v = ovalize((175., 56.), (350., 112.), (0., 1.), 1.)
    assert abs(v[0] - math.sqrt(.5)) < 1e-12 and abs(v[1] - v[0]) < 1e-12
    # Transposing a tall/wide shape and uniformly resizing it preserves the effect.
    for a in (0., .12, .5, 1.):
        v = ovalize((175., 56.), (350., 112.), (0., 1.), a)
        rotated = ovalize((56., 175.), (112., 350.), (1., 0.), a)
        scaled = ovalize((350., 112.), (700., 224.), (0., 1.), a)
        assert max(abs(v[i] - rotated[1-i]) for i in range(2)) < 1e-12
        assert max(abs(v[i] - scaled[i]) for i in range(2)) < 1e-12
    # Native circular profile is algebraically our profile=1; zero value/slope at H.
    for height in (.5, 18., 56., 500.):
        values = [refraction(height*i/1000, height, 36.) for i in range(1001)]
        assert values[0] == 36. and values[-1] == 0.
        assert all(a >= b for a, b in zip(values, values[1:]))
        assert refraction(height*2, height, 36.) == 0.
        eps = height * 1e-5
        assert abs(refraction(height-eps, height, 1.) / eps) < .00002 / height
        for i in range(1001):
            t = 1.-i/1000
            assert abs(values[i] - 36.*(1.-math.sqrt(1.-t*t))) < 1e-11
    # Circularized SDF agrees with the analytic rounded rectangle to half tolerance.
    for x in range(0, 121, 3):
        for y in range(0, 71, 3):
            q = (abs(x)-80., abs(y)-30.)
            exact = math.hypot(max(q[0], 0.), max(q[1], 0.))+min(max(q), 0.)-20.
            d, n = supercircle((x,y), (100.,50.), 20., (1.,1.))
            assert abs(d-exact) < .04, (x,y,d,exact)
            assert abs(math.hypot(*n)-1.) < 1e-12
    print('SPLAY27_EQUATIONS_OK: aspect, rotation, scale, circular profile, C1 transition, circularized SDF')


if __name__ == '__main__':
    check()
