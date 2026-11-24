// Sampling equations adapted from JWildfire 9.20 (Andreas Maschke and contributors, LGPL-2.1-or-later).
// This Rust port is distributed under GPL-3.0-or-later.
use crate::{
    filter::Kernel,
    scene::{Scene, Transform, VariationKind},
};

const TABLE_SIZE: usize = 1024;
const SEED: i64 = 741103597;

struct Marsaglia(i32, i32);
impl Marsaglia {
    fn seeded(seed: i64) -> Self {
        let u = (seed << 16) as i32;
        Self(u, u >> 16)
    }
    fn next(&mut self) -> f64 {
        self.1 = 36969i32
            .wrapping_mul(self.1 & 65535)
            .wrapping_add(self.1 >> 16);
        self.0 = 18000i32
            .wrapping_mul(self.0 & 65535)
            .wrapping_add(self.0 >> 16);
        (f64::from((self.1 << 16).wrapping_add(self.0)) * (1.0 / 2147483647.0)).abs()
    }
}

fn table(transforms: &[Transform]) -> [usize; TABLE_SIZE] {
    let total: f64 = transforms.iter().map(|t| t.weight).sum();
    let mut boundary = 0.0;
    std::array::from_fn(|_| {
        let mut cumulative = 0.0;
        let selected = transforms
            .iter()
            .position(|t| {
                cumulative += t.weight;
                cumulative > boundary
            })
            .unwrap_or(transforms.len() - 1);
        boundary += total / TABLE_SIZE as f64;
        selected
    })
}

struct Point {
    x: f64,
    y: f64,
    z: f64,
    color: f64,
}
impl Point {
    fn apply(&mut self, t: &Transform) {
        let [a, b, d, e, tx, ty] = t.affine;
        let ax = a * self.x + d * self.y + tx;
        let ay = b * self.x + e * self.y + ty;
        let mut x = 0.0;
        let mut y = 0.0;
        let mut z = 0.0;
        for variation in &t.variations {
            let amount = variation.amount;
            match variation.kind {
                VariationKind::Linear3D => {
                    x += amount * ax;
                    y += amount * ay;
                    z += amount * self.z;
                }
                VariationKind::Spherical3D => {
                    let r = amount / (ax * ax + ay * ay + self.z * self.z + 1.0e-300);
                    x += ax * r;
                    y += ay * r;
                    z += self.z * r;
                }
                VariationKind::Waves => {
                    x += amount * (ax + d * crate::fast_sin::sin(ay / (tx * tx + 1.0e-300)));
                    y += amount * (ay + e * crate::fast_sin::sin(ax / (ty * ty + 1.0e-300)));
                }
                VariationKind::Bubble => {
                    let radius = (ax * ax + ay * ay) / 4.0 + 1.0;
                    let scale = amount / radius;
                    x += scale * ax;
                    y += scale * ay;
                    z += amount * (2.0 / radius - 1.0);
                }
                VariationKind::Spiral => {
                    let radius = crate::fast_sqrt::sqrt(ax * ax + ay * ay) + 1.0e-300;
                    let scale = amount / radius;
                    x += (ay / radius + crate::fast_sin::sin(radius)) * scale;
                    y += (ax / radius - crate::fast_sin::cos(radius)) * scale;
                }
                VariationKind::Pdj {
                    a: pa,
                    b: pb,
                    c: pc,
                    d: pd,
                } => {
                    x += amount * (crate::fast_sin::sin(pa * ay) - crate::fast_sin::cos(pb * ax));
                    y += amount * (crate::fast_sin::sin(pc * ax) - crate::fast_sin::cos(pd * ay));
                }
                VariationKind::Disc => {
                    let radius_pi =
                        std::f64::consts::PI * crate::fast_sqrt::sqrt(ax * ax + ay * ay);
                    // JWildfire getPrecalcAtan takes the affine X coordinate first.
                    let scale = amount * crate::fast_atan::atan2(ax, ay) / std::f64::consts::PI;
                    x += crate::fast_sin::sin(radius_pi) * scale;
                    y += crate::fast_sin::cos(radius_pi) * scale;
                }
                VariationKind::Swirl => {
                    let r2 = ax * ax + ay * ay;
                    let sin = crate::fast_sin::sin(r2);
                    let cos = crate::fast_sin::cos(r2);
                    x += amount * (sin * ax - cos * ay);
                    y += amount * (cos * ax + sin * ay);
                }
                VariationKind::Rectangles {
                    x: cell_x,
                    y: cell_y,
                } => {
                    let reflect = |v: f64, cell: f64| {
                        if cell.abs() < 1.0e-8 {
                            v
                        } else {
                            (2.0 * (v / cell).floor() + 1.0) * cell - v
                        }
                    };
                    x += amount * reflect(ax, cell_x);
                    y += amount * reflect(ay, cell_y);
                }
                VariationKind::Hemisphere => {
                    let r = amount / crate::fast_sqrt::sqrt(ax * ax + ay * ay + 1.0);
                    x += ax * r;
                    y += ay * r;
                    z += r;
                }
            }
        }
        self.x = x;
        self.y = y;
        self.z = z;
        let c1 = (1.0 + t.symmetry) * 0.5;
        let c2 = t.color * (1.0 - t.symmetry) * 0.5;
        self.color = self.color * c1 + c2;
    }
}

pub struct Sampler<'a> {
    scene: &'a Scene,
    state: Point,
    rng: Marsaglia,
    table: [usize; TABLE_SIZE],
    size: [usize; 2],
    projection_offset: [f64; 2],
    sin_roll: f64,
    cos_roll: f64,
    span: [f64; 2],
    scale: [f64; 2],
}

impl<'a> Sampler<'a> {
    pub fn new(scene: &'a Scene, width: usize, height: usize) -> Self {
        Self::with_border(scene, width, height, 0)
    }

    fn with_border(scene: &'a Scene, width: usize, height: usize, border: usize) -> Self {
        let mut rng = Marsaglia::seeded(SEED);
        let mut state = Point {
            x: 2.0 * rng.next() - 1.0,
            y: 2.0 * rng.next() - 1.0,
            z: 0.0,
            color: rng.next(),
        };
        rng.next(); // Initial material draw, even for the ordinary color raster.
        let table = table(&scene.transforms);
        state.apply(&scene.transforms[0]);
        for _ in 0..=42 {
            state.apply(&scene.transforms[select(&mut rng, &table)]);
        }
        let ppu = pixels_per_unit(scene, width, height);
        let image = [width, height];
        let size = image.map(|n| n + 2 * border);
        let border_units = border as f64 / ppu;
        let corner: [f64; 2] =
            std::array::from_fn(|i| scene.center[i] - image[i] as f64 / ppu / 2.0);
        let origin = corner.map(|c| c - border_units);
        let span =
            std::array::from_fn(|i| corner[i] + image[i] as f64 / ppu + border_units - origin[i]);
        let scale = std::array::from_fn(|i| size[i] as f64 / span[i]);
        let (sin_roll, cos_roll) = (-std::f64::consts::PI * scene.roll_degrees / 180.0).sin_cos();
        let [cx, cy] = scene.center;
        let projection_offset = [
            cx * (1.0 - cos_roll) - cy * sin_roll - origin[0],
            cy * (1.0 - cos_roll) + cx * sin_roll - origin[1],
        ];
        Self {
            scene,
            state,
            rng,
            table,
            size,
            projection_offset,
            sin_roll,
            cos_roll,
            span,
            scale,
        }
    }

    pub fn step(&mut self) -> Sample {
        let transform = select(&mut self.rng, &self.table);
        self.state.apply(&self.scene.transforms[transform]);
        let point = [self.state.x, self.state.y];
        let color = self.state.color;
        let palette_index = ((color * 254.0 + 0.5) as usize).min(255);
        let [x, y] = point;
        let projected = [
            x * self.cos_roll + y * self.sin_roll + self.projection_offset[0],
            y * self.cos_roll - x * self.sin_roll + self.projection_offset[1],
        ];
        let pixel = if (0..2).all(|i| projected[i] >= 0.0 && projected[i] <= self.span[i]) {
            let pixel: [usize; 2] =
                std::array::from_fn(|i| (self.scale[i] * projected[i] + 0.5) as usize);
            (pixel[0] < self.size[0] && pixel[1] < self.size[1]).then_some(pixel)
        } else {
            None
        };
        Sample {
            transform,
            point,
            z: self.state.z,
            color,
            palette_index,
            pixel,
        }
    }
}

pub struct Sample {
    pub transform: usize,
    pub point: [f64; 2],
    pub z: f64,
    pub color: f64,
    pub palette_index: usize,
    pub pixel: Option<[usize; 2]>,
}

fn select(rng: &mut Marsaglia, table: &[usize; TABLE_SIZE]) -> usize {
    table[((rng.next() * TABLE_SIZE as f64) as usize).min(TABLE_SIZE - 1)]
}

fn pixels_per_unit(scene: &Scene, w: usize, h: usize) -> f64 {
    let scale = (w as f64 / scene.width as f64 + h as f64 / scene.height as f64) / 2.0;
    scene.ppu * scene.zoom * scale
}

#[derive(Default, Clone, Copy)]
pub struct Bin {
    pub count: u32,
    pub rgb: [f32; 3],
}
impl Bin {
    pub fn add(&mut self, color: [u8; 3]) {
        self.count += 1;
        for (sum, channel) in self.rgb.iter_mut().zip(color) {
            *sum += f32::from(channel) * (200.0 / 256.0);
        }
    }
}

pub fn render(
    scene: &Scene,
    width: usize,
    height: usize,
    quality: usize,
) -> Result<Vec<u8>, String> {
    render_with_raster(scene, width, height, quality, |_| Ok(()))
}

/// Render with the same raster made available before shading for parity capture.
#[doc(hidden)]
pub fn render_with_raster(
    scene: &Scene,
    width: usize,
    height: usize,
    quality: usize,
    capture: impl FnOnce(&[Bin]) -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    if scene.filter_radius != 0.0 && scene.filter_radius != 1.0 {
        return Err("only filter radii 0 and 1 are supported".into());
    }
    let filter = Kernel::mitchell_smooth(scene.filter_radius);
    let border = filter.border();
    let samples = sample_budget(width, height, quality, border)?;
    let stride = width + 2 * border;
    let mut sampler = Sampler::with_border(scene, width, height, border);
    let mut raster = vec![Bin::default(); stride * (height + 2 * border)];
    for _ in 0..samples {
        let sample = sampler.step();
        if let Some([x, y]) = sample.pixel {
            raster[y * stride + x].add(scene.palette[sample.palette_index]);
        }
    }
    capture(&raster)?;
    Ok(shade(scene, width, height, quality, &raster, &filter))
}

// Opaque-black path of JWildfire's LogScaleCalculator,
// LogDensityFilter and GammaCorrectionFilter, at the default color settings.
fn shade(
    scene: &Scene,
    width: usize,
    height: usize,
    quality: usize,
    raster: &[Bin],
    filter: &Kernel,
) -> Vec<u8> {
    const WHITE_LEVEL: f64 = 220.0;
    const LOW_DENSITY_BRIGHTNESS: f64 = 0.24;
    let ppu = pixels_per_unit(scene, width, height);
    let area = (width * height) as f64 / (ppu * ppu);
    let k1 = 2.0 * scene.brightness;
    let k2 = 1.0 / (area * quality as f64);
    let glow = LOW_DENSITY_BRIGHTNESS * k2 * area;
    let gamma = 1.0 / scene.gamma;
    let threshold = scene.threshold;
    let threshold_scale = threshold.powf(gamma - 1.0);
    let log_scale = |count: u32| {
        let count = f64::from(count);
        (k1 * (1.0 + count * k2).log10() + glow / (count + 1.0)) / (WHITE_LEVEL * count)
    };
    let scales: [f64; 513] = std::array::from_fn(|i| log_scale(i as u32));
    let border = filter.border();
    let stride = width + 2 * border;
    let smoothing = Kernel::gaussian();
    let sample = |(px, py, weight): (usize, usize, f64)| {
        if px >= stride || py >= height + 2 * border {
            return None;
        }
        let bin = raster[py * stride + px];
        if bin.count == 0 {
            return None;
        }
        let scale = scales
            .get(bin.count as usize)
            .copied()
            .unwrap_or_else(|| log_scale(bin.count));
        Some((weight, scale, bin))
    };
    let mut pixels = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            let mut rgb = [0.0; 3];
            for (weight, scale, bin) in filter.taps(x, y, border).filter_map(sample) {
                for (channel, sum) in rgb.iter_mut().zip(bin.rgb) {
                    *channel += weight * scale * f64::from(sum);
                }
            }
            // Despite its name, Java classifies Mitchell Smooth as sharpening.
            // It uses the Mitchell taps for color and Gaussian taps for intensity.
            let intensity = if filter.size == 1 {
                filter
                    .taps(x, y, border)
                    .filter_map(sample)
                    .map(|(_, scale, bin)| scale * f64::from(bin.count) * WHITE_LEVEL)
                    .sum::<f64>()
            } else {
                smoothing
                    .taps(x, y, border)
                    .filter_map(sample)
                    .map(|(weight, scale, bin)| {
                        (weight * WHITE_LEVEL) * (scale * f64::from(bin.count))
                    })
                    .sum::<f64>()
            };
            if intensity <= 0.0 {
                pixels.extend([0; 3]);
                continue;
            }
            let power = intensity.powf(gamma);
            let alpha = if intensity <= threshold {
                let fraction = intensity / threshold;
                (1.0 - fraction) * intensity * threshold_scale + fraction * power
            } else {
                power
            };
            let gamma_scale = 256.0 * alpha / intensity;
            pixels.extend(rgb.map(|channel| (gamma_scale * channel.max(0.0) + 0.5) as u8));
        }
    }
    pixels
}

fn sample_budget(w: usize, h: usize, q: usize, border: usize) -> Result<usize, String> {
    if !(1..=4096).contains(&w) || !(1..=4096).contains(&h) || q == 0 {
        return Err("width and height must be 1..=4096; quality must be positive".into());
    }
    let count = (w + 2 * border) * (h + 2 * border);
    match count.checked_mul(q) {
        Some(samples) if samples <= 200_000_000 => Ok(samples),
        _ => Err("preview sample budget exceeds 200,000,000".into()),
    }
}
