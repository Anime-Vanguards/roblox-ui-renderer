use egui::{Event, Key, Modifiers, MouseWheelUnit, PointerButton, Pos2, Rect};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Default)]
pub struct PreviewInput {
    pointer: Option<Pos2>,
    inside: bool,
    focused: bool,
    captured: BTreeSet<u8>,
    pressed_keys: BTreeSet<String>,
}

impl PreviewInput {
    pub fn collect(
        &mut self,
        events: &[Event],
        rect: Rect,
        clip_rect: Rect,
        viewport: [f64; 2],
        focused: bool,
        modifiers: Modifiers,
    ) -> Vec<Value> {
        let mut output = Vec::new();
        if self.focused && !focused {
            self.release(&mut output, rect, viewport);
        }
        self.focused = focused;
        if focused {
            self.sync_modifiers(modifiers, &mut output);
        }
        let has_paste = events.iter().any(|event| matches!(event, Event::Paste(_)));
        for event in events {
            match event {
                Event::PointerMoved(position) => {
                    self.pointer = Some(*position);
                    let inside = rect.contains(*position) && clip_rect.contains(*position);
                    if inside || !self.captured.is_empty() {
                        let [x, y] = logical(*position, rect, viewport);
                        push(&mut output, json!({"type":"move", "x":x, "y":y}));
                    } else if self.inside {
                        output.push(json!({"type":"leave"}));
                    }
                    self.inside = inside;
                }
                Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    ..
                } => {
                    self.pointer = Some(*pos);
                    let Some(button) = button_number(*button) else {
                        continue;
                    };
                    if (*pressed && rect.contains(*pos) && clip_rect.contains(*pos))
                        || (!*pressed && self.captured.contains(&button))
                    {
                        if *pressed {
                            self.captured.insert(button);
                        } else {
                            self.captured.remove(&button);
                        }
                        let [x, y] = logical(*pos, rect, viewport);
                        output.push(json!({"type":if *pressed {"down"} else {"up"}, "button":button, "x":x, "y":y}));
                    }
                }
                Event::PointerGone => {
                    if self.inside {
                        output.push(json!({"type":"leave"}));
                    }
                    self.inside = false;
                }
                Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                } if self
                    .pointer
                    .is_some_and(|p| rect.contains(p) && clip_rect.contains(p))
                    && !modifiers.ctrl
                    && !modifiers.command =>
                {
                    let [x, y] = logical(self.pointer.unwrap(), rect, viewport);
                    let notches = wheel_notches(
                        *unit,
                        delta.y as f64,
                        viewport[1] / rect.height().max(1.0) as f64,
                        viewport[1],
                    );
                    if notches != 0.0 {
                        output.push(json!({"type":"wheel", "x":x, "y":y, "dx":-delta.x, "dy":-delta.y, "notches":notches}));
                    }
                }
                Event::Key {
                    key,
                    physical_key,
                    pressed,
                    modifiers,
                    ..
                } if focused => {
                    self.sync_modifiers(*modifiers, &mut output);
                    let key = physical_key.unwrap_or(*key);
                    if has_paste && key == Key::V && (modifiers.ctrl || modifiers.command) {
                        continue;
                    }
                    let key = key_name(key);
                    if *pressed {
                        self.pressed_keys.insert(key.clone());
                    } else {
                        self.pressed_keys.remove(&key);
                    }
                    output.push(json!({"type":"key", "key":key, "down":pressed, "shift":modifiers.shift, "ctrl":modifiers.ctrl || modifiers.command, "alt":modifiers.alt}));
                }
                Event::Text(text) | Event::Ime(egui::ImeEvent::Commit(text)) if focused => {
                    output.push(json!({"type":"text", "text":text}))
                }
                Event::Paste(text) if focused => output.push(json!({"type":"paste", "text":text})),
                Event::Copy | Event::Cut if focused => {
                    let key = if matches!(event, Event::Copy) {
                        "C"
                    } else {
                        "X"
                    };
                    output.push(json!({"type":"key", "key":key, "down":true, "ctrl":true}));
                    output.push(json!({"type":"key", "key":key, "down":false, "ctrl":true}));
                }
                Event::WindowFocused(false) => {
                    self.release(&mut output, rect, viewport);
                    self.focused = false;
                }
                _ => {}
            }
        }
        if self.focused {
            self.sync_modifiers(modifiers, &mut output);
        }
        output
    }

    fn sync_modifiers(&mut self, modifiers: Modifiers, output: &mut Vec<Value>) {
        for (key, down) in [
            ("LeftShift", modifiers.shift),
            ("LeftControl", modifiers.ctrl || modifiers.command),
            ("LeftAlt", modifiers.alt),
        ] {
            if self.pressed_keys.contains(key) != down {
                if down {
                    self.pressed_keys.insert(key.into());
                } else {
                    self.pressed_keys.remove(key);
                }
                output.push(json!({"type":"key", "key":key, "down":down, "shift":modifiers.shift, "ctrl":modifiers.ctrl || modifiers.command, "alt":modifiers.alt}));
            }
        }
    }

    fn release(&mut self, output: &mut Vec<Value>, _rect: Rect, _viewport: [f64; 2]) {
        // Cancel outside the target: losing native focus must never complete a click.
        output.push(json!({"type":"leave"}));
        for button in std::mem::take(&mut self.captured) {
            output.push(json!({"type":"up", "button":button, "x":-1_000_000, "y":-1_000_000}));
        }
        for key in std::mem::take(&mut self.pressed_keys) {
            output.push(json!({"type":"key", "key":key, "down":false}));
        }
    }
}

pub fn logical(position: Pos2, rect: Rect, viewport: [f64; 2]) -> [f64; 2] {
    [
        (position.x - rect.min.x) as f64 / rect.width().max(1.0) as f64 * viewport[0],
        (position.y - rect.min.y) as f64 / rect.height().max(1.0) as f64 * viewport[1],
    ]
}

// Wheel input in notches. One Studio wheel unit moves the target
// CanvasPosition 100 logical px (VirtualInput / MCP, notes/interaction.md); no physical mouse notch
// was measured. A winit line delta is one physical tick (not the OS lines-per-notch setting); point
// deltas convert to logical px and stay fractional; a page is the viewport height. Every mode is
// then halved: a tuned travel distance (one tick = 50 px), not a measurement.
pub fn wheel_notches(
    unit: MouseWheelUnit,
    delta: f64,
    logical_per_point: f64,
    page_height: f64,
) -> f64 {
    if !delta.is_finite() {
        return 0.0;
    }
    0.5 * match unit {
        MouseWheelUnit::Line => delta,
        MouseWheelUnit::Point => delta * logical_per_point / 100.0,
        MouseWheelUnit::Page => delta * page_height / 100.0,
    }
}

/// Queues an input event for the next runtime frame. Consecutive pointer moves keep the last one. A
/// control change replaces a queued change of the same control: while the runtime is busy, a control
/// toggled several times is applied once, with its latest value, when the next frame goes out (an
/// earlier value nothing has shown would only cost a rebuild of whatever the story derives from it).
pub fn push(queue: &mut Vec<Value>, event: Value) {
    if event["type"] == "move" && queue.last().is_some_and(|last| last["type"] == "move") {
        *queue.last_mut().unwrap() = event;
    } else {
        if event["type"] == "control" {
            queue.retain(|queued| queued["type"] != "control" || queued["path"] != event["path"]);
        }
        queue.push(event);
    }
}

fn button_number(button: PointerButton) -> Option<u8> {
    match button {
        PointerButton::Primary => Some(1),
        PointerButton::Secondary => Some(2),
        PointerButton::Middle => Some(3),
        _ => None,
    }
}

fn key_name(key: Key) -> String {
    match key {
        Key::ArrowDown => "Down".into(),
        Key::ArrowUp => "Up".into(),
        Key::ArrowLeft => "Left".into(),
        Key::ArrowRight => "Right".into(),
        Key::Enter => "Return".into(),
        Key::OpenBracket => "LeftBracket".into(),
        Key::CloseBracket => "RightBracket".into(),
        Key::Backslash => "BackSlash".into(),
        Key::Backtick => "Backquote".into(),
        Key::Num0 => "Zero".into(),
        Key::Num1 => "One".into(),
        Key::Num2 => "Two".into(),
        Key::Num3 => "Three".into(),
        Key::Num4 => "Four".into(),
        Key::Num5 => "Five".into(),
        Key::Num6 => "Six".into(),
        Key::Num7 => "Seven".into(),
        Key::Num8 => "Eight".into(),
        Key::Num9 => "Nine".into(),
        _ => format!("{key:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_queued_control_change_is_replaced_by_a_later_one_of_the_same_control() {
        use serde_json::json;
        let mut queue = Vec::new();
        let control = |name: &str, value: Value| json!({"type":"control","path":[name],"value":value});
        push(&mut queue, control("isMatch", json!(true)));
        push(&mut queue, json!({"type":"move","x":1,"y":2}));
        push(&mut queue, control("showWindow", json!(false)));
        push(&mut queue, control("isMatch", json!(false)));
        push(&mut queue, control("isMatch", json!(true)));
        assert_eq!(
            queue,
            vec![json!({"type":"move","x":1,"y":2}), control("showWindow", json!(false)), control("isMatch", json!(true))]
        );
    }
    #[test]
    fn wheel_ticks_and_precision_pixels_preserve_units() {
        assert_eq!(wheel_notches(MouseWheelUnit::Line, -1.0, 3.0, 1080.0), -0.5);
        assert_eq!(
            wheel_notches(MouseWheelUnit::Point, -25.0, 2.0, 1080.0),
            -0.25
        );
        assert_eq!(
            wheel_notches(MouseWheelUnit::Line, f64::NAN, 1.0, 100.0),
            0.0
        );
    }
    #[test]
    fn fitted_preview_coordinates_and_drag_release() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), egui::vec2(500.0, 250.0));
        assert_eq!(
            logical(Pos2::new(260.0, 145.0), rect, [1000.0, 500.0]),
            [500.0, 250.0]
        );
        let mut state = PreviewInput::default();
        let down = Event::PointerButton {
            pos: rect.center(),
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        };
        assert_eq!(
            state.collect(
                &[down],
                rect,
                rect,
                [1000.0, 500.0],
                true,
                Modifiers::default()
            )[0]["type"],
            "down"
        );
        let up = Event::PointerButton {
            pos: Pos2::ZERO,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        };
        assert_eq!(
            state.collect(
                &[up],
                rect,
                rect,
                [1000.0, 500.0],
                true,
                Modifiers::default()
            )[0]["type"],
            "up"
        );
    }

    #[test]
    fn focus_loss_cancels_click_and_releases_modifiers() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 100.0));
        let mut input = PreviewInput::default();
        let modifiers = Modifiers {
            shift: true,
            ..Default::default()
        };
        input.collect(
            &[Event::PointerButton {
                pos: rect.center(),
                button: PointerButton::Primary,
                pressed: true,
                modifiers,
            }],
            rect,
            rect,
            [100.0, 100.0],
            true,
            modifiers,
        );
        let events = input.collect(&[], rect, rect, [100.0, 100.0], false, Modifiers::default());
        assert_eq!(events[0]["type"], "leave");
        let release = events.iter().find(|event| event["type"] == "up").unwrap();
        assert!(release["x"].as_i64().unwrap() < 0);
        assert!(
            events
                .iter()
                .any(|event| event["key"] == "LeftShift" && event["down"] == false)
        );
    }

    #[test]
    fn native_paste_does_not_also_paste_runtime_clipboard() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 100.0));
        let modifiers = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        let events = [
            Event::Key {
                key: Key::V,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            },
            Event::Paste("native clipboard".into()),
        ];
        let output =
            PreviewInput::default().collect(&events, rect, rect, [100.0, 100.0], true, modifiers);
        assert!(!output.iter().any(|event| event["key"] == "V"));
        assert_eq!(
            output
                .iter()
                .filter(|event| event["type"] == "paste")
                .count(),
            1
        );
    }

    #[test]
    fn native_keys_match_browser_physical_codes_and_roblox_punctuation_names() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 100.0));
        let event = Event::Key {
            key: Key::Q,
            physical_key: Some(Key::A),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::default(),
        };
        let events = PreviewInput::default().collect(
            &[event],
            rect,
            rect,
            [100.0, 100.0],
            true,
            Modifiers::default(),
        );
        assert_eq!(events[0]["key"], "A");
        assert_eq!(key_name(Key::OpenBracket), "LeftBracket");
        assert_eq!(key_name(Key::CloseBracket), "RightBracket");
        assert_eq!(key_name(Key::Backslash), "BackSlash");
        assert_eq!(key_name(Key::Backtick), "Backquote");
    }
}
