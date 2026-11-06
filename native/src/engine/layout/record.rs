//! Layout records (runtime/layout/init.luau `Record`) and their comparisons. Every optional field of
//! the Luau record is an `Option` here (`nil` = `None`), and the comparisons follow the Luau `==`
//! on each field (nil == nil, numbers by value).

use std::rc::Rc;

use crate::engine::{InstanceId, text::TextMeasure};

/// 2D affine transform [a, b, c, d, e, f]: x' = a*x + c*y + e, y' = b*x + d*y + f
pub type Transform = [f64; 6];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// `Record.ScrollBars`
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollBars {
    pub vertical: bool,
    pub horizontal: bool,
    pub thickness: f64,
    pub border: f64,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub left: bool,
}

/// A text object's measured text (`LayoutText`'s result); shared by identity like the Luau table.
pub type TextInfo = Rc<TextMeasure>;

/// Cells of a UITableLayout row / column: cell -> rect relative to the row.
pub type Cells = Rc<Vec<(InstanceId, [f64; 4])>>;

/// HeldWidth results per list item (see HeldWidth).
pub type Held = Rc<Vec<(InstanceId, f64)>>;

/// X/Y/W/H are in the unrotated frame of the nearest rotated ancestor (what the exporter's matrix
/// composition expects). `outer` is the accumulated ancestor rotation applied to this node's box and
/// `transform` the one applied to its descendants (None when there is no rotation).
#[derive(Clone, Debug, Default)]
pub struct Record {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rotation: f64,
    pub scale: f64,
    pub content: Rect,
    pub visible: bool,
    pub text: Option<TextInfo>,
    pub generation: u64,
    pub canvas: Option<Rect>,
    pub window: Option<Rect>,
    pub own_rotation: Option<f64>,
    pub outer: Option<Transform>,
    pub transform: Option<Transform>,
    pub parent_width: Option<f64>,
    pub parent_height: Option<f64>,
    pub scroll_bars: Option<ScrollBars>,
    pub sizing_w: Option<f64>,
    pub sizing_h: Option<f64>,
    pub available: Option<f64>,
    pub available_h: Option<f64>,
    /// `Measuring = { X, Y }`
    pub measuring: Option<(bool, bool)>,
    pub held_w: Option<Held>,
    pub item_w: Option<f64>,
    pub item_h: Option<f64>,
    pub size_w: Option<f64>,
    pub size_h: Option<f64>,
    pub gap_w: Option<f64>,
    pub gap_h: Option<f64>,
    pub scroll_max: Option<[f64; 2]>,
    pub measure_w: Option<f64>,
    pub measure_h: Option<f64>,
    pub cells: Option<Cells>,
    pub version: u64,
    pub place: Option<[f64; 6]>,
    pub canvas_place: Option<[f64; 6]>,
    /// `AlignStart = { X, Y }`
    pub align_start: Option<(bool, bool)>,
    pub local_w: Option<f64>,
    pub local_h: Option<f64>,
    pub local_sizing_w: Option<f64>,
    pub local_sizing_h: Option<f64>,
    pub local_outer_w: Option<f64>,
    pub local_outer_h: Option<f64>,
    pub local_space_w: Option<f64>,
    pub local_space_h: Option<f64>,
    pub text_box: Option<f64>,
}

#[inline]
fn same_transform(a: &Option<Transform>, b: &Option<Transform>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

#[inline]
fn same_rect(a: &Option<Rect>, b: &Option<Rect>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn same_cells(a: &Option<Cells>, b: &Option<Cells>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            if Rc::ptr_eq(a, b) {
                return true;
            }
            for (cell, rect) in a.iter() {
                match b.iter().find(|(other, _)| other == cell) {
                    Some((_, other)) if other == rect => {}
                    _ => return false,
                }
            }
            b.iter().all(|(cell, _)| a.iter().any(|(other, _)| other == cell))
        }
        _ => false,
    }
}

impl Record {
    /// Whether two records place a subtree identically (every input its layout reads from the record).
    pub fn same_inputs(&self, b: &Record) -> bool {
        let a = self;
        a.x == b.x
            && a.y == b.y
            && a.w == b.w
            && a.h == b.h
            && a.rotation == b.rotation
            && a.scale == b.scale
            && a.visible == b.visible
            && a.own_rotation == b.own_rotation
            && a.parent_width == b.parent_width
            && a.parent_height == b.parent_height
            && a.sizing_w == b.sizing_w
            && a.sizing_h == b.sizing_h
            && a.available == b.available
            && a.available_h == b.available_h
            && a.item_w == b.item_w
            && a.item_h == b.item_h
            && a.size_w == b.size_w
            && a.size_h == b.size_h
            && a.gap_w == b.gap_w
            && a.gap_h == b.gap_h
            && a.measure_w == b.measure_w
            && a.measure_h == b.measure_h
            && a.local_w == b.local_w
            && a.local_h == b.local_h
            && a.local_sizing_w == b.local_sizing_w
            && a.local_sizing_h == b.local_sizing_h
            && a.local_outer_w == b.local_outer_w
            && a.local_outer_h == b.local_outer_h
            && a.local_space_w == b.local_space_w
            && a.local_space_h == b.local_space_h
            && a.text_box == b.text_box
            && a.content == b.content
            && same_transform(&a.outer, &b.outer)
            && same_transform(&a.transform, &b.transform)
            && same_cells(&a.cells, &b.cells)
    }

    /// Whether a re-laid-out record shows the same thing as the previous one (exported content).
    pub fn same_output(&self, b: &Record) -> bool {
        let a = self;
        match (&a.text, &b.text) {
            (None, None) => {}
            (Some(ta), Some(tb)) if Rc::ptr_eq(ta, tb) => {}
            (Some(ta), Some(tb)) => {
                if !std::sync::Arc::ptr_eq(&ta.layout, &tb.layout) || ta.text_size != tb.text_size || ta.pixel_scale != tb.pixel_scale {
                    return false;
                }
            }
            _ => return false,
        }
        a.same_inputs(b) && same_rect(&a.canvas, &b.canvas) && same_rect(&a.window, &b.window) && a.scroll_bars == b.scroll_bars
    }

    /// Whether two records differ at most in where they are (position, rotation transforms,
    /// rotation, visibility): the subtree keeps every size and placement term and can be moved.
    pub fn same_shape(&self, b: &Record) -> bool {
        let a = self;
        a.w == b.w
            && a.h == b.h
            && a.scale == b.scale
            && a.own_rotation == b.own_rotation
            && a.parent_width == b.parent_width
            && a.parent_height == b.parent_height
            && a.sizing_w == b.sizing_w
            && a.sizing_h == b.sizing_h
            && a.available == b.available
            && a.available_h == b.available_h
            && a.item_w == b.item_w
            && a.item_h == b.item_h
            && a.size_w == b.size_w
            && a.size_h == b.size_h
            && a.gap_w == b.gap_w
            && a.gap_h == b.gap_h
            && a.measure_w == b.measure_w
            && a.measure_h == b.measure_h
            && a.local_w == b.local_w
            && a.local_h == b.local_h
            && a.local_sizing_w == b.local_sizing_w
            && a.local_sizing_h == b.local_sizing_h
            && a.local_outer_w == b.local_outer_w
            && a.local_outer_h == b.local_outer_h
            && a.local_space_w == b.local_space_w
            && a.local_space_h == b.local_space_h
            && a.text_box == b.text_box
            && a.content.w == b.content.w
            && a.content.h == b.content.h
            && same_cells(&a.cells, &b.cells)
    }
}

/// `Layout.AbsolutePosition(Record)`: under rotated ancestors, the ancestor-rotated centre of the
/// box minus half the (unrotated) AbsoluteSize; the object's own Rotation does not move it.
pub fn absolute_position(record: &Record) -> (f64, f64) {
    let Some(outer) = record.outer else {
        return (record.x, record.y);
    };
    let center_x = record.x + record.w / 2.0;
    let center_y = record.y + record.h / 2.0;
    let x = outer[0] * center_x + outer[2] * center_y + outer[4];
    let y = outer[1] * center_x + outer[3] * center_y + outer[5];
    (x - record.w / 2.0, y - record.h / 2.0)
}

/// `RotationAbout(Degrees, X, Y)`
pub fn rotation_about(degrees: f64, x: f64, y: f64) -> Transform {
    let radians = degrees.to_radians();
    let (cos, sin) = (radians.cos(), radians.sin());
    [cos, sin, -sin, cos, x - cos * x + sin * y, y - sin * x - cos * y]
}

/// `Compose(M, N)`
pub fn compose(m: &Transform, n: &Transform) -> Transform {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}
