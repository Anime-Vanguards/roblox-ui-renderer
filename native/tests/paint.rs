use rur_native::render::{geometry::Rect, paint::*};
use serde_json::Value;
fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn array<const N: usize>(v: &Value) -> [f64; N] {
    std::array::from_fn(|i| f(&v[i]))
}
fn hash(values: &[[f32; 4]]) -> String {
    let mut n = 0xcbf29ce484222325u64;
    for b in values.iter().flatten().flat_map(|v| v.to_le_bytes()) {
        n = (n ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    format!("{n:016x}")
}
#[test]
fn skia_paint_matches_js() {
    let cases: Value = serde_json::from_str(include_str!("goldens/paint.json")).unwrap();
    for (i, c) in cases.as_array().unwrap().iter().enumerate() {
        if c.get("gradient").is_some() {
            let raw: [f64; 4] = array(&c["raw"]);
            let rect = Rect::new(raw[0], raw[1], raw[2], raw[3]);
            let actual = gradient_stops(&c["gradient"], array(&c["base"]), f(&c["alpha"]));
            let expected: Vec<[f64; 5]> =
                c["stops"].as_array().unwrap().iter().map(array).collect();
            assert_eq!(actual, expected, "stops {i}");
            let mapping =
                gradient_mapping(gradient_line(&c["gradient"], rect), array(&c["toLocal"]));
            let expected: [f64; 3] = array(&c["mapping"]);
            for k in 0..3 {
                assert!((mapping[k] - expected[k]).abs() < 1e-12, "mapping {i}/{k}");
            }
        }
        let points: [f64; 4] = array(&c["points"]);
        let stops: Vec<[f64; 5]> = c["stops"].as_array().unwrap().iter().map(array).collect();
        let skia = skia_linear_gradient(points[0], points[1], points[2], points[3], &stops);
        if !c["solid"].is_null() {
            let SkiaGradient::Solid(actual) = skia else {
                panic!("solid {i}")
            };
            assert_eq!(actual.map(f64::from), array::<4>(&c["solid"]));
        } else {
            let mut texels = Vec::new();
            push_gradient_block(
                &mut texels,
                &skia,
                array(&c["toLocal"]),
                c["varying"].as_bool().unwrap(),
            );
            assert_eq!(
                hash(&texels),
                c["hash"].as_str().unwrap(),
                "gradient record {i}"
            );
        }
        if c.get("gradient").is_some() {
            let raw: [f64; 4] = array(&c["raw"]);
            let rect = Rect::new(raw[0], raw[1], raw[2], raw[3]);
            let paint = Paint::gradient(
                &c["gradient"],
                rect,
                array(&c["base"]),
                f(&c["alpha"]),
                array(&c["toLocal"]),
                [3.0, -7.0],
                c["varying"].as_bool().unwrap(),
            );
            let mut record = vec![[0.0; 4]; 5];
            let pending = paint.write(&mut record);
            record.push([17.0, 0.0, 0.0, 0.0]);
            paint.finish(&mut record, 0, pending);
            assert_eq!(record[5][1], 8.0, "paint offset {i}");
            assert_eq!(
                record[2][3],
                if c["varying"].as_bool().unwrap() {
                    8.0
                } else {
                    0.0
                },
                "varying offset {i}"
            );
            assert_eq!(
                hash(&record[8..]),
                c["hash"].as_str().unwrap(),
                "whole paint {i}"
            );
        }
    }
}
