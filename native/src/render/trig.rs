//! Deterministic sin/cos matching the calibrated V8 12.4 renderer.
//! Port of https://github.com/v8/v8/blob/12.4.254/src/base/ieee754.cc.
//
// ====================================================
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
// Developed at SunSoft, a Sun Microsystems, Inc. business.
// Permission to use, copy, modify, and distribute this software is freely
// granted, provided that this notice is preserved.
// ====================================================
// The original source code covered by the above license has been modified
// significantly by Google Inc. Copyright 2016 the V8 project authors.
// All rights reserved.
//
// Retain the evaluation order: a one-ULP change can cross a pixel-byte boundary.
#![allow(clippy::excessive_precision)]

fn high(x: f64) -> u32 {
    (x.to_bits() >> 32) as u32
}
fn with_high(x: f64, h: u32) -> f64 {
    f64::from_bits((u64::from(h) << 32) | (x.to_bits() & 0xffff_ffff))
}

pub fn sin(x: f64) -> f64 {
    let ix = high(x) & 0x7fff_ffff;
    if ix <= 0x3fe921fb {
        return kernel_sin(x, 0.0, false);
    }
    if ix >= 0x7ff00000 {
        return x - x;
    }
    let (n, y) = rem_pio2(x);
    match n & 3 {
        0 => kernel_sin(y[0], y[1], true),
        1 => kernel_cos(y[0], y[1]),
        2 => -kernel_sin(y[0], y[1], true),
        _ => -kernel_cos(y[0], y[1]),
    }
}

pub fn cos(x: f64) -> f64 {
    let ix = high(x) & 0x7fff_ffff;
    if ix <= 0x3fe921fb {
        return kernel_cos(x, 0.0);
    }
    if ix >= 0x7ff00000 {
        return x - x;
    }
    let (n, y) = rem_pio2(x);
    match n & 3 {
        0 => kernel_cos(y[0], y[1]),
        1 => -kernel_sin(y[0], y[1], true),
        2 => -kernel_cos(y[0], y[1]),
        _ => kernel_sin(y[0], y[1], true),
    }
}

fn kernel_sin(x: f64, y: f64, tail: bool) -> f64 {
    const S1: f64 = -1.66666666666666324348e-01;
    const S2: f64 = 8.33333333332248946124e-03;
    const S3: f64 = -1.98412698298579493134e-04;
    const S4: f64 = 2.75573137070700676789e-06;
    const S5: f64 = -2.50507602534068634195e-08;
    const S6: f64 = 1.58969099521155010221e-10;
    if high(x) & 0x7fff_ffff < 0x3e400000 {
        return x;
    }
    let z = x * x;
    let v = z * x;
    let r = S2 + z * (S3 + z * (S4 + z * (S5 + z * S6)));
    if tail {
        x - ((z * (0.5 * y - v * r) - y) - v * S1)
    } else {
        x + v * (S1 + z * r)
    }
}

fn kernel_cos(x: f64, y: f64) -> f64 {
    const C1: f64 = 4.16666666666666019037e-02;
    const C2: f64 = -1.38888888888741095749e-03;
    const C3: f64 = 2.48015872894767294178e-05;
    const C4: f64 = -2.75573143513906633035e-07;
    const C5: f64 = 2.08757232129817482790e-09;
    const C6: f64 = -1.13596475577881948265e-11;
    let ix = high(x) & 0x7fff_ffff;
    if ix < 0x3e400000 {
        return 1.0;
    }
    let z = x * x;
    let r = z * (C1 + z * (C2 + z * (C3 + z * (C4 + z * (C5 + z * C6)))));
    if ix < 0x3fd33333 {
        1.0 - (0.5 * z - (z * r - x * y))
    } else {
        let qx = if ix > 0x3fe90000 {
            0.28125
        } else {
            f64::from_bits(u64::from(ix - 0x00200000) << 32)
        };
        let iz = 0.5 * z - qx;
        let a = 1.0 - qx;
        a - (iz - (z * r - x * y))
    }
}

const TWO_OVER_PI: [i32; 66] = [
    0xA2F983, 0x6E4E44, 0x1529FC, 0x2757D1, 0xF534DD, 0xC0DB62, 0x95993C, 0x439041, 0xFE5163,
    0xABDEBB, 0xC561B7, 0x246E3A, 0x424DD2, 0xE00649, 0x2EEA09, 0xD1921C, 0xFE1DEB, 0x1CB129,
    0xA73EE8, 0x8235F5, 0x2EBB44, 0x84E99C, 0x7026B4, 0x5F7E41, 0x3991D6, 0x398353, 0x39F49C,
    0x845F8B, 0xBDF928, 0x3B1FF8, 0x97FFDE, 0x05980F, 0xEF2F11, 0x8B5A0A, 0x6D1F6D, 0x367ECF,
    0x27CB09, 0xB74F46, 0x3F669E, 0x5FEA2D, 0x7527BA, 0xC7EBE5, 0xF17B3D, 0x0739F7, 0x8A5292,
    0xEA6BFB, 0x5FB11F, 0x8D5D08, 0x560330, 0x46FC7B, 0x6BABF0, 0xCFBC20, 0x9AF436, 0x1DA9E3,
    0x91615E, 0xE61B08, 0x659985, 0x5F14A0, 0x68408D, 0xFFD880, 0x4D7327, 0x310606, 0x1556CA,
    0x73A8C9, 0x60E27B, 0xC08C6B,
];

fn rem_pio2(x: f64) -> (i32, [f64; 2]) {
    const NPIO2_HW: [u32; 32] = [
        0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C, 0x4025FDBB,
        0x402921FB, 0x402C463A, 0x402F6A7A, 0x4031475C, 0x4032D97C, 0x40346B9C, 0x4035FDBB,
        0x40378FDB, 0x403921FB, 0x403AB41B, 0x403C463A, 0x403DD85A, 0x403F6A7A, 0x40407E4C,
        0x4041475C, 0x4042106C, 0x4042D97C, 0x4043A28C, 0x40446B9C, 0x404534AC, 0x4045FDBB,
        0x4046C6CB, 0x40478FDB, 0x404858EB, 0x404921FB,
    ];
    const INVPIO2: f64 = 6.36619772367581382433e-01;
    const P1: f64 = 1.57079632673412561417e+00;
    const P1T: f64 = 6.07710050650619224932e-11;
    const P2: f64 = 6.07710050630396597660e-11;
    const P2T: f64 = 2.02226624879595063154e-21;
    const P3: f64 = 2.02226624871116645580e-21;
    const P3T: f64 = 8.47842766036889956997e-32;
    let hx = high(x);
    let ix = hx & 0x7fff_ffff;
    if ix <= 0x3fe921fb {
        return (0, [x, 0.0]);
    }
    if ix < 0x4002d97c {
        let positive = hx & 0x80000000 == 0;
        let sign = if positive { 1.0 } else { -1.0 };
        let mut z = x - sign * P1;
        let tail = if ix != 0x3ff921fb {
            P1T
        } else {
            z -= sign * P2;
            P2T
        };
        let y0 = z - sign * tail;
        return (if positive { 1 } else { -1 }, [y0, (z - y0) - sign * tail]);
    }
    if ix <= 0x413921fb {
        let mut t = x.abs();
        let n = (t * INVPIO2 + 0.5) as i32;
        let f = n as f64;
        let mut r = t - f * P1;
        let mut w = f * P1T;
        let mut y0 = r - w;
        if !(n < 32 && ix != NPIO2_HW[n as usize - 1]) {
            let j = (ix >> 20) as i32;
            if j - ((high(y0) >> 20) & 0x7ff) as i32 > 16 {
                t = r;
                w = f * P2;
                r = t - w;
                w = f * P2T - ((t - r) - w);
                y0 = r - w;
                if j - ((high(y0) >> 20) & 0x7ff) as i32 > 49 {
                    t = r;
                    w = f * P3;
                    r = t - w;
                    w = f * P3T - ((t - r) - w);
                    y0 = r - w;
                }
            }
        }
        let y1 = (r - y0) - w;
        return if hx & 0x80000000 != 0 {
            (-n, [-y0, -y1])
        } else {
            (n, [y0, y1])
        };
    }
    let e0 = (ix >> 20) as i32 - 1046;
    let mut z = with_high(x, ix.wrapping_sub((e0 as u32).wrapping_shl(20)));
    let mut tx = [0.0; 3];
    for item in tx.iter_mut().take(2) {
        *item = z as i32 as f64;
        z = (z - *item) * 16777216.0;
    }
    tx[2] = z;
    let mut nx = 3;
    while tx[nx - 1] == 0.0 {
        nx -= 1;
    }
    let (n, y) = kernel_rem_pio2(&tx[..nx], e0);
    if hx & 0x80000000 != 0 {
        (-n, [-y[0], -y[1]])
    } else {
        (n, y)
    }
}

fn kernel_rem_pio2(x: &[f64], e0: i32) -> (i32, [f64; 2]) {
    const PIO2: [f64; 8] = [
        1.57079625129699707031e+00,
        7.54978941586159635335e-08,
        5.39030252995776476554e-15,
        3.28200341580791294123e-22,
        1.27065575308067607349e-29,
        1.22933308981111328932e-36,
        2.73370053816464559624e-44,
        2.16741683877804819444e-51,
    ];
    const TWO24: f64 = 16777216.0;
    const TWON24: f64 = 5.96046447753906250000e-08;
    // All powers used by this reduction are normal and exactly representable.
    let pow2 = |n: i32| f64::from_bits(((1023 + n) as u64) << 52);
    let jk = 4;
    let jx = x.len() - 1;
    let jv = ((e0 - 3) / 24).max(0) as usize;
    let mut q0 = e0 - 24 * (jv as i32 + 1);
    let mut f = [0.0; 20];
    let mut q = [0.0; 20];
    let mut fq = [0.0; 20];
    let mut iq = [0i32; 20];
    for (i, item) in f.iter_mut().enumerate().take(jx + jk + 1) {
        let j = jv as i32 - jx as i32 + i as i32;
        *item = if j < 0 {
            0.0
        } else {
            TWO_OVER_PI[j as usize] as f64
        };
    }
    for i in 0..=jk {
        for j in 0..=jx {
            q[i] += x[j] * f[jx + i - j];
        }
    }
    let mut jz = jk;
    let (mut z, n, ih) = loop {
        let mut z = q[jz];
        for i in 0..jz {
            let fw = (TWON24 * z) as i32 as f64;
            iq[i] = (z - TWO24 * fw) as i32;
            z = q[jz - i - 1] + fw;
        }
        z *= pow2(q0);
        z -= 8.0 * (z * 0.125).floor();
        let mut n = z as i32;
        z -= n as f64;
        let mut ih = 0;
        if q0 > 0 {
            let i = iq[jz - 1] >> (24 - q0);
            n += i;
            iq[jz - 1] -= i << (24 - q0);
            ih = iq[jz - 1] >> (23 - q0);
        } else if q0 == 0 {
            ih = iq[jz - 1] >> 23;
        } else if z >= 0.5 {
            ih = 2;
        }
        if ih > 0 {
            n += 1;
            let mut carry = false;
            for item in iq.iter_mut().take(jz) {
                let j = *item;
                if !carry {
                    if j != 0 {
                        carry = true;
                        *item = 0x1000000 - j;
                    }
                } else {
                    *item = 0xffffff - j;
                }
            }
            if q0 == 1 {
                iq[jz - 1] &= 0x7fffff;
            } else if q0 == 2 {
                iq[jz - 1] &= 0x3fffff;
            }
            if ih == 2 {
                z = 1.0 - z;
                if carry {
                    z -= pow2(q0);
                }
            }
        }
        if z == 0.0 && iq[jk..jz].iter().all(|&i| i == 0) {
            let mut k = 1;
            while jk >= k && iq[jk - k] == 0 {
                k += 1;
            }
            for i in jz + 1..=jz + k {
                f[jx + i] = TWO_OVER_PI[jv + i] as f64;
                let mut fw = 0.0;
                for j in 0..=jx {
                    fw += x[j] * f[jx + i - j];
                }
                q[i] = fw;
            }
            jz += k;
            continue;
        }
        break (z, n, ih);
    };
    if z == 0.0 {
        jz -= 1;
        q0 -= 24;
        while iq[jz] == 0 {
            jz -= 1;
            q0 -= 24;
        }
    } else {
        z *= pow2(-q0);
        if z >= TWO24 {
            let fw = (TWON24 * z) as i32 as f64;
            iq[jz] = (z - TWO24 * fw) as i32;
            jz += 1;
            q0 += 24;
            iq[jz] = fw as i32;
        } else {
            iq[jz] = z as i32;
        }
    }
    let mut fw = pow2(q0);
    for i in (0..=jz).rev() {
        q[i] = fw * iq[i] as f64;
        fw *= TWON24;
    }
    for i in (0..=jz).rev() {
        let mut fw = 0.0;
        for k in 0..=jk.min(jz - i) {
            fw += PIO2[k] * q[i + k];
        }
        fq[jz - i] = fw;
    }
    let mut fw = 0.0;
    for i in (0..=jz).rev() {
        fw += fq[i];
    }
    let y0 = if ih == 0 { fw } else { -fw };
    fw = fq[0] - fw;
    for item in fq.iter().take(jz + 1).skip(1) {
        fw += item;
    }
    (n & 7, [y0, if ih == 0 { fw } else { -fw }])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_canonical_v8_bits() {
        for line in include_str!("../../tests/goldens/trig-v8.txt").lines() {
            let values: Vec<u64> = line
                .split_whitespace()
                .map(|s| u64::from_str_radix(s, 16).unwrap())
                .collect();
            let x = f64::from_bits(values[0]);
            for (actual, expected) in [(sin(x), values[1]), (cos(x), values[2])] {
                if f64::from_bits(expected).is_nan() {
                    assert!(actual.is_nan());
                } else {
                    assert_eq!(
                        actual.to_bits(),
                        expected,
                        "input {x:?} / {:016x}",
                        values[0]
                    );
                }
            }
        }
    }
}
