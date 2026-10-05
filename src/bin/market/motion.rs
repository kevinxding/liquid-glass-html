//! Time-based spring integration, with bounded substeps after a stalled frame.
#[derive(Clone, Copy)]
pub struct Spring {
    pub value: f32,
    pub target: f32,
    velocity: f32,
}
impl Spring {
    pub fn new(value: f32) -> Self {
        Self {
            value,
            target: value,
            velocity: 0.,
        }
    }
    pub fn toggle(&mut self) {
        self.target = 1. - self.target;
    }
    pub fn active(&self) -> bool {
        (self.value - self.target).abs() > 0.0005 || self.velocity.abs() > 0.005
    }
    pub fn step(&mut self, dt: f32, reduced: bool) {
        if reduced {
            self.value = self.target;
            self.velocity = 0.;
            return;
        }
        let dt = dt.min(0.05);
        let steps = (dt / 0.004).ceil().max(1.) as u32;
        for _ in 0..steps {
            let h = dt / steps as f32;
            self.velocity += (240. * (self.target - self.value) - 21. * self.velocity) * h;
            self.value += self.velocity * h;
        }
        if !self.active() {
            self.value = self.target;
            self.velocity = 0.;
        }
    }
}
/// Animate optics rather than cross-fading a filtered backdrop with the original.
pub fn materialise(mut p: gpui_glass::GlassParams, progress: f32) -> gpui_glass::GlassParams {
    let t = progress.clamp(0., 1.);
    p.dynamic_shape = false;
    p.surface = None;
    p.blur *= t;
    p.fog *= t;
    p.edge *= t;
    p.distortion *= t;
    p.dispersion *= t;
    p.opacity *= t;
    p.light *= t;
    p.saturation = 1. + (p.saturation - 1.) * t;
    p.brightness *= t;
    p.contrast = 1. + (p.contrast - 1.) * t;
    p.vibrance *= t;
    p.reflection.edge_intensity *= t;
    p.reflection.diffuse_intensity *= t;
    p.reflection.shadow_intensity *= t;
    p.shape_scale = [1., 1.];
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn materialisation_keeps_geometry_fixed() {
        for progress in [0., 0.1, 0.5, 1.] {
            let p = materialise(gpui_glass::GlassParams::default(), progress);
            assert_eq!(p.shape_scale, [1., 1.]);
            assert!(!p.dynamic_shape);
        }
    }
    #[test]
    fn spring_overshoots_and_settles_at_different_refresh_rates() {
        for hz in [30., 60., 120.] {
            let mut s = Spring::new(0.);
            s.target = 1.;
            let mut peak = 0_f32;
            for _ in 0..(hz * 3.) as usize {
                s.step(1. / hz, false);
                peak = peak.max(s.value);
            }
            assert!(peak > 1.01 && peak < 1.15);
            assert!(!s.active());
            assert_eq!(s.value, 1.);
            s.toggle();
            s.step(1. / hz, true);
            assert_eq!(s.value, 0.);
        }
    }
}
