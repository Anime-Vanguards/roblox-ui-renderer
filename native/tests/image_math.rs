use rur_native::{
    assets::{AssetId, TextureAsset},
    render::{images::*, sampling::*},
};
use serde_json::Value;
fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn u(v: &Value) -> usize {
    v.as_u64().unwrap() as usize
}
fn array<const N: usize>(v: &Value) -> [f64; N] {
    std::array::from_fn(|i| f(&v[i]))
}
fn hash(bytes: &[u8]) -> String {
    let mut n = 0xcbf29ce484222325u64;
    for b in bytes {
        n = (n ^ u64::from(*b)).wrapping_mul(0x100000001b3);
    }
    format!("{n:016x}")
}
fn float_hash(values: &[f32]) -> String {
    hash(
        &values
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
    )
}
fn check_layer(actual: Option<ImageLayer>, expected: &Value, index: usize, mode: &str) {
    if expected.is_null() {
        assert!(actual.is_none(), "{mode} case {index}: unexpected layer");
        return;
    }
    let actual = actual.unwrap_or_else(|| panic!("{mode} case {index}: absent layer"));
    assert_eq!(
        [
            actual.x,
            actual.y,
            actual.width as i32,
            actual.height as i32
        ],
        [
            f(&expected["x"]) as i32,
            f(&expected["y"]) as i32,
            u(&expected["width"]) as i32,
            u(&expected["height"]) as i32
        ],
        "{mode} geometry {index}"
    );
    assert_eq!(
        hash(&actual.rgba),
        expected["hash"].as_str().unwrap(),
        "{mode} pixels {index}"
    );
}
#[test]
fn canonical_js_image_goldens() {
    let gold: Value = serde_json::from_str(include_str!("goldens/image-math.json")).unwrap();
    let mut cache = ImageCache::default();
    for (i, c) in gold["cases"].as_array().unwrap().iter().enumerate() {
        let source = TextureAsset {
            id: AssetId(format!("golden:{i}")),
            width: u(&c["width"]) as u32,
            height: u(&c["height"]) as u32,
            rgba: c["data"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| u(v) as u8)
                .collect::<Vec<_>>()
                .into(),
        };
        let levels = texture_levels(
            source.width as usize,
            source.height as usize,
            source.rgba.clone(),
        );
        for (j, l) in levels.iter().enumerate() {
            assert_eq!(
                hash(&l.data),
                c["mips"][j]["hash"].as_str().unwrap(),
                "mip case {i}/{j}"
            );
        }
        let quads = image_quads(
            &c["image"],
            [source.width, source.height],
            array(&c["raw"]),
            f(&c["scale"]),
            f(&c["node"]["scale"]),
        );
        if let Some(hashes) = c["vertexHashes"].as_array() {
            for (j, q) in quads.iter().enumerate() {
                if hashes[j].is_null() {
                    continue;
                }
                let vertices = rur_native::render::gradients::gradient_vertex_field(
                    &c["node"]["gradient"],
                    array(&c["raw"]),
                    [q.x0, q.y0, q.x1, q.y1].map(|v| snap_device(v, f(&c["scale"]))),
                    array::<3>(&c["image"]["color"]).map(|v| quantize(v) as f64),
                    alpha8(1. - f(&c["image"]["transparency"])) as f64,
                );
                assert_eq!(
                    float_hash(&vertices),
                    hashes[j].as_str().unwrap(),
                    "vertex field {i}/{j}"
                );
            }
        }
        let actual = serde_json::to_value(quads).unwrap();
        assert_eq!(
            actual.as_array().unwrap().len(),
            c["quads"].as_array().unwrap().len(),
            "quad count {i}"
        );
        for (j, q) in actual.as_array().unwrap().iter().enumerate() {
            for field in ["x0", "y0", "x1", "y1", "u0", "v0", "u1", "v1"] {
                assert_eq!(
                    f(&q[field]),
                    f(&c["quads"][j][field]),
                    "quad case {i}/{j}/{field}"
                );
            }
            assert_eq!(q["wrap"], c["quads"][j]["wrap"]);
        }
        let q = &c["quad"];
        let samples = sample_quad(
            &levels,
            9,
            7,
            SampleQuad {
                u0: f(&q["u0"]),
                v0: f(&q["v0"]),
                u1: f(&q["u1"]),
                v1: f(&q["v1"]),
                lod: f(&q["lod"]),
                wrap: q["wrap"].as_bool().unwrap(),
                pixelated: q["pixelated"].as_bool().unwrap(),
            },
            SamplingGrid::pixels(9, 7),
            &mut cache.taps,
        );
        assert_eq!(
            float_hash(&samples),
            c["sampleHash"].as_str().unwrap(),
            "sample case {i}"
        );
        let r = &c["rect"];
        let rect = ImageRect {
            x: f(&r["x"]),
            y: f(&r["y"]),
            w: f(&r["w"]),
            h: f(&r["h"]),
            raw: array(&r["raw"]),
            snapped: r["snapped"].as_bool().unwrap(),
        };
        let node = &c["node"];
        let radius = array(&c["radius"]);
        let scale = f(&c["scale"]);
        check_layer(
            prepare_image(
                node,
                &source,
                rect,
                radius,
                array(&c["matrix"]),
                scale,
                &mut cache,
            ),
            &c["drawn"],
            i,
            "draw",
        );
        let s = &c["space"];
        let space = ImageSpace {
            l: f(&s["L"]),
            t: f(&s["T"]),
            sx: f(&s["sx"]),
            sy: f(&s["sy"]),
            nw: u(&s["nw"]) as u32,
            nh: u(&s["nh"]) as u32,
        };
        // The group reference derives its physical rect from the logical node rect.
        let p = rur_native::render::geometry::physical_rect(node, scale);
        let group_rect = ImageRect {
            x: p.rect.x,
            y: p.rect.y,
            w: p.rect.w,
            h: p.rect.h,
            raw: [p.raw.x, p.raw.y, p.raw.w, p.raw.h],
            snapped: p.snapped,
        };
        check_layer(
            prepare_texture_image(node, &source, group_rect, radius, space, scale, &mut cache),
            &c["group"],
            i,
            "group",
        );
    }
}

#[test]
fn canonical_js_packed_tap_goldens() {
    let gold: Value = serde_json::from_str(include_str!("goldens/image-math.json")).unwrap();
    let mut cache = ImageCache::default();
    for (i, t) in gold["taps"].as_array().unwrap().iter().enumerate() {
        let block = cache.taps.get(
            t["pixelated"].as_bool().unwrap(),
            u(&t["count"]),
            f(&t["t0"]),
            f(&t["t1"]),
            u(&t["size"]),
            t["wrap"].as_bool().unwrap(),
            f(&t["off"]),
            f(&t["span"]),
        );
        let flat: Vec<f32> = block.iter().flatten().copied().collect();
        assert_eq!(float_hash(&flat), t["hash"].as_str().unwrap(), "taps {i}");
    }
}

#[test]
fn translated_plain_images_reuse_pixels_and_update_placement() {
    use std::sync::Arc;
    let source = TextureAsset {
        id: AssetId("translation".into()),
        width: 7,
        height: 5,
        rgba: (0..7 * 5 * 4)
            .map(|i| (i * 43) as u8)
            .collect::<Vec<_>>()
            .into(),
    };
    let identity = [1., 0., 0., 1., 0., 0.];
    let rect = ImageRect {
        x: 3.,
        y: 7.,
        w: 17.,
        h: 13.,
        raw: [3., 7., 17., 13.],
        snapped: true,
    };
    let translated = ImageRect {
        x: 103.,
        y: -43.,
        raw: [103., -43., 17., 13.],
        ..rect
    };
    for scale_type in ["Stretch", "Fit", "Crop", "Tile", "Slice"] {
        let node = serde_json::json!({"scale": 1, "image": {
            "scaleType": scale_type, "color": [0.7, 0.3, 1], "transparency": 0.2,
            "sliceCenter": [1, 1, 5, 3], "tileSize": [0, 4, 0, 3]
        }});
        let mut cache = ImageCache::default();
        let first = prepare_image(&node, &source, rect, [2.; 4], identity, 1., &mut cache).unwrap();
        let moved = prepare_image(
            &node, &source, translated, [2.; 4], identity, 1., &mut cache,
        )
        .unwrap();
        let fresh = prepare_image(
            &node,
            &source,
            translated,
            [2.; 4],
            identity,
            1.,
            &mut ImageCache::default(),
        )
        .unwrap();
        assert!(
            Arc::ptr_eq(&first.rgba, &moved.rgba),
            "{scale_type}: repeated sampling"
        );
        assert_eq!((moved.x, moved.y), (103, -43));
        assert_eq!(moved.rgba, fresh.rgba, "{scale_type}: translated pixels");
    }
    // Float32 far edges change the local coverage at large coordinates. That translation
    // must select a new raster even though the requested width and source are unchanged.
    let node = serde_json::json!({"image": {"scaleType": "Stretch"}});
    let mut cache = ImageCache::default();
    let first = prepare_image(&node, &source, rect, [0.; 4], identity, 1., &mut cache).unwrap();
    let far = ImageRect {
        x: 16_777_216.,
        raw: [16_777_216., 7., 17., 13.],
        ..rect
    };
    let moved = prepare_image(&node, &source, far, [0.; 4], identity, 1., &mut cache).unwrap();
    let fresh = prepare_image(
        &node,
        &source,
        far,
        [0.; 4],
        identity,
        1.,
        &mut ImageCache::default(),
    )
    .unwrap();
    assert!(!Arc::ptr_eq(&first.rgba, &moved.rgba));
    assert_eq!(moved.rgba, fresh.rgba);
}
