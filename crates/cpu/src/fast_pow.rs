// SPDX-License-Identifier: GPL-3.0-or-later
// Table-based log/exp following the Jafama algorithm family (see NOTICE.md),
// with operation order and knots checked against JWildfire's pinned ODK backend.
const LN_2: f64 = std::f64::consts::LN_2;
const SCALE_UP: f64 = 4503599627370496.0;
const SCALE_DOWN: f64 = 1.0 / 18014398509481984.0;
const EXP_HIGH_LIMIT: f64 = f64::from_bits(4649454530587146735);
const EXP_LOW_LIMIT: f64 = f64::from_bits((-4573606559926636463_i64) as u64);
type Tables = ([u64; 4096], [u64; 2049], [u64; 1455]);
static TABLES: Tables = include!("strict_power_tables.in");

// Julia's base is a sum of squares and its exponent lies in [-1, 0].
pub(crate) fn pow(base: f64, exponent: f64) -> f64 {
    if exponent == 0.0 {
        return 1.0;
    }
    exp(exponent * log(base))
}

fn log(mut x: f64) -> f64 {
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x == f64::INFINITY {
        return x;
    }
    if x.is_nan() {
        return x;
    }
    if x > 0.95 && x < 1.14 {
        let t = (x - 1.0) / (x + 1.0);
        let t2 = t * t;
        return t
            * (2.0
                + t2 * (2.0 / 3.0
                    + t2 * (0.4 + t2 * (2.0 / 7.0 + t2 * (2.0 / 9.0 + t2 * (2.0 / 11.0))))));
    }
    let correction = if x < f64::MIN_POSITIVE {
        x *= SCALE_UP;
        -52.0 * LN_2
    } else {
        0.0
    };
    let bits = x.to_bits();
    let exponent = (bits >> 52) as i32 - 1023;
    let index = ((bits >> 40) & 4095) as usize;
    // For exponent 1023 the inverse scale (2^-1023) is subnormal.
    let inverse = f64::from_bits(if exponent == 1023 {
        1 << 51
    } else {
        ((1023 - exponent) as u64) << 52
    });
    let knot = 1.0 + index as f64 / 4096.0;
    let e = x * inverse * (1.0 / knot) - 1.0;
    (correction + f64::from(exponent) * LN_2)
        + (f64::from_bits(TABLES.0[index]) + e * (1.0 - e * (0.5 - e * (1.0 / 3.0))))
}

fn exp(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x > EXP_HIGH_LIMIT {
        return f64::INFINITY;
    }
    if x < EXP_LOW_LIMIT {
        return 0.0;
    }
    let index = (x * 1024.0) as i32;
    let whole = index / 1024;
    let fraction = index - whole * 1024;
    let e = x - f64::from(whole) - f64::from(fraction) / 1024.0;
    let polynomial = 1.0 + e * (1.0 + e * (0.5 + e * (1.0 / 6.0 + e * (1.0 / 24.0))));
    let y = f64::from_bits(TABLES.2[(whole + 745) as usize])
        * (f64::from_bits(TABLES.1[(fraction + 1024) as usize]) * polynomial);
    if whole < -705 {
        y * SCALE_DOWN
    } else {
        y
    }
}
