// SPDX-License-Identifier: GPL-3.0-or-later
// Adapted from JWildfire 9.20 FastMathLibImpl (Andreas Maschke, LGPL-2.1-or-later).
// Jafama algorithm attribution: Copyright 2012–2017 Jeff Hain (Apache-2.0). See NOTICE.md.
use std::sync::OnceLock;

const SIZE: usize = 4096;
static MANTISSAS: OnceLock<[(f64, f64); SIZE]> = OnceLock::new();

pub(crate) fn sqrt(mut x: f64) -> f64 {
    if x <= 0.0 || !x.is_finite() {
        return x.sqrt();
    }
    let scale = if x < f64::MIN_POSITIVE {
        x *= (1_u64 << 52) as f64;
        2.0 / (1_u64 << 26) as f64
    } else {
        2.0
    };
    let bits = x.to_bits();
    let exponent = (bits >> 52) as i32 - 1023;
    let mantissa = ((bits >> 40) & 4095) as usize;
    let exponent_root = f64::from_bits(((exponent.div_euclid(2) + 1023) as u64) << 52)
        * if exponent & 1 == 0 {
            1.0
        } else {
            // StrictMath.pow(2, e/2) seeds odd exponents one ulp below SQRT_2.
            f64::from_bits(0x3ff6_a09e_667f_3bcc)
        };
    let (root, inverse) = MANTISSAS.get_or_init(|| {
        std::array::from_fn(|i| {
            let root = if i == 0 {
                1.0
            } else {
                f64::from_bits(1.0_f64.to_bits() | (((i as u64) << 40) - 1)).sqrt()
            };
            (root, 1.0 / root)
        })
    })[mantissa];
    let mut estimate = (exponent_root * 0.5) * root;
    let slope = (1.0 / exponent_root) * inverse;
    let quarter = x * 0.25;
    for _ in 0..3 {
        estimate += (quarter - estimate * estimate) * slope;
    }
    scale * estimate
}
