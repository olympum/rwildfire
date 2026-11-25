// Sampling equations adapted from JWildfire 9.20 (Andreas Maschke and contributors, LGPL-2.1-or-later).
// This Rust port is distributed under GPL-3.0-or-later.
use crate::{
    filter::Kernel,
    scene::{Scene, Transform, VariationKind},
};

const TABLE_SIZE: usize = 1024;
pub const PREVIEW_SEED: i64 = 741103597;

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
struct Blur3DState {
    draws: [f64; 4],
    index: usize,
}

impl Point {
    fn apply(&mut self, t: &Transform, rng: &mut Marsaglia, blur3d: &mut Option<Blur3DState>) {
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
                VariationKind::Curl3D { cx, cy, cz } => {
                    let az = self.z;
                    let r2 = ax * ax + ay * ay + az * az;
                    let c2 = cx * cx + cy * cy + cz * cz;
                    let r = amount
                        / (r2 * c2 + (2.0 * cx) * ax - (2.0 * cy) * ay + (2.0 * cz) * az + 1.0);
                    x += r * (ax + cx * r2);
                    y += r * (ay - cy * r2);
                    z += r * (az + cz * r2);
                }
                VariationKind::Fan2 { x: fan_x, y: fan_y } => {
                    let radius = crate::fast_sqrt::sqrt(ax * ax + ay * ay);
                    let angle = if ax.abs() > 1.0e-300 || ay.abs() > 1.0e-300 {
                        crate::fast_atan::atan2(ax, ay)
                    } else {
                        0.0
                    };
                    let dx = std::f64::consts::PI * (fan_x * fan_x) + 1.0e-300;
                    let half = dx * 0.5;
                    let shifted = angle + fan_y;
                    // Java narrows the quotient to a 32-bit integer before multiplying.
                    let t = shifted - f64::from((shifted / dx) as i32) * dx;
                    let a = if t > half { angle - half } else { angle + half };
                    x += amount * radius * crate::fast_sin::sin(a);
                    y += amount * radius * crate::fast_sin::cos(a);
                }
                VariationKind::Julia3D { power } => {
                    let power = f64::from(power);
                    let abs_power = power.abs();
                    let depth = self.z / abs_power;
                    let xy2 = ax * ax + ay * ay;
                    let radius = amount
                        * crate::fast_pow::pow(xy2 + depth * depth, (1.0 / power - 1.0) * 0.5);
                    let xy = radius * crate::fast_sqrt::sqrt(xy2);
                    let branch = (rng.next() * abs_power) as i32;
                    let angle = (crate::fast_atan::atan2(ay, ax)
                        + std::f64::consts::TAU * f64::from(branch))
                        / power;
                    x += xy * crate::fast_sin::cos(angle);
                    y += xy * crate::fast_sin::sin(angle);
                    z += radius * depth;
                }
                VariationKind::Blur => {
                    let angle = rng.next() * std::f64::consts::TAU;
                    let radius = amount * rng.next();
                    x += radius * crate::fast_sin::cos(angle);
                    y += radius * crate::fast_sin::sin(angle);
                }
                VariationKind::Blur3D => {
                    let state = blur3d.as_mut().expect("initialized blur3D state");
                    let azimuth = rng.next() * 2.0 * std::f64::consts::PI;
                    let [a, b, c, d] = state.draws;
                    let radius = amount * (a + b + c + d - 2.0);
                    state.draws[state.index] = rng.next();
                    state.index = (state.index + 1) & 3;
                    let polar = rng.next() * std::f64::consts::PI;
                    let xy = radius * crate::fast_sin::sin(polar);
                    x += xy * crate::fast_sin::cos(azimuth);
                    y += xy * crate::fast_sin::sin(azimuth);
                    z += radius * crate::fast_sin::cos(polar);
                }
                VariationKind::Popcorn => {
                    let shift = |v: f64| {
                        let t = crate::fast_tan::tan(3.0 * v);
                        crate::fast_sin::sin(if t.is_nan() { 0.0 } else { t })
                    };
                    x += amount * (ax + tx * shift(ay));
                    y += amount * (ay + ty * shift(ax));
                }
                VariationKind::Diamond => {
                    let radius = crate::fast_sqrt::sqrt(ax * ax + ay * ay) + 1.0e-300;
                    x += amount * (ax / radius) * crate::fast_sin::cos(radius);
                    y += amount * (ay / radius) * crate::fast_sin::sin(radius);
                }
                VariationKind::Horseshoe => {
                    let radius = crate::fast_sqrt::sqrt(ax * ax + ay * ay) + 1.0e-300;
                    let sin_a = ax / radius;
                    let cos_a = ay / radius;
                    x += amount * (sin_a * ax - cos_a * ay);
                    y += amount * (cos_a * ax + sin_a * ay);
                }
                VariationKind::Heart => {
                    let radius = crate::fast_sqrt::sqrt(ax * ax + ay * ay);
                    let angle = radius * crate::fast_atan::atan2(ax, ay);
                    let scale = radius * amount;
                    x += scale * crate::fast_sin::sin(angle);
                    y -= scale * crate::fast_sin::cos(angle);
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

const CAMERA_EPSILON: f64 = 1.0e-8;

// FlameRendererView's matrix for the supported zero-bank camera, grouped
// by output coordinate to match Java's dot products.
fn camera_matrix(scene: &Scene) -> Option<[[f64; 3]; 3]> {
    if ![scene.pitch_degrees, scene.yaw_degrees, scene.perspective]
        .iter()
        .any(|v| v.abs() > CAMERA_EPSILON)
    {
        return None;
    }
    let [pitch, yaw, roll] = [scene.pitch_degrees, -scene.yaw_degrees, scene.roll_degrees]
        .map(|v| v * std::f64::consts::PI / 180.0);
    let [sp, sy, sr] = [pitch, yaw, roll].map(crate::fast_sin::sin);
    let [cp, cy, cr] = [pitch, yaw, roll].map(crate::fast_sin::cos);
    Some([
        [-cp * sr * sy + cr * cy, -cp * cy * sr - cr * sy, sp * sr],
        [cp * cr * sy + sr * cy, cp * cr * cy - sr * sy, -cr * sp],
        [sp * sy, cy * sp, cp],
    ])
}

pub struct Sampler<'a> {
    scene: &'a Scene,
    state: Point,
    rng: Marsaglia,
    blur3d: Vec<Option<Blur3DState>>,
    table: [usize; TABLE_SIZE],
    size: [usize; 2],
    projection_offset: [f64; 2],
    camera_matrix: Option<[[f64; 3]; 3]>,
    sin_roll: f64,
    cos_roll: f64,
    span: [f64; 2],
    scale: [f64; 2],
}

impl<'a> Sampler<'a> {
    pub fn new(scene: &'a Scene, width: usize, height: usize) -> Self {
        Self::with_border(scene, width, height, 0, PREVIEW_SEED)
    }

    fn with_border(
        scene: &'a Scene,
        width: usize,
        height: usize,
        border: usize,
        seed: i64,
    ) -> Self {
        let mut rng = Marsaglia::seeded(seed);
        // Java initializes each ordinary variation in xform order before drawing the first point.
        let mut blur3d: Vec<_> = scene
            .transforms
            .iter()
            .map(|t| {
                t.variations
                    .iter()
                    .any(|v| matches!(v.kind, VariationKind::Blur3D))
                    .then(|| Blur3DState {
                        draws: std::array::from_fn(|_| rng.next()),
                        index: 0,
                    })
            })
            .collect();
        let mut state = Point {
            x: 2.0 * rng.next() - 1.0,
            y: 2.0 * rng.next() - 1.0,
            z: 0.0,
            color: rng.next(),
        };
        rng.next(); // Initial material draw, even for the ordinary color raster.
        let table = table(&scene.transforms);
        state.apply(&scene.transforms[0], &mut rng, &mut blur3d[0]);
        for _ in 0..=42 {
            let index = select(&mut rng, &table);
            state.apply(&scene.transforms[index], &mut rng, &mut blur3d[index]);
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
        let camera_matrix = camera_matrix(scene);
        let (sin_roll, cos_roll, projection_offset) = if camera_matrix.is_some() {
            (0.0, 1.0, origin.map(|v| -v))
        } else {
            let angle = -std::f64::consts::PI * scene.roll_degrees / 180.0;
            let sin = crate::fast_sin::sin(angle);
            let cos = crate::fast_sin::cos(angle);
            let [cx, cy] = scene.center;
            (
                sin,
                cos,
                [
                    cx * (1.0 - cos) - cy * sin - origin[0],
                    cy * (1.0 - cos) + cx * sin - origin[1],
                ],
            )
        };
        Self {
            scene,
            state,
            rng,
            blur3d,
            table,
            size,
            projection_offset,
            camera_matrix,
            sin_roll,
            cos_roll,
            span,
            scale,
        }
    }

    pub fn step(&mut self) -> Sample {
        let transform = select(&mut self.rng, &self.table);
        self.state.apply(
            &self.scene.transforms[transform],
            &mut self.rng,
            &mut self.blur3d[transform],
        );
        let point = [self.state.x, self.state.y];
        let color = self.state.color;
        let palette_index = ((color * 254.0 + 0.5) as usize).min(255);
        let pixel = self.project_pixel([point[0], point[1], self.state.z]);
        Sample {
            transform,
            point,
            z: self.state.z,
            color,
            palette_index,
            pixel,
        }
    }

    /// Map a world-space point to the sample raster without modifying the orbit.
    pub fn project_pixel(&self, [x, y, z]: [f64; 3]) -> Option<[usize; 2]> {
        let [x, y] = if let Some(matrix) = self.camera_matrix {
            let [cx, cy, cz] = matrix.map(|[a, b, c]| a * x + b * y + c * z);
            let depth = 1.0 - self.scene.perspective * cz;
            if depth < CAMERA_EPSILON {
                return None;
            }
            [cx / depth, cy / depth]
        } else {
            [x, y]
        };
        let projected = [
            x * self.cos_roll + y * self.sin_roll + self.projection_offset[0],
            y * self.cos_roll - x * self.sin_roll + self.projection_offset[1],
        ];
        if (0..2).all(|i| projected[i] >= 0.0 && projected[i] <= self.span[i]) {
            let pixel: [usize; 2] =
                std::array::from_fn(|i| (self.scale[i] * projected[i] + 0.5) as usize);
            (pixel[0] < self.size[0] && pixel[1] < self.size[1]).then_some(pixel)
        } else {
            None
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

/// Explicit inputs for a single-worker CPU still render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RenderRequest {
    pub width: usize,
    pub height: usize,
    pub quality: usize,
    /// JWildfire Marsaglia seed; only the low 16 bits affect initialization.
    pub seed: i64,
}

impl RenderRequest {
    pub fn new(width: usize, height: usize, quality: usize, seed: i64) -> Self {
        Self {
            width,
            height,
            quality,
            seed,
        }
    }

    /// Validate dimensions and budget without allocating a raster or drawing samples.
    pub fn resolve(self, scene: &Scene) -> Result<ResolvedStill<'_>, String> {
        resolve(scene, self, 200_000_000)
    }
}

/// Inspectable CPU settings, not a serialized format or GPU layout.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct StillSettings {
    pub request: RenderRequest,
    pub raster_width: usize,
    pub raster_height: usize,
    pub border: usize,
    pub sample_budget: usize,
    pub pixels_per_unit: f64,
    pub filter_radius: f64,
}

/// Resolved inputs borrow the scene immutably; each render starts a new job.
pub struct ResolvedStill<'a> {
    scene: &'a Scene,
    settings: StillSettings,
    filter: Kernel,
}

impl ResolvedStill<'_> {
    pub fn settings(&self) -> StillSettings {
        self.settings
    }

    /// Return row-major RGB8 without encoding or writing a file.
    pub fn render(&self) -> Result<Vec<u8>, String> {
        self.render_with_raster(|_| Ok(()))
    }

    fn render_with_raster(
        &self,
        capture: impl FnOnce(&[Bin]) -> Result<(), String>,
    ) -> Result<Vec<u8>, String> {
        let StillSettings {
            request:
                RenderRequest {
                    width,
                    height,
                    quality,
                    seed,
                },
            raster_width,
            raster_height,
            border,
            sample_budget,
            ..
        } = self.settings;
        let mut sampler = Sampler::with_border(self.scene, width, height, border, seed);
        let mut raster = vec![Bin::default(); raster_width * raster_height];
        for _ in 0..sample_budget {
            let sample = sampler.step();
            if let Some([x, y]) = sample.pixel {
                raster[y * raster_width + x].add(self.scene.palette[sample.palette_index]);
            }
        }
        capture(&raster)?;
        Ok(shade(
            self.scene,
            width,
            height,
            quality,
            &raster,
            &self.filter,
        ))
    }
}

pub fn render(
    scene: &Scene,
    width: usize,
    height: usize,
    quality: usize,
) -> Result<Vec<u8>, String> {
    RenderRequest::new(width, height, quality, PREVIEW_SEED)
        .resolve(scene)?
        .render()
}

/// Make the raster available before shading for parity capture.
#[doc(hidden)]
pub fn render_with_raster(
    scene: &Scene,
    width: usize,
    height: usize,
    quality: usize,
    capture: impl FnOnce(&[Bin]) -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    RenderRequest::new(width, height, quality, PREVIEW_SEED)
        .resolve(scene)?
        .render_with_raster(capture)
}

/// Capture the fixed TINA quality-500 layout without raising the preview cap.
#[doc(hidden)]
pub fn render_quality_500_with_raster(
    scene: &Scene,
    capture: impl FnOnce(&[Bin]) -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    if (scene.width, scene.height, scene.filter_radius) != (466, 331, 1.0) {
        return Err("quality-500 parity requires a 466×331 source and filter radius 1".into());
    }
    resolve(
        scene,
        RenderRequest::new(932, 662, 500, PREVIEW_SEED),
        311_688_000,
    )?
    .render_with_raster(capture)
}

fn resolve(scene: &Scene, request: RenderRequest, cap: usize) -> Result<ResolvedStill<'_>, String> {
    let RenderRequest {
        width,
        height,
        quality,
        ..
    } = request;
    if scene.filter_radius != 0.0 && scene.filter_radius != 1.0 {
        return Err("only filter radii 0 and 1 are supported".into());
    }
    let filter = Kernel::mitchell_smooth(scene.filter_radius);
    let border = filter.border();
    let sample_budget = sample_budget(width, height, quality, border, cap)?;
    let settings = StillSettings {
        request,
        border,
        raster_width: width + 2 * border,
        raster_height: height + 2 * border,
        sample_budget,
        pixels_per_unit: pixels_per_unit(scene, width, height),
        filter_radius: scene.filter_radius,
    };
    Ok(ResolvedStill {
        scene,
        settings,
        filter,
    })
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

fn sample_budget(w: usize, h: usize, q: usize, border: usize, cap: usize) -> Result<usize, String> {
    if !(1..=4096).contains(&w) || !(1..=4096).contains(&h) || q == 0 {
        return Err("width and height must be 1..=4096; quality must be positive".into());
    }
    let count = (w + 2 * border) * (h + 2 * border);
    match count.checked_mul(q) {
        Some(samples) if samples <= cap => Ok(samples),
        _ => Err(format!("sample budget exceeds {cap} iterations")),
    }
}
