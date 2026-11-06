//! The MicroProfiler view: a frame strip, then every thread's nested scopes on a shared time axis
//! (microprofile.rs records them), and the scopes that took the most time in view.
//!
//! Wheel: zoom at the pointer · drag: pan · click a frame bar: show that frame · double-click a
//! scope: zoom to it · hover: its duration, self time and detail. While recording the view follows
//! the newest frames; paused (Ctrl+P) it holds still for inspection.

use crate::microprofile::{self, Event, Frame, Lane};
use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, Ui, pos2, vec2};
use std::collections::HashMap;

const ROW: f32 = 15.0;
const LANE_GAP: f32 = 6.0;
const STRIP: f32 = 56.0;
const RULER: f32 = 16.0;
/// Frame strip scale: 33.3 ms fills it
const STRIP_MS: f64 = 1000.0 / 30.0;
/// The widest window shown (keeps the copied window small)
const MAX_SPAN: u64 = 1_000_000_000;
const MIN_SPAN: u64 = 20_000;

pub struct MicroProfilerView {
    /// Visible window: [start, start + span) ns
    start: u64,
    span: u64,
    /// Following the newest frame (recording) or held (paused, or after the user moved the view)
    follow: bool,
    /// The frame marked in the strip
    selected: Option<u64>,
    last_paused: bool,
}

impl Default for MicroProfilerView {
    fn default() -> Self {
        Self { start: 0, span: 50_000_000, follow: true, selected: None, last_paused: false }
    }
}

struct Hit {
    lane: String,
    event: Event,
    self_ns: u64,
}

impl MicroProfilerView {
    pub fn show(&mut self, ui: &mut Ui, paused: bool) {
        if paused != self.last_paused {
            // Resuming follows the newest frames again
            if !paused {
                self.follow = true;
                self.selected = None;
            }
            self.last_paused = paused;
        }
        let frames = microprofile::frames();
        if self.follow && !paused {
            if let Some(last) = frames.last() {
                self.start = last.end.saturating_sub(self.span);
            }
        }
        ui.horizontal(|ui| {
            ui.strong("MicroProfiler");
            ui.label(if paused { "Paused (Ctrl+P)" } else { "Recording (Ctrl+P pauses)" });
            ui.separator();
            ui.label(format!("{:.2} ms in view", self.span as f64 / 1e6));
            if ui.button("Follow newest").clicked() {
                self.follow = true;
            }
            if ui.button("Clear").clicked() {
                microprofile::clear();
                self.selected = None;
            }
            ui.separator();
            ui.weak("Wheel: zoom · Drag: pan · Click a frame bar · Double-click a scope: zoom to it");
        });
        self.strip(ui, &frames);
        let lanes = microprofile::window(self.start, self.start + self.span);
        let summary_width = 360.0_f32.min(ui.available_width() * 0.35);
        ui.horizontal_top(|ui| {
            let timeline_width = (ui.available_width() - summary_width - 8.0).max(200.0);
            ui.vertical(|ui| {
                ui.set_width(timeline_width);
                self.timeline(ui, &lanes, &frames);
            });
            ui.vertical(|ui| {
                ui.set_width(summary_width);
                self.summary(ui, &lanes);
            });
        });
    }

    /// Frame bars: the interval to the next frame (light) and the app's update inside it (dark)
    fn strip(&mut self, ui: &mut Ui, frames: &[Frame]) {
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), STRIP), Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, Color32::from_rgb(34, 34, 34));
        let count = ((rect.width() / 7.0) as usize).max(10);
        let first = frames.len().saturating_sub(count);
        let shown = &frames[first..];
        let slot = rect.width() / count as f32;
        let height = |ms: f64| (ms / STRIP_MS).clamp(0.0, 1.0) as f32 * (rect.height() - 4.0);
        let budget_y = rect.bottom() - height(1000.0 / 60.0);
        painter.line_segment([pos2(rect.left(), budget_y), pos2(rect.right(), budget_y)], Stroke::new(1.0, Color32::from_gray(90)));
        for (index, frame) in shown.iter().enumerate() {
            let next = frames.get(first + index + 1).map_or(frame.end, |next| next.start);
            let interval = (next.saturating_sub(frame.start)) as f64 / 1e6;
            let cpu = (frame.end.saturating_sub(frame.start)) as f64 / 1e6;
            let x0 = rect.left() + index as f32 * slot + 1.0;
            let x1 = x0 + slot - 2.0;
            let over = interval > 1000.0 / 60.0;
            let colour = if over { Color32::from_rgb(242, 140, 40) } else { Color32::from_rgb(54, 162, 235) };
            painter.rect_filled(Rect::from_min_max(pos2(x0, rect.bottom() - height(interval)), pos2(x1, rect.bottom())), 0.0, colour);
            painter.rect_filled(
                Rect::from_min_max(pos2(x0, rect.bottom() - height(cpu)), pos2(x1, rect.bottom())),
                0.0,
                colour.gamma_multiply(0.55),
            );
            let in_view = frame.end > self.start && frame.start < self.start + self.span;
            if in_view || self.selected == Some(frame.id) {
                painter.rect_filled(
                    Rect::from_min_max(pos2(x0, rect.bottom() - 3.0), pos2(x1, rect.bottom())),
                    0.0,
                    if self.selected == Some(frame.id) { Color32::WHITE } else { Color32::from_rgb(60, 200, 90) },
                );
            }
            if let Some(pointer) = response.hover_pos() {
                if pointer.x >= x0 && pointer.x < x1 + 2.0 {
                    response.clone().on_hover_text(format!(
                        "Frame {}\nframe time {interval:.2} ms\napp update {cpu:.2} ms",
                        frame.id
                    ));
                    if response.clicked() {
                        // Show the frame with a little context on both sides
                        let span = (next.saturating_sub(frame.start)).max(1_000_000);
                        self.span = (span * 13 / 10).clamp(MIN_SPAN, MAX_SPAN);
                        self.start = frame.start.saturating_sub(span * 15 / 100);
                        self.follow = false;
                        self.selected = Some(frame.id);
                    }
                }
            }
        }
    }

    fn timeline(&mut self, ui: &mut Ui, lanes: &[Lane], frames: &[Frame]) {
        let lanes: Vec<&Lane> = lanes.iter().filter(|lane| !lane.events.is_empty()).collect();
        let depth = |lane: &Lane| lane.events.iter().map(|e| e.depth).max().map_or(0, |d| d as usize + 1);
        let content_height =
            RULER + lanes.iter().map(|lane| ROW + depth(lane) as f32 * ROW + LANE_GAP).sum::<f32>() + 8.0;
        let height = ui.available_height().max(120.0);
        egui::ScrollArea::vertical().id_salt("microprofiler-lanes").max_height(height).auto_shrink([false, false]).show(ui, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), content_height.max(height - 4.0)), Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, Color32::from_rgb(40, 40, 40));
            let (start, span) = (self.start, self.span.max(1));
            let x_of = |t: u64| rect.left() + ((t as f64 - start as f64) / span as f64) as f32 * rect.width();
            // Zoom (wheel) and pan (drag)
            if response.hovered() {
                let wheel = ui.input(|input| input.smooth_scroll_delta.y + input.zoom_delta().ln() * 400.0);
                if wheel != 0.0 {
                    let pointer = response.hover_pos().map_or(rect.center().x, |p| p.x);
                    let at = start as f64 + ((pointer - rect.left()) / rect.width()) as f64 * span as f64;
                    let factor = (-wheel as f64 / 200.0).exp();
                    let new_span = ((span as f64 * factor) as u64).clamp(MIN_SPAN, MAX_SPAN);
                    let left = ((pointer - rect.left()) / rect.width()) as f64;
                    self.start = (at - left * new_span as f64).max(0.0) as u64;
                    self.span = new_span;
                    self.follow = false;
                }
            }
            if response.dragged() {
                let dx = response.drag_delta().x;
                let shift = (dx / rect.width()) as f64 * span as f64;
                self.start = (self.start as f64 - shift).max(0.0) as u64;
                self.follow = false;
            }
            // Frame boundaries and the ruler
            for frame in frames.iter().filter(|f| f.start >= start && f.start < start + span) {
                let x = x_of(frame.start);
                painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0, Color32::from_rgb(70, 110, 70)));
                painter.text(pos2(x + 2.0, rect.top() + 1.0), Align2::LEFT_TOP, format!("#{}", frame.id), FontId::monospace(9.0), Color32::from_rgb(120, 170, 120));
            }
            let step = tick_step(span, rect.width());
            let mut tick = start / step * step;
            while tick < start + span {
                if tick >= start {
                    let x = x_of(tick);
                    painter.line_segment([pos2(x, rect.top() + RULER - 4.0), pos2(x, rect.top() + RULER)], Stroke::new(1.0, Color32::GRAY));
                    painter.text(pos2(x + 2.0, rect.top() + RULER - 4.0), Align2::LEFT_BOTTOM, format_time(tick - start), FontId::monospace(9.0), Color32::from_gray(150));
                }
                tick += step;
            }
            let pointer = response.hover_pos();
            let mut hit: Option<Hit> = None;
            let mut y = rect.top() + RULER;
            for lane in &lanes {
                let rows = depth(lane);
                painter.rect_filled(Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), ROW)), 0.0, Color32::from_rgb(30, 45, 45));
                painter.text(
                    pos2(rect.left() + 4.0, y + ROW / 2.0),
                    Align2::LEFT_CENTER,
                    format!("{} [{} scopes]", lane.name, lane.events.len()),
                    FontId::monospace(11.0),
                    Color32::from_rgb(230, 230, 230),
                );
                let top = y + ROW;
                let self_times = self_times(&lane.events);
                // Merge scopes narrower than a pixel per row so thousands of tiny ones stay cheap
                let mut last_pixel: Vec<f32> = vec![f32::NEG_INFINITY; rows];
                for (index, event) in lane.events.iter().enumerate() {
                    let (x0, x1) = (x_of(event.start).max(rect.left()), x_of(event.end).min(rect.right()));
                    if x1 < rect.left() || x0 > rect.right() {
                        continue;
                    }
                    let row = event.depth as usize;
                    let ey = top + row as f32 * ROW;
                    let width = (x1 - x0).max(1.0);
                    if width < 2.0 && x0.floor() <= last_pixel[row] {
                        continue;
                    }
                    last_pixel[row] = x0.floor();
                    let bar = Rect::from_min_size(pos2(x0, ey + 1.0), vec2(width, ROW - 2.0));
                    painter.rect_filled(bar, 0.0, colour_of(event.name));
                    if width > 34.0 {
                        let ms = (event.end - event.start) as f64 / 1e6;
                        let text = if width > 90.0 { format!("{}  {ms:.3}ms", event.name) } else { event.name.to_string() };
                        let text_painter = painter.with_clip_rect(bar);
                        text_painter.text(pos2(x0 + 3.0, ey + ROW / 2.0), Align2::LEFT_CENTER, text, FontId::monospace(10.0), Color32::BLACK);
                    }
                    if let Some(p) = pointer {
                        if bar.expand2(vec2(0.5, 0.0)).contains(p) {
                            hit = Some(Hit { lane: lane.name.clone(), event: *event, self_ns: self_times[index] });
                        }
                    }
                }
                y = top + rows as f32 * ROW + LANE_GAP;
            }
            if let Some(hit) = hit {
                let e = hit.event;
                let ms = (e.end - e.start) as f64 / 1e6;
                let mut text = format!(
                    "{}\n{ms:.3} ms (self {:.3} ms)\n{} · depth {}",
                    e.name,
                    hit.self_ns as f64 / 1e6,
                    hit.lane,
                    e.depth
                );
                if e.detail > 0 {
                    text.push_str(&format!("\ndetail: {}", e.detail));
                }
                response.clone().on_hover_text_at_pointer(text);
                if response.double_clicked() {
                    let duration = (e.end - e.start).max(MIN_SPAN);
                    self.span = (duration * 12 / 10).clamp(MIN_SPAN, MAX_SPAN);
                    self.start = e.start.saturating_sub(duration / 10);
                    self.follow = false;
                }
            }
        });
    }

    /// The scopes in view by self time (their time minus their children's), with calls, total and max
    fn summary(&self, ui: &mut Ui, lanes: &[Lane]) {
        struct Row {
            calls: u64,
            total: u64,
            own: u64,
            max: u64,
        }
        let (from, to) = (self.start, self.start + self.span);
        let mut rows: HashMap<&'static str, Row> = HashMap::new();
        for lane in lanes {
            let self_times = self_times(&lane.events);
            for (event, own) in lane.events.iter().zip(self_times) {
                // Only the part inside the window counts
                let inside = event.end.min(to).saturating_sub(event.start.max(from));
                if inside == 0 {
                    continue;
                }
                let duration = (event.end - event.start).max(1);
                let row = rows.entry(event.name).or_insert(Row { calls: 0, total: 0, own: 0, max: 0 });
                row.calls += 1;
                row.total += inside;
                row.own += (own as u128 * inside as u128 / duration as u128) as u64;
                row.max = row.max.max(event.end - event.start);
            }
        }
        let mut rows: Vec<_> = rows.into_iter().collect();
        rows.sort_by(|a, b| b.1.own.cmp(&a.1.own));
        ui.strong("Most time in view (self)");
        egui::ScrollArea::vertical().id_salt("microprofiler-summary").auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("microprofiler-summary-grid").striped(true).num_columns(5).show(ui, |ui| {
                ui.label("scope");
                ui.label("self ms");
                ui.label("total ms");
                ui.label("calls");
                ui.label("max ms");
                ui.end_row();
                for (name, row) in rows.iter().take(60) {
                    let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                    ui.painter().rect_filled(dot, 1.0, colour_of(name));
                    ui.add(egui::Label::new(egui::RichText::new(*name).monospace().size(11.0)).truncate());
                    ui.monospace(format!("{:.3}", row.own as f64 / 1e6));
                    ui.monospace(format!("{:.3}", row.total as f64 / 1e6));
                    ui.monospace(row.calls.to_string());
                    ui.monospace(format!("{:.3}", row.max as f64 / 1e6));
                    ui.end_row();
                }
            });
        });
    }
}

/// Each event's time minus its direct children's (events on one thread, in close order)
fn self_times(events: &[Event]) -> Vec<u64> {
    let mut own: Vec<u64> = events.iter().map(|e| e.end.saturating_sub(e.start)).collect();
    // A child closes before its parent: walk back from each event over later-closing... simpler:
    // for every event, find its parent as the nearest later event one level up that contains it
    let mut open: Vec<usize> = Vec::new();
    for index in (0..events.len()).rev() {
        let event = &events[index];
        while let Some(&parent) = open.last() {
            let p = &events[parent];
            if p.depth < event.depth && p.start <= event.start && event.end <= p.end {
                break;
            }
            open.pop();
        }
        if let Some(&parent) = open.last() {
            if events[parent].depth + 1 == event.depth {
                own[parent] = own[parent].saturating_sub(event.end.saturating_sub(event.start));
            }
        }
        open.push(index);
    }
    own
}

fn tick_step(span: u64, width: f32) -> u64 {
    let target = span as f64 / (width as f64 / 90.0).max(1.0);
    let mut step = 1_000u64;
    for candidate in [1_000u64, 2_000, 5_000, 10_000, 20_000, 50_000, 100_000, 200_000, 500_000, 1_000_000, 2_000_000, 5_000_000, 10_000_000, 20_000_000, 50_000_000, 100_000_000, 200_000_000, 500_000_000] {
        step = candidate;
        if candidate as f64 >= target {
            break;
        }
    }
    step
}

fn format_time(ns: u64) -> String {
    if ns >= 1_000_000 {
        format!("{:.2}ms", ns as f64 / 1e6)
    } else {
        format!("{:.0}us", ns as f64 / 1e3)
    }
}

/// A stable colour per scope name (MicroProfiler-like saturated palette)
pub fn colour_of(name: &str) -> Color32 {
    const PALETTE: [(u8, u8, u8); 18] = [
        (214, 68, 170), (160, 196, 60), (236, 140, 46), (70, 160, 230), (200, 180, 90), (120, 200, 200),
        (230, 90, 90), (150, 120, 230), (90, 200, 120), (240, 200, 60), (180, 110, 60), (100, 140, 220),
        (220, 120, 200), (130, 180, 90), (200, 200, 200), (60, 180, 170), (240, 110, 150), (170, 150, 90),
    ];
    let mut hash: u32 = 2166136261;
    for byte in name.bytes() {
        hash = (hash ^ u32::from(byte)).wrapping_mul(16777619);
    }
    let (r, g, b) = PALETTE[(hash as usize) % PALETTE.len()];
    Color32::from_rgb(r, g, b)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn event(name: &'static str, start: u64, end: u64, depth: u16) -> Event {
        Event { start, end, name, depth, detail: 0 }
    }

    #[test]
    fn self_time_subtracts_direct_children_only() {
        // Close order: grandchild, child, sibling, parent
        let events = [
            event("grandchild", 2, 4, 2),
            event("child", 1, 6, 1),
            event("sibling", 7, 9, 1),
            event("parent", 0, 10, 0),
        ];
        assert_eq!(self_times(&events), vec![2, 3, 2, 3]);
    }
}
