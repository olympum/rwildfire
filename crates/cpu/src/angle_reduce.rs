// SPDX-License-Identifier: GPL-3.0-or-later
// Adapted from JWildfire 9.20 FastMathLibImpl, with Jafama and fdlibm
// ancestry (see NOTICE.md). Keep the floating point operation order.
use std::f64::consts::TAU;

const TWO_PI_HI: f64 = 4.0 * f64::from_bits(4609753056924401664);
const TWO_PI_LO: f64 = 4.0 * f64::from_bits(4454258360616903473);
const INV_TWO_PI: f64 = f64::from_bits(4603909380684499075) / 4.0;
const BASE: i32 = 1 << 24;
const INV_BASE: f64 = 1.0 / BASE as f64;
const RECIPROCAL: [u32; 47] = [
    2670176, 14390161, 346751, 644596, 8211767, 7354072, 10839631, 1106960, 8361048, 15398830,
    15816813, 13179790, 9474932, 12059026, 4962946, 7627911, 4163450, 13053002, 6934458, 2133373,
    4959953, 2177639, 1837485, 1564560, 5137525, 9330900, 13532455, 2168802, 15695434, 968702,
    2490359, 8480259, 16501700, 6477442, 10176475, 5087155, 13234882, 7197649, 9427367, 9960075,
    6113774, 11664121, 8150735, 4312701, 14849188, 12229374, 14150727,
];
const TWO_PI: [f64; 5] = [
    f64::from_bits(4618760255839404032),
    f64::from_bits(4509304086968926208),
    f64::from_bits(4402346256551116800),
    f64::from_bits(4294406894572797952),
    f64::from_bits(4183874305429340160),
];

pub(crate) fn remainder_two_pi(x: f64) -> f64 {
    let abs = x.abs();
    let rem = if abs <= 1048576.0 * TAU {
        let turns = f64::from((abs * INV_TWO_PI + 0.5) as i32);
        abs - turns * TWO_PI_HI - turns * TWO_PI_LO
    } else if abs.is_finite() {
        large_remainder(abs)
    } else {
        return f64::NAN;
    };
    if x < 0.0 {
        -rem
    } else {
        rem
    }
}

fn large_remainder(angle: f64) -> f64 {
    let bits = angle.to_bits();
    let exponent = ((bits >> 52) as i32) - 1046;
    let scaled = f64::from_bits(bits.wrapping_sub((exponent as u64) << 52));
    let high = scaled.trunc();
    let mid = (scaled - high) * f64::from(BASE);
    let low = (mid - mid.trunc()) * f64::from(BASE);
    let group = (exponent - 3) / 24;
    let shift = exponent - group * 24 - 24;
    let first = group + 4;
    let knot = |i: i32| {
        if i < 0 {
            0.0
        } else {
            f64::from(RECIPROCAL[i as usize])
        }
    };
    let terms = |i| {
        if low == 0.0 {
            high * knot(i) + mid.trunc() * knot(i - 1)
        } else {
            high * knot(i) + mid.trunc() * knot(i - 1) + low * knot(i - 2)
        }
    };
    let scale = f64::from_bits(((shift + 1023) as u64) << 52);
    let reduce = |extra: usize| {
        let mut carry = 0.0;
        let mut digits = [0; 5];
        for (k, digit) in digits.iter_mut().enumerate().take(4 + extra) {
            let v = terms(first + extra as i32 - k as i32) + carry;
            carry = f64::from((INV_BASE * v) as i32);
            *digit = (v - f64::from(BASE) * carry) as i32;
        }
        let fraction = ((terms(first - 4) + carry) * scale % 8.0).fract();
        let top = &mut digits[3 + extra];
        let sign = if shift >= 0 {
            if shift > 0 {
                *top &= (BASE - 1) >> shift;
            }
            *top >> (23 - shift)
        } else if fraction >= 0.5 {
            2
        } else {
            0
        };
        let mut carried = false;
        if sign != 0 {
            for d in &mut digits[..4 + extra] {
                if carried {
                    *d = BASE - 1 - *d;
                } else if *d != 0 {
                    *d = BASE - *d;
                    carried = true;
                }
            }
            if shift > 0 {
                digits[3 + extra] &= (BASE >> shift) - 1;
            }
        }
        (digits, fraction, sign, carried)
    };
    let (mut digits, mut fraction, mut sign, carried) = reduce(0);
    if sign == 2 {
        fraction = 1.0 - fraction;
        if carried {
            fraction -= scale;
        }
    }
    let mut scale = scale;
    if fraction == 0.0 {
        (digits, _, sign, _) = reduce(1);
        scale *= INV_BASE;
    } else {
        digits[4] = (fraction / scale) as i32;
    }
    let mut parts = [0.0; 5];
    for (part, digit) in parts.iter_mut().zip(digits.into_iter().rev()) {
        *part = scale * f64::from(digit);
        scale *= INV_BASE;
    }
    let mut result = TWO_PI[0] * parts[0];
    for i in 1..5 {
        let mut term = TWO_PI[0] * parts[i];
        for j in 1..=i {
            term += TWO_PI[j] * parts[i - j];
        }
        result += term;
    }
    if sign == 0 {
        result
    } else {
        -result
    }
}
