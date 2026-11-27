// Adapted from JWildfire 9.20 (Copyright Andreas Maschke, LGPL-2.1-or-later):
// FilterKernel, FilterHolder, MitchellSmoothFilterKernel, MitchellFilterKernel,
// GaussianFilterKernel and LogDensityFilter. Distributed under GPL-3.0-or-later.
pub(crate) struct Kernel {
    pub(crate) size: usize,
    pub(crate) weights: Vec<f64>,
}

impl Kernel {
    pub(crate) fn mitchell_smooth(radius: f64, oversample: usize) -> Self {
        Self::new(radius, oversample, 2.0, coefficient)
    }

    pub(crate) fn sine_pow10(radius: f64, oversample: usize) -> Self {
        Self::new(radius, oversample, 1.0, |x| {
            let v = (2.0 * (10.0 * x.log10()).exp() * 2.0 - 1.0).acos() / std::f64::consts::PI;
            if v.is_finite() {
                v
            } else {
                0.0
            }
        })
    }

    pub(crate) fn gaussian(oversample: usize) -> Self {
        Self::new(0.75, oversample, 1.5, |x| {
            (-2.0 * x * x).exp() * (2.0 / std::f64::consts::PI).sqrt()
        })
    }

    fn new(radius: f64, oversample: usize, support: f64, coefficient: impl Fn(f64) -> f64) -> Self {
        let width = (2.0 * oversample as f64 * support * radius) as usize;
        if width == 0 {
            return Self {
                size: 1,
                weights: vec![1.0],
            };
        }
        let size = (width + 1) | 1;
        let adjust = support * size as f64 / width as f64;
        let mut weights = Vec::with_capacity(size * size);
        let mut sum = 0.0;
        for i in 0..size {
            for j in 0..size {
                let u = ((2.0 * i as f64 + 1.0) / size as f64 - 1.0) * adjust;
                let v = ((2.0 * j as f64 + 1.0) / size as f64 - 1.0) * adjust;
                let weight = coefficient((u * u + v * v).sqrt());
                sum += weight;
                weights.push(weight);
            }
        }
        for weight in &mut weights {
            *weight = *weight / sum * oversample as f64 * oversample as f64;
        }
        Self { size, weights }
    }

    pub(crate) fn taps(
        &self,
        x: usize,
        y: usize,
        border: usize,
    ) -> impl Iterator<Item = (usize, usize, f64)> + '_ {
        // Java offsets the kernel center by one subpixel, then adds the raster border.
        let half = self.size / 2;
        self.weights
            .iter()
            .copied()
            .enumerate()
            .filter_map(move |(i, w)| {
                Some((
                    (x + border + 1 + i % self.size).checked_sub(half)?,
                    (y + border + 1 + i / self.size).checked_sub(half)?,
                    w,
                ))
            })
    }

    pub(crate) fn border(&self, oversample: usize) -> usize {
        self.size.saturating_sub(oversample) / 2
    }
}

fn coefficient(t: f64) -> f64 {
    let (b, c) = (0.42, 0.29);
    let tt = t * t;
    if t < 1.0 {
        ((12.0 - 9.0 * b - 6.0 * c) * (t * tt)
            + (-18.0 + 12.0 * b + 6.0 * c) * tt
            + (6.0 - 2.0 * b))
            / 6.0
    } else if t < 2.0 {
        ((-b - 6.0 * c) * (t * tt)
            + (6.0 * b + 30.0 * c) * tt
            + (-12.0 * b - 48.0 * c) * t
            + (8.0 * b + 24.0 * c))
            / 6.0
    } else {
        0.0
    }
}
