use rwildfire_cpu::{
    scene::{parse, VariationKind},
    Bin, Sampler,
};
use std::collections::BTreeMap;

fn exact(got: &[f64], fields: &[&str], context: &str) {
    let expected: Vec<f64> = fields.iter().map(|v| v.parse().unwrap()).collect();
    assert_eq!(got, expected, "{context}");
}

fn verify(xml: &str, reference: &str) {
    let sc = parse(xml).unwrap();
    let w = sc.width * 2;
    let mut sampler = Sampler::new(&sc, w, sc.height * 2);
    let mut bins: BTreeMap<usize, Bin> = BTreeMap::new();
    let mut lines = reference
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>());
    let mut row = |tag| {
        let v = lines.next().expect("missing Java trace row");
        assert_eq!(v[0], tag);
        v
    };
    exact(
        &[
            sc.width as f64,
            sc.height as f64,
            sc.ppu,
            sc.zoom,
            sc.center[0],
            sc.center[1],
            sc.brightness,
            sc.gamma,
            sc.threshold,
            sc.roll_degrees,
        ],
        &row("scene")[1..],
        "scene",
    );
    for (i, t) in sc.transforms.iter().enumerate() {
        let [a, b, c, d, e, f] = t.affine;
        exact(
            &[t.weight, t.color, t.symmetry, a, b, c, d, e, f],
            &row("xform")[1..],
            &format!("xform {i}"),
        );
        for v in &t.variations {
            let fields = row("variation");
            assert_eq!(fields[1], v.kind.name(), "xform {i} variation order");
            let values = match v.kind {
                VariationKind::Pdj { a, b, c, d } => vec![v.amount, a, b, c, d],
                VariationKind::Rectangles { x, y } => vec![v.amount, x, y],
                VariationKind::Curl3D { cx, cy, cz } => vec![v.amount, cx, cy, cz],
                VariationKind::Julia3D { power } => vec![v.amount, f64::from(power)],
                VariationKind::Fan2 { x, y } => vec![v.amount, x, y],
                _ => vec![v.amount],
            };
            exact(
                &values,
                &fields[2..],
                &format!("xform {i} {}", v.kind.name()),
            );
        }
        assert_eq!(row("end-xform").len(), 1);
    }
    for (i, rgb) in sc.palette.iter().enumerate() {
        exact(
            &rgb.map(f64::from),
            &row("palette")[1..],
            &format!("palette {i}"),
        );
    }
    for i in 0..256 {
        let step = sampler.step();
        let [x, y] = step.pixel.map_or([-1., -1.], |xy| xy.map(|p| p as f64));
        exact(
            &[
                step.transform as f64,
                step.point[0],
                step.point[1],
                step.z,
                step.color,
                x,
                y,
            ],
            &row("point")[1..],
            &format!("point {i}"),
        );
        if let Some([x, y]) = step.pixel {
            bins.entry(y * w + x)
                .or_default()
                .add(sc.palette[step.palette_index]);
        }
    }
    assert!(bins.len() > 100, "too few occupied raster bins");
    for (k, bin) in bins {
        let [r, g, b] = bin.rgb.map(f64::from);
        exact(
            &[k as f64, f64::from(bin.count), r, g, b],
            &row("bin")[1..],
            &format!("bin {k}"),
        );
    }
    assert!(lines.next().is_none(), "unexpected Java trace rows");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths: Vec<_> = std::env::args_os().skip(1).collect();
    let [source, captures, synthetic] = paths.as_slice() else {
        return Err("usage: check_java_scene UPSTREAM_EXPORT CAPTURE_DIR SYNTHETIC_FLAME".into());
    };
    let source = std::path::Path::new(source).join("resources/flames");
    let captures = std::path::Path::new(captures);
    for name in [
        "TINA0010", "TINA0004", "TINA0006", "TINA0008", "TINA0011", "TINA0021", "TINA0009",
        "TINA0018", "TINA0013", "TINA0016", "TINA0003", "TINA0001", "TINA0007", "TINA0005",
        "TINA0015", "TINA0023", "TINA0025", "TINA0032", "TINA0002", "TINA0012", "TINA0017",
        "TINA0020", "TINA0022", "TINA0014", "TINA0019", "TINA0024",
    ] {
        let filename = format!("{name}.flame");
        let controlled = captures.join("inputs").join(&filename);
        let path = if controlled.is_file() {
            let original = std::fs::read_to_string(source.join(&filename))?;
            assert!(
                matches!(parse(&original), Err(reason) if reason.contains("julia3D_power")),
                "{name} should reject omitted power"
            );
            controlled
        } else {
            source.join(&filename)
        };
        let xml = std::fs::read_to_string(path)?;
        let trace = std::fs::read_to_string(captures.join(format!("{name}.txt")))?;
        println!("{name}: comparing Rust against fresh Java scene trace");
        verify(&xml, &trace);
        println!("{name}: model, 256 points and occupied bins match exactly");
    }
    let xml = std::fs::read_to_string(synthetic)?;
    let trace = std::fs::read_to_string(captures.join("ordered-variations.txt"))?;
    println!("ordered-variations: comparing Rust against fresh Java scene trace");
    verify(&xml, &trace);
    println!("ordered-variations: model, 256 points and occupied bins match exactly");
    Ok(())
}
