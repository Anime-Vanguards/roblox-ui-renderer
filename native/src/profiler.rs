//! Bounded display-frame history. Runtime replies contribute duration totals to the next
//! display frame; those totals are never presented as sequential spans on the host clock.
use std::collections::{BTreeMap, VecDeque};
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use egui::{Color32, Sense, Stroke, Ui, pos2, vec2};
use serde::Serialize;
use serde_json::{Value, json};

/// Display frames kept: the app runs uncapped, so an idle stretch adds a frame every millisecond or so;
/// this keeps 30 s of that (minutes of 60-120 Hz drawing). Names are interned (Name), so a frame holds
/// its numbers and shared strings only.
pub const HISTORY_CAPACITY: usize = 30_000;
pub const PANEL_HEIGHT: f32 = 440.0;
const MAX_PHASES: usize = 96;
const MAX_COUNTERS: usize = 96;
const MAX_CALLBACKS: usize = 32;
const MAX_TEXT: usize = 256;

/// A profiler name (phase, counter, story): interned, so every frame holding it shares one allocation
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name(Arc<str>);
impl Name {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl From<&str> for Name {
    fn from(text: &str) -> Self {
        use std::collections::HashSet;
        use std::sync::{Mutex, OnceLock};
        static NAMES: OnceLock<Mutex<HashSet<Arc<str>>>> = OnceLock::new();
        let mut names = NAMES.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
        if let Some(name) = names.get(text) {
            return Self(name.clone());
        }
        let name: Arc<str> = Arc::from(text);
        // Names are a small set (phases, counters, stories); an unbounded source is not interned
        if names.len() < 65_536 {
            names.insert(name.clone());
        }
        Self(name)
    }
}
impl From<String> for Name {
    fn from(text: String) -> Self {
        Self::from(text.as_str())
    }
}
impl From<&String> for Name {
    fn from(text: &String) -> Self {
        Self::from(text.as_str())
    }
}
impl std::ops::Deref for Name {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}
impl std::borrow::Borrow<str> for Name {
    fn borrow(&self) -> &str {
        &self.0
    }
}
impl PartialEq<str> for Name {
    fn eq(&self, other: &str) -> bool {
        &*self.0 == other
    }
}
impl PartialEq<&str> for Name {
    fn eq(&self, other: &&str) -> bool {
        &*self.0 == *other
    }
}
impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::fmt::Debug for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&*self.0, f)
    }
}
impl Serialize for Name {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PhaseSample {
    pub name: Name,
    pub duration_ms: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CallbackSample {
    pub phase: String,
    pub name: String,
    pub source: String,
    pub line: u32,
    pub duration_ms: f64,
}

/// Record once per native display callback, even when the runtime produced no update.
/// Durations are milliseconds. Zero timestamps/intervals are filled from the host clock.
/// `cpu_draw_ms` measures CPU draw preparation/submission, never GPU execution.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ProfileFrame {
    pub timestamp_ms: f64,
    pub frame_interval_ms: f64,
    pub cpu_draw_ms: f64,
    pub runtime_ms: f64,
    pub layout_ms: f64,
    pub export_ms: f64,
    pub serialize_ms: f64,
    pub input_ms: f64,
    pub scheduler_ms: f64,
    /// Supply only a measured GPU timestamp duration, attributed to this frame.
    pub gpu_ms: Option<f64>,
    pub story: Name,
    pub scroll_active: bool,
    pub scroll_session: Option<u64>,
    pub counters: BTreeMap<Name, f64>,
    pub phases: Vec<PhaseSample>,
    pub callbacks: Vec<CallbackSample>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FrameRecord {
    pub id: u64,
    pub frame: ProfileFrame,
    pub active_interval_ms: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Channel {
    #[default]
    All,
    Draw,
    Runtime,
}

impl Channel {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Draw => "Draw frames",
            Self::Runtime => "Runtime updates",
        }
    }
    fn cpu(self, frame: &ProfileFrame) -> f64 {
        match self {
            Self::All => frame.cpu_draw_ms.max(frame.runtime_ms),
            Self::Draw => frame.cpu_draw_ms,
            Self::Runtime => frame.runtime_ms,
        }
    }
}

pub struct Profiler {
    records: VecDeque<FrameRecord>,
    capacity: usize,
    sequence: u64,
    paused: bool,
    selection: Option<u64>,
    last_display: Option<(f64, Name, bool, Option<u64>)>,
    pending: ProfileFrame,
    worst_draw: Option<FrameRecord>,
    worst_runtime: Option<FrameRecord>,
    worst_gap: Option<FrameRecord>,
    channel: Channel,
    interval_metric: bool,
    scale_ms: f64,
    grow_ceiling_ms: f64,
    view_count: usize,
    /// Ctrl + wheel over the chart: frames shown = view_count / zoom
    zoom: f32,
    /// First frame shown while zoomed in (None: the newest frames)
    zoom_start: Option<u64>,
    clock: Instant,
    pub metadata: BTreeMap<String, String>,
}

impl Default for Profiler {
    fn default() -> Self {
        Self::with_capacity(HISTORY_CAPACITY)
    }
}

impl Profiler {
    pub fn new() -> Self {
        Self::default()
    }

    fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.clamp(1, HISTORY_CAPACITY);
        Self {
            records: VecDeque::with_capacity(capacity.min(4096)),
            capacity,
            sequence: 0,
            paused: false,
            selection: None,
            last_display: None,
            pending: ProfileFrame::default(),
            worst_draw: None,
            worst_runtime: None,
            worst_gap: None,
            channel: Channel::All,
            // CPU duration on a 60 Hz frame budget scale
            interval_metric: false,
            scale_ms: 16.67,
            grow_ceiling_ms: 33.34,
            view_count: 240,
            zoom: 1.0,
            zoom_start: None,
            clock: Instant::now(),
            metadata: BTreeMap::from([
                ("mode".into(), "native".into()),
                ("renderer".into(), "wgpu".into()),
                ("runtimeTransport".into(), "in-process".into()),
            ]),
        }
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn set_paused(&mut self, paused: bool) {
        if paused != self.paused {
            self.last_display = None;
            self.pending = ProfileFrame::default();
        }
        self.paused = paused;
        if !paused {
            self.selection = None;
        }
    }

    /// Clear measurements without losing the story/device metadata or UI settings.
    pub fn clear(&mut self) {
        self.records.clear();
        self.sequence = 0;
        self.selection = None;
        self.last_display = None;
        self.pending = ProfileFrame::default();
        self.worst_draw = None;
        self.worst_runtime = None;
        self.worst_gap = None;
        self.grow_ceiling_ms = 33.34;
    }

    pub fn record(&mut self, mut frame: ProfileFrame) {
        if self.paused {
            return;
        }
        merge(&mut frame, std::mem::take(&mut self.pending));
        sanitize(&mut frame);
        if frame.timestamp_ms == 0.0 {
            frame.timestamp_ms = self.clock.elapsed().as_secs_f64() * 1000.0;
        }
        let mut active_interval_ms = None;
        if let Some((at, story, active, session)) = &self.last_display {
            if frame.frame_interval_ms == 0.0 {
                frame.frame_interval_ms = (frame.timestamp_ms - at).max(0.0);
            }
            if *active
                && frame.scroll_active
                && *story == frame.story
                && *session == frame.scroll_session
                && frame.frame_interval_ms > 0.0
            {
                active_interval_ms = Some(frame.frame_interval_ms);
            }
        }
        if let Some((at, story, active, session)) = &mut self.last_display {
            *at = frame.timestamp_ms;
            story.clone_from(&frame.story);
            *active = frame.scroll_active;
            *session = frame.scroll_session;
        } else {
            self.last_display = Some((
                frame.timestamp_ms,
                frame.story.clone(),
                frame.scroll_active,
                frame.scroll_session,
            ));
        }
        self.sequence += 1;
        let record = FrameRecord {
            id: self.sequence,
            frame,
            active_interval_ms,
        };
        if self
            .worst_draw
            .as_ref()
            .is_none_or(|worst| record.frame.cpu_draw_ms > worst.frame.cpu_draw_ms)
        {
            self.worst_draw = Some(record.clone());
        }
        if self
            .worst_runtime
            .as_ref()
            .is_none_or(|worst| record.frame.runtime_ms > worst.frame.runtime_ms)
        {
            self.worst_runtime = Some(record.clone());
        }
        if active_interval_ms.is_some_and(|gap| {
            self.worst_gap
                .as_ref()
                .is_none_or(|worst| gap > worst.active_interval_ms.unwrap_or(0.0))
        }) {
            self.worst_gap = Some(record.clone());
        }
        if self.records.len() == self.capacity {
            self.records.pop_front();
        }
        let mut record = record;
        record.frame.phases.shrink_to_fit();
        self.records.push_back(record);
    }

    /// Read only bounded numeric profiling fields. Never retain the reply or its scene nodes.
    /// The runtime's `timings` and sampled callback durations are in seconds.
    pub fn record_runtime(&mut self, reply: &Value) {
        if self.paused {
            return;
        }
        let timings = &reply["timings"];
        let seconds = |name: &str| number(&timings[name]) * 1000.0;
        self.pending.runtime_ms += if timings["frameTotal"].is_number() {
            seconds("frameTotal")
        } else {
            seconds("total") + number(&reply["stepTime"]) * 1000.0
        };
        self.pending.layout_ms += seconds("layout");
        self.pending.export_ms += if timings["export"].is_number() {
            seconds("export")
        } else {
            seconds("build")
        };
        self.pending.serialize_ms += seconds("serialize");
        self.pending.input_ms += seconds("input");
        self.pending.scheduler_ms += if timings["simulation"].is_number() {
            seconds("simulation")
        } else {
            number(&reply["stepTime"]) * 1000.0
        };
        *self
            .pending
            .counters
            .entry("runtimeUpdates".into())
            .or_default() += 1.0;
        // Gauges of the engine: its live instances and the Luau heap
        if timings["instances"].is_number() {
            insert_counter(&mut self.pending.counters, "instances", number(&timings["instances"]));
        }
        if timings["luauHeapBytes"].is_number() {
            insert_counter(&mut self.pending.counters, "luauHeapMB", number(&timings["luauHeapBytes"]) / 1_048_576.0);
        }
        if timings["processBytes"].is_number() {
            insert_counter(&mut self.pending.counters, "processMB", number(&timings["processBytes"]) / 1_048_576.0);
        }
        if let Some(counters) = reply["counters"].as_object() {
            for (key, value) in counters.iter().take(MAX_COUNTERS) {
                if let Some(value) = value.as_f64().filter(|value| value.is_finite()) {
                    insert_counter(&mut self.pending.counters, key, value);
                }
            }
            for group in ["inputDetail", "simulationDetail"] {
                if let Some(details) = counters.get(group).and_then(Value::as_object) {
                    for (key, value) in details.iter().take(MAX_COUNTERS) {
                        if let Some(value) = value.as_f64().filter(|value| value.is_finite()) {
                            insert_counter(
                                &mut self.pending.counters,
                                &format!("{group}/{key}"),
                                value,
                            );
                        }
                    }
                }
            }
        }
        for (key, prefix) in [
            ("inputParts", "Input"),
            ("simulationParts", "Simulation"),
            ("exportParts", "Export build"),
        ] {
            if let Some(parts) = timings[key].as_object() {
                for (name, duration) in parts.iter().take(MAX_PHASES) {
                    add_phase(
                        &mut self.pending.phases,
                        &format!("{prefix}/{}", bounded(name)),
                        number(duration) * 1000.0,
                    );
                }
            }
        }
        if timings["textureProbeTime"].is_number() {
            add_phase(
                &mut self.pending.phases,
                "Export build/traversal/texture probes (included)",
                seconds("textureProbeTime"),
            );
        }
        if timings["textureProbeCount"].is_number() {
            insert_counter(
                &mut self.pending.counters,
                "exportDetail/textureProbes",
                number(&timings["textureProbeCount"]),
            );
        }
        for (key, phase) in [
            ("inputCallbacks", "input"),
            ("simulationCallbacks", "simulation"),
        ] {
            if let Some(callbacks) = timings[key].as_array() {
                let remaining = MAX_CALLBACKS.saturating_sub(self.pending.callbacks.len());
                for sample in callbacks.iter().take(remaining) {
                    self.pending.callbacks.push(CallbackSample {
                        phase: phase.into(),
                        name: bounded(sample["name"].as_str().unwrap_or("callback")),
                        source: bounded(sample["source"].as_str().unwrap_or("unknown")),
                        line: sample["line"].as_u64().unwrap_or(0).min(u32::MAX as u64) as u32,
                        duration_ms: number(&sample["duration"]) * 1000.0,
                    });
                }
            }
        }
    }

    pub fn selected(&self) -> Option<&FrameRecord> {
        self.selection.and_then(|id| self.get(id))
    }

    pub fn select(&mut self, id: u64) {
        if self.get(id).is_some() {
            self.set_paused(true);
            self.selection = Some(id);
        }
    }

    fn get(&self, id: u64) -> Option<&FrameRecord> {
        let first = self.records.front().map(|frame| frame.id).unwrap_or(0);
        id.checked_sub(first)
            .and_then(|index| self.records.get(index as usize))
            .filter(|frame| frame.id == id)
            .or_else(|| {
                [&self.worst_draw, &self.worst_runtime, &self.worst_gap]
                    .into_iter()
                    .filter_map(Option::as_ref)
                    .find(|frame| frame.id == id)
            })
    }

    fn worst(&self) -> Option<&FrameRecord> {
        match self.channel {
            Channel::Draw => self.worst_draw.as_ref(),
            Channel::Runtime => self.worst_runtime.as_ref(),
            Channel::All => [&self.worst_draw, &self.worst_runtime]
                .into_iter()
                .filter_map(Option::as_ref)
                .max_by(|left, right| {
                    self.channel
                        .cpu(&left.frame)
                        .total_cmp(&self.channel.cpu(&right.frame))
                }),
        }
    }

    /// A fixed-height panel. Live mode paints only a bounded visible chart and static labels.
    pub fn show(&mut self, parent: &mut Ui) {
        let (rect, _) = parent
            .allocate_exact_size(vec2(parent.available_width(), PANEL_HEIGHT), Sense::hover());
        let mut ui = parent.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        ui.set_clip_rect(rect.intersect(parent.clip_rect()));
        ui.horizontal(|ui| {
            if ui
                .add_sized(
                    [90.0, 24.0],
                    egui::Button::new(if self.paused { "Resume" } else { "Pause" }),
                )
                .on_hover_text("Ctrl+P")
                .clicked()
            {
                self.set_paused(!self.paused);
            }
            if ui.button("Clear").clicked() {
                self.clear();
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Worst frame").clicked() {
                if let Some(id) = self.worst().map(|frame| frame.id) {
                    self.select(id);
                }
            }
            if ui.button("Worst active gap").clicked() {
                if let Some(id) = self.worst_gap.as_ref().map(|frame| frame.id) {
                    self.select(id);
                }
            }
        });
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("profiler-channel")
                .width(105.0)
                .selected_text(self.channel.label())
                .show_ui(ui, |ui| {
                    for channel in [Channel::All, Channel::Draw, Channel::Runtime] {
                        ui.selectable_value(&mut self.channel, channel, channel.label());
                    }
                });
            egui::ComboBox::from_id_salt("profiler-metric")
                .width(130.0)
                .selected_text(if self.interval_metric {
                    "Active draw interval"
                } else {
                    "CPU duration"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.interval_metric, true, "Active draw interval");
                    ui.selectable_value(&mut self.interval_metric, false, "CPU duration");
                });
        });
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("profiler-scale")
                .width(120.0)
                .selected_text(if self.scale_ms == 0.0 {
                    "Grow to fit".into()
                } else {
                    format!("{:.0} ms scale", self.scale_ms)
                })
                .show_ui(ui, |ui| {
                    for scale in [8.34, 16.67, 33.34, 66.68, 133.36, 266.72] {
                        ui.selectable_value(
                            &mut self.scale_ms,
                            scale,
                            format!("{scale:.0} ms scale"),
                        );
                    }
                    ui.selectable_value(&mut self.scale_ms, 0.0, "Grow to fit");
                });
            egui::ComboBox::from_id_salt("profiler-window")
                .width(120.0)
                .selected_text(format!("{} frames", self.view_count))
                .show_ui(ui, |ui| {
                    for count in [120, 240, 600, 3600] {
                        ui.selectable_value(&mut self.view_count, count, format!("{count} frames"));
                    }
                });
        });
        self.chart(&mut ui);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.paused, egui::Button::new("Previous"))
                .clicked()
            {
                self.step(-1);
            }
            if ui
                .add_enabled(self.paused, egui::Button::new("Next"))
                .clicked()
            {
                self.step(1);
            }
            ui.label(if self.paused {
                "Paused — select a frame"
            } else {
                "Recording — click a frame to inspect"
            });
        });
        if let (Some(first), Some(last)) = (self.records.front(), self.records.back()) {
            let mut id = self.selection.unwrap_or(last.id).clamp(first.id, last.id);
            if ui
                .add(egui::Slider::new(&mut id, first.id..=last.id).show_value(false))
                .changed()
            {
                self.select(id);
            }
        } else {
            ui.add_enabled(
                false,
                egui::Slider::new(&mut 0_u64, 0..=0).show_value(false),
            );
        }
        ui.label(
            egui::RichText::new("CPU draw submission ≠ GPU execution. Runtime phases may overlap.")
                .small(),
        );
        if self.paused {
            if let Some(record) = self.selected() {
                egui::ScrollArea::vertical()
                    .id_salt("profiler-details")
                    .auto_shrink([false, false])
                    .max_height((rect.bottom() - ui.cursor().top()).max(0.0))
                    .show(&mut ui, |ui| {
                        ui.label(format!("Frame {} · {}", record.id, record.frame.story));
                        for (name, value) in phase_totals(&record.frame) {
                            ui.monospace(format!("{name}: {value:.3} ms"));
                        }
                        if let Some(gap) = record.active_interval_ms {
                            ui.monospace(format!("Active draw interval: {gap:.3} ms"));
                        }
                        if let Some(gpu) = record.frame.gpu_ms {
                            ui.monospace(format!("GPU timestamp duration: {gpu:.3} ms"));
                        } else {
                            ui.label("GPU execution: not measured");
                        }
                        for phase in &record.frame.phases {
                            ui.monospace(format!("{}: {:.3} ms", phase.name, phase.duration_ms));
                        }
                        for (key, value) in &record.frame.counters {
                            ui.monospace(format!("{key}: {value}"));
                        }
                        for sample in &record.frame.callbacks {
                            ui.monospace(format!(
                                "{} / {}: {:.3} ms · {}:{}",
                                sample.phase,
                                sample.name,
                                sample.duration_ms,
                                sample.source,
                                sample.line
                            ));
                        }
                    });
            }
        }
    }

    fn chart(&mut self, ui: &mut Ui) {
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 132.0), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, Color32::from_rgb(12, 17, 25));
        // The frames shown: the newest ones, or from zoom_start while zoomed in (Ctrl + wheel)
        let visible = ((self.view_count as f32 / self.zoom).round() as usize).clamp(1, self.view_count);
        let first_id = self.records.front().map_or(0, |record| record.id);
        let offset = match self.zoom_start {
            Some(id) => (id.saturating_sub(first_id) as usize).min(self.records.len().saturating_sub(visible)),
            None => self.records.len().saturating_sub(visible),
        };
        let count = (self.records.len() - offset).min(visible);
        if self.scale_ms == 0.0 {
            for record in self.records.iter().skip(offset).take(count) {
                self.grow_ceiling_ms = self.grow_ceiling_ms.max(self.chart_value(record) * 1.1);
            }
        }
        let ceiling = if self.scale_ms == 0.0 {
            self.grow_ceiling_ms
        } else {
            self.scale_ms
        };
        let y =
            |value: f64| rect.bottom() - (value / ceiling).clamp(0.0, 1.0) as f32 * rect.height();
        let budget_y = y(1000.0 / 120.0);
        painter.line_segment(
            [pos2(rect.left(), budget_y), pos2(rect.right(), budget_y)],
            Stroke::new(1.0, Color32::from_gray(75)),
        );
        painter.text(
            pos2(rect.left() + 4.0, budget_y - 2.0),
            egui::Align2::LEFT_BOTTOM,
            "120 fps · 8.33 ms",
            egui::FontId::monospace(10.0),
            Color32::GRAY,
        );
        let slot = rect.width() / visible.max(1) as f32;
        let start_x = if self.zoom_start.is_some() {
            rect.left()
        } else {
            rect.right() - count as f32 * slot
        };
        for (index, record) in self.records.iter().skip(offset).take(count).enumerate() {
            let x = start_x + (index as f32 + 0.5) * slot;
            let value = self.chart_value(record);
            let color = if self.selection == Some(record.id) {
                Color32::WHITE
            } else if value > 1000.0 / 120.0 {
                Color32::from_rgb(237, 158, 84)
            } else {
                Color32::from_rgb(100, 161, 240)
            };
            painter.line_segment(
                [pos2(x, rect.bottom()), pos2(x, y(value))],
                Stroke::new((slot * 0.8).clamp(1.0, 48.0), color),
            );
            if self.channel == Channel::All && self.interval_metric && record.frame.runtime_ms > 0.0
            {
                painter.circle_filled(
                    pos2(x, y(record.frame.runtime_ms)),
                    1.5,
                    Color32::from_rgb(187, 127, 244),
                );
            }
        }
        if response.hovered() && !self.records.is_empty() {
            let (zoom, pan) = ui.input(|input| (input.zoom_delta(), input.smooth_scroll_delta));
            let pointer = response.hover_pos().map_or(rect.center().x, |pos| pos.x);
            if zoom != 1.0 {
                // Keep the frame under the pointer where it is
                let under = offset as f32 + (pointer - start_x) / slot;
                let max_zoom = (self.view_count as f32 / 8.0).max(1.0);
                self.zoom = (self.zoom * zoom).clamp(1.0, max_zoom);
                if self.zoom <= 1.0 {
                    self.zoom_start = None;
                } else {
                    let shown = self.view_count as f32 / self.zoom;
                    let start = (under - (pointer - rect.left()) / rect.width() * shown).round().max(0.0) as u64;
                    self.zoom_start = Some(first_id + start);
                }
            } else if self.zoom > 1.0 && (pan.y != 0.0 || pan.x != 0.0) {
                // A plain wheel pans while zoomed in (up / left: older frames)
                let frames = (-(pan.y + pan.x) / slot.max(1.0)).round() as i64;
                let start = (offset as i64 + frames).clamp(0, self.records.len().saturating_sub(visible) as i64) as u64;
                self.zoom_start = Some(first_id + start);
            }
            if zoom != 1.0 || (self.zoom > 1.0 && (pan.y != 0.0 || pan.x != 0.0)) {
                ui.ctx().request_repaint();
            }
        }
        if (response.clicked() || response.dragged()) && count > 0 {
            if let Some(pointer) = response.interact_pointer_pos() {
                let index = ((pointer.x - start_x) / slot).floor().max(0.0) as usize;
                let id = self.records[offset + index.min(count - 1)].id;
                // (index counts from the first frame shown)
                self.select(id);
                response.request_focus();
            }
        }
        if response.has_focus() && self.paused {
            if ui.input(|input| input.key_pressed(egui::Key::ArrowLeft)) {
                self.step(-1);
            }
            if ui.input(|input| input.key_pressed(egui::Key::ArrowRight)) {
                self.step(1);
            }
        }
    }

    fn chart_value(&self, record: &FrameRecord) -> f64 {
        if self.interval_metric && self.channel != Channel::Runtime {
            record.active_interval_ms.unwrap_or(0.0)
        } else {
            self.channel.cpu(&record.frame)
        }
    }

    fn step(&mut self, direction: i32) {
        let selected = self
            .selection
            .unwrap_or_else(|| self.records.back().map_or(0, |record| record.id));
        let accepts = |record: &&FrameRecord| {
            self.channel != Channel::Runtime
                || record
                    .frame
                    .counters
                    .get("runtimeUpdates")
                    .copied()
                    .unwrap_or(0.0)
                    > 0.0
                || record.frame.runtime_ms > 0.0
        };
        let next = if direction < 0 {
            self.records
                .iter()
                .rev()
                .filter(accepts)
                .find(|record| record.id < selected)
        } else {
            self.records
                .iter()
                .filter(accepts)
                .find(|record| record.id > selected)
        };
        if let Some(id) = next.map(|record| record.id) {
            self.select(id);
        }
    }

    /// Writes the trace (trace()) to `path`, one record at a time: a full history is too large to build
    /// as one JSON value first
    pub fn export(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let mut writer = BufWriter::new(File::create(path)?);
        let records = self.export_records();
        let json = |writer: &mut BufWriter<File>, value: &Value| serde_json::to_writer(writer, value).map_err(io::Error::other);
        writer.write_all(b"{\"displayTimeUnit\":\"ms\",\"traceEvents\":[")?;
        json(&mut writer, &process_event())?;
        for record in &records {
            for event in record_events(record) {
                writer.write_all(b",")?;
                json(&mut writer, &event)?;
            }
        }
        write!(writer, "],\"rur\":{{\"version\":2,\"mode\":\"native\",\"paused\":{},\"frames\":[", self.paused)?;
        for (index, record) in records.iter().enumerate() {
            if index > 0 {
                writer.write_all(b",")?;
            }
            json(&mut writer, &frame_json(record))?;
        }
        writer.write_all(b"],\"metadata\":")?;
        serde_json::to_writer(&mut writer, &self.metadata).map_err(io::Error::other)?;
        writer.write_all(b",\"note\":")?;
        serde_json::to_writer(&mut writer, TRACE_NOTE).map_err(io::Error::other)?;
        writer.write_all(b"}}")?;
        writer.flush()
    }

    /// The history and the worst frames, in frame order
    fn export_records(&self) -> Vec<&FrameRecord> {
        let mut records: Vec<&FrameRecord> = self.records.iter().collect();
        for worst in [&self.worst_draw, &self.worst_runtime, &self.worst_gap]
            .into_iter()
            .filter_map(Option::as_ref)
        {
            if !records.iter().any(|record| record.id == worst.id) {
                records.push(worst);
            }
        }
        records.sort_unstable_by_key(|record| record.id);
        records
    }

    pub fn trace(&self) -> Value {
        let records = self.export_records();
        let mut events = vec![process_event()];
        let mut frames = Vec::with_capacity(records.len());
        for record in records {
            events.extend(record_events(record));
            frames.push(frame_json(record));
        }
        json!({"displayTimeUnit":"ms","traceEvents":events,"rur":{"version":2,"mode":"native","paused":self.paused,
            "frames":frames,"metadata":self.metadata,"note":TRACE_NOTE}})
    }
}

const TRACE_NOTE: &str = "One record per native display callback. Draw submission is CPU work, not GPU execution or physical display presentation. Runtime phases are accumulated duration totals attached to the next display callback; they may overlap and are not clock-synchronized spans. Export build subdivisions include layoutEnsure (already counted in Layout); traversal includes texture probe time, and encoded transports may also include serialization. Subdivisions must not be added to parent totals. Active intervals exclude idle transitions, story/session changes and pauses. GPU duration is absent unless measured using GPU timestamp queries (the desktop app samples every drawn frame; native.gpu_ms is the sample that completed during that frame, from an earlier submission; counters.gpuSubmitToCompleteMs is the wall time from a submission to the GPU finishing it, queueing behind other GPU work included). Desktop subphases 'Previous callback/...' split the interval: the previous update, eframe/egui CPU around it (eframe's cpu_usage minus the update), and the rest (egui-wgpu's surface acquire, queue submit and present, which cpu_usage leaves out, and the event loop); 'Update/glyph wait' is time this update blocked on glyph rasterization. Scene values stay in process; no browser or HTTP transport is involved.";

fn process_event() -> Value {
    json!({"name":"process_name","ph":"M","pid":1,"tid":0,"args":{"name":"rur native"}})
}

/// A frame record's trace events
fn record_events(record: &FrameRecord) -> Vec<Value> {
    let frame = &record.frame;
    let phases: BTreeMap<_, _> = phase_totals(frame).into_iter().collect();
    let start = (frame.timestamp_ms - frame.cpu_draw_ms).max(0.0);
    let args = json!({"id":record.id,"story":frame.story,"renderer":"wgpu","kind":"presentation",
        "phasesMs":phases,"counters":frame.counters,"callbackSamples":frame.callbacks,"subphases":frame.phases});
    let mut events = vec![json!({"name":"Draw submission","cat":"native","ph":"X","pid":1,"tid":3,
        "ts":start * 1000.0,"dur":frame.cpu_draw_ms * 1000.0,"args":args})];
    if let Some(interval) = record.active_interval_ms {
        events.push(json!({"name":"Active draw interval","cat":"presentation","ph":"X","pid":1,"tid":4,
            "ts":(frame.timestamp_ms - interval).max(0.0) * 1000.0,"dur":interval * 1000.0,"args":{"frame":record.id}}));
    }
    if frame.runtime_ms > 0.0 || frame.counters.get("runtimeUpdates").copied().unwrap_or(0.0) > 0.0 {
        events.push(json!({"name":"Runtime phase totals (ms)","cat":"native","ph":"C","pid":1,"tid":1,
            "ts":frame.timestamp_ms * 1000.0,"args":phases}));
    }
    if !frame.phases.is_empty() {
        let subdivisions: BTreeMap<_, _> = frame
            .phases
            .iter()
            .map(|phase| (phase.name.as_str(), phase.duration_ms))
            .collect();
        events.push(json!({"name":"Frame subdivisions (ms; overlapping)","cat":"native","ph":"C","pid":1,"tid":6,
            "ts":frame.timestamp_ms * 1000.0,"args":subdivisions}));
    }
    if let Some(gpu_ms) = frame.gpu_ms {
        // A duration query does not establish a GPU/CPU clock synchronization.
        events.push(json!({"name":"GPU timestamp duration (ms)","cat":"gpu","ph":"C","pid":1,"tid":5,
            "ts":frame.timestamp_ms * 1000.0,"args":{"durationMs":gpu_ms}}));
    }
    events
}

/// A frame record as the trace's rur.frames lists it
fn frame_json(record: &FrameRecord) -> Value {
    let frame = &record.frame;
    let phases: BTreeMap<_, _> = phase_totals(frame).into_iter().collect();
    let start = (frame.timestamp_ms - frame.cpu_draw_ms).max(0.0);
    json!({"id":record.id,"kind":"presentation","story":frame.story,"renderer":"wgpu",
        "start":start,"duration":frame.cpu_draw_ms,"interval":frame.frame_interval_ms,"activeInterval":record.active_interval_ms,
        "phases":phases,"counters":frame.counters,"native":frame})
}

/// Nested draw diagnostics are already included in their parent CPU categories.
pub fn append_draw_details(
    stats: &crate::render::DrawStats,
    phases: &mut Vec<PhaseSample>,
    counters: &mut BTreeMap<Name, f64>,
) {
    for (name, duration) in [
        ("Draw other/setup (included)", stats.setup_ms),
        ("Draw other/finalize (included)", stats.finalize_ms),
        (
            "Draw other/record capture (included)",
            stats.record_capture_ms,
        ),
        (
            "Draw other/record insert (included)",
            stats.record_insert_ms,
        ),
        (
            "Draw preparation/image cache raster (included)",
            stats.image_cache_stats.raster_ms,
        ),
        (
            "Draw preparation/image cache mips (included)",
            stats.image_cache_stats.mip_ms,
        ),
    ] {
        add_phase(phases, name, duration);
    }
    for (name, value) in [
        ("changedNodes", stats.changed_nodes as f64),
        ("offscreenNodes", stats.offscreen_nodes as f64),
        ("displayListHits", stats.display_list_hits as f64),
        ("retainedWalkHits", stats.retained_walk_hits as f64),
        ("assetsChanged", stats.assets_changed as u8 as f64),
        ("walkRetained", stats.walk_retained as u8 as f64),
        ("exactShadows", stats.exact_shadows as f64),
    ] {
        insert_counter(counters, &format!("draw/{name}"), value);
    }
    let cache = &stats.image_cache_stats;
    for (name, value) in [
        ("screenHits", cache.screen_hits as f64),
        ("screenMisses", cache.screen_misses as f64),
        ("groupHits", cache.group_hits as f64),
        ("groupMisses", cache.group_misses as f64),
        ("mipHits", cache.mip_hits as f64),
        ("mipMisses", cache.mip_misses as f64),
        ("layerEvictions", cache.layer_evictions as f64),
        ("mipEvictions", cache.mip_evictions as f64),
        ("rasterPixels", cache.raster_pixels as f64),
        ("layerEntries", cache.layer_entries as f64),
        ("mipEntries", cache.mip_entries as f64),
        ("layerBytes", cache.layer_bytes as f64),
        ("mipBytes", cache.mip_bytes as f64),
    ] {
        insert_counter(counters, &format!("imageCache/{name}"), value);
    }
}

fn number(value: &Value) -> f64 {
    finite(value.as_f64().unwrap_or(0.0))
}
fn finite(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}
fn bounded(value: &str) -> String {
    value.chars().take(MAX_TEXT).collect()
}

fn insert_counter(counters: &mut BTreeMap<Name, f64>, name: &str, value: f64) {
    let name = bounded(name);
    if counters.contains_key(name.as_str()) || counters.len() < MAX_COUNTERS {
        counters.insert(name.into(), value);
    }
}

fn add_phase(phases: &mut Vec<PhaseSample>, name: &str, duration_ms: f64) {
    if let Some(phase) = phases.iter_mut().find(|phase| phase.name == name) {
        phase.duration_ms += duration_ms;
    } else if phases.len() < MAX_PHASES {
        phases.push(PhaseSample {
            name: bounded(name).into(),
            duration_ms,
        });
    }
}

fn merge(frame: &mut ProfileFrame, pending: ProfileFrame) {
    frame.runtime_ms += pending.runtime_ms;
    frame.layout_ms += pending.layout_ms;
    frame.export_ms += pending.export_ms;
    frame.serialize_ms += pending.serialize_ms;
    frame.input_ms += pending.input_ms;
    frame.scheduler_ms += pending.scheduler_ms;
    for (key, value) in pending.counters {
        insert_counter(&mut frame.counters, &key, value);
    }
    for phase in pending.phases {
        add_phase(&mut frame.phases, &phase.name, phase.duration_ms);
    }
    frame.callbacks.extend(
        pending
            .callbacks
            .into_iter()
            .take(MAX_CALLBACKS.saturating_sub(frame.callbacks.len())),
    );
}

fn sanitize(frame: &mut ProfileFrame) {
    for value in [
        &mut frame.timestamp_ms,
        &mut frame.frame_interval_ms,
        &mut frame.cpu_draw_ms,
        &mut frame.runtime_ms,
        &mut frame.layout_ms,
        &mut frame.export_ms,
        &mut frame.serialize_ms,
        &mut frame.input_ms,
        &mut frame.scheduler_ms,
    ] {
        *value = finite(*value);
    }
    frame.gpu_ms = frame
        .gpu_ms
        .filter(|value| value.is_finite() && *value >= 0.0);
    truncate_name(&mut frame.story);
    frame.phases.truncate(MAX_PHASES);
    frame.callbacks.truncate(MAX_CALLBACKS);
    for phase in &mut frame.phases {
        truncate_name(&mut phase.name);
        phase.duration_ms = finite(phase.duration_ms);
    }
    for sample in &mut frame.callbacks {
        truncate_text(&mut sample.phase);
        truncate_text(&mut sample.name);
        truncate_text(&mut sample.source);
        sample.duration_ms = finite(sample.duration_ms);
    }
    frame.counters.retain(|_, value| value.is_finite());
    while frame.counters.len() > MAX_COUNTERS {
        frame.counters.pop_last();
    }
    if frame
        .counters
        .keys()
        .any(|name| name.chars().nth(MAX_TEXT).is_some())
    {
        frame.counters = std::mem::take(&mut frame.counters)
            .into_iter()
            .map(|(mut name, value)| {
                truncate_name(&mut name);
                (name, value)
            })
            .collect();
    }
}

fn truncate_name(name: &mut Name) {
    if name.len() > MAX_TEXT && name.chars().nth(MAX_TEXT).is_some() {
        *name = bounded(name).into();
    }
}

fn truncate_text(text: &mut String) {
    if text.len() > MAX_TEXT {
        if let Some((byte, _)) = text.char_indices().nth(MAX_TEXT) {
            text.truncate(byte);
        }
    }
}

fn phase_totals(frame: &ProfileFrame) -> Vec<(&'static str, f64)> {
    let phase = |name: &str| {
        frame
            .phases
            .iter()
            .filter(|phase| phase.name == name)
            .map(|phase| phase.duration_ms)
            .sum::<f64>()
    };
    vec![
        // The desktop app's interval attribution (app.rs update_frame): the previous callback's egui /
        // eframe CPU, and its surface acquire + submit + present + event loop (outside eframe's CPU time)
        ("egui CPU (previous callback)", phase("Previous callback/egui and eframe CPU")),
        (
            "Present path and event loop (previous callback)",
            phase("Previous callback/acquire, submit, present and event loop"),
        ),
        ("Glyph wait", phase("Update/glyph wait (included in receive)")),
        ("Draw CPU", frame.cpu_draw_ms),
        ("Runtime CPU", frame.runtime_ms),
        ("Input", frame.input_ms),
        ("Simulation", frame.scheduler_ms),
        ("Layout", frame.layout_ms),
        ("Export", frame.export_ms),
        ("Serialize", frame.serialize_ms),
        ("Display callback interval", frame.frame_interval_ms),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(timestamp: f64, duration: f64) -> ProfileFrame {
        ProfileFrame {
            timestamp_ms: timestamp,
            cpu_draw_ms: duration,
            story: "story".into(),
            ..Default::default()
        }
    }

    #[test]
    fn ring_is_bounded_and_worst_remains_inspectable() {
        let mut profiler = Profiler::with_capacity(3);
        profiler.record(frame(100.0, 90.0));
        let allocated = profiler.records.capacity();
        for index in 1..100 {
            profiler.record(frame(100.0 + index as f64 * 10.0, 2.0));
        }
        assert_eq!(profiler.len(), 3);
        assert_eq!(profiler.records.capacity(), allocated);
        profiler.select(1);
        assert_eq!(profiler.selected().unwrap().frame.cpu_draw_ms, 90.0);
        assert_eq!(
            profiler.trace()["rur"]["frames"].as_array().unwrap().len(),
            4
        );
        profiler.clear();
        assert!(profiler.is_empty());
        assert!(profiler.selected().is_none());
        assert_eq!(profiler.metadata["mode"], "native");
    }

    #[test]
    fn active_gaps_exclude_idle_story_session_and_pause_transitions() {
        let mut profiler = Profiler::default();
        let mut value = frame(1.0, 1.0);
        profiler.record(value.clone());
        value.timestamp_ms = 5000.0;
        value.scroll_active = true;
        profiler.record(value.clone());
        assert!(
            profiler
                .records
                .back()
                .unwrap()
                .active_interval_ms
                .is_none()
        );
        value.timestamp_ms += 130.0;
        profiler.record(value.clone());
        assert_eq!(
            profiler.records.back().unwrap().active_interval_ms,
            Some(130.0)
        );
        value.timestamp_ms += 9000.0;
        value.scroll_session = Some(1);
        profiler.record(value.clone());
        assert!(
            profiler
                .records
                .back()
                .unwrap()
                .active_interval_ms
                .is_none()
        );
        value.timestamp_ms += 9000.0;
        value.story = "other".into();
        profiler.record(value.clone());
        assert!(
            profiler
                .records
                .back()
                .unwrap()
                .active_interval_ms
                .is_none()
        );
        profiler.set_paused(true);
        let count = profiler.len();
        profiler.record(value.clone());
        assert_eq!(profiler.len(), count);
        profiler.set_paused(false);
        value.timestamp_ms += 9000.0;
        profiler.record(value);
        assert!(
            profiler
                .records
                .back()
                .unwrap()
                .active_interval_ms
                .is_none()
        );
        assert_eq!(
            profiler.worst_gap.as_ref().unwrap().active_interval_ms,
            Some(130.0)
        );
    }

    #[test]
    fn runtime_snapshots_are_consumed_once_and_never_retain_scene_data() {
        let mut profiler = Profiler::default();
        let reply = json!({"timings":{"frameTotal":0.004,"layout":0.001,"simulationParts":{"timers":0.002},
            "inputCallbacks":[{"name":"signal","source":"source.luau","line":3,"duration":0.0005}]},
            "counters":{"nodes":500,"inputEvents":1,"inputDetail":{"dispatch":2}},"scene":{"nodes":[{"sentinel":"DO_NOT_RETAIN"}]}});
        profiler.record_runtime(&reply);
        profiler.record_runtime(&reply);
        profiler.record(frame(100.0, 1.0));
        let first = profiler.records.back().unwrap();
        assert_eq!(first.frame.runtime_ms, 8.0);
        assert_eq!(first.frame.layout_ms, 2.0);
        assert_eq!(first.frame.counters["runtimeUpdates"], 2.0);
        assert_eq!(first.frame.phases[0].duration_ms, 4.0);
        assert_eq!(first.frame.callbacks.len(), 2);
        assert!(!profiler.trace().to_string().contains("DO_NOT_RETAIN"));
        profiler.record(frame(110.0, 1.0));
        assert_eq!(profiler.records.back().unwrap().frame.runtime_ms, 0.0);
        profiler.set_paused(true);
        profiler.record_runtime(&reply);
        profiler.set_paused(false);
        profiler.record(frame(120.0, 1.0));
        assert!(profiler.records.back().unwrap().frame.callbacks.is_empty());
    }

    #[test]
    fn export_subdivisions_survive_selection_and_trace_without_inflating_totals() {
        let mut profiler = Profiler::default();
        profiler.record_runtime(&json!({"timings":{
            "frameTotal":0.010,"export":0.006,"layout":0.001,
            "exportParts":{"layoutEnsure":0.001,"traversal":0.005,"images":0.001},
            "textureProbeTime":0.002,"textureProbeCount":3
        }}));
        profiler.record(frame(100.0, 1.0));
        let id = profiler.records.back().unwrap().id;
        profiler.select(id);
        let selected = &profiler.selected().unwrap().frame;
        assert_eq!(selected.runtime_ms, 10.0);
        assert_eq!(selected.export_ms, 6.0);
        assert_eq!(selected.layout_ms, 1.0);
        assert_eq!(selected.counters["exportDetail/textureProbes"], 3.0);
        assert!(
            selected
                .phases
                .iter()
                .any(|phase| phase.name == "Export build/traversal" && phase.duration_ms == 5.0)
        );
        assert!(
            selected
                .phases
                .iter()
                .any(|phase| phase.name.ends_with("texture probes (included)")
                    && phase.duration_ms == 2.0)
        );
        let trace = profiler.trace();
        let subdivisions = trace["traceEvents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["name"] == "Frame subdivisions (ms; overlapping)")
            .unwrap();
        assert_eq!(subdivisions["args"]["Export build/traversal"], 5.0);
        assert_eq!(trace["rur"]["frames"][0]["phases"]["Export"], 6.0);
    }

    #[test]
    fn combined_draw_runtime_diagnostics_keep_export_and_image_details() {
        let mut profiler = Profiler::default();
        let mut draw = frame(100.0, 8.0);
        for i in 0..29 {
            draw.counters.insert(format!("draw{i}").into(), i as f64);
        }
        for i in 0..11 {
            add_phase(&mut draw.phases, &format!("draw{i}"), 0.1);
        }
        let mut stats = crate::render::DrawStats::default();
        stats.record_capture_ms = 0.7;
        stats.image_cache_stats.mip_misses = 4;
        append_draw_details(&stats, &mut draw.phases, &mut draw.counters);
        let mut runtime_counters = serde_json::Map::new();
        let mut parts = serde_json::Map::new();
        for i in 0..32 {
            runtime_counters.insert(format!("runtime{i}"), json!(i));
        }
        for i in 0..20 {
            parts.insert(format!("stage{i}"), json!(0.001));
        }
        profiler.record_runtime(&json!({"counters":runtime_counters,"timings":{
            "frameTotal":0.030,"simulationParts":parts,"export":0.005,
            "exportParts":{"traversal":0.004},"textureProbeTime":0.002,"textureProbeCount":2
        }}));
        profiler.record(draw);
        let record = profiler.records.back().unwrap();
        assert_eq!(record.frame.counters["runtime31"], 31.0);
        assert_eq!(record.frame.counters["imageCache/mipMisses"], 4.0);
        assert_eq!(record.frame.counters["exportDetail/textureProbes"], 2.0);
        assert!(
            record
                .frame
                .phases
                .iter()
                .any(|p| p.name == "Export build/traversal" && p.duration_ms == 4.0)
        );
        assert!(
            record
                .frame
                .phases
                .iter()
                .any(|p| p.name == "Draw other/record capture (included)" && p.duration_ms == 0.7)
        );
        assert_eq!(record.frame.runtime_ms, 30.0);
        assert_eq!(record.frame.cpu_draw_ms, 8.0);
        assert_eq!(record.frame.export_ms, 5.0);
        assert!(record.frame.phases.len() > 32);
        assert!(record.frame.counters.len() > 64);
    }

    #[test]
    fn snapshot_sizes_and_nonfinite_values_are_bounded() {
        let mut profiler = Profiler::default();
        let mut value = frame(100.0, f64::NAN);
        value.gpu_ms = Some(f64::INFINITY);
        value.story = "界".repeat(1000).into();
        value.callbacks = vec![
            CallbackSample {
                source: "x".repeat(10000),
                ..Default::default()
            };
            100
        ];
        value.counters = (0..100)
            .map(|index| (format!("counter{index}").into(), index as f64))
            .collect();
        profiler.record(value);
        let record = &profiler.records[0];
        assert_eq!(record.frame.cpu_draw_ms, 0.0);
        assert_eq!(record.frame.gpu_ms, None);
        assert_eq!(record.frame.story.chars().count(), MAX_TEXT);
        assert_eq!(record.frame.callbacks.len(), MAX_CALLBACKS);
        assert_eq!(record.frame.callbacks[0].source.len(), MAX_TEXT);
        assert_eq!(record.frame.counters.len(), MAX_COUNTERS);
        assert_eq!(profiler.channel, Channel::All);
        assert_eq!(profiler.scale_ms, 16.67);
        assert!(!profiler.interval_metric);
    }

    #[test]
    fn names_are_shared_and_the_streamed_export_is_the_trace() {
        let a = Name::from("Draw backgrounds");
        let b = Name::from(String::from("Draw backgrounds"));
        assert!(Arc::ptr_eq(&a.0, &b.0));
        let mut profiler = Profiler::default();
        profiler.record_runtime(&json!({"timings":{"frameTotal":0.002,"simulationParts":{"timers":0.001}},"counters":{"nodes":3}}));
        let mut value = frame(100.0, 2.0);
        value.gpu_ms = Some(0.5);
        add_phase(&mut value.phases, "Draw text", 0.25);
        profiler.record(value);
        profiler.record(frame(110.0, 1.0));
        let path = std::env::temp_dir().join(format!("rur-trace-{}.json", std::process::id()));
        profiler.export(&path).unwrap();
        let written: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(written, profiler.trace());
    }

    #[test]
    fn trace_uses_microseconds_and_does_not_invent_gpu_spans() {
        let mut profiler = Profiler::default();
        profiler.record(frame(100.0, 2.0));
        let trace = profiler.trace();
        let draw = &trace["traceEvents"][1];
        assert_eq!(draw["name"], "Draw submission");
        assert_eq!(draw["ts"], 98000.0);
        assert_eq!(draw["dur"], 2000.0);
        assert!(
            !trace["traceEvents"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["cat"] == "gpu")
        );
        assert_eq!(trace["rur"]["version"], 2);
    }
}
