use roxmltree::{Document, Node};

pub struct Scene {
    pub width: usize,
    pub height: usize,
    pub ppu: f64,
    pub zoom: f64,
    pub roll_degrees: f64,
    pub pitch_degrees: f64,
    pub yaw_degrees: f64,
    pub perspective: f64,
    pub center: [f64; 2],
    pub brightness: f64,
    pub gamma: f64,
    pub threshold: f64,
    pub filter_radius: f64,
    pub layers: Vec<Layer>,
}

pub struct Layer {
    pub(crate) weight: f64,
    pub(crate) density: f64,
    pub transforms: Vec<Transform>,
    pub(crate) final_transforms: Vec<Transform>,
    pub palette: [[u8; 3]; 256],
}
pub struct Transform {
    pub weight: f64,
    pub(crate) chaos: Option<Vec<f64>>,
    pub color: f64,
    pub symmetry: f64,
    pub affine: [f64; 6],
    pub(crate) post: Option<[f64; 6]>,
    pub variations: Vec<Variation>,
}
pub struct Variation {
    pub amount: f64,
    pub kind: VariationKind,
}

#[derive(Clone, Copy)]
pub enum VariationKind {
    Linear3D,
    Spherical3D,
    Waves,
    Bubble,
    Spiral,
    Pdj { a: f64, b: f64, c: f64, d: f64 },
    Disc,
    Hemisphere,
    Swirl,
    Rectangles { x: f64, y: f64 },
    Curl3D { cx: f64, cy: f64, cz: f64 },
    Heart,
    Horseshoe,
    Fan2 { x: f64, y: f64 },
    Diamond,
    Popcorn,
    Blur,
    Blur3D,
    Julia3D { power: i32 },
}

impl VariationKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Linear3D => "linear3D",
            Self::Spherical3D => "spherical3D",
            Self::Waves => "waves",
            Self::Bubble => "bubble",
            Self::Spiral => "spiral",
            Self::Pdj { .. } => "pdj",
            Self::Disc => "disc",
            Self::Hemisphere => "hemisphere",
            Self::Swirl => "swirl",
            Self::Rectangles { .. } => "rectangles",
            Self::Curl3D { .. } => "curl3D",
            Self::Heart => "heart",
            Self::Horseshoe => "horseshoe",
            Self::Fan2 { .. } => "fan2",
            Self::Diamond => "diamond",
            Self::Popcorn => "popcorn",
            Self::Blur => "blur",
            Self::Blur3D => "blur3D",
            Self::Julia3D { .. } => "julia3D",
        }
    }
}

type Element<'a, 'i> = Node<'a, 'i>;
fn required<'i>(n: Element<'_, 'i>, key: &str) -> Result<String, String> {
    n.attribute(key)
        .map(str::to_owned)
        .ok_or_else(|| format!("missing {key}"))
}
fn finite(s: &str, key: &str) -> Result<f64, String> {
    s.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("invalid {key}: {s}"))
}
fn number(n: Element<'_, '_>, key: &str, default: f64) -> Result<f64, String> {
    n.attribute(key).map_or(Ok(default), |s| finite(s, key))
}
fn vector<T: std::str::FromStr, const N: usize>(
    n: Element<'_, '_>,
    key: &str,
) -> Result<[T; N], String> {
    required(n, key)?
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<Vec<_>, _>>()
        .ok()
        .and_then(|v| v.try_into().ok())
        .ok_or_else(|| format!("expected {N} values for {key}"))
}
fn allowed(n: Element<'_, '_>, keys: &[&str]) -> Result<(), String> {
    if n.tag_name().namespace().is_some() {
        return Err("namespaced element is unsupported".into());
    }
    for a in n.attributes() {
        if a.namespace().is_some() || !keys.contains(&a.name()) {
            return Err(format!(
                "unsupported {} attribute: {}",
                n.tag_name().name(),
                a.name()
            ));
        }
    }
    Ok(())
}
fn child_elements<'a, 'i>(n: Element<'a, 'i>) -> Result<Vec<Element<'a, 'i>>, String> {
    let mut elements = Vec::new();
    for child in n.children() {
        if child.is_element() {
            elements.push(child);
        } else if child.is_pi()
            || child.text().is_some_and(|s| !s.trim().is_empty()) && child.is_text()
        {
            return Err(format!("unexpected content inside {}", n.tag_name().name()));
        }
    }
    Ok(elements)
}

pub fn parse(xml: &str) -> Result<Scene, String> {
    let doc = Document::parse(xml).map_err(|err| err.to_string())?;
    let n = doc.root_element();
    let layered = match n.tag_name().name() {
        "jwf-flame" => true,
        "flame" => false,
        _ => return Err("expected a single flame or jwf-flame element".into()),
    };
    // Quality is a CLI setting; legacy estimator fields are not implemented.
    allowed(
        n,
        &[
            "name",
            "version",
            "size",
            "center",
            "scale",
            "rotate",
            "oversample",
            "filter",
            "quality",
            "background",
            "brightness",
            "gamma",
            "gamma_threshold",
            "estimator_radius",
            "estimator_minimum",
            "estimator_curve",
            "temporal_samples",
            "cam_zoom",
            "cam_pitch",
            "cam_yaw",
            "cam_perspective",
        ],
    )?;
    for (key, expected) in [("oversample", 1.0), ("temporal_samples", 1.0)] {
        if number(n, key, expected)? != expected {
            return Err(format!("unsupported non-default {key}"));
        }
    }
    if n.attribute("background").is_some() && vector::<f64, 3>(n, "background")? != [0.; 3] {
        return Err("only a black background is supported".into());
    }
    let [width, height] = vector::<usize, 2>(n, "size")?;
    let center = vector::<f64, 2>(n, "center")?;
    let ppu = finite(&required(n, "scale")?, "scale")?;
    let zoom = number(n, "cam_zoom", 1.)?;
    let roll_degrees = number(n, "rotate", 0.)?;
    // Legacy pitch/yaw are radians in XML; Java stores them in degrees.
    let pitch_degrees = number(n, "cam_pitch", 0.)? * 180.0 / std::f64::consts::PI;
    let yaw_degrees = number(n, "cam_yaw", 0.)? * 180.0 / std::f64::consts::PI;
    let perspective = number(n, "cam_perspective", 0.)?;
    let brightness = number(n, "brightness", 4.)?;
    let gamma = number(n, "gamma", 4.)?;
    let threshold = number(n, "gamma_threshold", 0.04)?;
    let filter_radius = number(n, "filter", 0.0)?;
    if filter_radius != 0.0 && filter_radius != 1.0 {
        return Err("only filter radii 0 and 1 are supported".into());
    }
    if !(1..=2048).contains(&width)
        || !(1..=2048).contains(&height)
        || center.iter().any(|v| !v.is_finite())
        || ppu <= 0.
        || zoom <= 0.
        || brightness <= 0.
        || gamma <= 0.
        || threshold <= 0.
        || !(ppu * zoom).is_finite()
        || [roll_degrees, pitch_degrees, yaw_degrees]
            .iter()
            .any(|angle| !(angle * std::f64::consts::PI).is_finite())
    {
        return Err("invalid size, camera or tone settings".into());
    }
    let layers = if layered {
        let nodes = child_elements(n)?;
        if !(1..=16).contains(&nodes.len()) {
            return Err("expected 1..=16 layers".into());
        }
        nodes
            .into_iter()
            .map(|node| {
                if node.tag_name().name() != "layer" {
                    return Err("expected layer inside jwf-flame".into());
                }
                allowed(node, &["name", "visible", "weight", "density"])?;
                if node.attribute("visible").is_some_and(|v| v != "1") {
                    return Err("only visible layers are supported".into());
                }
                let weight = number(node, "weight", 1.0)?;
                let density = number(node, "density", 1.0)?;
                if !(0.0..=1.0).contains(&weight) || !(0.0..=1.0).contains(&density) {
                    return Err("layer weight and density must be in 0..=1".into());
                }
                read_layer(node, weight, density)
            })
            .collect::<Result<Vec<_>, String>>()?
    } else {
        vec![read_layer(n, 1.0, 1.0)?]
    };
    Ok(Scene {
        width,
        height,
        ppu,
        zoom,
        roll_degrees,
        pitch_degrees,
        yaw_degrees,
        perspective,
        center,
        brightness,
        gamma,
        threshold,
        filter_radius,
        layers,
    })
}

fn read_layer(n: Element<'_, '_>, weight: f64, density: f64) -> Result<Layer, String> {
    let mut transforms = Vec::new();
    let mut final_transforms = Vec::new();
    let mut finals = Vec::new();
    let mut palette = None;
    let mut xforms = Vec::new();
    for c in child_elements(n)? {
        match c.tag_name().name() {
            "xform" => xforms.push(c),
            "finalxform" => finals.push(c),
            "palette" if palette.is_none() => palette = Some(read_palette(c)?),
            name => return Err(format!("unsupported or duplicate flame child: {name}")),
        }
    }
    if !(1..=64).contains(&xforms.len()) {
        return Err("expected 1..=64 xforms".into());
    }
    if finals.len() > 64 {
        return Err("expected at most 64 finalxforms".into());
    }
    for (c, is_final) in xforms
        .iter()
        .map(|&c| (c, false))
        .chain(finals.into_iter().map(|c| (c, true)))
    {
        allowed(
            c,
            &[
                "weight",
                "color",
                "symmetry",
                "coefs",
                "post",
                "chaos",
                "linear3D",
                "spherical3D",
                "waves",
                "bubble",
                "spiral",
                "pdj",
                "pdj_a",
                "pdj_b",
                "pdj_c",
                "pdj_d",
                "disc",
                "hemisphere",
                "swirl",
                "rectangles",
                "rectangles_x",
                "rectangles_y",
                "curl3D",
                "curl3D_cx",
                "curl3D_cy",
                "curl3D_cz",
                "heart",
                "horseshoe",
                "fan2",
                "fan2_x",
                "fan2_y",
                "diamond",
                "popcorn",
                "blur",
                "blur3D",
                "julia3D",
                "julia3D_power",
            ],
        )?;
        if !child_elements(c)?.is_empty() {
            return Err("xform children are unsupported".into());
        }
        let chaos = if let Some(chaos) = c.attribute("chaos") {
            if is_final {
                return Err("chaos is unsupported on finalxform".into());
            }
            let values = chaos
                .split_whitespace()
                .map(|v| finite(v, "chaos"))
                .collect::<Result<Vec<_>, _>>()?;
            if values.len() != xforms.len() || values.iter().any(|v| *v < 0.0) {
                return Err("chaos requires one nonnegative multiplier per xform".into());
            }
            Some(values)
        } else {
            None
        };
        let (weight, color) = if is_final {
            (number(c, "weight", 0.0)?, number(c, "color", 0.0)?)
        } else {
            (
                finite(&required(c, "weight")?, "weight")?,
                finite(&required(c, "color")?, "color")?,
            )
        };
        let symmetry = number(c, "symmetry", 0.)?;
        let affine = vector::<f64, 6>(c, "coefs")?;
        let post = if c.attribute("post").is_some() {
            let coefficients = vector::<f64, 6>(c, "post")?;
            if coefficients.iter().any(|v| !v.is_finite()) {
                return Err("post coefficients must be finite".into());
            }
            // XForm.updateHasXYPostCoeffs uses a strict epsilon test.
            let identity = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
            coefficients
                .iter()
                .zip(identity)
                .any(|(v, id)| (v - id).abs() > 1.0e-8)
                .then_some(coefficients)
        } else {
            None
        };
        if c.attribute("pdj").is_none()
            && ["pdj_a", "pdj_b", "pdj_c", "pdj_d"]
                .iter()
                .any(|key| c.attribute(*key).is_some())
        {
            return Err("pdj parameters require a pdj variation".into());
        }
        if c.attribute("rectangles").is_none()
            && (c.attribute("rectangles_x").is_some() || c.attribute("rectangles_y").is_some())
        {
            return Err("rectangles parameters require a rectangles variation".into());
        }
        if c.attribute("curl3D").is_none()
            && ["curl3D_cx", "curl3D_cy", "curl3D_cz"]
                .iter()
                .any(|key| c.attribute(*key).is_some())
        {
            return Err("curl3D parameters require a curl3D variation".into());
        }
        if c.attribute("fan2").is_none()
            && (c.attribute("fan2_x").is_some() || c.attribute("fan2_y").is_some())
        {
            return Err("fan2 parameters require a fan2 variation".into());
        }
        if c.attribute("julia3D").is_none() && c.attribute("julia3D_power").is_some() {
            return Err("julia3D_power requires a julia3D variation".into());
        }
        // AbstractFlameReader inserts variations as it walks XML attributes;
        // XForm evaluates the supported priority-zero variations in that order.
        let mut variations = Vec::new();
        for attr in c.attributes() {
            let kind = match attr.name() {
                "linear3D" => VariationKind::Linear3D,
                "spherical3D" => VariationKind::Spherical3D,
                "waves" => VariationKind::Waves,
                "bubble" => VariationKind::Bubble,
                "spiral" => VariationKind::Spiral,
                "pdj" => VariationKind::Pdj {
                    a: number(c, "pdj_a", 1.0)?,
                    b: number(c, "pdj_b", 2.0)?,
                    c: number(c, "pdj_c", 3.0)?,
                    d: number(c, "pdj_d", 4.0)?,
                },
                "disc" => VariationKind::Disc,
                "hemisphere" => VariationKind::Hemisphere,
                "swirl" => VariationKind::Swirl,
                "heart" => VariationKind::Heart,
                "horseshoe" => VariationKind::Horseshoe,
                "fan2" => VariationKind::Fan2 {
                    x: number(c, "fan2_x", 0.5)?,
                    y: number(c, "fan2_y", 1.2)?,
                },
                "diamond" => VariationKind::Diamond,
                "popcorn" => VariationKind::Popcorn,
                "blur" => VariationKind::Blur,
                "blur3D" => VariationKind::Blur3D,
                "julia3D" => {
                    let power = finite(&required(c, "julia3D_power")?, "julia3D_power")?;
                    let power = ((power.abs() + 0.5).copysign(power)) as i32;
                    if power == 0 {
                        return Err("julia3D_power rounds to zero".into());
                    }
                    VariationKind::Julia3D { power }
                }
                "curl3D" => VariationKind::Curl3D {
                    cx: number(c, "curl3D_cx", 0.0)?,
                    cy: number(c, "curl3D_cy", 0.05)?,
                    cz: number(c, "curl3D_cz", 0.05)?,
                },
                "rectangles" => VariationKind::Rectangles {
                    x: number(c, "rectangles_x", 0.3)?,
                    y: number(c, "rectangles_y", 0.2)?,
                },
                _ => continue,
            };
            variations.push(Variation {
                kind,
                amount: finite(attr.value(), attr.name())?,
            });
        }
        if weight < 0.
            || !(0.0..=1.0).contains(&color)
            || !(-1.0..=1.0).contains(&symmetry)
            || affine.iter().any(|v| !v.is_finite())
            || variations.iter().all(|v| v.amount == 0.0)
        {
            return Err("invalid xform weight, color, coefficients or variation amounts".into());
        }
        let target = if is_final {
            &mut final_transforms
        } else {
            &mut transforms
        };
        target.push(Transform {
            weight,
            chaos,
            color,
            symmetry,
            affine,
            post,
            variations,
        });
    }
    let total = transforms.iter().map(|t| t.weight).sum::<f64>();
    if total <= 0. || !total.is_finite() {
        return Err("invalid total xform weight".into());
    }
    for (index, transform) in transforms.iter().enumerate() {
        if let Some(row) = &transform.chaos {
            let outgoing = transforms
                .iter()
                .zip(row)
                .map(|(target, multiplier)| target.weight * multiplier)
                .sum::<f64>();
            if outgoing <= 0.0 || !outgoing.is_finite() {
                return Err(format!(
                    "chaos row {index} requires a finite positive outgoing weight"
                ));
            }
        }
    }
    Ok(Layer {
        weight,
        density,
        transforms,
        final_transforms,
        palette: palette.ok_or("missing palette")?,
    })
}
fn read_palette(n: Element<'_, '_>) -> Result<[[u8; 3]; 256], String> {
    allowed(n, &["count", "format"])?;
    if n.attribute("count") != Some("256") || n.attribute("format") != Some("RGB") {
        return Err("expected a 256-entry RGB palette".into());
    }
    let mut hex = Vec::new();
    for child in n.children() {
        if child.is_text() {
            hex.extend(
                child
                    .text()
                    .unwrap_or_default()
                    .bytes()
                    .filter(|b| !b.is_ascii_whitespace()),
            );
        } else if !child.is_comment() {
            return Err("palette must contain only hexadecimal text".into());
        }
    }
    if hex.len() != 256 * 6 || !hex.iter().all(u8::is_ascii_hexdigit) {
        return Err("palette must have 768 hexadecimal byte values".into());
    }
    let mut palette = [[0; 3]; 256];
    for (channel, pair) in palette.iter_mut().flatten().zip(hex.as_chunks::<2>().0) {
        *channel = u8::from_str_radix(
            std::str::from_utf8(pair).map_err(|_| "invalid palette")?,
            16,
        )
        .map_err(|_| "invalid palette")?;
    }
    Ok(palette)
}
