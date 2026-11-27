// Sampling equations adapted from JWildfire 9.20 (Andreas Maschke and contributors, LGPL-2.1-or-later).
// This Rust port is distributed under GPL-3.0-or-later.
use crate::{
    filter::Kernel,
    scene::{Layer, Scene, Transform, VariationKind},
};

const TABLE_SIZE: usize = 1024;
const PLOT_BUFFER_SIZE: usize = 8192;
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

fn table(transforms: &[Transform], chaos: Option<&[f64]>) -> [usize; TABLE_SIZE] {
    let weights: Vec<_> = transforms
        .iter()
        .enumerate()
        .map(|(i, t)| t.weight * chaos.map_or(1.0, |row| row[i]))
        .collect();
    let total: f64 = weights.iter().sum();
    let mut boundary = 0.0;
    std::array::from_fn(|_| {
        let mut cumulative = 0.0;
        let selected = weights
            .iter()
            .position(|weight| {
                cumulative += weight;
                cumulative > boundary
            })
            .unwrap_or(transforms.len() - 1);
        boundary += total / TABLE_SIZE as f64;
        selected
    })
}

#[derive(Clone, Copy)]
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
        [self.x, self.y] = if let Some([a, b, d, e, tx, ty]) = t.post {
            [a * x + d * y + tx, b * x + e * y + ty]
        } else {
            [x, y]
        };
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
    orbit: Orbit<'a>,
    rng: Marsaglia,
    view: View,
}

impl<'a> Sampler<'a> {
    /// Trace the first layer by itself. Use a render request for multilayer scheduling.
    pub fn new(scene: &'a Scene, width: usize, height: usize) -> Self {
        let mut rng = Marsaglia::seeded(PREVIEW_SEED);
        let mut orbit = Orbit::new(&scene.layers[0], &mut rng);
        orbit.warm_up(&mut rng);
        Self {
            orbit,
            rng,
            view: View::new(
                scene,
                [width, height],
                [width * scene.oversample, height * scene.oversample],
                0,
                pixels_per_unit(scene, width, height),
                Antialias::default(),
            ),
        }
    }

    pub fn step(&mut self) -> Sample {
        self.orbit.step(&self.view, &mut self.rng)
    }

    /// Map a world-space point to the sample raster without modifying the orbit.
    pub fn project_pixel(&self, point: [f64; 3]) -> Option<[usize; 2]> {
        self.view
            .project(point)
            .and_then(|p| self.view.raster_pixel(p, [0.0; 2]))
    }
}

struct Orbit<'a> {
    layer: &'a Layer,
    state: Point,
    blur3d: Vec<Option<Blur3DState>>,
    final_blur3d: Vec<Option<Blur3DState>>,
    tables: Vec<[usize; TABLE_SIZE]>,
    previous: usize,
}

impl<'a> Orbit<'a> {
    fn new(layer: &'a Layer, rng: &mut Marsaglia) -> Self {
        let mut init_blur3d = |transforms: &[Transform]| {
            transforms
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
                .collect::<Vec<_>>()
        };
        let blur3d = init_blur3d(&layer.transforms);
        let final_blur3d = init_blur3d(&layer.final_transforms);
        let tables = layer
            .transforms
            .iter()
            .map(|from| table(&layer.transforms, from.chaos.as_deref()))
            .collect();
        Self {
            layer,
            state: Point {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                color: 0.0,
            },
            blur3d,
            final_blur3d,
            tables,
            previous: 0,
        }
    }

    fn warm_up(&mut self, rng: &mut Marsaglia) {
        let mut state = Point {
            x: 2.0 * rng.next() - 1.0,
            y: 2.0 * rng.next() - 1.0,
            z: 0.0,
            color: rng.next(),
        };
        rng.next(); // Initial material draw, even for the ordinary color raster.
        let mut previous = 0;
        state.apply(&self.layer.transforms[0], rng, &mut self.blur3d[0]);
        for _ in 0..=42 {
            let index = select(rng, &self.tables[previous]);
            previous = index;
            state.apply(&self.layer.transforms[index], rng, &mut self.blur3d[index]);
        }
        self.state = state;
        self.previous = previous;
    }

    fn step(&mut self, view: &View, rng: &mut Marsaglia) -> Sample {
        let transform = select(rng, &self.tables[self.previous]);
        self.previous = transform;
        self.state.apply(
            &self.layer.transforms[transform],
            rng,
            &mut self.blur3d[transform],
        );
        let point = [self.state.x, self.state.y];
        let color = self.state.color;
        let palette_index = ((color * 254.0 + 0.5) as usize).min(255);
        let mut plot = self.state;
        for (final_xform, blur) in self
            .layer
            .final_transforms
            .iter()
            .zip(&mut self.final_blur3d)
        {
            plot.apply(final_xform, rng, blur);
        }
        let pixel = view
            .project([plot.x, plot.y, plot.z])
            .and_then(|projected| {
                let jitter = view.antialias.jitter(rng);
                view.raster_pixel(projected, jitter)
            });
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

struct View {
    perspective: f64,
    size: [usize; 2],
    projection_offset: [f64; 2],
    camera_matrix: Option<[[f64; 3]; 3]>,
    sin_roll: f64,
    cos_roll: f64,
    span: [f64; 2],
    scale: [f64; 2],
    antialias: Antialias,
}

impl View {
    fn new(
        scene: &Scene,
        image: [usize; 2],
        size: [usize; 2],
        border: usize,
        ppu: f64,
        antialias: Antialias,
    ) -> Self {
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
            perspective: scene.perspective,
            size,
            projection_offset,
            camera_matrix,
            sin_roll,
            cos_roll,
            span,
            scale,
            antialias,
        }
    }

    fn project(&self, [x, y, z]: [f64; 3]) -> Option<[f64; 2]> {
        let [x, y] = if let Some(matrix) = self.camera_matrix {
            let [cx, cy, cz] = matrix.map(|[a, b, c]| a * x + b * y + c * z);
            let depth = 1.0 - self.perspective * cz;
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
        (0..2)
            .all(|i| projected[i] >= 0.0 && projected[i] <= self.span[i])
            .then_some(projected)
    }

    fn raster_pixel(&self, projected: [f64; 2], jitter: [f64; 2]) -> Option<[usize; 2]> {
        let pixel: [i32; 2] =
            std::array::from_fn(|i| (self.scale[i] * projected[i] + jitter[i] + 0.5) as i32);
        (0..2)
            .all(|i| pixel[i] >= 0 && (pixel[i] as usize) < self.size[i])
            .then(|| pixel.map(|v| v as usize))
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
        self.add_weighted(color, 1.0);
    }

    fn add_weighted(&mut self, color: [u8; 3], weight: f64) {
        self.count += 1;
        for (sum, channel) in self.rgb.iter_mut().zip(color) {
            *sum += (f64::from(channel) * (200.0 / 256.0) * weight) as f32;
        }
    }
}

fn flush_layer(raster: &mut [Bin], buffer: &mut Vec<(usize, [u8; 3])>, weight: f64) {
    for (index, color) in buffer.drain(..) {
        raster[index].add_weighted(color, weight);
    }
}

/// Legacy random projection jitter, separate from the reconstruction filter.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Antialias {
    pub amount: f64,
    pub radius: f64,
}

impl Antialias {
    fn jitter(self, rng: &mut Marsaglia) -> [f64; 2] {
        if self.amount > CAMERA_EPSILON
            && self.radius > CAMERA_EPSILON
            && rng.next() > 1.0 - self.amount
        {
            let dr = crate::fast_pow::exp(
                self.radius * crate::fast_sqrt::sqrt(-crate::fast_pow::log(rng.next())),
            ) - 1.0;
            let da = rng.next() * 2.0 * std::f64::consts::PI;
            [dr * crate::fast_sin::cos(da), dr * crate::fast_sin::sin(da)]
        } else {
            [0.0; 2]
        }
    }
}

/// Explicit inputs for a single-worker CPU still render.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct RenderRequest {
    pub width: usize,
    pub height: usize,
    pub quality: usize,
    /// JWildfire Marsaglia seed; only the low 16 bits affect initialization.
    /// A zero low word intentionally gives Java's all-zero stream.
    pub seed: i64,
    /// Off by default to preserve the preview contract.
    pub antialias: Antialias,
}

impl RenderRequest {
    pub fn new(width: usize, height: usize, quality: usize, seed: i64) -> Self {
        Self {
            width,
            height,
            quality,
            seed,
            antialias: Antialias::default(),
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
    pub oversample: usize,
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

    /// Return row-major RGBA8 with the configured backdrop and unassociated alpha.
    pub fn render_rgba(&self) -> Result<Vec<u8>, String> {
        self.render_rgba_with_raster(|_| Ok(()))
    }

    /// Capture the padded raster before shading for parity diagnostics.
    #[doc(hidden)]
    pub fn render_with_raster(
        &self,
        capture: impl FnOnce(&[Bin]) -> Result<(), String>,
    ) -> Result<Vec<u8>, String> {
        self.render_pixels(false, capture)
    }

    #[doc(hidden)]
    pub fn render_rgba_with_raster(
        &self,
        capture: impl FnOnce(&[Bin]) -> Result<(), String>,
    ) -> Result<Vec<u8>, String> {
        self.render_pixels(true, capture)
    }

    fn render_pixels(
        &self,
        with_alpha: bool,
        capture: impl FnOnce(&[Bin]) -> Result<(), String>,
    ) -> Result<Vec<u8>, String> {
        let StillSettings {
            request:
                RenderRequest {
                    width,
                    height,
                    seed,
                    antialias,
                    ..
                },
            raster_width,
            raster_height,
            border,
            sample_budget,
            pixels_per_unit,
            ..
        } = self.settings;
        let view = View::new(
            self.scene,
            [width, height],
            [raster_width, raster_height],
            border,
            pixels_per_unit,
            antialias,
        );
        let mut rng = Marsaglia::seeded(seed);
        // Java refreshes every layer's variation state before the worker warms any orbit.
        let mut orbits: Vec<_> = self
            .scene
            .layers
            .iter()
            .map(|layer| Orbit::new(layer, &mut rng))
            .collect();
        for orbit in &mut orbits {
            orbit.warm_up(&mut rng);
        }
        let mut raster = vec![Bin::default(); raster_width * raster_height];
        let mut buffers = vec![Vec::new(); orbits.len()];
        for _ in (0..sample_budget).step_by(orbits.len()) {
            for (orbit, buffer) in orbits.iter_mut().zip(&mut buffers) {
                let layer = orbit.layer;
                if layer.density < 1.0 && rng.next() > layer.density * layer.density {
                    continue;
                }
                let sample = orbit.step(&view, &mut rng);
                if let Some([x, y]) = sample.pixel {
                    let index = y * raster_width + x;
                    let color = layer.palette[sample.palette_index];
                    if self.scene.layers.len() == 1 {
                        raster[index].add_weighted(color, layer.weight);
                    } else {
                        buffer.push((index, color));
                        if buffer.len() == PLOT_BUFFER_SIZE {
                            flush_layer(&mut raster, buffer, layer.weight);
                        }
                    }
                }
            }
        }
        for (orbit, buffer) in orbits.iter().zip(&mut buffers) {
            flush_layer(&mut raster, buffer, orbit.layer.weight);
        }
        capture(&raster)?;
        Ok(shade(
            self.scene,
            self.settings,
            &raster,
            &self.filter,
            with_alpha,
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
    if (
        scene.width,
        scene.height,
        scene.filter_radius,
        scene.oversample,
    ) != (466, 331, 1.0, 1)
    {
        return Err(
            "quality-500 parity requires a 466×331 source, filter radius 1 and oversample 1".into(),
        );
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
    if !(0.0..=1.0).contains(&request.antialias.amount)
        || !request.antialias.radius.is_finite()
        || request.antialias.radius < 0.0
    {
        return Err(
            "antialias amount must be in 0..=1 and radius must be finite and nonnegative".into(),
        );
    }
    let oversample = scene.oversample;
    if !(1..=3).contains(&oversample) {
        return Err("oversample must be in 1..=3".into());
    }
    if !(1..=4096).contains(&width) || !(1..=4096).contains(&height) || quality == 0 {
        return Err("width and height must be 1..=4096; quality must be positive".into());
    }
    let filter = Kernel::mitchell_smooth(scene.filter_radius, oversample);
    let border = filter.border(oversample);
    let raster_width = width * oversample + 2 * border;
    let raster_height = height * oversample + 2 * border;
    let raster_area = raster_width * raster_height;
    // Preserve the previous maximum raster allocation independently of the work budget.
    if raster_area > (4096 + 4) * (4096 + 4) {
        return Err("padded raster exceeds preview size limit".into());
    }
    let layers = scene.layers.len();
    if layers == 0 || scene.layers.iter().any(|layer| layer.transforms.is_empty()) {
        return Err("expected a nonempty set of layers with transforms".into());
    }
    let requested = sample_budget(raster_area, quality, oversample, cap)?;
    // The Java worker completes a full layer round even at the budget boundary.
    let sample_budget = requested.next_multiple_of(layers);
    if sample_budget > cap {
        return Err(format!("sample budget exceeds {cap} iterations"));
    }
    let settings = StillSettings {
        request,
        border,
        oversample,
        raster_width,
        raster_height,
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

// Non-solid path of JWildfire's LogScaleCalculator, LogDensityFilter and GammaCorrectionFilter.
fn shade(
    scene: &Scene,
    settings: StillSettings,
    raster: &[Bin],
    filter: &Kernel,
    with_alpha: bool,
) -> Vec<u8> {
    let StillSettings {
        request:
            RenderRequest {
                width,
                height,
                quality,
                ..
            },
        pixels_per_unit: ppu,
        oversample,
        border,
        raster_width: stride,
        raster_height,
        ..
    } = settings;
    const WHITE_LEVEL: f64 = 220.0;
    let area = (width * height) as f64 / (ppu * ppu);
    let k1 = scene.contrast * 2.0 * scene.brightness / oversample as f64;
    let k2 = 1.0 / (scene.contrast * area * quality as f64);
    let glow = scene.low_density_brightness * k2 * area / oversample as f64;
    let gamma = 1.0 / scene.gamma;
    let vibrancy = (scene.vibrancy * 256.0 + 0.5).trunc().clamp(0.0, 256.0);
    let inverse_vibrancy = 256.0 - vibrancy;
    let threshold = scene.threshold;
    let threshold_scale = threshold.powf(gamma - 1.0);
    let log_scale = |count: u32| {
        let count = f64::from(count);
        (k1 * (1.0 + count * k2).log10() + glow / (count + 1.0)) / (WHITE_LEVEL * count)
    };
    let scales: [f64; 513] = std::array::from_fn(|i| log_scale(i as u32));
    let smoothing = Kernel::gaussian(oversample);
    let adaptive = scene
        .adaptive_filter
        .filter(|_| settings.filter_radius == 1.0)
        .map(|config| {
            let radius = settings.filter_radius + 0.25;
            (
                config,
                Kernel::sine_pow10(1.5 * radius, oversample),
                Kernel::sine_pow10(radius, oversample),
                Kernel::mitchell_smooth(0.75 * radius, oversample),
            )
        });
    let sample = |(px, py, weight): (usize, usize, f64)| {
        if px >= stride || py >= raster_height {
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
    let channels = if with_alpha { 4 } else { 3 };
    let alpha_scale = 1.0 - crate::fast_atan::atan(3.0 * (scene.foreground_opacity - 1.0)) / 1.25;
    let mut pixels = Vec::with_capacity(width * height * channels);
    for y in 0..height {
        for x in 0..width {
            let mut rgb = [0.0; 3];
            let sx = x * oversample;
            let sy = y * oversample;
            let (color_filter, sharpen) = match &adaptive {
                // The selector uses the raw raster origin, not the padded tap origin.
                Some((config, low, soft, detail)) => match sample((sx, sy, 1.0)) {
                    None => (filter, true),
                    Some((_, scale, bin))
                        if scale * f64::from(bin.count) * WHITE_LEVEL < config.low_density =>
                    {
                        (low, false)
                    }
                    Some(_)
                        if scharr_log(raster, stride, raster_height, sx, sy) < config.sharpness =>
                    {
                        (soft, false)
                    }
                    Some(_) => (detail, true),
                },
                None => (filter, true),
            };
            let intensity = if filter.size == 1 {
                let mut intensity = 0.0;
                for px in sx..sx + oversample {
                    for py in sy..sy + oversample {
                        if let Some((_, scale, bin)) = sample((px + border, py + border, 1.0)) {
                            for (channel, sum) in rgb.iter_mut().zip(bin.rgb) {
                                *channel += scale * f64::from(sum);
                            }
                            intensity += scale * f64::from(bin.count) * WHITE_LEVEL;
                        }
                    }
                }
                intensity
            } else {
                for (weight, scale, bin) in color_filter.taps(sx, sy, border).filter_map(sample) {
                    for (channel, sum) in rgb.iter_mut().zip(bin.rgb) {
                        *channel += weight * scale * f64::from(sum);
                    }
                }
                // Java substitutes Gaussian intensity only for a sharpening kernel.
                let intensity_filter = if sharpen { &smoothing } else { color_filter };
                intensity_filter
                    .taps(sx, sy, border)
                    .filter_map(sample)
                    .map(|(weight, scale, bin)| {
                        (weight * WHITE_LEVEL) * (scale * f64::from(bin.count))
                    })
                    .sum::<f64>()
            };
            let background = background_at(scene, x, y, width, height);
            if intensity <= 0.0 {
                pixels.extend(background);
                if with_alpha {
                    pixels.push(0);
                }
                continue;
            }
            let power = intensity.powf(gamma);
            let alpha = if intensity <= threshold {
                let fraction = intensity / threshold;
                (1.0 - fraction) * intensity * threshold_scale + fraction * power
            } else {
                power
            };
            let gamma_scale = vibrancy * alpha / intensity;
            let alpha_byte = (alpha * 255.0 * alpha_scale + 0.5) as u8;
            pixels.extend(
                rgb.into_iter()
                    .zip(background)
                    .map(|(channel, background)| {
                        let channel = channel.max(0.0);
                        let scaled = gamma_scale * channel;
                        let mixed = if inverse_vibrancy > 0.0 {
                            scaled + inverse_vibrancy * channel.powf(gamma)
                        } else {
                            scaled
                        };
                        let backdrop = (u16::from(255 - alpha_byte) * u16::from(background)) >> 8;
                        ((mixed + 0.5) as u8).saturating_add(backdrop as u8)
                    }),
            );
            if with_alpha {
                pixels.push(alpha_byte);
            }
        }
    }
    pixels
}

// JWildfire LogDensityFilter.calculateBGColor, evaluated in output-pixel coordinates.
fn background_at(scene: &Scene, x: usize, y: usize, width: usize, height: usize) -> [u8; 3] {
    let Some(gradient) = &scene.gradient else {
        return scene.background;
    };
    let lerp = |s: f64, e, t| s + (e - s) * t;
    let bilerp = |[ul, ur, ll, lr]: [f64; 4], x, y| lerp(lerp(ul, ur, x), lerp(ll, lr, x), y);
    if let Some(center) = gradient.center {
        // Java splits at floor(size / 2) - 1; the far edge may extrapolate.
        let split_x = (width / 2) as f64 - 1.0;
        let split_y = (height / 2) as f64 - 1.0;
        let (x, y) = (x as f64, y as f64);
        let left = x <= split_x;
        let top = y <= split_y;
        let tx = if left { x } else { x - split_x } / split_x;
        let ty = if top { y } else { y - split_y } / split_y;
        std::array::from_fn(|i| {
            let [ul, ur, ll, lr] = gradient.corners.map(|c| f64::from(c[i]));
            let cc = f64::from(center[i]);
            let corners = match (left, top) {
                (true, true) => [ul, lerp(ul, ur, 0.5), lerp(ll, ul, 0.5), cc],
                (true, false) => [lerp(ul, ll, 0.5), cc, ll, lerp(ll, lr, 0.5)],
                (false, true) => [lerp(ul, ur, 0.5), ur, cc, lerp(ur, lr, 0.5)],
                (false, false) => [cc, lerp(ur, lr, 0.5), lerp(ll, lr, 0.5), lr],
            };
            (bilerp(corners, tx, ty) + 0.5) as u8
        })
    } else {
        let tx = x as f64 / (width - 1) as f64;
        let ty = y as f64 / (height - 1) as f64;
        std::array::from_fn(|i| {
            (bilerp(gradient.corners.map(|c| f64::from(c[i])), tx, ty) + 0.5) as u8
        })
    }
}

fn scharr_log(raster: &[Bin], w: usize, h: usize, x: usize, y: usize) -> f64 {
    let lum = |dx, dy| {
        let (Some(px), Some(py)) = (x.checked_add_signed(dx), y.checked_add_signed(dy)) else {
            return 0.0;
        };
        if px >= w || py >= h {
            return 0.0;
        }
        let [r, g, b] = raster[py * w + px].rgb.map(f64::from);
        0.299 * r + 0.588 * g + 0.113 * b
    };
    let (tl, tr, bl, br) = (lum(-1, -1), lum(1, -1), lum(-1, 1), lum(1, 1));
    // Pinned Java's x derivative reads the top-right tap twice.
    let sx = 3.0 * tl - 3.0 * tr + 10.0 * lum(-1, 0) - 10.0 * tr + 3.0 * bl - 3.0 * br;
    let sy = 3.0 * tl + 10.0 * lum(0, -1) + 3.0 * tr - 3.0 * bl - 10.0 * lum(0, 1) - 3.0 * br;
    (1.0 + (sx * sx + sy * sy).sqrt()).log10()
}

fn sample_budget(
    area: usize,
    quality: usize,
    oversample: usize,
    cap: usize,
) -> Result<usize, String> {
    // Round half up after dividing by O, not O².
    area.checked_mul(quality)
        .and_then(|n| n.checked_add(oversample / 2))
        .map(|n| n / oversample)
        .filter(|&n| n <= cap)
        .ok_or_else(|| format!("sample budget exceeds {cap} iterations"))
}
