// Sampling equations adapted from JWildfire 9.20 (Andreas Maschke and contributors, LGPL-2.1-or-later).
// This Rust port is distributed under GPL-3.0-or-later.
use crate::scene::{Scene, Transform};

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
    color: f64,
}
impl Point {
    fn apply(&mut self, t: &Transform) {
        let [a, b, d, e, tx, ty] = t.affine;
        let ax = a * self.x + d * self.y + tx;
        let ay = b * self.x + e * self.y + ty;
        let mut x = 0.0;
        let mut y = 0.0;
        if t.linear != 0.0 {
            x += t.linear * ax;
            y += t.linear * ay;
        }
        if t.spherical != 0.0 {
            let r = t.spherical / (ax * ax + ay * ay + 1.0e-300);
            x += ax * r;
            y += ay * r;
        }
        self.x = x;
        self.y = y;
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
        let mut rng = Marsaglia::seeded(SEED);
        let mut state = Point {
            x: 2.0 * rng.next() - 1.0,
            y: 2.0 * rng.next() - 1.0,
            color: rng.next(),
        };
        rng.next(); // Initial material draw, even for the ordinary color raster.
        let table = table(&scene.transforms);
        state.apply(&scene.transforms[0]);
        for _ in 0..=42 {
            state.apply(&scene.transforms[select(&mut rng, &table)]);
        }
        let ppu = pixels_per_unit(scene, width, height);
        let size = [width, height];
        let origin: [f64; 2] =
            std::array::from_fn(|i| scene.center[i] - size[i] as f64 / ppu / 2.0);
        let span = std::array::from_fn(|i| origin[i] + size[i] as f64 / ppu - origin[i]);
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
            color,
            palette_index,
            pixel,
        }
    }
}

pub struct Sample {
    pub transform: usize,
    pub point: [f64; 2],
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
    let (count, samples) = dimensions(width, height, quality)?;
    let mut sampler = Sampler::new(scene, width, height);
    let mut raster = vec![Bin::default(); count];
    for _ in 0..samples {
        let sample = sampler.step();
        if let Some([x, y]) = sample.pixel {
            raster[y * width + x].add(scene.palette[sample.palette_index]);
        }
    }
    let mut pixels = Vec::with_capacity(count * 3);
    let k2 = pixels_per_unit(scene, width, height).powi(2) / samples as f64;
    let gamma = 1.0 / scene.gamma;
    for bin in raster {
        let t = 2.0 * scene.brightness * (1.0 + f64::from(bin.count) * k2).log10();
        let alpha = if t >= scene.threshold {
            t.powf(gamma)
        } else {
            let fraction = t / scene.threshold;
            (1.0 - fraction) * t * scene.threshold.powf(gamma - 1.0) + fraction * t.powf(gamma)
        };
        let scale = if bin.count == 0 {
            0.0
        } else {
            alpha / f64::from(bin.count) * (256.0 / 255.0) * (256.0 / 200.0)
        };
        pixels.extend(bin.rgb.map(|v| (scale * f64::from(v)).round() as u8));
    }
    Ok(pixels)
}

fn dimensions(w: usize, h: usize, q: usize) -> Result<(usize, usize), String> {
    if !(1..=4096).contains(&w) || !(1..=4096).contains(&h) || q == 0 {
        return Err("width and height must be 1..=4096; quality must be positive".into());
    }
    let count = w * h;
    match count.checked_mul(q) {
        Some(samples) if samples <= 200_000_000 => Ok((count, samples)),
        _ => Err("preview sample budget exceeds 200,000,000".into()),
    }
}
