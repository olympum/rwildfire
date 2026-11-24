// SPDX-License-Identifier: GPL-3.0-or-later
// Adapted from JWildfire 9.20 FastMathLibImpl (Andreas Maschke, LGPL-2.1-or-later).
// Jafama algorithm attribution: Copyright 2012–2017 Jeff Hain (Apache-2.0).
// This port uses fixed lookup bits. See NOTICE.md.
const SIZE: usize = 2048;
const STEP_HI: f64 = 4.0 * f64::from_bits(4609753056924401664) / SIZE as f64;
const STEP_LO: f64 = 4.0 * f64::from_bits(4454258360616903473) / SIZE as f64;
const INDEXER: f64 = 1.0 / (STEP_HI + STEP_LO);
// Exact StrictMath bits from oracle/StrictTrigTable.java; no runtime JDK needed.
static TABLE: [(u64, u64); SIZE] = include!("strict_trig_table.in");

fn lookup(mut abs: f64) -> Option<(f64, f64, f64)> {
    if !abs.is_finite() {
        return None;
    }
    if abs > 4194303.0 / INDEXER * 0.99 {
        abs = crate::angle_reduce::remainder_two_pi(abs);
        if abs < 0.0 {
            abs += std::f64::consts::TAU;
        }
    }
    let index = (abs * INDEXER + 0.5) as usize;
    let i = index as f64;
    let d = abs - i * STEP_HI - i * STEP_LO;
    let (s, c) = TABLE[index & (SIZE - 1)];
    Some((d, f64::from_bits(s), f64::from_bits(c)))
}

pub(crate) fn sin(angle: f64) -> f64 {
    let Some((d, s, c)) = lookup(angle.abs()) else {
        return angle.sin();
    };
    let result = s + d * (c + d * (-s * 0.5 + d * (-c * (1.0 / 6.0) + d * s * (1.0 / 24.0))));
    if angle < 0.0 {
        -result
    } else {
        result
    }
}

pub(crate) fn cos(angle: f64) -> f64 {
    let Some((d, s, c)) = lookup(angle.abs()) else {
        return angle.cos();
    };
    c + d * (-s + d * (-c * 0.5 + d * (s * (1.0 / 6.0) + d * c * (1.0 / 24.0))))
}
