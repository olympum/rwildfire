//! Decoded CPU backdrop assets and the local PNG file adapter.

use crate::scene::{parse, Scene};
use std::{
    error::Error,
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

const MAX_AXIS: usize = 4096;

pub struct BackdropImage {
    width: usize,
    height: usize,
    rgb: Vec<u8>,
}

impl BackdropImage {
    /// Decode an opaque RGB8 PNG. Other pixel formats are not yet supported.
    pub fn read_png(input: impl Read) -> Result<Self, Box<dyn Error>> {
        let mut reader = png::Decoder::new(input).read_info()?;
        let info = reader.info();
        let (width, height) = (info.width as usize, info.height as usize);
        if !(2..=MAX_AXIS).contains(&width) || !(2..=MAX_AXIS).contains(&height) {
            return Err("background PNG dimensions must be in 2..=4096".into());
        }
        if info.color_type != png::ColorType::Rgb
            || info.bit_depth != png::BitDepth::Eight
            || info.trns.is_some()
            || info.animation_control.is_some()
        {
            return Err("background PNG must be opaque RGB8 and not animated".into());
        }
        let mut rgb = vec![0; reader.output_buffer_size()];
        reader.next_frame(&mut rgb)?;
        Ok(Self { width, height, rgb })
    }

    pub(crate) fn sample(&self, x: usize, y: usize, width: usize, height: usize) -> [u8; 3] {
        let pixel = |x: usize, y: usize| -> [u8; 3] {
            if x >= self.width || y >= self.height {
                return [0; 3];
            }
            let offset = 3 * (y * self.width + x);
            [self.rgb[offset], self.rgb[offset + 1], self.rgb[offset + 2]]
        };
        if (width, height) == (self.width, self.height) {
            return pixel(x, y);
        }
        // Java maps between endpoint pixel centers using output dimensions.
        let sx = x as f64 * (self.width - 1) as f64 / (width - 1) as f64;
        let sy = y as f64 * (self.height - 1) as f64 / (height - 1) as f64;
        let (ix, iy) = (sx as usize, sy as usize);
        let (tx, ty) = (sx.fract(), sy.fract());
        let corners = [
            pixel(ix, iy),
            pixel(ix + 1, iy),
            pixel(ix, iy + 1),
            pixel(ix + 1, iy + 1),
        ];
        let lerp = |a: f64, b, t| a + (b - a) * t;
        std::array::from_fn(|i| {
            let [ul, ur, ll, lr] = corners.map(|c| f64::from(c[i]));
            (lerp(lerp(ul, ur, tx), lerp(ll, lr, tx), ty) + 0.5) as u8
        })
    }
}

/// Read a local flame and bind any PNG named relative to its directory (or absolute).
pub fn load_scene_file(path: &Path) -> Result<Scene, Box<dyn Error>> {
    let mut scene = parse(&std::fs::read_to_string(path)?)?;
    if let Some(reference) = scene.background_image_path() {
        let path = path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(reference);
        let image = File::open(&path)
            .map(BufReader::new)
            .map_err(Box::<dyn Error>::from)
            .and_then(BackdropImage::read_png)
            .map_err(|err| format!("background image {}: {err}", path.display()))?;
        scene.bind_background_image(image);
    }
    Ok(scene)
}
