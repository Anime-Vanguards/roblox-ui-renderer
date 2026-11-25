//! Scripted pointer compositing for `rur sequence --cursor`.
//!
//! Draws the platform's default cursor at each frame's logical pointer position scaled by the
//! viewport's device scale, with hover, text, drag, gamepad and click states. On machines with a
//! local Roblox Studio install, cursor art is loaded from its `content/textures/` folder via
//! [`Resolver`] (no Roblox textures are stored in this repository); without a Studio install
//! (for example CI runners), equivalent anti-aliased procedural cursors are drawn at the same
//! logical size and hotspot.

use std::collections::HashMap;

use crate::{
    engine::input::{PointerKind, PointerState},
    resolve::Resolver,
};

struct Sprite {
    width: u32,
    height: u32,
    /// Straight-alpha RGBA8 pixels.
    rgba: Vec<u8>,
    /// Logical dimensions at device scale 1.0.
    logical_w: f64,
    logical_h: f64,
    /// Hotspot in logical pixels from the sprite's top-left corner.
    hotspot_x: f64,
    hotspot_y: f64,
}

/// Decoded cursor sprites cached across the frames of one sequence.
#[derive(Default)]
pub struct CursorRenderer {
    /// Cached Studio or custom images by URI (`None` = unavailable, fall back to procedural).
    loaded: HashMap<String, Option<(u32, u32, Vec<u8>)>>,
}

impl CursorRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Composites `pointer` onto a cropped straight-alpha RGBA8 frame (`crop = [x, y, w, h]` in
    /// physical pixels) at viewport `scale`. No-op when `pointer.visible` is false.
    pub fn draw(
        &mut self,
        resolver: &mut Resolver,
        rgba: &mut [u8],
        crop: [u32; 4],
        scale: f64,
        pointer: &PointerState,
    ) {
        if !pointer.visible || !(scale.is_finite() && scale > 0.0) {
            return;
        }
        let px = pointer.x * scale - f64::from(crop[0]);
        let py = pointer.y * scale - f64::from(crop[1]);
        let (frame_w, frame_h) = (crop[2], crop[3]);
        if frame_w == 0 || frame_h == 0 {
            return;
        }
        // Touch devices have no Studio cursor texture; draw a crisp translucent contact disc.
        if pointer.kind == PointerKind::Touch {
            draw_touch_contact(rgba, frame_w, frame_h, px, py, scale);
            return;
        }
        // Custom UserInputService.MouseIcon (centered, 1x logical pixels scaled by device scale).
        if let Some(uri) = &pointer.custom_icon {
            if let Some((w, h, data)) = self.load_uri(resolver, uri) {
                let sprite = Sprite {
                    width: w,
                    height: h,
                    rgba: data,
                    logical_w: f64::from(w),
                    logical_h: f64::from(h),
                    hotspot_x: f64::from(w) * 0.5,
                    hotspot_y: f64::from(h) * 0.5,
                };
                let (press_scale, press_tint) = if pointer.pressed { (0.88, 0.86) } else { (1.0, 1.0) };
                blit_sprite(rgba, frame_w, frame_h, &sprite, px, py, scale * press_scale, press_tint);
                return;
            }
        }
        if let Some(sprite) = self.studio_sprite(resolver, pointer.kind, pointer.pressed) {
            let (press_scale, press_tint) = match pointer.kind {
                PointerKind::DragActive | PointerKind::Gamepad | PointerKind::GamepadHover => (1.0, 1.0),
                _ if pointer.pressed => (0.88, 0.86),
                _ => (1.0, 1.0),
            };
            blit_sprite(rgba, frame_w, frame_h, &sprite, px, py, scale * press_scale, press_tint);
            return;
        }
        draw_procedural(rgba, frame_w, frame_h, px, py, scale, pointer.kind, pointer.pressed);
    }

    fn load_uri(&mut self, resolver: &mut Resolver, uri: &str) -> Option<(u32, u32, Vec<u8>)> {
        if let Some(cached) = self.loaded.get(uri) {
            return cached.clone();
        }
        let decoded = resolver
            .resolve_all(&[uri.to_string()])
            .remove(uri)
            .and_then(|res| res.ok())
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| image::load_from_memory(&bytes).ok())
            .map(|img| {
                let rgba = img.to_rgba8();
                (rgba.width(), rgba.height(), rgba.into_raw())
            });
        self.loaded.insert(uri.to_string(), decoded.clone());
        decoded
    }

    fn studio_sprite(&mut self, resolver: &mut Resolver, kind: PointerKind, pressed: bool) -> Option<Sprite> {
        // Measured against Studio 0.747.0.7471481 content/textures:
        //   KeyboardMouse/ArrowFarCursor.png (64x64, white arrow, hotspot (32, 32))
        //   KeyboardMouse/ArrowCursor.png    (64x64, pointing hand, hotspot (32, 32))
        //   KeyboardMouse/IBeamCursor.png    (64x64, I-beam, hotspot (32, 32))
        //   DragDetector/HoverCursor.png     (23x25, open hand, centered)
        //   DragDetector/ActivatedCursor.png (23x25, closed hand, centered)
        //   ui/VirtualCursor/cursor*.png     (80x80 / 68x68, @2x 160x160 / 136x136, centered)
        let (candidates, logical_w, logical_h, hotspot_x, hotspot_y): (&[&str], f64, f64, f64, f64) = match kind {
            PointerKind::Default => (
                &["rbxasset://textures/Cursors/KeyboardMouse/ArrowFarCursor.png", "rbxasset://textures/ArrowFarCursor.png"],
                64.0,
                64.0,
                32.0,
                32.0,
            ),
            PointerKind::Hover => (
                &["rbxasset://textures/Cursors/KeyboardMouse/ArrowCursor.png", "rbxasset://textures/ArrowCursor.png"],
                64.0,
                64.0,
                32.0,
                32.0,
            ),
            PointerKind::Text => (
                &["rbxasset://textures/Cursors/KeyboardMouse/IBeamCursor.png", "rbxasset://textures/IBeamCursor.png"],
                64.0,
                64.0,
                32.0,
                32.0,
            ),
            PointerKind::DragHover => (&["rbxasset://textures/Cursors/DragDetector/HoverCursor.png"], 23.0, 25.0, 11.5, 12.5),
            PointerKind::DragActive => (&["rbxasset://textures/Cursors/DragDetector/ActivatedCursor.png"], 23.0, 25.0, 11.5, 12.5),
            PointerKind::Gamepad | PointerKind::GamepadHover if pressed => (
                &["rbxasset://textures/ui/VirtualCursor/cursorPressed.png"],
                34.0,
                34.0,
                17.0,
                17.0,
            ),
            PointerKind::GamepadHover => (
                &["rbxasset://textures/ui/VirtualCursor/cursorHover.png"],
                40.0,
                40.0,
                20.0,
                20.0,
            ),
            PointerKind::Gamepad => (
                &["rbxasset://textures/ui/VirtualCursor/cursorDefault.png"],
                40.0,
                40.0,
                20.0,
                20.0,
            ),
            PointerKind::Touch => return None,
        };
        for &uri in candidates {
            if let Some((width, height, rgba)) = self.load_uri(resolver, uri) {
                return Some(Sprite { width, height, rgba, logical_w, logical_h, hotspot_x, hotspot_y });
            }
        }
        None
    }
}

/// Blends a premultiplied-alpha source pixel (`sr, sg, sb, sa` in `0.0..=1.0`) over a straight-alpha
/// RGBA8 destination pixel.
#[inline]
fn blend_premul(dst: &mut [u8], sr: f64, sg: f64, sb: f64, sa: f64) {
    if sa <= 1e-6 {
        return;
    }
    let da = f64::from(dst[3]) / 255.0;
    let inv = 1.0 - sa;
    let out_a = sa + da * inv;
    if out_a <= 1e-6 {
        return;
    }
    let dr = f64::from(dst[0]) / 255.0 * da;
    let dg = f64::from(dst[1]) / 255.0 * da;
    let db = f64::from(dst[2]) / 255.0 * da;
    let out_r = (sr + dr * inv) / out_a;
    let out_g = (sg + dg * inv) / out_a;
    let out_b = (sb + db * inv) / out_a;
    dst[0] = (out_r.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    dst[1] = (out_g.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    dst[2] = (out_b.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    dst[3] = (out_a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
}

/// Bilinearly samples premultiplied RGBA from `sprite` at continuous source texel coordinate `(sx, sy)`.
fn sample_premul(sprite: &Sprite, sx: f64, sy: f64) -> [f64; 4] {
    let w = sprite.width as i32;
    let h = sprite.height as i32;
    let x0 = sx.floor() as i32;
    let y0 = sy.floor() as i32;
    let fx = sx - f64::from(x0);
    let fy = sy - f64::from(y0);
    let mut acc = [0.0; 4];
    let weights = [
        (x0, y0, (1.0 - fx) * (1.0 - fy)),
        (x0 + 1, y0, fx * (1.0 - fy)),
        (x0, y0 + 1, (1.0 - fx) * fy),
        (x0 + 1, y0 + 1, fx * fy),
    ];
    for (x, y, weight) in weights {
        if x < 0 || y < 0 || x >= w || y >= h || weight <= 0.0 {
            continue;
        }
        let o = ((y as u32 * sprite.width + x as u32) * 4) as usize;
        let a = f64::from(sprite.rgba[o + 3]) / 255.0;
        let wa = weight * a;
        acc[0] += f64::from(sprite.rgba[o]) / 255.0 * wa;
        acc[1] += f64::from(sprite.rgba[o + 1]) / 255.0 * wa;
        acc[2] += f64::from(sprite.rgba[o + 2]) / 255.0 * wa;
        acc[3] += wa;
    }
    acc
}

fn blit_sprite(
    rgba: &mut [u8],
    frame_w: u32,
    frame_h: u32,
    sprite: &Sprite,
    px: f64,
    py: f64,
    scale: f64,
    tint: f64,
) {
    let dst_w = sprite.logical_w * scale;
    let dst_h = sprite.logical_h * scale;
    let left = px - sprite.hotspot_x * scale;
    let top = py - sprite.hotspot_y * scale;
    let x_start = (left.floor() as i32).max(0);
    let y_start = (top.floor() as i32).max(0);
    let x_end = ((left + dst_w).ceil() as i32).min(frame_w as i32);
    let y_end = ((top + dst_h).ceil() as i32).min(frame_h as i32);
    if x_start >= x_end || y_start >= y_end {
        return;
    }
    let inv_w = f64::from(sprite.width) / dst_w;
    let inv_h = f64::from(sprite.height) / dst_h;
    for dy in y_start..y_end {
        let sy = (f64::from(dy) + 0.5 - top) * inv_h - 0.5;
        let row = (dy as u32 * frame_w) as usize;
        for dx in x_start..x_end {
            let sx = (f64::from(dx) + 0.5 - left) * inv_w - 0.5;
            let s = sample_premul(sprite, sx, sy);
            if s[3] > 1e-4 {
                let o = (row + dx as usize) * 4;
                blend_premul(&mut rgba[o..o + 4], s[0] * tint, s[1] * tint, s[2] * tint, s[3]);
            }
        }
    }
}

/// Translucent touch contact disc for `--device touch` / `--device phone` while pressed.
fn draw_touch_contact(rgba: &mut [u8], frame_w: u32, frame_h: u32, px: f64, py: f64, scale: f64) {
    let radius = 16.0 * scale;
    let border = 2.0 * scale;
    let extent = (radius + border + 2.0).ceil() as i32;
    let x0 = (px.floor() as i32 - extent).max(0);
    let y0 = (py.floor() as i32 - extent).max(0);
    let x1 = (px.ceil() as i32 + extent).min(frame_w as i32);
    let y1 = (py.ceil() as i32 + extent).min(frame_h as i32);
    for dy in y0..y1 {
        let row = (dy as u32 * frame_w) as usize;
        for dx in x0..x1 {
            let dist = ((f64::from(dx) + 0.5 - px).powi(2) + (f64::from(dy) + 0.5 - py).powi(2)).sqrt();
            let outer_cov = (radius + border + 0.5 - dist).clamp(0.0, 1.0);
            if outer_cov <= 0.0 {
                continue;
            }
            let inner_cov = (radius + 0.5 - dist).clamp(0.0, 1.0);
            let ring_cov = (outer_cov - inner_cov).max(0.0);
            let fill_a = inner_cov * 0.42;
            let ring_a = ring_cov * 0.65;
            let sa = (fill_a + ring_a).min(1.0);
            let sr = fill_a * 1.0 + ring_a * 0.12;
            let o = (row + dx as usize) * 4;
            blend_premul(&mut rgba[o..o + 4], sr, sr, sr, sa);
        }
    }
}

/// Signed distance to a closed polygon (negative inside, positive outside).
fn sd_polygon(px: f64, py: f64, poly: &[(f64, f64)]) -> f64 {
    let n = poly.len();
    let mut min_d2 = f64::INFINITY;
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        let ex = xj - xi;
        let ey = yj - yi;
        let wx = px - xi;
        let wy = py - yi;
        let denom = ex * ex + ey * ey;
        let t = if denom > 0.0 { ((wx * ex + wy * ey) / denom).clamp(0.0, 1.0) } else { 0.0 };
        let dx = wx - ex * t;
        let dy = wy - ey * t;
        min_d2 = min_d2.min(dx * dx + dy * dy);
        let cond = (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi;
        if cond {
            inside = !inside;
        }
        j = i;
    }
    let d = min_d2.sqrt();
    if inside { -d } else { d }
}

/// Signed distance to an axis-aligned box centered at `(cx, cy)` with half-extents `(hx, hy)`.
fn sd_box(px: f64, py: f64, cx: f64, cy: f64, hx: f64, hy: f64) -> f64 {
    let dx = (px - cx).abs() - hx;
    let dy = (py - cy).abs() - hy;
    dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0)
}

/// Signed distance to a capsule segment `(ax, ay)-(bx, by)` with radius `r`.
fn sd_capsule(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64, r: f64) -> f64 {
    let pax = px - ax;
    let pay = py - ay;
    let bax = bx - ax;
    let bay = by - ay;
    let denom = bax * bax + bay * bay;
    let h = if denom > 0.0 { ((pax * bax + pay * bay) / denom).clamp(0.0, 1.0) } else { 0.0 };
    (pax - bax * h).hypot(pay - bay * h) - r
}

/// Procedural fallback cursors (when no local Studio textures are available, e.g. CI).
#[allow(clippy::too_many_arguments)]
fn draw_procedural(
    rgba: &mut [u8],
    frame_w: u32,
    frame_h: u32,
    px: f64,
    py: f64,
    scale: f64,
    kind: PointerKind,
    pressed: bool,
) {
    let eff_scale = match kind {
        PointerKind::DragActive | PointerKind::Gamepad | PointerKind::GamepadHover => scale,
        _ if pressed => scale * 0.88,
        _ => scale,
    };
    let fill = if pressed && !matches!(kind, PointerKind::Gamepad | PointerKind::GamepadHover) { 0.86 } else { 1.0 };
    let (min_x, min_y, max_x, max_y) = match kind {
        PointerKind::Default => (-3.0, -3.0, 19.0, 27.0),
        PointerKind::Hover => (-10.0, -3.0, 15.0, 28.0),
        PointerKind::Text => (-6.0, -13.0, 6.0, 13.0),
        PointerKind::DragHover | PointerKind::DragActive => (-14.0, -14.0, 14.0, 14.0),
        PointerKind::Gamepad | PointerKind::GamepadHover => (-22.0, -22.0, 22.0, 22.0),
        PointerKind::Touch => return,
    };
    let x0 = ((px + min_x * eff_scale).floor() as i32).max(0);
    let y0 = ((py + min_y * eff_scale).floor() as i32).max(0);
    let x1 = ((px + max_x * eff_scale).ceil() as i32).min(frame_w as i32);
    let y1 = ((py + max_y * eff_scale).ceil() as i32).min(frame_h as i32);
    let aa = 1.0 / eff_scale;
    const ARROW: &[(f64, f64)] = &[
        (0.0, 0.0),
        (0.0, 19.0),
        (4.5, 15.2),
        (7.8, 22.2),
        (10.8, 20.8),
        (7.4, 13.8),
        (13.4, 13.8),
    ];
    for dy in y0..y1 {
        let ly = (f64::from(dy) + 0.5 - py) / eff_scale;
        let row = (dy as u32 * frame_w) as usize;
        for dx in x0..x1 {
            let lx = (f64::from(dx) + 0.5 - px) / eff_scale;
            let (sr, sg, sb, sa) = match kind {
                PointerKind::Default => {
                    let d = sd_polygon(lx, ly, ARROW);
                    outlined_shape(d, 1.35, aa, fill)
                }
                PointerKind::Hover => {
                    // Pointing hand silhouette, index fingertip at (0, 0)
                    let mut d = sd_capsule(lx, ly, 0.0, 1.8, 0.0, 12.0, 1.9);
                    d = d.min(sd_capsule(lx, ly, 3.6, 8.2, 3.6, 13.0, 1.7));
                    d = d.min(sd_capsule(lx, ly, 6.8, 9.2, 6.8, 13.6, 1.6));
                    d = d.min(sd_capsule(lx, ly, 9.6, 10.6, 9.6, 14.5, 1.5));
                    d = d.min(sd_capsule(lx, ly, -4.4, 11.4, -1.2, 16.0, 1.8));
                    d = d.min(sd_box(lx, ly, 3.6, 16.2, 6.2, 4.6));
                    outlined_shape(d, 1.35, aa, fill)
                }
                PointerKind::Text => {
                    let stem = sd_box(lx, ly, 0.0, 0.0, 0.85, 9.5);
                    let top = sd_box(lx, ly, 0.0, -9.5, 3.2, 0.85);
                    let bot = sd_box(lx, ly, 0.0, 9.5, 3.2, 0.85);
                    let d = stem.min(top).min(bot);
                    outlined_shape(d, 1.1, aa, fill)
                }
                PointerKind::DragHover => {
                    let mut d = sd_box(lx, ly, 0.0, 3.0, 5.5, 5.0);
                    for (fx, fy) in [(-4.2, -5.5), (-1.4, -7.2), (1.6, -6.8), (4.4, -4.8)] {
                        d = d.min(sd_capsule(lx, ly, fx, fy, fx * 0.85, 1.5, 1.45));
                    }
                    d = d.min(sd_capsule(lx, ly, -7.2, 1.0, -4.2, 5.0, 1.55));
                    outlined_shape(d, 1.25, aa, fill)
                }
                PointerKind::DragActive => {
                    let mut d = sd_box(lx, ly, 0.0, 2.0, 6.0, 5.2);
                    for fx in [-4.0, -1.3, 1.4, 4.0] {
                        d = d.min(sd_capsule(lx, ly, fx, -3.5, fx, 0.5, 1.55));
                    }
                    outlined_shape(d, 1.25, aa, fill)
                }
                PointerKind::Gamepad | PointerKind::GamepadHover => {
                    let r = if pressed { 14.5 } else { 17.5 };
                    let dist = lx.hypot(ly);
                    let ring_d = (dist - r).abs() - 2.2;
                    let fill_a = if pressed {
                        ((r - 3.5 + aa - dist) / (2.0 * aa)).clamp(0.0, 1.0) * 0.45
                    } else if kind == PointerKind::GamepadHover {
                        ((r - 3.5 + aa - dist) / (2.0 * aa)).clamp(0.0, 1.0) * 0.20
                    } else {
                        0.0
                    };
                    let ring = outlined_shape(ring_d, 1.0, aa, 1.0);
                    let sa = (ring.3 + fill_a * (1.0 - ring.3)).min(1.0);
                    let c = ring.0 + fill_a * 0.55 * (1.0 - ring.3);
                    (c, c, c, sa)
                }
                PointerKind::Touch => (0.0, 0.0, 0.0, 0.0),
            };
            if sa > 1e-4 {
                let o = (row + dx as usize) * 4;
                blend_premul(&mut rgba[o..o + 4], sr, sg, sb, sa);
            }
        }
    }
}

/// Premultiplied white-fill + dark-outline coverage from a signed distance `d` (in logical px).
#[inline]
fn outlined_shape(d: f64, outline: f64, aa: f64, fill_brightness: f64) -> (f64, f64, f64, f64) {
    let outer = ((outline + aa - d) / (2.0 * aa)).clamp(0.0, 1.0);
    if outer <= 0.0 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let inner = ((aa - d) / (2.0 * aa)).clamp(0.0, 1.0);
    let border = (outer - inner).max(0.0);
    let rgb = inner * fill_brightness + border * 0.08;
    (rgb, rgb, rgb, outer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff_count(a: &[u8], b: &[u8]) -> usize {
        a.chunks_exact(4).zip(b.chunks_exact(4)).filter(|(x, y)| x != y).count()
    }

    #[test]
    fn procedural_cursors_render_distinct_states_and_scale() {
        let mut renderer = CursorRenderer::new();
        // Force procedural path by giving an empty resolver and no Studio content roots
        let mut resolver = Resolver::new(std::env::temp_dir(), true);
        let bg = vec![30u8; 200 * 200 * 4];
        let render = |r: &mut CursorRenderer, res: &mut Resolver, scale: f64, kind: PointerKind, pressed: bool| {
            let mut buf = bg.clone();
            let p = PointerState { visible: true, x: 50.0, y: 50.0, kind, pressed, custom_icon: None };
            r.draw(res, &mut buf, [0, 0, 200, 200], scale, &p);
            buf
        };
        let idle_1x = render(&mut renderer, &mut resolver, 1.0, PointerKind::Default, false);
        let idle_2x = render(&mut renderer, &mut resolver, 2.0, PointerKind::Default, false);
        let hover_1x = render(&mut renderer, &mut resolver, 1.0, PointerKind::Hover, false);
        let press_1x = render(&mut renderer, &mut resolver, 1.0, PointerKind::Hover, true);
        let text_1x = render(&mut renderer, &mut resolver, 1.0, PointerKind::Text, false);
        let touch_1x = render(&mut renderer, &mut resolver, 1.0, PointerKind::Touch, true);

        let n1 = diff_count(&bg, &idle_1x);
        let n2 = diff_count(&bg, &idle_2x);
        assert!(n1 > 40, "default cursor must draw visible pixels ({n1})");
        assert!(n2 > n1 * 2, "2x cursor must be larger than 1x ({n2} vs {n1})");
        assert_ne!(idle_1x, hover_1x, "hover state must differ from default arrow");
        assert_ne!(hover_1x, press_1x, "pressed state must differ from hover state");
        assert_ne!(idle_1x, text_1x, "text I-beam must differ from default arrow");
        assert!(diff_count(&bg, &touch_1x) > 100, "touch contact must draw visible pixels");

        // Invisible pointer leaves pixels untouched
        let mut hidden = bg.clone();
        renderer.draw(
            &mut resolver,
            &mut hidden,
            [0, 0, 200, 200],
            1.5,
            &PointerState { visible: false, x: 50.0, y: 50.0, kind: PointerKind::Default, pressed: false, custom_icon: None },
        );
        assert_eq!(hidden, bg);
    }
}
