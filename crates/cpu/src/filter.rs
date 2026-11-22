// Adapted from JWildfire's FilterKernel, FilterHolder and MitchellSmoothFilterKernel.
pub(crate) struct Kernel {
    pub(crate) size: usize,
    pub(crate) weights: Vec<f64>,
}

impl Kernel {
    pub(crate) fn mitchell_smooth(radius: f64) -> Self {
        Self::new(radius, 2.0, coefficient)
    }

    pub(crate) fn gaussian() -> Self {
        Self::new(0.75, 1.5, |x| {
            (-2.0 * x * x).exp() * (2.0 / std::f64::consts::PI).sqrt()
        })
    }

    fn new(radius: f64, support: f64, coefficient: impl Fn(f64) -> f64) -> Self {
        // At oversampling 1 the Java filter width is trunc(2 * support * radius).
        let width = (2.0 * support * radius) as usize;
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
            *weight /= sum;
        }
        Self { size, weights }
    }

    pub(crate) fn taps(
        &self,
        x: usize,
        y: usize,
        border: usize,
    ) -> impl Iterator<Item = (usize, usize, f64)> + '_ {
        // Java samples output + border - (kernel_size / 2 - 1).
        let offset = if self.size == 1 {
            border
        } else {
            border + 1 - self.border()
        };
        self.weights
            .iter()
            .copied()
            .enumerate()
            .map(move |(i, w)| (x + offset + i % self.size, y + offset + i / self.size, w))
    }

    pub(crate) fn border(&self) -> usize {
        self.size / 2
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
