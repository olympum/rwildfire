#[path = "../src/angle_reduce.rs"]
mod angle_reduce;
#[path = "../src/fast_sin.rs"]
mod fast_sin;
#[path = "../src/fast_tan.rs"]
mod fast_tan;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("expected Java trig capture")?;
    let capture = std::fs::read_to_string(path)?;
    let mut count = 0;
    for row in capture.lines() {
        let fields = row
            .split_ascii_whitespace()
            .map(|value| u64::from_str_radix(value, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let &[x, sine, cosine, tangent] = fields.as_slice() else {
            return Err("invalid Java trig row".into());
        };
        let x = f64::from_bits(x);
        for (name, got, expected) in [
            ("sin", fast_sin::sin(x), sine),
            ("cos", fast_sin::cos(x), cosine),
            ("tan", fast_tan::tan(x), tangent),
        ] {
            assert!(
                got.to_bits() == expected || (got.is_nan() && f64::from_bits(expected).is_nan()),
                "{name}({x:e}): got {:016x}, Java {expected:016x}",
                got.to_bits()
            );
        }
        count += 1;
    }
    assert_eq!(count, 2091);
    println!(
        "FastMath: {count} sine, cosine and tangent inputs match Java exactly (NaNs equivalent)"
    );
    Ok(())
}
