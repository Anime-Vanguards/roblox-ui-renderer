//! Scene export (port of `runtime/export/scene.luau`): LayerCollectors into the scene JSON the
//! renderers read, with the Roblox draw order (Sibling and Global ZIndexBehavior, CanvasGroup draw
//! lists, scroll bar entries), node appearance (background, border, corner, strokes, shadows,
//! gradient, image with its @2x density variant, group) and geometry from the layout records
//! (`engine.layout`: rect, rotation matrix, AbsolutePosition under rotation, clips with scroll
//! windows, text glyph runs and fonts, scroll bars, AutomaticSize culling), and the Path2Ds of each
//! node as nodes of their own ([`path_json`]).
//!
//! Two paths share one traversal ([`Walk`]), as scene.luau's Scene.Build with and without a cache:
//! - [`build`]: the one-shot export (CLI dumps, the session's `scene` command): every node, with
//!   `order`, a fresh font registry.
//! - [`retained::Retained`] (live sessions, Scene.NewCache): only nodes whose inputs changed are
//!   rebuilt (CachedNode), with the cache's appearance / text reuse, the persistent font registry,
//!   the culling of nodes their clip hides (outside presentation mode) and the presentation fields.
//!
//! Interaction visuals come from runtime/input's port (engine::input).

pub mod retained;
mod viewport;
#[cfg(test)]
mod scene_tests;

use std::{rc::Rc, sync::Arc};

use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::{Map, Value as Json, json};

use crate::engine::GuiEngine;
use super::{
    ClassId, Engine, InstanceId, K, NameId, Reflection, State,
    layout::{BoxSlots, Layout, Record, absolute_position, record::{Transform, compose, rotation_about}},
    path2d::{self, Segment},
    path2d_layout,
    text::{FaceId, Line, TextEngine, TextLayout, XAlign, is_space, line_pen, ppem, reveal::LineReveal},
    value::{ContentValue, Value},
};

/// Marker returned by the layout step (`story::layout`); the records themselves live in
/// `engine.layout`.
#[derive(Default)]
pub struct Records {
    _laid_out: (),
}

/// Studio keeps GUI geometry in float32: exported as the nearest float32 in 9 significant digits;
/// values within 1e-6 of 0 are the layout's rounding noise.
pub fn float32(value: f64) -> f64 {
    if value.abs() < 1e-6 {
        return 0.0;
    }
    if !value.is_finite() {
        return value;
    }
    if value.abs() <= 16777216.0 && value.fract() == 0.0 {
        return value;
    }
    let single = value as f32;
    // The result depends on the float32 only: memoized per thread in a small direct-mapped
    // table (layout positions repeat from frame to frame)
    thread_local! {
        static MEMO: std::cell::RefCell<Vec<(u32, f64)>> = std::cell::RefCell::new(vec![(u32::MAX, 0.0); 4096]);
    }
    let bits = single.to_bits();
    let slot = ((bits ^ (bits >> 12) ^ (bits >> 24)) & 4095) as usize;
    if let Some(known) = MEMO.with(|memo| {
        let entry = memo.borrow()[slot];
        (entry.0 == bits).then_some(entry.1)
    }) {
        return known;
    }
    let result = float32_digits(single).unwrap_or_else(|| {
        let single = single as f64;
        format!("{single:.8e}").parse().unwrap_or(single)
    });
    MEMO.with(|memo| memo.borrow_mut()[slot] = (bits, result));
    result
}

/// `tonumber(string.format("%.9g", f))` of a float32 without formatting: the exact 9-digit decimal
/// (integer arithmetic on the float's mantissa and exponent, round half even) divided or multiplied
/// by an exact power of ten, which rounds like the parse. None outside the exact range and on an
/// exact decimal tie (the caller formats).
fn float32_digits(single: f32) -> Option<f64> {
    const POW10: [u128; 23] = {
        let mut table = [1u128; 23];
        let mut i = 1;
        while i < 23 {
            table[i] = table[i - 1] * 10;
            i += 1;
        }
        table
    };
    let bits = single.to_bits();
    let negative = bits >> 31 == 1;
    let exponent = ((bits >> 23) & 0xff) as i32;
    if exponent == 0 || exponent == 0xff {
        return None;
    }
    let mantissa = ((bits & 0x7f_ffff) | 0x80_0000) as u128;
    let exp2 = exponent - 150;
    if !(-90..=40).contains(&exp2) {
        return None;
    }
    let magnitude = single.abs() as f64;
    let mut e10 = magnitude.log10().floor() as i32;
    for _ in 0..3 {
        let k = 8 - e10;
        if !(-22..=22).contains(&k) {
            return None;
        }
        let mut num = mantissa;
        let mut den: u128 = 1;
        if exp2 >= 0 {
            num <<= exp2;
        } else {
            den <<= -exp2;
        }
        if k >= 0 {
            num = num.checked_mul(POW10[k as usize])?;
        } else {
            den = den.checked_mul(POW10[(-k) as usize])?;
        }
        let mut q = num / den;
        let rem = num % den;
        let twice = rem * 2;
        if twice == den {
            return None;
        }
        if twice > den {
            q += 1;
        }
        if q >= 1_000_000_000 {
            // (log10 estimate one short, or rounded up into the next decade)
            e10 += 1;
            continue;
        }
        if q < 100_000_000 {
            e10 -= 1;
            continue;
        }
        let q = q as f64;
        let result = if k >= 0 { q / POW10[k as usize] as f64 } else { q * POW10[(-k) as usize] as f64 };
        return Some(if negative { -result } else { result });
    }
    None
}

fn round(value: f64, digits: i32) -> f64 {
    let factor = 10f64.powi(digits);
    (value * factor + 0.5).floor() / factor
}

/// A number as the Luau JSON encoder writes it: integral values as integers.
pub fn number(value: f64) -> Json {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 9.0e15 {
        Json::from(value as i64)
    } else {
        serde_json::Number::from_f64(value).map(Json::Number).unwrap_or(Json::Null)
    }
}

fn numbers(values: &[f64]) -> Json {
    Json::Array(values.iter().map(|v| number(*v)).collect())
}

/// Class ids the export tests (resolved once per build; exact names where scene.luau compares
/// `Class.Name`, `is_a` where it reads `Class.Ancestors`).
pub(crate) struct Classes {
    gui_object: Option<ClassId>,
    folder: Option<ClassId>,
    text_label: Option<ClassId>,
    text_button: Option<ClassId>,
    text_box: Option<ClassId>,
    image_label: Option<ClassId>,
    image_button: Option<ClassId>,
    scrolling_frame: Option<ClassId>,
    canvas_group: Option<ClassId>,
    viewport_frame: Option<ClassId>,
    ui_corner: Option<ClassId>,
    ui_stroke: Option<ClassId>,
    ui_shadow: Option<ClassId>,
    ui_gradient: Option<ClassId>,
    player_gui: Option<ClassId>,
    path2d: Option<ClassId>,
    grid_layouts: [Option<ClassId>; 4],
}

impl Classes {
    pub(crate) fn new(reflection: &Reflection) -> Classes {
        let id = |name: &str| reflection.class_id(name);
        Classes {
            gui_object: id("GuiObject"),
            folder: id("Folder"),
            text_label: id("TextLabel"),
            text_button: id("TextButton"),
            text_box: id("TextBox"),
            image_label: id("ImageLabel"),
            image_button: id("ImageButton"),
            scrolling_frame: id("ScrollingFrame"),
            canvas_group: id("CanvasGroup"),
            viewport_frame: id("ViewportFrame"),
            ui_corner: id("UICorner"),
            ui_stroke: id("UIStroke"),
            ui_shadow: id("UIShadow"),
            ui_gradient: id("UIGradient"),
            player_gui: id("PlayerGui"),
            path2d: id("Path2D"),
            grid_layouts: [id("UIListLayout"), id("UIGridLayout"), id("UIPageLayout"), id("UITableLayout")],
        }
    }
}

/// Property names outside `K` (resolved once per build).
struct Names {
    placeholder_color3: Option<NameId>,
    text_stroke_transparency: Option<NameId>,
    text_stroke_color3: Option<NameId>,
    text_truncate: Option<NameId>,
    max_visible_graphemes: Option<NameId>,
    closed: Option<NameId>,
    color3: Option<NameId>,
}

impl Names {
    fn new(reflection: &Reflection) -> Names {
        let id = |name: &str| reflection.name_id(name);
        Names {
            placeholder_color3: id("PlaceholderColor3"),
            text_stroke_transparency: id("TextStrokeTransparency"),
            text_stroke_color3: id("TextStrokeColor3"),
            text_truncate: id("TextTruncate"),
            max_visible_graphemes: id("MaxVisibleGraphemes"),
            closed: id("Closed"),
            color3: id("Color3"),
        }
    }
}

struct Ctx<'a> {
    state: &'a State,
    reflection: &'a Reflection,
    engine: &'a Engine,
    layout: &'a Layout,
    c: Classes,
    n: Names,
}

impl Ctx<'_> {
    fn class(&self, id: InstanceId) -> ClassId {
        self.state.dom.at(id).class
    }

    fn class_name(&self, id: InstanceId) -> &str {
        &self.reflection.class(self.class(id)).name
    }

    /// `Class.Ancestors[name]`
    fn is(&self, id: InstanceId, class: Option<ClassId>) -> bool {
        class.is_some_and(|c| self.reflection.class(self.class(id)).is_a(c))
    }

    /// `Class.Name == name`
    fn is_exactly(&self, id: InstanceId, class: Option<ClassId>) -> bool {
        class.is_some() && Some(self.class(id)) == class
    }

    fn get(&self, id: InstanceId, name: NameId) -> Option<&Value> {
        self.state.dom.raw_get(id, name)
    }

    fn get_opt(&self, id: InstanceId, name: Option<NameId>) -> Option<&Value> {
        name.and_then(|n| self.get(id, n))
    }

    fn number(&self, id: InstanceId, name: NameId) -> Option<f64> {
        self.get(id, name).and_then(Value::as_number)
    }

    fn enum_name(&self, value: Option<&Value>) -> Option<&str> {
        value.and_then(Value::as_enum).map(|item| &*self.reflection.enum_item_info(item).name)
    }

    fn color(&self, value: Option<&Value>) -> Json {
        match value.and_then(Value::as_color3) {
            Some(c) => numbers(&[c.r as f64, c.g as f64, c.b as f64]),
            None => Json::Null,
        }
    }

    fn children(&self, id: InstanceId) -> &[InstanceId] {
        &self.state.dom.at(id).children
    }

    fn enabled(&self, id: InstanceId) -> bool {
        !matches!(self.get(id, K::Enabled as NameId), Some(Value::Bool(false)))
    }

    fn record(&self, id: InstanceId) -> Option<&Record> {
        self.layout.record(id)
    }

    /// Layout.IsTextObject
    fn is_text_object(&self, id: InstanceId) -> bool {
        self.is(id, self.c.text_label) || self.is(id, self.c.text_button) || self.is(id, self.c.text_box)
    }

    /// GuiChildren: GuiObject children, looking through Folders.
    fn gui_children_into(&self, id: InstanceId, out: &mut Vec<InstanceId>) {
        for &child in self.children(id) {
            if self.is(child, self.c.gui_object) {
                out.push(child);
            } else if self.is_exactly(child, self.c.folder) {
                self.gui_children_into(child, out);
            }
        }
    }

    /// The children a walk visits in order: the instance's own Path2Ds, then its GuiChildren, sorted by
    /// ZIndex (then that order) unless the root is Global. Measured (fixtures/path2d-order.luau, Studio
    /// 0.747): a path draws under its parent's GuiObjects of the same ZIndex wherever it sits among them,
    /// and over an earlier path of the same ZIndex; under a Global root it takes its place by ZIndex among
    /// every descendant, as if it were its parent's first child.
    fn ordered_children(&self, id: InstanceId, global: bool) -> Vec<InstanceId> {
        let mut children = Vec::new();
        if self.c.path2d.is_some() {
            children.extend(self.children(id).iter().copied().filter(|&child| self.is_exactly(child, self.c.path2d)));
        }
        self.gui_children_into(id, &mut children);
        if !global {
            // (stable: equal ZIndex keeps the child order)
            children.sort_by(|a, b| {
                let za = self.number(*a, K::ZIndex as NameId).unwrap_or(1.0);
                let zb = self.number(*b, K::ZIndex as NameId).unwrap_or(1.0);
                za.partial_cmp(&zb).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        children
    }
}

/// GuiChildren: GuiObject children, looking through Folders.
pub fn gui_children(state: &State, reflection: &Reflection, id: InstanceId) -> Vec<InstanceId> {
    let gui_object = reflection.class_id("GuiObject");
    let folder = reflection.class_id("Folder");
    let mut result = Vec::new();
    for &child in &state.dom.at(id).children {
        let class = state.dom.at(child).class;
        if gui_object.is_some_and(|g| reflection.class(class).is_a(g)) {
            result.push(child);
        } else if Some(class) == folder {
            result.extend(gui_children(state, reflection, child));
        }
    }
    result
}

trait DefaultUDim2 {
    fn unwrap_or_default_udim2(self) -> rbx_dom_weak::types::UDim2;
}

impl DefaultUDim2 for Option<rbx_dom_weak::types::UDim2> {
    fn unwrap_or_default_udim2(self) -> rbx_dom_weak::types::UDim2 {
        use rbx_dom_weak::types::{UDim, UDim2};
        self.unwrap_or(UDim2::new(UDim::new(0.0, 0), UDim::new(0.0, 0)))
    }
}

// Node appearance

fn color_sequence(value: Option<&Value>) -> Json {
    match value {
        Some(Value::ColorSequence(sequence)) => Json::Array(
            sequence
                .keypoints
                .iter()
                .map(|k| numbers(&[k.time as f64, k.color.r as f64, k.color.g as f64, k.color.b as f64]))
                .collect(),
        ),
        _ => Json::Array(Vec::new()),
    }
}

fn number_sequence(value: Option<&Value>) -> Json {
    match value {
        Some(Value::NumberSequence(sequence)) => Json::Array(
            sequence
                .keypoints
                .iter()
                .map(|k| numbers(&[k.time as f64, k.value as f64, k.envelope as f64]))
                .collect(),
        ),
        _ => Json::Array(Vec::new()),
    }
}

fn gradient(ctx: &Ctx, id: InstanceId) -> Json {
    let offset = ctx.get(id, K::Offset as NameId).and_then(Value::as_vector2);
    json!({
        "color": color_sequence(ctx.get(id, K::Color as NameId)),
        "transparency": number_sequence(ctx.get(id, K::Transparency as NameId)),
        "rotation": number(ctx.number(id, K::Rotation as NameId).unwrap_or(0.0)),
        "offset": numbers(&offset.map(|o| [o.x as f64, o.y as f64]).unwrap_or([0.0, 0.0])),
    })
}

#[inline]
fn f32r(value: f64) -> f64 {
    value as f32 as f64
}

fn decorations(ctx: &Ctx, id: InstanceId, record: &Record, node: &mut Map<String, Json>) {
    let mut corner: Option<Json> = None;
    let mut strokes: Vec<(f64, Json)> = Vec::new();
    let mut shadows = Vec::new();
    let mut fill_gradient: Option<Json> = None;
    for &child in ctx.children(id) {
        let class = Some(ctx.class(child));
        if class.is_none() {
            continue;
        }
        if class == ctx.c.ui_corner {
            if corner.is_some() {
                continue;
            }
            // Negative sizes draw nothing, and their corners collapse to 0
            let shortest = record.w.min(record.h).max(0.0);
            let mut radii = Vec::new();
            for name in [K::TopLeftRadius, K::TopRightRadius, K::BottomRightRadius, K::BottomLeftRadius] {
                let radius = ctx.get(child, name as NameId).and_then(Value::as_udim);
                let value = radius.map_or(0.0, |r| r.scale as f64 * shortest + r.offset as f64 * record.scale);
                radii.push(round(value.clamp(0.0, shortest / 2.0), 4));
            }
            corner = Some(if radii.iter().all(|r| *r == radii[0]) { number(radii[0]) } else { numbers(&radii) });
        } else if class == ctx.c.ui_stroke {
            if !ctx.enabled(child) {
                continue;
            }
            let stroke_gradient = ctx
                .children(child)
                .iter()
                .copied()
                .filter(|&g| ctx.is_exactly(g, ctx.c.ui_gradient) && ctx.enabled(g))
                .last()
                .map(|g| gradient(ctx, g));
            // Measured (fixtures/stroke-misc.luau, stroke-text-e.luau): ScaledSize thickness is a
            // fraction of the shorter side (of the text size for text strokes). The text size is the
            // UIScale'd whole size the text is laid out at, floor(TextSize x UIScale) (TextScaled's /
            // a UITextSizeConstraint's pick), not the local TextSize: measured 2026-09-28,
            // fixtures/stroke-probe-scaledui.luau (plain, TextScaled and MaxTextSize-capped labels under
            // UIScale 1.03 / 0.885 / 1.25, 1x and 1.5x; a card list's names at 1x) and -scaledui2.luau
            // (60 plain labels, TextSize x UIScale fractional: 40 x 0.885 = 35.4 strokes like 35; 60/60,
            // the unfloored size 29/60).
            let shortest = record.w.min(record.h);
            let sizing = ctx.enum_name(ctx.get(child, K::StrokeSizingMode as NameId));
            let thickness = f32r(ctx.number(child, K::Thickness as NameId).unwrap_or(1.0));
            let border_offset = ctx.get(child, K::BorderOffset as NameId).and_then(Value::as_udim);
            let on_text = ctx.enum_name(ctx.get(child, K::ApplyStrokeMode as NameId)) == Some("Contextual") && record.text.is_some();
            let reference = if sizing != Some("ScaledSize") {
                record.scale
            } else if on_text {
                record.text.as_ref().unwrap().effective_size
            } else {
                shortest
            };
            let z_index = ctx.number(child, K::ZIndex as NameId).unwrap_or(1.0);
            let mut stroke = Map::new();
            stroke.insert("thickness".into(), number(f32r(thickness * f32r(reference))));
            stroke.insert("color".into(), ctx.color(ctx.get(child, K::Color as NameId)));
            stroke.insert("transparency".into(), number(ctx.number(child, K::Transparency as NameId).unwrap_or(0.0)));
            insert_str(&mut stroke, "joins", ctx.enum_name(ctx.get(child, K::LineJoinMode as NameId)));
            insert_str(&mut stroke, "mode", ctx.enum_name(ctx.get(child, K::ApplyStrokeMode as NameId)));
            insert_str(&mut stroke, "position", ctx.enum_name(ctx.get(child, K::BorderStrokePosition as NameId)));
            insert_str(&mut stroke, "sizing", sizing);
            if let Some(offset) = border_offset {
                stroke.insert(
                    "borderOffset".into(),
                    number(round(offset.scale as f64 * shortest + offset.offset as f64 * record.scale, 4)),
                );
            }
            stroke.insert("zIndex".into(), number(z_index));
            if let Some(g) = stroke_gradient {
                stroke.insert("gradient".into(), g);
            }
            // Strokes render in ZIndex order (child order among equals)
            let at = strokes.iter().position(|(z, _)| *z > z_index).unwrap_or(strokes.len());
            strokes.insert(at, (z_index, Json::Object(stroke)));
        } else if class == ctx.c.ui_shadow {
            if !ctx.enabled(child) {
                continue;
            }
            let shortest = record.w.min(record.h);
            let blur = ctx.get(child, K::BlurRadius as NameId).and_then(Value::as_udim);
            let spread = ctx.get(child, K::Spread as NameId).and_then(Value::as_udim2).unwrap_or_default_udim2();
            let offset = ctx.get(child, K::Offset as NameId).and_then(Value::as_udim2).unwrap_or_default_udim2();
            let mut shadow = Map::new();
            shadow.insert("blur".into(), number(round(blur.map_or(0.0, |b| b.scale as f64 * shortest + b.offset as f64), 4)));
            shadow.insert(
                "spread".into(),
                numbers(&[
                    round(spread.x.scale as f64 * record.w + spread.x.offset as f64 * record.scale, 4),
                    round(spread.y.scale as f64 * record.h + spread.y.offset as f64 * record.scale, 4),
                ]),
            );
            shadow.insert(
                "offset".into(),
                numbers(&[
                    round(offset.x.scale as f64 * record.w + offset.x.offset as f64 * record.scale, 4),
                    round(offset.y.scale as f64 * record.h + offset.y.offset as f64 * record.scale, 4),
                ]),
            );
            shadow.insert("color".into(), ctx.color(ctx.get(child, K::Color as NameId)));
            shadow.insert("transparency".into(), number(ctx.number(child, K::Transparency as NameId).unwrap_or(0.0)));
            shadow.insert("inset".into(), Json::Bool(matches!(ctx.get(child, K::Inset as NameId), Some(Value::Bool(true)))));
            shadow.insert(
                "behind".into(),
                Json::Bool(!matches!(ctx.get(child, K::ShowBehindParent as NameId), Some(Value::Bool(false)))),
            );
            if let Some(z) = ctx.number(child, K::ZIndex as NameId) {
                shadow.insert("zIndex".into(), number(z));
            }
            insert_str(&mut shadow, "mode", ctx.enum_name(ctx.get(child, K::Mode as NameId)));
            shadows.push(Json::Object(shadow));
        } else if class == ctx.c.ui_gradient && ctx.enabled(child) && fill_gradient.is_none() {
            fill_gradient = Some(gradient(ctx, child));
        }
    }
    if let Some(corner) = corner {
        node.insert("corner".into(), corner);
    }
    if !strokes.is_empty() {
        node.insert("strokes".into(), Json::Array(strokes.into_iter().map(|(_, s)| s).collect()));
    }
    if !shadows.is_empty() {
        node.insert("shadows".into(), Json::Array(shadows));
    }
    if let Some(g) = fill_gradient {
        node.insert("gradient".into(), g);
    }
}

fn insert_str(map: &mut Map<String, Json>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        map.insert(key.into(), Json::String(value.into()));
    }
}

/// `Uri(Value)`: a string, a Content's URI, else "".
fn uri(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.to_string(),
        Some(Value::Content(content)) => match &**content {
            ContentValue::Uri(uri) => uri.to_string(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

/// Measured: on a high-DPI screen Studio samples the @2x file of a built-in texture when it exists
/// (DensityVariant; memoized per image: per build for one-shot exports, for the session in the
/// retained cache, like scene.luau's module-level memo).
#[derive(Default)]
pub(crate) struct Variants {
    cache: FxHashMap<String, (String, f64)>,
}

impl Variants {
    fn variant(&mut self, image: &str, pixel_scale: f64, content_root: Option<&str>) -> (String, f64) {
        if let Some(known) = self.cache.get(image) {
            return known.clone();
        }
        let (mut variant, mut density) = (image.to_string(), 1.0);
        if pixel_scale > 1.0 && image.starts_with("rbxasset://") && image.len() > "rbxasset://".len() + 4 && image.ends_with(".png") {
            // string.match(ContentRoot, "^(.*)/content/?$")
            let version_root = content_root.and_then(|root| {
                let trimmed = root.strip_suffix('/').unwrap_or(root);
                trimmed.strip_suffix("/content").or_else(|| (root.ends_with("/content")).then(|| &root[..root.len() - 8]))
            });
            if let Some(version_root) = version_root {
                let relative = &image["rbxasset://".len()..];
                let mut factor = pixel_scale.ceil().min(3.0);
                while factor >= 2.0 {
                    let scaled = format!("{}@{}x.png", &relative[..relative.len() - 4], factor as i64);
                    for base in ["content", "ExtraContent", "PlatformContent/pc"] {
                        if density == 1.0 && std::path::Path::new(&format!("{version_root}/{base}/{scaled}")).is_file() {
                            variant = format!("rbxasset://{scaled}");
                            density = factor;
                        }
                    }
                    factor -= 1.0;
                }
            }
        }
        self.cache.insert(image.to_string(), (variant.clone(), density));
        (variant, density)
    }
}

fn image(ctx: &Ctx, out: &mut Export, id: InstanceId) -> Option<Json> {
    if !(ctx.is(id, ctx.c.image_label) || ctx.is(id, ctx.c.image_button)) {
        return None;
    }
    let mut image = uri(ctx.get(id, K::ImageContent as NameId));
    if image.is_empty() {
        image = uri(ctx.get(id, K::Image as NameId));
    }
    if image.is_empty() {
        return None;
    }
    let (variant, density) = out.variants.variant(&image, out.pixel_scale, out.content_root.as_deref());
    let vector = |name: K| ctx.get(id, name as NameId).and_then(Value::as_vector2).map_or([0.0, 0.0], |v| [v.x as f64, v.y as f64]);
    let slice = ctx.get(id, K::SliceCenter as NameId).and_then(Value::as_rect);
    let tile = ctx.get(id, K::TileSize as NameId).and_then(Value::as_udim2).unwrap_or_default_udim2();
    let mut map = Map::new();
    if variant != image {
        map.insert("source".into(), Json::String(image.clone()));
    }
    if density != 1.0 {
        map.insert("density".into(), number(density));
    }
    map.insert("uri".into(), Json::String(variant));
    map.insert("color".into(), ctx.color(ctx.get(id, K::ImageColor3 as NameId)));
    map.insert("transparency".into(), number(ctx.number(id, K::ImageTransparency as NameId).unwrap_or(0.0)));
    map.insert("rectOffset".into(), numbers(&vector(K::ImageRectOffset)));
    map.insert("rectSize".into(), numbers(&vector(K::ImageRectSize)));
    insert_str(&mut map, "scaleType", ctx.enum_name(ctx.get(id, K::ScaleType as NameId)));
    map.insert(
        "sliceCenter".into(),
        numbers(&slice.map_or([0.0; 4], |r| [r.min.x as f64, r.min.y as f64, r.max.x as f64, r.max.y as f64])),
    );
    map.insert("sliceScale".into(), number(ctx.number(id, K::SliceScale as NameId).unwrap_or(1.0)));
    map.insert(
        "tileSize".into(),
        numbers(&[tile.x.scale as f64, tile.x.offset as f64, tile.y.scale as f64, tile.y.offset as f64]),
    );
    insert_str(&mut map, "resample", ctx.enum_name(ctx.get(id, K::ResampleMode as NameId)));
    Some(Json::Object(map))
}

fn lua_string(ctx: &Ctx, id: InstanceId, name: NameId) -> Json {
    match ctx.get(id, name) {
        Some(Value::String(s)) => Json::String(s.to_string()),
        _ => Json::Null,
    }
}

// Text

/// The font registry (`fonts`): faces in order of first use. A live session keeps one for its
/// lifetime (scene.luau Cache.Registry), so glyph font indices stay valid across replies.
#[derive(Default)]
pub(crate) struct Registry {
    list: Vec<Json>,
    index: FxHashMap<String, usize>,
    /// FaceId -> index (skips the path lookup per glyph)
    faces: FxHashMap<FaceId, usize>,
}

impl Registry {
    fn font_index(&mut self, text: &TextEngine, face: FaceId) -> usize {
        if let Some(&index) = self.faces.get(&face) {
            return index;
        }
        let entry = text.face(face);
        let index = match self.index.get(&*entry.path) {
            Some(&index) => index,
            None => {
                let font = &entry.font;
                let mut row = json!({
                    "file": &*entry.path,
                    "unitsPerEm": number(font.units_per_em),
                    "ascender": number(font.ascender),
                    "descender": number(font.descender),
                    "lineGap": number(font.line_gap),
                    "family": entry.family,
                    "weight": number(entry.weight),
                    "style": entry.style,
                });
                // A face with a line gap is sized with it: its glyphs' ppem spreads these units over
                // S + 1 px (the renderer reads ascender - descender without the field, as scenes saved
                // before that rule mean)
                if font.line_gap != 0.0 {
                    row["sizeUnits"] = number(super::text::layout::height_units(font));
                }
                self.list.push(row);
                let index = self.list.len() - 1;
                self.index.insert(entry.path.to_string(), index);
                index
            }
        };
        self.faces.insert(face, index);
        index
    }

    pub(crate) fn len(&self) -> usize {
        self.list.len()
    }

    pub(crate) fn json(&self) -> Json {
        Json::Array(self.list.clone())
    }
}

fn color_json(color: &Option<[f64; 3]>) -> Option<Json> {
    color.map(|c| numbers(&c))
}

/// ExportRuns: RichText runs for the renderer (glyph entries index them).
fn export_runs(ctx: &Ctx, text: &mut TextEngine, id: InstanceId, runs: &[super::text::Run], dpi: f64, ui_scale: f64) -> Json {
    let (family, weight, style) = font_parts(ctx, id);
    let scale = f32r(ui_scale);
    let mut result = Vec::new();
    for run in runs {
        let run_style = &run.style;
        let run_weight = run_style.run_weight(weight);
        let font_style = run_style.run_style(style).to_string();
        let mut entry = Map::new();
        entry.insert("bold".into(), Json::Bool(run_style.bold));
        entry.insert("italic".into(), Json::Bool(run_style.italic));
        entry.insert("underline".into(), Json::Bool(run_style.underline));
        entry.insert("strike".into(), Json::Bool(run_style.strikethrough));
        entry.insert("weight".into(), number(run_weight));
        if let Some(color) = color_json(&run_style.color) {
            entry.insert("color".into(), color);
        }
        if let Some(transparency) = run_style.transparency {
            entry.insert("transparency".into(), number(transparency));
        }
        if let Some(stroke) = &run_style.stroke {
            let mut map = Map::new();
            if let Some(color) = color_json(&stroke.color) {
                map.insert("color".into(), color);
            }
            map.insert("thickness".into(), number(stroke.thickness));
            map.insert("transparency".into(), number(stroke.transparency));
            map.insert("joins".into(), Json::String(stroke.joins.clone()));
            entry.insert("stroke".into(), Json::Object(map));
        }
        if let Some(mark) = &run_style.mark {
            let mut map = Map::new();
            if let Some(color) = color_json(&mark.color) {
                map.insert("color".into(), color);
            }
            map.insert("transparency".into(), number(mark.transparency));
            entry.insert("mark".into(), Json::Object(map));
        }
        let run_family = run_style.family.clone().unwrap_or_else(|| family.clone());
        if let Some(face) = text.fonts.get_face(&run_family, run_weight, &font_style) {
            let face = text.face(face);
            entry.insert(
                "face".into(),
                json!({ "bold": face.weight >= 700.0, "italic": face.style == "italic", "weight": number(face.weight) }),
            );
        }
        // A <font size> run's glyphs are rasterized at floor(f32(f32(size * UIScale) * DPI)) and drawn
        // with their quads scaled by the unfloored size over it
        if let Some(size) = run_style.size {
            let unfloored = f32r(f32r(size * scale) * dpi);
            entry.insert("quadScale".into(), number(unfloored / unfloored.floor().max(1.0)));
        }
        result.push(Json::Object(entry));
    }
    Json::Array(result)
}

/// FontInternal.Parts of a text object's FontFace.
fn font_parts(ctx: &Ctx, id: InstanceId) -> (String, f64, &'static str) {
    match ctx.get(id, K::FontFace as NameId) {
        Some(Value::Font(font)) => super::text_service::font_parts(ctx.engine, font),
        _ => ("rbxasset://fonts/families/SourceSansPro.json".to_string(), 400.0, "normal"),
    }
}

/// The glyph entries and rich run segments of one layout's lines (scene.luau's Registry.Lines
/// LineCache): they depend only on the line, so a label that moves reuses them.
#[derive(Default)]
pub(crate) struct LineCache {
    layout: Option<Arc<TextLayout>>,
    lines: Vec<(Json, Option<Json>)>,
}

/// ExportText's lines and content box (the parts that move with the label).
fn text_lines(
    ctx: &Ctx,
    text: &TextEngine,
    registry: &mut Registry,
    id: InstanceId,
    record: &Record,
    pixel_scale: f64,
    cache: Option<&mut LineCache>,
) -> Option<(Json, Json)> {
    let info = record.text.as_ref()?;
    let result = &info.layout;
    let content = record.content;
    let (content_x, content_y) = (content.x * pixel_scale, content.y * pixel_scale);
    let (content_w, content_h) = (content.w * pixel_scale, content.h * pixel_scale);
    let x_align = ctx.enum_name(ctx.get(id, K::TextXAlignment as NameId)).unwrap_or("Center");
    let y_align = ctx.enum_name(ctx.get(id, K::TextYAlignment as NameId)).unwrap_or("Center");

    let top = if y_align == "Top" {
        content_y
    } else if y_align == "Bottom" {
        content_y + content_h - result.height
    } else {
        content_y + (content_h - result.height) / 2.0
    };

    let size = result.size;
    let rich = result.runs.is_some();
    let alignment = XAlign::from_name(x_align);
    let mut cache = cache;
    let caching = cache.is_some();
    let cached = match cache.as_deref_mut() {
        Some(lines) if lines.layout.as_ref().is_some_and(|l| Arc::ptr_eq(l, result)) => Some(&lines.lines),
        Some(lines) => {
            lines.layout = None;
            lines.lines.clear();
            None
        }
        None => None,
    };
    let mut built: Vec<(Json, Option<Json>)> = Vec::new();
    let reveal = text_reveal(ctx, text, id, result);
    let mut lines = Vec::with_capacity(result.lines.len());
    for (index, line) in result.lines.iter().enumerate() {
        let line_width = line.width;
        let left = if x_align == "Left" {
            content_x
        } else if x_align == "Right" {
            content_x + content_w - line_width
        } else {
            content_x + (content_w - line_width) / 2.0
        };
        let line_top = top + line.top;
        let (glyphs, runs) = match cached.and_then(|lines| lines.get(index)) {
            Some((glyphs, runs)) => (glyphs.clone(), runs.clone()),
            None => {
                // Glyph entries: font index, glyph id, x (px from the pen), ppem (per glyph) and, for
                // rich text, the 0-based index into `runs`
                let mut glyphs = Vec::with_capacity(line.glyphs.len());
                let mut run_index: u32 = 0;
                for glyph in &line.glyphs {
                    if glyph.run != 0 {
                        run_index = glyph.run - 1;
                    }
                    if is_space(glyph.code) {
                        continue;
                    }
                    let glyph_size = if glyph.size != 0.0 { glyph.size } else { size };
                    let font_index = registry.font_index(text, glyph.face);
                    let face_ppem = ppem(&text.face(glyph.face).font, glyph_size);
                    let mut entry = Vec::with_capacity(if rich { 5 } else { 4 });
                    entry.push(Json::from(font_index));
                    entry.push(Json::from(glyph.glyph));
                    entry.push(number(round(glyph.x + glyph.x_offset, 3)));
                    entry.push(number(round(face_ppem, 5)));
                    if rich {
                        entry.push(Json::from(run_index));
                    }
                    glyphs.push(Json::Array(entry));
                }
                // Rich text: the horizontal extent of each run segment on the line, spaces included
                let runs = rich.then(|| {
                    let mut segments: Vec<[f64; 3]> = Vec::new();
                    let mut segment_run: f64 = 0.0;
                    for glyph in &line.glyphs {
                        if glyph.run != 0 {
                            segment_run = (glyph.run - 1) as f64;
                        }
                        match segments.last_mut() {
                            Some(current) if current[0] == segment_run => current[2] = glyph.x + glyph.advance,
                            _ => segments.push([segment_run, glyph.x, glyph.x + glyph.advance]),
                        }
                    }
                    Json::Array(segments.iter().map(|s| numbers(s)).collect())
                });
                let glyphs = Json::Array(glyphs);
                if caching {
                    built.push((glyphs.clone(), runs.clone()));
                }
                (glyphs, runs)
            }
        };
        // px: the whole device pixel Studio starts the pen at; ux: the unsnapped pen (only when not whole)
        let pen = line_pen(alignment, content_x, content_w, line_width, rich, false, pixel_scale);
        let exact_pen = round(line_pen(alignment, content_x, content_w, line_width, rich, true, pixel_scale), 3);
        let mut entry = Map::new();
        entry.insert("x".into(), number(round(left, 3)));
        entry.insert("px".into(), number(pen));
        if exact_pen != pen {
            entry.insert("ux".into(), number(exact_pen));
        }
        entry.insert("top".into(), number(round(line_top, 3)));
        entry.insert("height".into(), number(line.height));
        entry.insert("width".into(), number(line_width));
        let (glyphs, runs) = match reveal.as_ref().and_then(|lines| lines[index]) {
            Some(cut) => reveal_line(line, cut, glyphs, runs, &mut entry),
            None => (glyphs, runs),
        };
        entry.insert("glyphs".into(), glyphs);
        if let Some(runs) = runs {
            entry.insert("runs".into(), runs);
        }
        lines.push(Json::Object(entry));
    }
    if let Some(cache) = cache {
        if cache.layout.is_none() {
            cache.layout = Some(result.clone());
            cache.lines = built;
        }
    }
    let content = numbers(&[round(content_x, 3), round(content_y, 3), round(content_w, 3), round(content_h, 3)]);
    Some((Json::Array(lines), content))
}

/// MaxVisibleGraphemes per line (text/reveal.rs); None when it hides nothing. A TextBox's placeholder is
/// drawn whole (not measured).
fn text_reveal(ctx: &Ctx, text: &TextEngine, id: InstanceId, result: &TextLayout) -> Option<Vec<Option<LineReveal>>> {
    let max = ctx.get_opt(id, ctx.n.max_visible_graphemes).and_then(Value::as_number)?;
    if max < 0.0 {
        return None;
    }
    let source = match ctx.get(id, K::Text as NameId) {
        Some(Value::String(s)) => s.as_bytes(),
        _ => b"",
    };
    if source.is_empty() && ctx.is(id, ctx.c.text_box) {
        return None;
    }
    text.reveal(result, source, max)
}

/// A line MaxVisibleGraphemes cuts: `shown` = its leading glyph entries drawn, `ellipsis` = its trailing
/// entries drawn after them (the truncation ellipsis, moved to follow the visible prefix); rich run
/// segments end at the cut. The entries stay (and keep the line's baseline): the layout is unchanged.
fn reveal_line(line: &Line, cut: LineReveal, glyphs: Json, runs: Option<Json>, entry: &mut Map<String, Json>) -> (Json, Option<Json>) {
    let count = line.glyphs.len();
    let entries = |range: std::ops::Range<usize>| line.glyphs[range].iter().filter(|glyph| !is_space(glyph.code)).count();
    let shown = entries(0..cut.visible);
    let tail = entries(count - cut.ellipsis..count);
    let mut glyphs = glyphs;
    if cut.ellipsis_drawn && tail > 0 {
        if let Some(list) = glyphs.as_array_mut() {
            let start = line.glyphs[count - cut.ellipsis].x;
            let moved = line.glyphs[count - cut.ellipsis..].iter().filter(|glyph| !is_space(glyph.code));
            let first = list.len() - tail;
            for (glyph, item) in moved.zip(list[first..].iter_mut()) {
                item[2] = number(round(cut.cut + glyph.x - start + glyph.x_offset, 3));
            }
        }
        entry.insert("ellipsis".into(), Json::from(tail));
    }
    entry.insert("shown".into(), Json::from(shown));
    let runs = runs.map(|runs| {
        let segments = runs.as_array().map(Vec::as_slice).unwrap_or(&[]);
        Json::Array(
            segments
                .iter()
                .filter(|segment| segment[1].as_f64().is_some_and(|x0| x0 < cut.cut))
                .map(|segment| {
                    let x1 = segment[2].as_f64().unwrap_or(0.0).min(cut.cut);
                    json!([segment[0].clone(), segment[1].clone(), number(x1)])
                })
                .collect(),
        )
    });
    (glyphs, runs)
}

/// ExportText's fields that do not move with the label (everything but `lines` / `content`).
fn text_static(ctx: &Ctx, text: &mut TextEngine, id: InstanceId, record: &Record, pixel_scale: f64) -> Option<Map<String, Json>> {
    let info = record.text.as_ref()?;
    let result = &info.layout;
    let x_align = ctx.enum_name(ctx.get(id, K::TextXAlignment as NameId)).unwrap_or("Center").to_string();
    let y_align = ctx.enum_name(ctx.get(id, K::TextYAlignment as NameId)).unwrap_or("Center").to_string();
    let raw_text_empty = match ctx.get(id, K::Text as NameId) {
        Some(Value::String(s)) => s.as_bytes().is_empty(),
        _ => true,
    };
    let placeholder = ctx.is(id, ctx.c.text_box) && raw_text_empty;
    let color = if placeholder { ctx.get_opt(id, ctx.n.placeholder_color3) } else { ctx.get(id, K::TextColor3 as NameId) };
    let stroke_transparency = ctx.get_opt(id, ctx.n.text_stroke_transparency).and_then(Value::as_number).unwrap_or(1.0);
    let mut export = Map::new();
    export.insert("size".into(), number(result.size));
    export.insert("textSize".into(), number(info.text_size));
    export.insert("color".into(), ctx.color(color));
    export.insert("transparency".into(), number(ctx.number(id, K::TextTransparency as NameId).unwrap_or(0.0)));
    export.insert("lineHeight".into(), number(result.line_height));
    export.insert("height".into(), number(result.height));
    export.insert("width".into(), number(result.width));
    export.insert("xAlign".into(), Json::String(x_align));
    export.insert("yAlign".into(), Json::String(y_align));
    insert_str(&mut export, "truncate", ctx.enum_name(ctx.get_opt(id, ctx.n.text_truncate)));
    export.insert(
        "maxGraphemes".into(),
        number(ctx.get_opt(id, ctx.n.max_visible_graphemes).and_then(Value::as_number).unwrap_or(-1.0)),
    );
    if stroke_transparency < 1.0 {
        export.insert(
            "stroke".into(),
            json!({ "color": ctx.color(ctx.get_opt(id, ctx.n.text_stroke_color3)), "transparency": number(stroke_transparency) }),
        );
    }
    // The FontFace's requested weight (100-900) and style ("normal" / "italic")
    export.insert("weight".into(), number(info.weight));
    export.insert("style".into(), Json::String(info.style.clone()));
    if let Some(quad_scale) = info.quad_scale {
        export.insert("quadScale".into(), number(quad_scale));
    }
    // Measured (fixtures/text-probe-selfclip.luau): the object's own ClipsDescendants also cuts its own
    // text (fill, strokes, decorations) to its rect, axis-aligned in screen space even when rotated; a
    // zero-width / zero-height clipping label draws no text. The renderers apply the cut (node rect /
    // `absolute`); without ClipsDescendants text overflows its box, zero-size boxes included.
    if matches!(ctx.get(id, K::ClipsDescendants as NameId), Some(Value::Bool(true))) {
        export.insert("clip".into(), Json::Bool(true));
    }
    // Diagnostic only (renderers ignore it): wrapped lines the box height left out (inspect / summary)
    if result.dropped > 0 {
        export.insert("droppedLines".into(), Json::from(result.dropped));
    }
    if let Some(runs) = &result.runs {
        export.insert("rich".into(), Json::Bool(true));
        export.insert("runs".into(), export_runs(ctx, text, id, runs, pixel_scale, info.pixel_scale / pixel_scale));
    }
    Some(export)
}

/// Studio's AbsolutePosition under rotated ancestors (the rect is exported unrotated); None when equal.
fn absolute_of(record: &Record, matrix: &Option<Transform>) -> Option<Json> {
    matrix.as_ref()?;
    let (x, y) = absolute_position(record);
    if (x - record.x).abs() < 1e-4 && (y - record.y).abs() < 1e-4 {
        return None;
    }
    Some(numbers(&[float32(x), float32(y)]))
}

/// The export's per-build tools: the text engine, the font registry, the image variant memo.
struct Export<'t> {
    text: &'t mut TextEngine,
    registry: &'t mut Registry,
    variants: &'t mut Variants,
    content_root: Option<String>,
    pixel_scale: f64,
}

/// ExportNode's appearance (scene.luau's cached Appearance): every node key that does not move
/// with the layout, except `parent` and the ZInfo fields.
/// (`path`: the node's GetFullName when the caller has it cached)
fn appearance(ctx: &Ctx, out: &mut Export, id: InstanceId, record: &Record, path: Option<String>) -> Map<String, Json> {
    let instance = ctx.state.dom.at(id);
    let mut node = Map::new();
    node.insert("id".into(), Json::from(instance.serial));
    node.insert("class".into(), Json::String(ctx.class_name(id).into()));
    node.insert("name".into(), Json::String(instance.name.to_string()));
    node.insert("path".into(), Json::String(path.unwrap_or_else(|| ctx.state.dom.full_name(ctx.reflection, id))));
    node.insert("zIndex".into(), number(ctx.number(id, K::ZIndex as NameId).unwrap_or(1.0)));
    if let Some(order) = ctx.number(id, K::LayoutOrder as NameId) {
        node.insert("layoutOrder".into(), number(order));
    }
    if ctx.is(id, ctx.c.gui_object) {
        let transparency = ctx.number(id, K::BackgroundTransparency as NameId).unwrap_or(0.0);
        node.insert(
            "bg".into(),
            json!({ "color": ctx.color(ctx.get(id, K::BackgroundColor3 as NameId)), "transparency": number(transparency) }),
        );
        let border = ctx.number(id, K::BorderSizePixel as NameId).unwrap_or(0.0);
        if border > 0.0 {
            let mut map = Map::new();
            map.insert("size".into(), number(border * record.scale));
            map.insert("color".into(), ctx.color(ctx.get(id, K::BorderColor3 as NameId)));
            insert_str(&mut map, "mode", ctx.enum_name(ctx.get(id, K::BorderMode as NameId)));
            node.insert("border".into(), Json::Object(map));
        }
        decorations(ctx, id, record, &mut node);
        if let Some(image) = image(ctx, out, id) {
            node.insert("image".into(), image);
        }
        if ctx.is_text_object(id) {
            let mut props = Map::new();
            props.insert("text".into(), lua_string(ctx, id, K::Text as NameId));
            for (key, name) in [("richText", K::RichText), ("textScaled", K::TextScaled), ("textWrapped", K::TextWrapped)] {
                if matches!(ctx.get(id, name as NameId), Some(Value::Bool(true))) {
                    props.insert(key.into(), Json::Bool(true));
                }
            }
            node.insert("props".into(), Json::Object(props));
        }
        if ctx.is_exactly(id, ctx.c.canvas_group) {
            node.insert(
                "group".into(),
                json!({
                    "color": ctx.color(ctx.get(id, K::GroupColor3 as NameId)),
                    "transparency": number(ctx.number(id, K::GroupTransparency as NameId).unwrap_or(0.0)),
                }),
            );
        } else if ctx.is_exactly(id, ctx.c.viewport_frame) {
            viewport::export(ctx, out, id, record, &mut node);
        }
        export_interaction(ctx, id, &mut node);
    }
    node
}

/// ExportNode's geometry: rect, rotation, matrix, clip, absolute, visible, scale.
/// (`placement` false leaves out matrix and clip: a patch whose matrix and clip did not change.)
/// Returns the DYNAMIC_KEYS bits of the keys it inserted.
fn geometry(ctx: &Ctx, id: InstanceId, record: &Record, matrix: &Option<Transform>, clip: Option<[f64; 4]>, placement: bool, node: &mut Map<String, Json>) -> u16 {
    let mut keys = KEY_RECT | KEY_ROTATION | KEY_VISIBLE;
    node.insert("rect".into(), numbers(&[float32(record.x), float32(record.y), float32(record.w), float32(record.h)]));
    let rotation = record.own_rotation.unwrap_or_else(|| ctx.number(id, K::Rotation as NameId).unwrap_or(0.0));
    node.insert("rotation".into(), number(rotation));
    if let Some(m) = matrix.filter(|_| placement) {
        node.insert(
            "matrix".into(),
            numbers(&[round(m[0], 6), round(m[1], 6), round(m[2], 6), round(m[3], 6), float32(m[4]), float32(m[5])]),
        );
        keys |= KEY_MATRIX;
    }
    if let Some(clip) = clip.filter(|_| placement) {
        node.insert("clip".into(), numbers(&clip));
        keys |= KEY_CLIP;
    }
    if let Some(absolute) = absolute_of(record, matrix) {
        node.insert("absolute".into(), absolute);
        keys |= KEY_ABSOLUTE;
    }
    node.insert("visible".into(), Json::Bool(record.visible));
    if record.scale != 1.0 {
        node.insert("scale".into(), number(record.scale));
        keys |= KEY_SCALE;
    }
    keys
}

/// ExportNode's `scroll` (a ScrollingFrame with scroll bars).
fn scroll(ctx: &Ctx, id: InstanceId, record: &Record) -> Option<Map<String, Json>> {
    if !ctx.is_exactly(id, ctx.c.scrolling_frame) {
        return None;
    }
    let bars = record.scroll_bars?;
    let window = record.window.unwrap_or_default();
    let canvas = record.canvas.unwrap_or_default();
    let canvas_position = ctx.get(id, K::CanvasPosition as NameId).and_then(Value::as_vector2);
    let mut scroll = Map::new();
    scroll.insert("window".into(), numbers(&[float32(window.x), float32(window.y), float32(window.w), float32(window.h)]));
    scroll.insert("canvas".into(), numbers(&[float32(canvas.x), float32(canvas.y), float32(canvas.w), float32(canvas.h)]));
    scroll.insert("vertical".into(), Json::Bool(bars.vertical));
    scroll.insert("horizontal".into(), Json::Bool(bars.horizontal));
    scroll.insert("thickness".into(), number(bars.thickness));
    scroll.insert("color".into(), ctx.color(ctx.get(id, K::ScrollBarImageColor3 as NameId)));
    scroll.insert("transparency".into(), number(ctx.number(id, K::ScrollBarImageTransparency as NameId).unwrap_or(0.0)));
    scroll.insert("topImage".into(), Json::String(uri(ctx.get(id, K::TopImage as NameId))));
    scroll.insert("midImage".into(), Json::String(uri(ctx.get(id, K::MidImage as NameId))));
    scroll.insert("bottomImage".into(), Json::String(uri(ctx.get(id, K::BottomImage as NameId))));
    if let Some(Value::Bool(enabled)) = ctx.get(id, K::ScrollingEnabled as NameId) {
        scroll.insert("scrollingEnabled".into(), Json::Bool(*enabled));
    }
    insert_str(&mut scroll, "verticalPosition", ctx.enum_name(ctx.get(id, K::VerticalScrollBarPosition as NameId)));
    let position = canvas_position.map_or([0.0, 0.0], |p| [p.x as f64, p.y as f64]);
    scroll.insert("canvasPosition".into(), numbers(&[float32(position[0]), float32(position[1])]));
    scroll.insert("scrollOffset".into(), numbers(&[bars.scroll_x, bars.scroll_y]));
    Some(scroll)
}

/// ExportInteraction (runtime/input Input.Visual; added by the input port, phase 5): `interaction`
/// = { state, bg } is the exact 8-bit AutoButtonColor background of a hovered / pressed
/// GuiButton, an ImageButton shows its HoverImage / PressedImage, a focused TextBox adds its caret
/// and selection.
fn export_interaction(ctx: &Ctx, id: InstanceId, node: &mut Map<String, Json>) {
    let Some(input) = super::input::get(ctx.engine) else { return };
    let Some(mut visual) = input.visual(ctx.engine, ctx.state, ctx.layout, id) else { return };
    if let Some(Json::String(uri)) = visual.remove("imageUri") {
        if let Some(Json::Object(image)) = node.get_mut("image") {
            image.insert("uri".into(), Json::String(uri));
            image.remove("source");
            image.remove("density");
        }
    }
    for (key, value) in visual {
        node.insert(key, value);
    }
}

/// The ZInfo fields of a node (draw-related, set by the walk).
#[derive(Clone, Default, PartialEq)]
pub(crate) struct ZInfo {
    culled: bool,
    clip_owners: Option<Rc<[u32]>>,
    /// (owners, baseCulled)
    rotation_cull: Option<(Rc<[u32]>, bool)>,
    display_order: Option<f64>,
    enabled: Option<bool>,
}

fn owners_json(owners: &[u32]) -> Json {
    Json::Array(owners.iter().map(|&id| Json::from(id)).collect())
}

impl ZInfo {
    /// Returns the DYNAMIC_KEYS bits of the keys it inserted.
    fn write(&self, node: &mut Map<String, Json>) -> u16 {
        let mut keys = 0;
        if self.culled {
            node.insert("culled".into(), Json::Bool(true));
            keys |= KEY_CULLED;
        }
        if let Some(owners) = &self.clip_owners {
            node.insert("clipOwners".into(), owners_json(owners));
            keys |= KEY_CLIP_OWNERS;
        }
        if let Some((owners, base_culled)) = &self.rotation_cull {
            node.insert("rotationCull".into(), json!({ "owners": owners_json(owners), "baseCulled": base_culled }));
            keys |= KEY_ROTATION_CULL;
        }
        if let Some(display_order) = self.display_order {
            node.insert("displayOrder".into(), number(display_order));
            keys |= KEY_DISPLAY_ORDER;
        }
        if let Some(enabled) = self.enabled {
            node.insert("enabled".into(), Json::Bool(enabled));
            keys |= KEY_ENABLED;
        }
        keys
    }
}

/// ExportNode (uncached): appearance, geometry, text, scroll and the ZInfo fields.
#[allow(clippy::too_many_arguments)]
fn export_node(
    ctx: &Ctx,
    out: &mut Export,
    id: InstanceId,
    record: &Record,
    parent: Option<u32>,
    matrix: &Option<Transform>,
    clip: Option<[f64; 4]>,
    z: &ZInfo,
) -> Map<String, Json> {
    let mut node = appearance(ctx, out, id, record, None);
    if let Some(parent) = parent {
        node.insert("parent".into(), Json::from(parent));
    }
    geometry(ctx, id, record, matrix, clip, true, &mut node);
    if ctx.is(id, ctx.c.gui_object) {
        if ctx.is_text_object(id) {
            if let Some((lines, content)) = text_lines(ctx, out.text, out.registry, id, record, out.pixel_scale, None) {
                let mut text = text_static(ctx, out.text, id, record, out.pixel_scale).unwrap_or_default();
                text.insert("lines".into(), lines);
                text.insert("content".into(), content);
                node.insert("text".into(), Json::Object(text));
            }
        }
        if let Some(scroll) = scroll(ctx, id, record) {
            node.insert("scroll".into(), Json::Object(scroll));
        }
    }
    z.write(&mut node);
    node
}

/// A Path2D's node: `rect` is the box its stroke can reach (GUI pixels), and `path2d` what draws: the curve's
/// segments on the screen (a line's two points, a quadratic's three, a cubic's four, as x, y pairs; the parent's
/// rotation is in them, so the node has no matrix), the stroke's thickness and colour (Path2D.Transparency is not
/// drawn: Studio 0.747 keeps it and draws the path opaque whatever it is). `origin` is
/// `rect`'s corner when the segments were made: a renderer that finds `rect` moved (a predicted scroll) moves the
/// segments by the difference. A path draws over its parent's own content (background, text, stroke) and is cut
/// by the clip its parent's children get, never by its parent's rounded corners.
#[allow(clippy::too_many_arguments)]
fn path_json(ctx: &Ctx, id: InstanceId, record: &Record, parent: u32, clip: Option<[f64; 4]>, visible: bool, z: &ZInfo, path: Option<String>) -> Map<String, Json> {
    let instance = ctx.state.dom.at(id);
    let frame = path2d_layout::frame_of(record);
    let points = path2d::control_points(ctx.state, ctx.reflection, id);
    let closed = matches!(ctx.get_opt(id, ctx.n.closed), Some(Value::Bool(true)));
    let curve = path2d::Curve::new(&points, closed, &frame);
    let thickness = ctx.number(id, K::Thickness as NameId).unwrap_or(1.0);
    // (what GetBoundingRect answers: the stroke's quads reach half a pixel past the thickness on each side,
    // render/path2d.rs, and a path of no thickness reaches nothing)
    let reach = path2d::margin(thickness as f32) as f64;
    let rect = match curve.bounds() {
        Some((least, most)) => {
            let (x, y) = (least[0] as f64 - reach, least[1] as f64 - reach);
            [x, y, most[0] as f64 + reach - x, most[1] as f64 + reach - y]
        }
        None => [record.x, record.y, 0.0, 0.0],
    };
    let rect = rect.map(float32);
    let mut node = Map::new();
    node.insert("id".into(), Json::from(instance.serial));
    node.insert("class".into(), Json::String(ctx.class_name(id).into()));
    node.insert("name".into(), Json::String(instance.name.to_string()));
    node.insert("path".into(), Json::String(path.unwrap_or_else(|| ctx.state.dom.full_name(ctx.reflection, id))));
    node.insert("zIndex".into(), number(ctx.number(id, K::ZIndex as NameId).unwrap_or(1.0)));
    node.insert("parent".into(), Json::from(parent));
    node.insert("rect".into(), numbers(&rect));
    node.insert("rotation".into(), Json::from(0));
    if let Some(clip) = clip {
        node.insert("clip".into(), numbers(&clip));
    }
    node.insert("visible".into(), Json::Bool(visible));
    let segments: Vec<Json> = curve
        .segments
        .iter()
        .map(|segment| {
            let points: Vec<[f32; 2]> = match *segment {
                Segment::Line(start, end) => vec![start, end],
                Segment::Quadratic(start, control, end) => vec![start, control, end],
                Segment::Cubic(start, first, second, end) => vec![start, first, second, end],
            };
            Json::Array(points.iter().flatten().map(|&value| number(float32(value as f64))).collect())
        })
        .collect();
    let mut stroke = Map::new();
    stroke.insert("segments".into(), Json::Array(segments));
    stroke.insert("origin".into(), numbers(&rect[..2]));
    stroke.insert("thickness".into(), number(thickness));
    stroke.insert("color".into(), ctx.color(ctx.get_opt(id, ctx.n.color3)));
    if closed {
        stroke.insert("closed".into(), Json::Bool(true));
    }
    node.insert("path2d".into(), Json::Object(stroke));
    z.write(&mut node);
    node
}

/// A node's image URIs (`images`): its image, its scroll bar images and their @2x variants.
/// (`known`: URIs the caller will skip anyway; the node's own image is not copied for them)
fn node_images(node: &Map<String, Json>, known: &FxHashSet<String>, images: &mut Vec<String>) {
    if let Some(uri) = node.get("image").and_then(|i| i.get("uri")).and_then(|u| u.as_str()) {
        if !known.contains(uri) {
            images.push(uri.to_string());
        }
    }
    if let Some(scroll) = node.get("scroll") {
        for key in ["topImage", "midImage", "bottomImage"] {
            if let Some(image) = scroll.get(key).and_then(|v| v.as_str()) {
                if !image.is_empty() {
                    images.push(image.to_string());
                    // Scroll bars use the @2x variant of built-in textures when it exists, at any DPI
                    // (render/scene_draw.rs draw_post_inner draws it; notes/images.md)
                    if image.starts_with("rbxasset://") && image.ends_with(".png") {
                        images.push(format!("{}@2x.png", &image[..image.len() - 4]));
                    }
                }
            }
        }
    }
}

fn has_grid_layout(ctx: &Ctx, id: InstanceId) -> bool {
    ctx.children(id).iter().any(|&child| {
        let class = Some(ctx.class(child));
        ctx.c.grid_layouts.iter().any(|layout| layout.is_some() && *layout == class)
    })
}

/// AutoCulled: an AutomaticSize object whose centre is not strictly inside its clip.
fn auto_culled(ctx: &Ctx, id: InstanceId, record: &Record, clip: Option<[f64; 4]>) -> bool {
    let Some(clip) = clip else { return false };
    match ctx.enum_name(ctx.get(id, K::AutomaticSize as NameId)) {
        None | Some("None") => return false,
        _ => {}
    }
    let (cx, cy) = (record.x + record.w / 2.0, record.y + record.h / 2.0);
    !(cx > clip[0] && cx < clip[0] + clip[2] && cy > clip[1] && cy < clip[1] + clip[3])
}

/// Clipping.Outside: the (unrotated, AbsolutePosition-placed) box is wholly outside the clip.
fn outside(record: &Record, clip: Option<[f64; 4]>) -> bool {
    let Some(clip) = clip else { return false };
    if clip[2] <= 0.0 || clip[3] <= 0.0 {
        return true;
    }
    let (x, y) = absolute_position(record);
    x + record.w <= clip[0] || x >= clip[0] + clip[2] || y + record.h <= clip[1] || y >= clip[1] + clip[3]
}

/// Clipping.Rect(Record, true): the node's clip rect (its scroll window when it has one).
fn clip_rect(record: &Record) -> [f64; 4] {
    let (x, y) = absolute_position(record);
    match record.window {
        Some(window) => [x + window.x - record.x, y + window.y - record.y, window.w, window.h],
        None => [x, y, record.w, record.h],
    }
}

/// PaintMargins: how far outside its rect a node can paint (logical px, per axis): strokes,
/// shadows, and text that overflows the box. Unknown text bounds cannot be culled.
fn paint_margins(ctx: &Ctx, id: InstanceId, record: &Record) -> (f64, f64) {
    let mut margin: f64 = 4.0;
    let longest = record.w.max(record.h).max(0.0);
    for &child in ctx.children(id) {
        let class = Some(ctx.class(child));
        if class.is_none() {
            continue;
        }
        if class == ctx.c.ui_stroke {
            let thickness = ctx.number(child, K::Thickness as NameId).unwrap_or(0.0);
            let mut extent = if ctx.enum_name(ctx.get(child, K::StrokeSizingMode as NameId)) == Some("ScaledSize") {
                thickness * longest
            } else {
                thickness * record.scale
            };
            if let Some(offset) = ctx.get(child, K::BorderOffset as NameId).and_then(Value::as_udim) {
                extent += (offset.scale as f64).abs() * longest + (offset.offset as f64).abs() * record.scale;
            }
            margin = margin.max(extent + 4.0);
        } else if class == ctx.c.ui_shadow {
            let blur = ctx.get(child, K::BlurRadius as NameId).and_then(Value::as_udim);
            let spread = ctx.get(child, K::Spread as NameId).and_then(Value::as_udim2).unwrap_or_default_udim2();
            let offset = ctx.get(child, K::Offset as NameId).and_then(Value::as_udim2).unwrap_or_default_udim2();
            let mut extent = 1.5 * blur.map_or(0.0, |b| (b.scale as f64).abs() * longest + (b.offset as f64).abs());
            for u in [spread, offset] {
                extent += (u.x.scale as f64).abs() * longest
                    + (u.x.offset as f64).abs() * record.scale
                    + (u.y.scale as f64).abs() * longest
                    + (u.y.offset as f64).abs() * record.scale;
            }
            margin = margin.max(extent + 4.0);
        }
    }
    let (mut x, mut y) = (margin, margin);
    if let Some(info) = &record.text {
        let Some(bounds) = ctx.get(id, K::TextBounds as NameId).and_then(Value::as_vector2) else {
            return (f64::INFINITY, f64::INFINITY);
        };
        // Glyphs overhang their bounds by less than a text size
        let size = info.text_size;
        x += (bounds.x as f64 - record.w).max(0.0) + size;
        y += (bounds.y as f64 - record.h).max(0.0) + size;
    }
    (x, y)
}

/// Clipped: live sessions skip nodes their clip hides entirely (unrotated nodes only; the clip is
/// in screen space).
fn clipped(ctx: &Ctx, id: InstanceId, record: &Record, clip: [f64; 4]) -> bool {
    let (margin_x, margin_y) = paint_margins(ctx, id, record);
    record.x + record.w + margin_x <= clip[0]
        || record.x - margin_x >= clip[0] + clip[2]
        || record.y + record.h + margin_y <= clip[1]
        || record.y - margin_y >= clip[1] + clip[3]
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Draw {
    Id(i64),
    Group { group: u32, draw: Vec<Draw> },
}

impl Draw {
    fn json(&self) -> Json {
        match self {
            Draw::Id(id) => Json::from(*id),
            Draw::Group { group, draw } => json!({ "group": group, "draw": draw.iter().map(Draw::json).collect::<Vec<_>>() }),
        }
    }
}

/// One LayerCollector's draw list.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Layer {
    root: u32,
    display_order: f64,
    draw: Vec<Draw>,
    global: bool,
    /// Under a PlayerGui: drawn in Roblox's PlayerGui pass, below every CoreGui layer whatever their
    /// DisplayOrders (`playerGui: true`; only `--all-layers` exports such layers)
    player_gui: bool,
}

impl Layer {
    fn json(&self) -> Json {
        let mut layer = json!({
            "root": self.root,
            "displayOrder": number(self.display_order),
            "draw": self.draw.iter().map(Draw::json).collect::<Vec<_>>(),
            "zIndexBehavior": if self.global { "Global" } else { "Sibling" },
        });
        if self.player_gui {
            layer["playerGui"] = Json::Bool(true);
        }
        layer
    }
}

pub(crate) fn layers_json(layers: &[Layer]) -> Json {
    Json::Array(layers.iter().map(Layer::json).collect())
}

struct GlobalItem {
    id: u32,
    z: f64,
    sequence: u32,
    group: bool,
    post: bool,
    skip_self: bool,
    children: Vec<GlobalItem>,
}

/// AutomaticContent (scene.luau): an AutomaticSize object or one with such a descendant; memoized
/// in the retained cache (invalidated by its hooks, as the Luau cache's).
fn automatic_content(ctx: &Ctx, memo: &mut FxHashMap<InstanceId, bool>, id: InstanceId) -> bool {
    if let Some(&known) = memo.get(&id) {
        return known;
    }
    let mut found = matches!(ctx.enum_name(ctx.get(id, K::AutomaticSize as NameId)), Some(name) if name != "None");
    if !found {
        for &child in ctx.children(id) {
            if automatic_content(ctx, memo, child) {
                found = true;
                break;
            }
        }
    }
    memo.insert(id, found);
    found
}

/// The node keys a rebuild recomputes (scene.luau DynamicFields plus parent and the root's ZInfo);
/// bit i of `Entry::dynamic_keys` = key i present.
const DYNAMIC_KEYS: [&str; 15] = [
    "parent", "rect", "rotation", "matrix", "clip", "absolute", "visible", "scale", "text", "scroll", "culled", "clipOwners",
    "rotationCull", "displayOrder", "enabled",
];

const KEY_PARENT: u16 = 1;
const KEY_RECT: u16 = 1 << 1;
const KEY_ROTATION: u16 = 1 << 2;
const KEY_MATRIX: u16 = 1 << 3;
const KEY_CLIP: u16 = 1 << 4;
const KEY_ABSOLUTE: u16 = 1 << 5;
const KEY_VISIBLE: u16 = 1 << 6;
const KEY_SCALE: u16 = 1 << 7;
const KEY_TEXT: u16 = 1 << 8;
const KEY_SCROLL: u16 = 1 << 9;
const KEY_CULLED: u16 = 1 << 10;
const KEY_CLIP_OWNERS: u16 = 1 << 11;
const KEY_ROTATION_CULL: u16 = 1 << 12;
const KEY_DISPLAY_ORDER: u16 = 1 << 13;
const KEY_ENABLED: u16 = 1 << 14;
const KEY_PLACEMENT: u16 = 1 << 3 | 1 << 4;
const KEY_Z: u16 = 1 << 10 | 1 << 11 | 1 << 12 | 1 << 13 | 1 << 14;

/// The walk inputs of a child subtree (the arguments of the `visit` that reaches it).
#[derive(Clone, PartialEq)]
pub(crate) struct SubtreeInputs {
    parent_id: u32,
    matrix: Option<Transform>,
    clip: Option<[f64; 4]>,
    suppressed: bool,
    scrolled: bool,
    owners: Option<Rc<[u32]>>,
}

/// What a child subtree whose nodes were all reused added to a retained build: its draw entries
/// (in its caller's list), its exported ids in walk order and the nodes it visited. A later build
/// adds them again instead of walking the subtree while nothing in it changed.
pub(crate) struct Subtree {
    /// The build that recorded it
    build: u64,
    structure: u64,
    inputs: SubtreeInputs,
    draw: Vec<Draw>,
    exported: Vec<(u32, InstanceId)>,
    visited: u32,
}

/// The retained cache as one build sees it (scene.luau's Cache inside Scene.Build).
pub(crate) struct Cached<'a> {
    entries: &'a mut BoxSlots<retained::Entry>,
    versions: &'a mut retained::Versions,
    frame: u64,
    presentation: bool,
    /// Nodes exported this build (serial, instance), in order
    exported: Vec<(u32, InstanceId)>,
    /// Image URIs announced to the client (never shrinks)
    announced: &'a mut FxHashSet<String>,
    /// Newly announced images this build
    new_images: Vec<String>,
    /// CachedNode misses (nodes rebuilt)
    misses: usize,
}

/// One root's traversal (scene.luau ExportRoot's Visit).
struct Walk<'w, 'c, 'e, 'k> {
    ctx: &'w Ctx<'c>,
    out: &'w mut Export<'e>,
    global: bool,
    /// One-shot: every node (with `order`); retained: the rebuilt nodes
    nodes: Vec<Json>,
    sequence: u32,
    cache: Option<&'w mut Cached<'k>>,
}

impl Walk<'_, '_, '_, '_> {
    fn presentation(&self) -> bool {
        self.cache.as_ref().is_some_and(|c| c.presentation)
    }

    /// A node through the cache (CachedNode) or, one-shot, ExportNode with `order`; returns
    /// whether the node has a `scroll` (its scroll bar draw entry).
    #[allow(clippy::too_many_arguments)]
    fn node(&mut self, id: InstanceId, record: &Record, parent: Option<u32>, matrix: &Option<Transform>, clip: Option<[f64; 4]>, z: ZInfo, order: Option<u32>) -> bool {
        if self.cache.is_some() {
            return self.cached_node(id, record, parent, matrix, clip, z);
        }
        let mut node = export_node(self.ctx, self.out, id, record, parent, matrix, clip, &z);
        if let Some(order) = order {
            node.insert("order".into(), Json::from(order));
        }
        let has_scroll = node.contains_key("scroll");
        self.nodes.push(Json::Object(node));
        has_scroll
    }

    /// CachedNode: reuses a node the client holds while its instance version (own properties and
    /// decoration children), the structure, its layout record version and every walk input (parent,
    /// matrix, clip, ZInfo) are unchanged; else rebuilds it, reusing the appearance while the style
    /// version, size and scale are unchanged (not for TextBoxes) and the text's static fields while
    /// the text layout is the same.
    fn cached_node(&mut self, id: InstanceId, record: &Record, parent: Option<u32>, matrix: &Option<Transform>, clip: Option<[f64; 4]>, z: ZInfo) -> bool {
        let ctx = self.ctx;
        let cache = self.cache.as_deref_mut().expect("retained walk");
        let (version, style_version) = cache.versions.of(id);
        let structure = cache.versions.structure;
        let serial = ctx.state.dom.at(id).serial;
        cache.exported.push((serial, id));
        let previous = cache.entries.get_mut(id);
        if let Some(entry) = previous {
            if entry.sent
                && entry.version == version
                && entry.structure == structure
                && entry.record_version == record.version
                && entry.parent == parent
                && entry.z == z
                && entry.matrix == *matrix
                && entry.clip == clip
            {
                entry.exported = cache.frame;
                return entry.has_scroll;
            }
        }
        cache.misses += 1;
        let mut entry = cache.entries.take(id).unwrap_or_default();
        let reuse_style = entry.built
            && entry.style_version == style_version
            && entry.structure == structure
            && entry.w == record.w
            && entry.h == record.h
            && entry.scale == record.scale
            && !ctx.is_exactly(id, ctx.c.text_box);
        if !reuse_style {
            // (rebuilt below if the node goes out whole)
            entry.appearance = None;
        }
        // The keys that move with the layout; a node the client holds with the same appearance
        // gets only these (a patch it merges), else the whole node
        let mut node = Map::new();
        let patch = reuse_style && entry.sent;
        // A patch leaves out the keys whose inputs did not change (the client keeps them)
        let keep_parent = patch && entry.parent == parent;
        let keep_placement = patch && entry.matrix == *matrix && entry.clip == clip;
        let keep_z = patch && entry.z == z;
        let kept = (if keep_parent { KEY_PARENT } else { 0 }) | (if keep_placement { KEY_PLACEMENT } else { 0 }) | (if keep_z { KEY_Z } else { 0 });
        // (the DYNAMIC_KEYS bits of the keys this rebuild inserts)
        let mut inserted = 0;
        if let Some(parent) = parent.filter(|_| !keep_parent) {
            node.insert("parent".into(), Json::from(parent));
            inserted |= KEY_PARENT;
        }
        inserted |= geometry(ctx, id, record, matrix, clip, !keep_placement, &mut node);
        let mut text_layout = None;
        if ctx.is(id, ctx.c.gui_object) {
            if ctx.is_text_object(id) {
                if let Some((lines, content)) =
                    text_lines(ctx, self.out.text, self.out.registry, id, record, self.out.pixel_scale, Some(&mut entry.lines))
                {
                    let layout = &record.text.as_ref().expect("text lines").layout;
                    let reuse_text = reuse_style && entry.text_layout.as_ref().is_some_and(|l| Arc::ptr_eq(l, layout));
                    if !reuse_text || entry.text_static.is_none() {
                        entry.text_static = text_static(ctx, self.out.text, id, record, self.out.pixel_scale);
                    }
                    let mut text = entry.text_static.clone().unwrap_or_default();
                    text.insert("lines".into(), lines);
                    text.insert("content".into(), content);
                    node.insert("text".into(), Json::Object(text));
                    inserted |= KEY_TEXT;
                    text_layout = Some(layout.clone());
                }
            }
            if let Some(mut scroll) = scroll(ctx, id, record) {
                if cache.presentation {
                    // Crossing zero changes Studio's AutomaticSize culling and therefore the draw
                    // lists: an authoritative update crosses that boundary before descendants are
                    // predicted again
                    let zero_sensitive = ctx.children(id).iter().any(|&c| automatic_content(ctx, &mut cache.versions.automatic, c));
                    let at_zero = scroll.get("canvasPosition").and_then(Json::as_array).is_some_and(|p| {
                        p.len() == 2 && p[0].as_f64() == Some(0.0) && p[1].as_f64() == Some(0.0)
                    });
                    let range = record.scroll_max.unwrap_or([0.0, 0.0]);
                    let mut info = Map::new();
                    info.insert("range".into(), numbers(&range));
                    insert_str(&mut info, "direction", ctx.enum_name(ctx.get(id, K::ScrollingDirection as NameId)));
                    if zero_sensitive {
                        info.insert("zeroSensitive".into(), Json::Bool(true));
                        if at_zero {
                            info.insert("enabled".into(), Json::Bool(false));
                        }
                    }
                    scroll.insert("presentation".into(), Json::Object(info));
                }
                node.insert("scroll".into(), Json::Object(scroll));
                inserted |= KEY_SCROLL;
            }
        }
        if !keep_z {
            inserted |= z.write(&mut node);
        }
        let has_scroll = inserted & KEY_SCROLL != 0;
        let dynamic_keys = (entry.dynamic_keys & kept) | inserted;
        if patch {
            // runtime/session.luau NodePatch: the keys the node no longer has
            let gone = entry.dynamic_keys & !dynamic_keys;
            if gone != 0 {
                let unset = DYNAMIC_KEYS.iter().enumerate().filter(|(bit, _)| gone & 1 << bit != 0).map(|(_, key)| Json::String((*key).into()));
                node.insert("unset".into(), Json::Array(unset.collect()));
            }
            node.insert("id".into(), Json::from(serial));
            node.insert("patch".into(), Json::Bool(true));
        } else {
            // The appearance moves into the node (a later whole resend while it stays valid
            // rebuilds it: it is a function of the style inputs the reuse checks)
            let appearance = match entry.appearance.take() {
                Some(appearance) => appearance,
                None => {
                    let path_version = cache.versions.path_of(id);
                    let path = match &entry.path {
                        Some((version, path)) if *version == path_version => path.clone(),
                        _ => {
                            let path = ctx.state.dom.full_name(ctx.reflection, id);
                            entry.path = Some((path_version, path.clone()));
                            path
                        }
                    };
                    appearance(ctx, self.out, id, record, Some(path))
                }
            };
            let dynamic = std::mem::replace(&mut node, appearance);
            node.extend(dynamic);
        }
        entry.dynamic_keys = dynamic_keys;
        let mut images = Vec::new();
        node_images(&node, cache.announced, &mut images);
        for image in images {
            if !cache.announced.contains(&image) {
                if let Some(slot) = crate::assets::viewport_slot(&image) {
                    cache.announced.retain(|old| crate::assets::viewport_slot(old) != Some(slot));
                }
                cache.announced.insert(image.clone());
                cache.new_images.push(image);
            }
        }
        self.nodes.push(Json::Object(node));
        entry.built = true;
        entry.sent = true;
        entry.exported = cache.frame;
        entry.version = version;
        entry.style_version = style_version;
        entry.structure = structure;
        entry.record_version = record.version;
        entry.parent = parent;
        entry.z = z;
        entry.matrix = *matrix;
        entry.clip = clip;
        entry.w = record.w;
        entry.h = record.h;
        entry.scale = record.scale;
        entry.text_layout = text_layout;
        entry.has_scroll = has_scroll;
        cache.entries.set(id, entry);
        has_scroll
    }

    /// The children to visit, in order (the retained cache keeps them: Cache.Children).
    fn children(&mut self, id: InstanceId) -> Rc<[InstanceId]> {
        let global = self.global;
        match self.cache.as_deref_mut() {
            Some(cache) => cache
                .versions
                .children
                .entry(id)
                .or_insert_with(|| self.ctx.ordered_children(id, global).into())
                .clone(),
            None => self.ctx.ordered_children(id, global).into(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn visit(
        &mut self,
        id: InstanceId,
        parent_id: u32,
        flat: &mut Vec<Draw>,
        tree: &mut Vec<GlobalItem>,
        matrix: Option<Transform>,
        clip: Option<[f64; 4]>,
        suppressed: bool,
        scrolled: bool,
        owners: Option<&Rc<[u32]>>,
    ) {
        let retained = self.cache.is_some();
        let children = self.children(id);
        // Retained, per-sibling roots: a child subtree in which nothing changed since a build
        // recorded it, visited with the same inputs, adds that build's nodes (all reused), draw
        // entries and ids again without being walked (see `Subtree`)
        let skippable = retained && !self.global;
        let inputs = skippable.then(|| SubtreeInputs { parent_id, matrix, clip, suppressed, scrolled, owners: owners.cloned() });
        for &child in children.iter() {
            if self.ctx.is_exactly(child, self.ctx.c.path2d) {
                self.visit_path(child, id, parent_id, flat, tree, clip, suppressed, owners);
                continue;
            }
            if let Some(inputs) = &inputs {
                if self.reuse_subtree(child, inputs, flat) {
                    continue;
                }
            }
            let before = (flat.len(), self.sequence, self.cache.as_ref().map_or((0, 0), |c| (c.exported.len(), c.misses)));
            self.visit_child(child, parent_id, flat, tree, matrix, clip, suppressed, scrolled, owners);
            if let Some(inputs) = &inputs {
                self.record_subtree(child, inputs, before, flat);
            }
        }
    }

    /// A Path2D of `parent` (whose node is `parent_id`): its node and its draw entry. The path is drawn where
    /// its parent's rectangle is, so the node is rebuilt when the parent's record changes; it is not drawn under
    /// a parent that draws nothing (AutomaticSize culling) or when its own Visible is false.
    #[allow(clippy::too_many_arguments)]
    fn visit_path(
        &mut self,
        path: InstanceId,
        parent: InstanceId,
        parent_id: u32,
        flat: &mut Vec<Draw>,
        tree: &mut Vec<GlobalItem>,
        clip: Option<[f64; 4]>,
        suppressed: bool,
        owners: Option<&Rc<[u32]>>,
    ) {
        let ctx = self.ctx;
        let Some(record) = ctx.record(parent) else { return };
        // Before Roblox clipped rotated elements (a place that keeps ClipsDescendantsSupportsRotation off) no
        // ancestor clipped a Path2D: its announcement says so; not measured (notes/path2d.md)
        let modern = ctx.engine.options.clips_descendants_supports_rotation;
        let clip = if modern { clip } else { None };
        let owners: Option<Rc<[u32]>> = if self.presentation() && !modern { Some(Rc::from(Vec::new())) } else { owners.cloned() };
        self.sequence += 1;
        let serial = ctx.state.dom.at(path).serial;
        let visible = record.visible && !matches!(ctx.get(path, K::Visible as NameId), Some(Value::Bool(false)));
        let z = ZInfo { culled: suppressed, clip_owners: owners, rotation_cull: None, display_order: None, enabled: None };
        match self.cache.as_deref_mut() {
            None => {
                let mut node = path_json(ctx, path, record, parent_id, clip, visible, &z, None);
                node.insert("order".into(), Json::from(self.sequence));
                self.nodes.push(Json::Object(node));
            }
            Some(cache) => {
                let (version, _) = cache.versions.of(path);
                let structure = cache.versions.structure;
                cache.exported.push((serial, path));
                let reused = cache.entries.get_mut(path).is_some_and(|entry| {
                    let same = entry.sent
                        && entry.version == version
                        && entry.structure == structure
                        && entry.record_version == record.version
                        && entry.parent == Some(parent_id)
                        && entry.z == z
                        && entry.clip == clip;
                    if same {
                        entry.exported = cache.frame;
                    }
                    same
                });
                if !reused {
                    cache.misses += 1;
                    let mut entry = cache.entries.take(path).unwrap_or_default();
                    let path_version = cache.versions.path_of(path);
                    let name = match &entry.path {
                        Some((version, name)) if *version == path_version => name.clone(),
                        _ => {
                            let name = ctx.state.dom.full_name(ctx.reflection, path);
                            entry.path = Some((path_version, name.clone()));
                            name
                        }
                    };
                    self.nodes.push(Json::Object(path_json(ctx, path, record, parent_id, clip, visible, &z, Some(name))));
                    entry.built = true;
                    entry.sent = true;
                    entry.exported = cache.frame;
                    entry.version = version;
                    entry.structure = structure;
                    entry.record_version = record.version;
                    entry.parent = Some(parent_id);
                    entry.z = z;
                    entry.matrix = None;
                    entry.clip = clip;
                    cache.entries.set(path, entry);
                }
            }
        }
        if !visible || suppressed {
            return;
        }
        if self.global {
            tree.push(GlobalItem {
                id: serial,
                z: ctx.number(path, K::ZIndex as NameId).unwrap_or(1.0),
                sequence: self.sequence,
                group: false,
                post: false,
                skip_self: false,
                children: Vec::new(),
            });
        } else {
            flat.push(Draw::Id(serial as i64));
        }
    }

    /// Adds the recorded output of `child`'s subtree when it is still valid: nothing in the
    /// subtree changed since the build that recorded it (the cache's dirty marks), the walk inputs
    /// are the same and the client holds every node (the previous build exported the child).
    fn reuse_subtree(&mut self, child: InstanceId, inputs: &SubtreeInputs, flat: &mut Vec<Draw>) -> bool {
        let Some(cache) = self.cache.as_deref_mut() else { return false };
        let dirty = cache.versions.dirty_of(child);
        let structure = cache.versions.structure;
        let frame = cache.frame;
        let Some(entry) = cache.entries.get_mut(child) else { return false };
        let valid = entry.exported + 1 == frame
            && entry.subtree.as_deref().is_some_and(|sub| sub.structure == structure && dirty <= sub.build && sub.inputs == *inputs);
        if !valid {
            return false;
        }
        let sub = entry.subtree.take().expect("checked above");
        flat.extend_from_slice(&sub.draw);
        cache.exported.extend_from_slice(&sub.exported);
        for &(_, id) in &sub.exported {
            if let Some(entry) = cache.entries.get_mut(id) {
                entry.exported = frame;
            }
        }
        self.sequence += sub.visited;
        if let Some(entry) = cache.entries.get_mut(child) {
            entry.subtree = Some(sub);
        }
        true
    }

    /// Records what `child`'s subtree added to this build when none of its nodes was rebuilt
    /// (a subtree that changed is likely to change again: it is recorded once it settles).
    fn record_subtree(&mut self, child: InstanceId, inputs: &SubtreeInputs, before: (usize, u32, (usize, usize)), flat: &[Draw]) {
        let sequence = self.sequence;
        let Some(cache) = self.cache.as_deref_mut() else { return };
        let (flat_len, sequence_before, (exported_len, misses)) = before;
        let frame = cache.frame;
        let structure = cache.versions.structure;
        let clean = cache.misses == misses;
        let Some(entry) = cache.entries.get_mut(child) else { return };
        if !clean || entry.exported != frame {
            entry.subtree = None;
            return;
        }
        entry.subtree = Some(Box::new(Subtree {
            build: frame,
            structure,
            inputs: inputs.clone(),
            draw: flat[flat_len..].to_vec(),
            exported: cache.exported[exported_len..].to_vec(),
            visited: sequence - sequence_before,
        }));
    }

    /// One child of `visit` (the node, its clip and draw entries, its subtree).
    #[allow(clippy::too_many_arguments)]
    fn visit_child(
        &mut self,
        child: InstanceId,
        parent_id: u32,
        flat: &mut Vec<Draw>,
        tree: &mut Vec<GlobalItem>,
        matrix: Option<Transform>,
        clip: Option<[f64; 4]>,
        suppressed: bool,
        scrolled: bool,
        owners: Option<&Rc<[u32]>>,
    ) {
        let ctx = self.ctx;
        let presentation = self.presentation();
        let retained = self.cache.is_some();
        let Some(record) = ctx.record(child) else { return };
        // Children placed by a UIListLayout/UIGridLayout ignore their own Rotation (OwnRotation)
        let rotation = record.own_rotation.unwrap_or_else(|| ctx.number(child, K::Rotation as NameId).unwrap_or(0.0));
        let child_rotated = record.rotation != 0.0;
        let mut child_matrix = matrix;
        if rotation != 0.0 {
            let about = rotation_about(rotation, record.x + record.w / 2.0, record.y + record.h / 2.0);
            child_matrix = Some(compose(&matrix.unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]), &about));
        }
        let escapes = child_rotated && !ctx.engine.options.clips_descendants_supports_rotation;
        let node_clip = if escapes { None } else { clip };
        let is_text = ctx.is_text_object(child);
        let canvas_group = ctx.is_exactly(child, ctx.c.canvas_group);
        let cull_self = !suppressed && !child_rotated && !scrolled && auto_culled(ctx, child, record, node_clip);
        let base_skip_self = suppressed || (cull_self && !is_text);
        let rotation_culled = child_rotated && outside(record, clip);
        let skip_self = base_skip_self || rotation_culled;
        // Presentation: the clip owners of this node (an escaping rotated node keeps only the
        // root's) and, for a rotated clipped node, the owners the client culls it against
        // (shared lists: most nodes pass their parent's on unchanged)
        let node_owners: Option<Rc<[u32]>> = if presentation && escapes { Some(Rc::from(Vec::new())) } else { owners.cloned() };
        let draw_skip_self = if presentation { base_skip_self } else { skip_self };
        let child_suppressed = suppressed || (cull_self && (canvas_group || !has_grid_layout(ctx, child)));
        let canvas_position = if ctx.is_exactly(child, ctx.c.scrolling_frame) {
            ctx.get(child, K::CanvasPosition as NameId).and_then(Value::as_vector2)
        } else {
            None
        };
        let child_scrolled = scrolled || canvas_position.is_some_and(|p| p.x != 0.0 || p.y != 0.0);

        self.sequence += 1;
        let serial = ctx.state.dom.at(child).serial;
        // Live sessions skip nodes their clip hides entirely (unrotated; not in presentation
        // mode, which retains instantiated offscreen nodes)
        let culled = retained && !presentation && node_clip.is_some() && child_matrix.is_none() && clipped(ctx, child, record, node_clip.unwrap());
        let mut has_scroll = false;
        if !culled {
            let rotation_cull = (presentation && child_rotated && clip.is_some()).then(|| (owners.cloned().unwrap_or_else(|| Rc::from(Vec::new())), base_skip_self));
            let z = ZInfo { culled: skip_self, clip_owners: node_owners.clone(), rotation_cull, display_order: None, enabled: None };
            let order = (!retained).then_some(self.sequence);
            has_scroll = self.node(child, record, Some(parent_id), &child_matrix, node_clip, z, order);
        }

        // Measured (fixtures/stories-scroll-noclip.luau): a ScrollingFrame clips its canvas to its
        // window only while ClipsDescendants is true; a CanvasGroup clips whatever it says.
        let mut child_clip = clip;
        let mut child_owners = owners.cloned();
        let clips = matches!(ctx.get(child, K::ClipsDescendants as NameId), Some(Value::Bool(true))) || canvas_group;
        if clips && !child_rotated {
            if presentation {
                let mut list = node_owners.as_deref().map(<[u32]>::to_vec).unwrap_or_default();
                list.push(serial);
                child_owners = Some(Rc::from(list));
            }
            let rect = clip_rect(record);
            let (mut x1, mut y1) = (rect[0], rect[1]);
            let (mut x2, mut y2) = (rect[0] + rect[2], rect[1] + rect[3]);
            if let Some(c) = clip {
                x1 = x1.max(c[0]);
                y1 = y1.max(c[1]);
                x2 = x2.min(c[0] + c[2]);
                y2 = y2.min(c[1] + c[3]);
            }
            child_clip = Some([float32(x1), float32(y1), float32((x2 - x1).max(0.0)), float32((y2 - y1).max(0.0))]);
        }

        // An offscreen ordinary ancestor may have visible descendants. Only a culled CanvasGroup's
        // texture hides its entire subtree.
        if record.visible && !(canvas_group && (culled || (rotation_culled && !presentation))) {
            if self.global {
                let mut item = GlobalItem {
                    id: serial,
                    z: ctx.number(child, K::ZIndex as NameId).unwrap_or(1.0),
                    sequence: self.sequence,
                    group: canvas_group,
                    post: has_scroll,
                    skip_self: draw_skip_self,
                    children: Vec::new(),
                };
                let mut unused = Vec::new();
                self.visit(child, serial, &mut unused, &mut item.children, child_matrix, child_clip, child_suppressed, child_scrolled, child_owners.as_ref());
                // A subtree that draws nothing is still visited (its nodes are exported) but not listed
                if !(base_skip_self && child_suppressed) {
                    tree.push(item);
                }
            } else {
                let group = canvas_group && !draw_skip_self;
                let hidden = base_skip_self && child_suppressed;
                let mut own = Vec::new();
                let target: &mut Vec<Draw> = if group || hidden { &mut own } else { flat };
                if !draw_skip_self {
                    target.push(Draw::Id(serial as i64));
                    if has_scroll {
                        target.push(Draw::Id(-(serial as i64)));
                    }
                }
                let mut unused = Vec::new();
                self.visit(child, serial, target, &mut unused, child_matrix, child_clip, child_suppressed, child_scrolled, child_owners.as_ref());
                if group && !hidden {
                    flat.push(Draw::Group { group: serial, draw: own });
                }
            }
        }
    }
}

fn global_collect(item_children: &[GlobalItem], list: &mut Vec<(f64, f64, Draw)>) {
    for child in item_children {
        if child.skip_self {
            global_collect(&child.children, list);
        } else if child.group {
            list.push((child.z, child.sequence as f64, Draw::Group { group: child.id, draw: global_list(&child.children, Some((child.id, child.post))) }));
        } else {
            list.push((child.z, child.sequence as f64, Draw::Id(child.id as i64)));
            if child.post {
                list.push((child.z, child.sequence as f64 + 0.5, Draw::Id(-(child.id as i64))));
            }
            global_collect(&child.children, list);
        }
    }
}

fn global_list(children: &[GlobalItem], include_self: Option<(u32, bool)>) -> Vec<Draw> {
    let mut list = Vec::new();
    global_collect(children, &mut list);
    list.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)));
    let mut draw = Vec::new();
    if let Some((id, post)) = include_self {
        draw.push(Draw::Id(id as i64));
        if post {
            draw.push(Draw::Id(-(id as i64)));
        }
    }
    draw.extend(list.into_iter().map(|(_, _, d)| d));
    draw
}

/// Scene.Build's traversal of `roots` (ExportRoot per LayerCollector): the nodes (one-shot: all;
/// retained: the rebuilt ones) and the draw lists.
fn traverse(ctx: &Ctx, out: &mut Export, roots: &[InstanceId], mut cache: Option<&mut Cached>) -> (Vec<Json>, Vec<Layer>) {
    let mut nodes = Vec::new();
    let mut layers = Vec::new();
    for &root in roots {
        let Some(root_record) = ctx.record(root) else { continue };
        let global = ctx.enum_name(ctx.get(root, K::ZIndexBehavior as NameId)) == Some("Global");
        let display_order = ctx.number(root, K::DisplayOrder as NameId).unwrap_or(0.0);
        let enabled = ctx.enabled(root);
        // Presentation retains instantiated offscreen nodes and the original clip owners
        let presentation = cache.as_ref().is_some_and(|c| c.presentation);
        let root_owners: Option<Rc<[u32]>> = presentation.then(|| Rc::from(Vec::new()));
        let serial = ctx.state.dom.at(root).serial;
        let mut walk = Walk { ctx, out, global, nodes: Vec::new(), sequence: 0, cache: cache.as_deref_mut() };
        let z = ZInfo { culled: false, clip_owners: root_owners.clone(), rotation_cull: None, display_order: Some(display_order), enabled: Some(enabled) };
        walk.node(root, root_record, None, &None, None, z, None);
        let mut flat = Vec::new();
        let mut tree = Vec::new();
        walk.visit(root, serial, &mut flat, &mut tree, None, None, false, false, root_owners.as_ref());
        nodes.append(&mut walk.nodes);
        let draw = if !enabled {
            Vec::new()
        } else if global {
            global_list(&tree, None)
        } else {
            flat
        };
        let player_gui = ctx.c.player_gui.is_some_and(|class| {
            std::iter::successors(ctx.state.dom.at(root).parent, |&id| ctx.state.dom.at(id).parent).any(|id| ctx.class(id) == class)
        });
        layers.push(Layer { root: serial, display_order, draw, global, player_gui });
    }
    (nodes, layers)
}

/// The text engine of `engine` (created on first use).
fn text_engine(engine: &Engine) -> std::cell::RefMut<'_, TextEngine> {
    std::cell::RefMut::map(engine.text().borrow_mut(), |text| {
        text.get_or_insert_with(|| TextEngine::new(super::text::FontPaths::for_repo(&engine.options.repo.to_string_lossy())))
    })
}

/// Scene.Build (one-shot, uncached): the scene JSON of `roots` plus `extra` top-level fields.
pub fn build(engine: &Rc<Engine>, roots: &[InstanceId], _records: &Records, extra: Map<String, Json>) -> Json {
    let _memory = crate::memtrace::scope(crate::memtrace::Tag::Export);
    for &root in roots {
        super::layout::ensure(engine, root);
    }
    let state = engine.state.borrow();
    let layout_guard = engine.layout().borrow();
    let Some(layout) = layout_guard.as_ref() else { return Json::Null };
    let mut text = text_engine(engine);
    let content_root = text.fonts.content_root().map(str::to_string);
    let ctx = Ctx { state: &state, reflection: &engine.reflection, engine, layout, c: Classes::new(&engine.reflection), n: Names::new(&engine.reflection) };
    let options = &engine.options;
    let mut registry = Registry::default();
    let mut variants = Variants::default();
    let mut out = Export { text: &mut *text, registry: &mut registry, variants: &mut variants, content_root, pixel_scale: options.pixel_scale };
    let (nodes, layers) = traverse(&ctx, &mut out, roots, None);
    let mut images: Vec<String> = Vec::new();
    let known = FxHashSet::default();
    for node in &nodes {
        if let Json::Object(node) = node {
            node_images(node, &known, &mut images);
        }
    }
    images.sort();
    images.dedup();
    let mut scene = Map::new();
    scene.insert("version".into(), Json::from(1));
    scene.insert("viewport".into(), viewport_json(engine));
    scene.insert("nodes".into(), Json::Array(nodes));
    scene.insert("layers".into(), layers_json(&layers));
    scene.insert("fonts".into(), Json::Array(std::mem::take(&mut registry.list)));
    scene.insert("images".into(), Json::Array(images.into_iter().map(Json::String).collect()));
    for (key, value) in extra {
        scene.insert(key, value);
    }
    Json::Object(scene)
}

/// The scene header's `viewport`.
pub(crate) fn viewport_json(engine: &Engine) -> Json {
    let options = &engine.options;
    json!({ "width": number(options.viewport_width), "height": number(options.viewport_height), "scale": number(options.pixel_scale) })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The integer path gives exactly `tonumber(string.format("%.9g", f))` (the formatting path).
    #[test]
    fn float32_digits_match_formatting() {
        let reference = |single: f32| -> f64 {
            let single = single as f64;
            format!("{single:.8e}").parse().unwrap()
        };
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut checked = 0;
        for i in 0..400_000u32 {
            let value = match i % 4 {
                // pixel geometry: fractions of UI coordinates
                0 => (next() % 4_000_000) as f64 / 1000.0 - 2000.0,
                1 => (next() % 1_000_000) as f64 / 3.0 / 64.0,
                // any float32 bit pattern in the fast path's range
                2 => f32::from_bits((next() as u32 & 0x7fff_ffff) % 0x4f80_0000 + 0x2000_0000) as f64,
                _ => (next() % 10_000) as f64 * 1.5 / 7.0 * 0.885,
            };
            let single = value as f32;
            if let Some(fast) = float32_digits(single) {
                assert_eq!(fast.to_bits(), reference(single).to_bits(), "{single:e}");
                checked += 1;
            }
        }
        assert!(checked > 300_000, "{checked}");
        for value in [0.1f32, 330.5, 220.333328, 1e-5, 12345.678, 16777215.0, 999_999_999.0, 0.000123456789] {
            if let Some(fast) = float32_digits(value) {
                assert_eq!(fast.to_bits(), reference(value).to_bits(), "{value:e}");
            }
        }
    }
}
