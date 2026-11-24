// SPDX-License-Identifier: GPL-3.0-or-later
#[path = "../src/fast_pow.rs"]
mod fast_pow;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::args_os()
        .nth(1)
        .ok_or("expected Java power capture")?;
    let capture = std::fs::read_to_string(input)?;
    let mut count = 0;
    for row in capture.lines() {
        let mut fields = row
            .split_ascii_whitespace()
            .map(|value| u64::from_str_radix(value, 16).map(f64::from_bits));
        let base = fields.next().ok_or("missing base")??;
        let exponent = fields.next().ok_or("missing exponent")??;
        let expected = fields.next().ok_or("missing result")??;
        assert!(fields.next().is_none());
        let actual = fast_pow::pow(base, exponent);
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "pow({base:e}, {exponent:e})"
        );
        count += 1;
    }
    assert_eq!(count, 113_292);
    println!("FastMath: {count} Julia3D radial powers match Java bit-for-bit");
    Ok(())
}
