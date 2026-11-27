// SPDX-License-Identifier: GPL-3.0-or-later
// Quadrant handling adapted from JWildfire 9.20 FastMathLibImpl (Andreas Maschke,
// LGPL-2.1-or-later); interpolation from its bundled FastMath (Jafama algorithm,
// Copyright 2012–2017 Jeff Hain, Apache-2.0, with fdlibm ancestry). See NOTICE.md.
use std::{
    f64::consts::{FRAC_PI_2, FRAC_PI_4, PI},
    sync::OnceLock,
};

const INTERVALS: usize = 4096;
const STEP: f64 = f64::from_bits(0x3f4b_e639_8b3f_2869);
const MAX: f64 = STEP * INTERVALS as f64;
const INDEXER: f64 = 1.0 / STEP;
static KNOTS: [u64; INTERVALS + 1] = include!("strict_atan_table.in");
static DERIVATIVES: OnceLock<Vec<[f64; 4]>> = OnceLock::new();

pub(crate) fn atan(angle: f64) -> f64 {
    let abs = angle.abs();
    let result = if abs == 1.0 {
        FRAC_PI_4
    } else if abs <= MAX {
        let index = (abs * INDEXER + 0.5) as usize;
        let delta = abs - index as f64 * STEP;
        let [a, b, c, d] = DERIVATIVES.get_or_init(|| {
            (0..=INTERVALS)
                .map(|i| {
                    let x = i as f64 * STEP;
                    let r = 1.0 / (1.0 + x * x);
                    let r2 = r * r;
                    let r3 = r2 * r;
                    let r4 = r2 * r2;
                    [
                        r,
                        -2.0 * x * r2 * 0.5,
                        (-2.0 + 6.0 * x * x) * r3 * (1.0 / 6.0),
                        24.0 * x * (1.0 - x * x) * r4 * (1.0 / 24.0),
                    ]
                })
                .collect()
        })[index];
        f64::from_bits(KNOTS[index]) + delta * (a + delta * (b + delta * (c + delta * d)))
    } else if abs < (1_u128 << 66) as f64 {
        const C: [f64; 11] = [
            f64::from_bits(0x3fd5_5555_5555_550d),
            f64::from_bits(0xbfc9_9999_9998_ebc4),
            f64::from_bits(0x3fc2_4924_9200_83ff),
            f64::from_bits(0xbfbc_71c6_fe23_1671),
            f64::from_bits(0x3fb7_45cd_c54c_206e),
            f64::from_bits(0xbfb3_b0f2_af74_9a6d),
            f64::from_bits(0x3fb1_0d66_a0d0_3d51),
            f64::from_bits(0xbfad_de2d_52de_fd9a),
            f64::from_bits(0x3fa9_7b4b_2476_0deb),
            f64::from_bits(0xbfa2_b444_2c6a_6c2f),
            f64::from_bits(0x3f90_ad3a_e322_da11),
        ];
        const LO: f64 = f64::from_bits(0x3c91_a626_3314_5c07);
        let x = -1.0 / abs;
        let z = x * x;
        let w = z * z;
        let even = z * (C[0] + w * (C[2] + w * (C[4] + w * (C[6] + w * (C[8] + w * C[10])))));
        let odd = w * (C[1] + w * (C[3] + w * (C[5] + w * (C[7] + w * C[9]))));
        FRAC_PI_2 - ((x * (even + odd) - LO) - x)
    } else if abs.is_nan() {
        return abs;
    } else {
        FRAC_PI_2
    };
    if angle < 0.0 {
        -result
    } else {
        result
    }
}

pub(crate) fn atan2(y: f64, x: f64) -> f64 {
    if y.is_nan() || x.is_nan() {
        return f64::NAN;
    }
    if y == 0.0 {
        return if x < 0.0 { PI } else { 0.0 };
    }
    if x.is_infinite() {
        let magnitude = if y.is_infinite() {
            if x > 0.0 {
                FRAC_PI_4
            } else {
                3.0 * FRAC_PI_4
            }
        } else if x > 0.0 {
            0.0
        } else {
            PI
        };
        return magnitude.copysign(y);
    }
    if x > 0.0 {
        atan(y / x)
    } else if x == 0.0 {
        FRAC_PI_2.copysign(y)
    } else if y > 0.0 {
        FRAC_PI_2 + atan(-x / y)
    } else {
        -FRAC_PI_2 - atan(x / y)
    }
}
