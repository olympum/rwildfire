// SPDX-License-Identifier: GPL-3.0-or-later
// Adapted from JWildfire 9.20 FastMathLibImpl (LGPL-2.1-or-later).
const SIZE: usize = 2048;
const STEP_HI: f64 = 4.0 * f64::from_bits(4609753056924401664) / SIZE as f64;
const STEP_LO: f64 = 4.0 * f64::from_bits(4454258360616903473) / SIZE as f64;
const INDEXER: f64 = 1.0 / (STEP_HI + STEP_LO);
// Exact StrictMath bits from oracle/StrictTrigTable.java; no runtime JDK needed.
static TABLE: [(u64, u64); SIZE] = include!("strict_trig_table.in");

pub(crate) fn sin(angle: f64) -> f64 {
    let abs = angle.abs();
    if abs > 4194303.0 / INDEXER * 0.99 || !abs.is_finite() {
        // Upstream's large-angle reduction is outside the verified range.
        return angle.sin();
    }
    let index = (abs * INDEXER + 0.5) as usize;
    let i = index as f64;
    let d = abs - i * STEP_HI - i * STEP_LO;
    let (s, c) = TABLE[index & (SIZE - 1)];
    let (s, c) = (f64::from_bits(s), f64::from_bits(c));
    let result = s + d * (c + d * (-s * 0.5 + d * (-c * (1.0 / 6.0) + d * s * (1.0 / 24.0))));
    if angle < 0.0 {
        -result
    } else {
        result
    }
}
