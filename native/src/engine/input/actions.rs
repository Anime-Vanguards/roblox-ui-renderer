//! `--actions` timelines (story.rs `run_timeline`): the schema of an action list, checked before
//! anything runs, and the compound actions `click` and `drag`, which target a point or a GUI node by
//! path and become move / down / up events when the timeline reaches them. A node is located then,
//! against the current layout (its AbsolutePosition / AbsoluteSize), so a layout change does not
//! move the click off its target. cli/actions.mjs checks the same schema in the JS CLI.
//!
//!   {t?, type:"move", x, y}   {t?, type:"down"|"up", x?, y?, button?: 1|2|3}   {t?, type:"leave"}
//!   {t?, type:"wheel", x?, y?, notches | dy, dx?}   {t?, type:"key", key, down?, shift?, ctrl?, alt?}
//!   {t?, type:"text"|"paste", text}
//!   {t?, type:"click", <point>, button?, hold?: seconds between down and up (0)}
//!   {t?, type:"drag", from: <point> (or <point> fields on the action), to: <point>, button?,
//!    steps?: moves from `from` to `to` (10), duration?: seconds they take (0.25)}
//!
//! <point>: `node` / `target` (a path as `inspect` prints it: the full name or a dot-separated
//! suffix of it, e.g. "StoryTarget.Stack.Button"; a name holding dots is quoted, `["Item.Name"]`),
//! with `anchor` {x, y} (fraction of the node's size, default the centre 0.5, 0.5) and `offset`
//! {x, y} (logical px), or `x`, `y` (logical px). In `from` / `to` a point is also a bare path string.
//!
//! Fields other than these are ignored (live events carry `frame`), as the input dispatch does.

use serde_json::{Map, Value as Json, json};

use crate::engine::GuiEngine;
use crate::engine::{Engine, InstanceId, dom::flags, layout::absolute_position};

/// The event types of an action list
pub const TYPES: [&str; 10] = ["move", "down", "up", "wheel", "leave", "key", "text", "paste", "click", "drag"];

/// Whether an action is expanded by the timeline (not dispatched as it is)
pub fn is_compound(action: &Json) -> bool {
    matches!(action.get("type").and_then(Json::as_str), Some("click" | "drag"))
}

/// A point an action targets
#[derive(Clone, Debug, PartialEq)]
pub enum Point {
    At(f64, f64),
    Node { path: String, anchor: (f64, f64), offset: (f64, f64) },
}

fn number(value: &Json) -> Option<f64> {
    value.as_f64().filter(|v| v.is_finite())
}

fn describe(value: &Json) -> &'static str {
    match value {
        Json::Null => "null",
        Json::Bool(_) => "a boolean",
        Json::Number(_) => "a number",
        Json::String(_) => "a string",
        Json::Array(_) => "a list",
        Json::Object(_) => "an object",
    }
}

fn optional_number(object: &Map<String, Json>, key: &str) -> Result<Option<f64>, String> {
    match object.get(key) {
        None => Ok(None),
        Some(value) => number(value).map(Some).ok_or_else(|| format!("`{key}` must be a number, not {}", describe(value))),
    }
}

fn optional_bool(object: &Map<String, Json>, key: &str) -> Result<(), String> {
    match object.get(key) {
        None | Some(Json::Bool(_)) => Ok(()),
        Some(value) => Err(format!("`{key}` must be true or false, not {}", describe(value))),
    }
}

fn required_string(object: &Map<String, Json>, key: &str) -> Result<(), String> {
    match object.get(key) {
        Some(Json::String(_)) => Ok(()),
        None => Err(format!("`{key}` (a string) is missing")),
        Some(value) => Err(format!("`{key}` must be a string, not {}", describe(value))),
    }
}

/// {x, y} of an `anchor` / `offset` field (either may be left out: `default`)
fn pair(object: &Map<String, Json>, key: &str, default: (f64, f64)) -> Result<(f64, f64), String> {
    match object.get(key) {
        None => Ok(default),
        Some(Json::Object(pair)) => {
            if let Some(extra) = pair.keys().find(|k| *k != "x" && *k != "y") {
                return Err(format!("`{key}` takes x and y, not `{extra}`"));
            }
            let x = optional_number(pair, "x").map_err(|e| format!("`{key}`: {e}"))?.unwrap_or(default.0);
            let y = optional_number(pair, "y").map_err(|e| format!("`{key}`: {e}"))?.unwrap_or(default.1);
            Ok((x, y))
        }
        Some(value) => Err(format!("`{key}` must be an object {{\"x\": .., \"y\": ..}}, not {}", describe(value))),
    }
}

/// The point of an object's `node` / `target` or `x`, `y` fields; None when it has neither
fn point_fields(object: &Map<String, Json>, what: &str) -> Result<Option<Point>, String> {
    let node = match (object.get("node"), object.get("target")) {
        (Some(_), Some(_)) => return Err(format!("{what}: give `node` or `target`, not both")),
        (Some(value), None) => Some(("node", value)),
        (None, Some(value)) => Some(("target", value)),
        (None, None) => None,
    };
    let x = optional_number(object, "x").map_err(|e| format!("{what}: {e}"))?;
    let y = optional_number(object, "y").map_err(|e| format!("{what}: {e}"))?;
    if let Some((key, value)) = node {
        let Json::String(path) = value else { return Err(format!("{what}: `{key}` must be a node path string, not {}", describe(value))) };
        if path.trim().is_empty() {
            return Err(format!("{what}: `{key}` is empty"));
        }
        if x.is_some() || y.is_some() {
            return Err(format!("{what}: give a node or x, y, not both (use `offset` to move off the node's anchor)"));
        }
        let anchor = pair(object, "anchor", (0.5, 0.5)).map_err(|e| format!("{what}: {e}"))?;
        let offset = pair(object, "offset", (0.0, 0.0)).map_err(|e| format!("{what}: {e}"))?;
        return Ok(Some(Point::Node { path: path.trim().to_string(), anchor, offset }));
    }
    for key in ["anchor", "offset"] {
        if object.contains_key(key) {
            return Err(format!("{what}: `{key}` needs a `node`"));
        }
    }
    match (x, y) {
        (Some(x), Some(y)) => Ok(Some(Point::At(x, y))),
        (None, None) => Ok(None),
        _ => Err(format!("{what}: needs both x and y")),
    }
}

/// `from` / `to` of a drag: a path string, or an object with point fields
fn point_value(value: &Json, what: &str) -> Result<Point, String> {
    match value {
        Json::String(path) if !path.trim().is_empty() => Ok(Point::Node { path: path.trim().to_string(), anchor: (0.5, 0.5), offset: (0.0, 0.0) }),
        Json::Object(object) => point_fields(object, what)?.ok_or_else(|| format!("{what}: needs `node` (or `target`) or x, y")),
        _ => Err(format!("{what}: must be a node path or {{\"node\": ..}} / {{\"x\": .., \"y\": ..}}, not {}", describe(value))),
    }
}

/// A click's target
pub fn click_point(action: &Json) -> Result<Point, String> {
    let Json::Object(object) = action else { return Err("an object expected".into()) };
    point_fields(object, "click")?.ok_or_else(|| "click: needs `node` (or `target`) or x, y".into())
}

/// A drag's start and end
pub fn drag_points(action: &Json) -> Result<(Point, Point), String> {
    let Json::Object(object) = action else { return Err("an object expected".into()) };
    let own = point_fields(object, "drag")?;
    let from = match (object.get("from"), own) {
        (Some(_), Some(_)) => return Err("drag: give `from` or node / x, y on the action, not both".into()),
        (Some(value), None) => point_value(value, "drag `from`")?,
        (None, Some(point)) => point,
        (None, None) => return Err("drag: needs `from` (a node path or {\"x\": .., \"y\": ..})".into()),
    };
    let to = match object.get("to") {
        Some(value) => point_value(value, "drag `to`")?,
        None => return Err("drag: needs `to` (a node path or {\"x\": .., \"y\": ..})".into()),
    };
    Ok((from, to))
}

fn check_button(object: &Map<String, Json>) -> Result<(), String> {
    match optional_number(object, "button")? {
        None => Ok(()),
        Some(b) if b == 1.0 || b == 2.0 || b == 3.0 => Ok(()),
        Some(b) => Err(format!("`button` must be 1, 2 or 3, not {b}")),
    }
}

/// One action's schema; the error names what is wrong
fn check(action: &Json) -> Result<(), String> {
    let Json::Object(object) = action else { return Err(format!("an action is an object, not {}", describe(action))) };
    let kind = match object.get("type") {
        Some(Json::String(kind)) => kind.as_str(),
        None => return Err(format!("`type` is missing (one of {})", TYPES.join(", "))),
        Some(value) => return Err(format!("`type` must be a string, not {}", describe(value))),
    };
    if !TYPES.contains(&kind) {
        return Err(format!("unknown type \"{kind}\" (one of {})", TYPES.join(", ")));
    }
    if let Some(t) = optional_number(object, "t")? {
        if t < 0.0 {
            return Err(format!("`t` must be 0 or more, not {t}"));
        }
    }
    match kind {
        "move" | "down" | "up" | "wheel" => {
            let x = optional_number(object, "x")?;
            let y = optional_number(object, "y")?;
            if kind == "move" && (x.is_none() || y.is_none()) {
                return Err("move: needs x and y".into());
            }
            if kind == "down" || kind == "up" {
                check_button(object)?;
            }
            if kind == "wheel" {
                let notches = optional_number(object, "notches")?;
                let dy = optional_number(object, "dy")?;
                optional_number(object, "dx")?;
                if notches.is_none() && dy.is_none() {
                    return Err("wheel: needs `notches` (+1 = one notch away from the user) or `dy` (px, 100 per notch)".into());
                }
            }
        }
        "key" => {
            required_string(object, "key")?;
            for flag in ["down", "shift", "ctrl", "alt"] {
                optional_bool(object, flag)?;
            }
        }
        "text" | "paste" => required_string(object, "text")?,
        "click" => {
            click_point(action)?;
            check_button(object)?;
            if let Some(hold) = optional_number(object, "hold")? {
                if hold < 0.0 {
                    return Err(format!("`hold` must be 0 or more, not {hold}"));
                }
            }
        }
        "drag" => {
            drag_points(action)?;
            check_button(object)?;
            if let Some(steps) = optional_number(object, "steps")? {
                if steps < 1.0 || steps > 10000.0 || steps.fract() != 0.0 {
                    return Err(format!("`steps` must be a whole number from 1 to 10000, not {steps}"));
                }
            }
            if let Some(duration) = optional_number(object, "duration")? {
                if duration < 0.0 {
                    return Err(format!("`duration` must be 0 or more, not {duration}"));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// The whole list's schema: `--actions[<index>] (<type>): <problem>` for the first bad action
pub fn validate(actions: &Json) -> Result<(), String> {
    let Json::Array(items) = actions else { return Err(format!("--actions: a JSON list of actions expected, not {}", describe(actions))) };
    for (index, action) in items.iter().enumerate() {
        if let Err(problem) = check(action) {
            let kind = action.get("type").and_then(Json::as_str).map(|k| format!(" ({k})")).unwrap_or_default();
            return Err(format!("--actions[{index}]{kind}: {problem}"));
        }
    }
    Ok(())
}

// Node paths

fn needs_quote(name: &str) -> bool {
    name.is_empty() || name.contains(['.', '[', ']', '"'])
}

/// The two spellings of an instance's path: GetFullName, and the unambiguous one `inspect` prints
/// (cli/diagnostics.mjs nodePaths: names holding dots quoted as `["a.b"]`)
fn paths_of(engine: &Engine, id: InstanceId) -> (String, String) {
    let state = engine.state.borrow();
    let data_model = engine.reflection.class_id("DataModel");
    let mut names = Vec::new();
    let mut current = Some(id);
    while let Some(node) = current {
        let instance = state.dom.at(node);
        if Some(instance.class) == data_model {
            break;
        }
        names.push(instance.name.to_str_lossy().into_owned());
        current = instance.parent;
    }
    names.reverse();
    let plain = names.join(".");
    let mut quoted = String::new();
    for (index, name) in names.iter().enumerate() {
        if needs_quote(name) {
            quoted.push('[');
            quoted.push_str(&Json::String(name.clone()).to_string());
            quoted.push(']');
        } else {
            if index > 0 {
                quoted.push('.');
            }
            quoted.push_str(name);
        }
    }
    (plain, quoted)
}

/// Whether `path` names the instance whose full paths are `plain` / `quoted`: one of them, or a
/// suffix of one that starts at a name
fn path_matches(plain: &str, quoted: &str, path: &str) -> bool {
    let suffix = |full: &str| {
        full == path
            || (full.len() > path.len() && full.ends_with(path) && (path.starts_with('[') || full.as_bytes()[full.len() - path.len() - 1] == b'.'))
    };
    suffix(plain) || suffix(quoted)
}

/// The GUI node `path` names (one with a layout record; a visible one when several match)
pub fn find_node(engine: &Engine, path: &str) -> Result<InstanceId, String> {
    let guard = engine.layout().borrow();
    let Some(layout) = guard.as_ref() else { return Err(format!("no node matches \"{path}\" (nothing is laid out)")) };
    let candidates: Vec<(InstanceId, bool)> = {
        let state = engine.state.borrow();
        (0..state.dom.slot_count() as u32)
            .filter_map(|slot| state.dom.id_at_slot(slot))
            .filter(|&id| !state.dom.at(id).has(flags::DESTROYED))
            .filter_map(|id| layout.record(id).map(|record| (id, record.visible)))
            .collect()
    };
    let mut matches = Vec::new();
    for &(id, visible) in &candidates {
        let (plain, quoted) = paths_of(engine, id);
        if path_matches(&plain, &quoted, path) {
            matches.push((id, visible, quoted));
        }
    }
    let visible: Vec<_> = matches.iter().filter(|(_, visible, _)| *visible).collect();
    let chosen: Vec<_> = if visible.is_empty() { matches.iter().collect() } else { visible };
    match chosen.len() {
        1 => Ok(chosen[0].0),
        0 => {
            // Nodes with the path's last name, as hints
            let last = path.rsplit(['.', '[']).next().unwrap_or(path).trim_matches(['"', ']']);
            let mut named: Vec<String> = candidates
                .iter()
                .filter(|(id, _)| engine.state.borrow().dom.at(*id).name.to_str_lossy() == last)
                .map(|&(id, _)| paths_of(engine, id).1)
                .collect();
            named.sort();
            let hint = if named.is_empty() {
                String::from("; `node cli/inspect.mjs tree <render dir>` lists the node paths")
            } else {
                format!("; nodes named \"{last}\": {}{}", named.iter().take(4).cloned().collect::<Vec<_>>().join(", "), if named.len() > 4 { ", ..." } else { "" })
            };
            Err(format!("no GUI node matches \"{path}\"{hint}"))
        }
        count => {
            let names: Vec<&str> = chosen.iter().take(4).map(|(_, _, quoted)| quoted.as_str()).collect();
            Err(format!("\"{path}\" matches {count} visible nodes ({}{}); give a longer path", names.join(", "), if count > 4 { ", ..." } else { "" }))
        }
    }
}

/// A point in logical viewport pixels, a node's from its current AbsolutePosition / AbsoluteSize
pub fn resolve(engine: &Engine, point: &Point) -> Result<(f64, f64), String> {
    match point {
        Point::At(x, y) => Ok((*x, *y)),
        Point::Node { path, anchor, offset } => {
            let id = find_node(engine, path)?;
            let guard = engine.layout().borrow();
            let record = guard.as_ref().and_then(|layout| layout.record(id)).ok_or_else(|| format!("\"{path}\" is not laid out"))?;
            let (x, y) = absolute_position(record);
            Ok((x + record.w * anchor.0 + offset.0, y + record.h * anchor.1 + offset.1))
        }
    }
}

fn event(kind: &str, (x, y): (f64, f64), button: Option<f64>) -> Json {
    let mut event = json!({ "type": kind, "x": x, "y": y });
    if let Some(button) = button {
        event["button"] = json!(button);
    }
    event
}

/// A click / drag as timed input events from `now` (seconds of virtual time), located against the
/// current layout (laid out first). The error says which node could not be found.
pub fn expand(engine: &std::rc::Rc<Engine>, action: &Json, now: f64) -> Result<Vec<(f64, Json)>, String> {
    super::super::layout::update_all(engine);
    let number = |key: &str| action.get(key).and_then(Json::as_f64);
    let button = number("button").unwrap_or(1.0);
    match action.get("type").and_then(Json::as_str) {
        Some("click") => {
            let at = resolve(engine, &click_point(action)?)?;
            let hold = number("hold").unwrap_or(0.0).max(0.0);
            Ok(vec![(now, event("move", at, None)), (now, event("down", at, Some(button))), (now + hold, event("up", at, Some(button)))])
        }
        Some("drag") => {
            let (from, to) = drag_points(action)?;
            let (a, b) = (resolve(engine, &from)?, resolve(engine, &to)?);
            let steps = number("steps").unwrap_or(10.0).max(1.0) as usize;
            let duration = number("duration").unwrap_or(0.25).max(0.0);
            let mut events = vec![(now, event("move", a, None)), (now, event("down", a, Some(button)))];
            for step in 1..=steps {
                let f = step as f64 / steps as f64;
                events.push((now + duration * f, event("move", (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f), None)));
            }
            events.push((now + duration, event("up", b, Some(button))));
            Ok(events)
        }
        _ => Err("not a click or drag".into()),
    }
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    fn error(text: &str) -> String {
        validate(&serde_json::from_str(text).unwrap()).unwrap_err()
    }

    #[test]
    fn accepts_the_existing_and_compound_actions() {
        let ok = r#"[
            {"t": 0.1, "type": "move", "x": 1, "y": 2}, {"type": "down", "button": 1, "x": 1, "y": 2}, {"type": "up"},
            {"type": "wheel", "x": 1, "y": 2, "notches": -0.5}, {"type": "wheel", "dy": 100, "dx": 0}, {"type": "leave"},
            {"type": "key", "key": "Left", "down": false, "ctrl": true}, {"type": "text", "text": "!"}, {"type": "paste", "text": "a"},
            {"type": "click", "node": "StoryTarget.Button"}, {"type": "click", "target": "Button", "anchor": {"x": 0}, "offset": {"x": 2, "y": -1}, "button": 2, "hold": 0.1},
            {"type": "click", "x": 10, "y": 20}, {"frame": 3, "type": "click", "x": 10, "y": 20},
            {"type": "drag", "from": "A", "to": {"x": 5, "y": 6}, "steps": 4, "duration": 0.5},
            {"type": "drag", "node": "A", "to": {"node": "B", "anchor": {"x": 1, "y": 1}}}
        ]"#;
        validate(&serde_json::from_str(ok).unwrap()).unwrap();
        validate(&json!([])).unwrap();
    }

    #[test]
    fn names_the_offending_action() {
        assert_eq!(error(r#"{"type": "move"}"#), "--actions: a JSON list of actions expected, not an object");
        assert!(error(r#"[{"type": "move", "x": 1, "y": 1}, {"type": "clik"}]"#).starts_with("--actions[1] (clik): unknown type \"clik\""));
        assert_eq!(error(r#"[{"x": 1}]"#), "--actions[0]: `type` is missing (one of move, down, up, wheel, leave, key, text, paste, click, drag)");
        assert_eq!(error(r#"[{"type": "move", "x": "1", "y": 2}]"#), "--actions[0] (move): `x` must be a number, not a string");
        assert_eq!(error(r#"[{"type": "move", "x": 1}]"#), "--actions[0] (move): move: needs x and y");
        assert_eq!(error(r#"[{"type": "key"}]"#), "--actions[0] (key): `key` (a string) is missing");
        assert_eq!(error(r#"[{"t": -1, "type": "leave"}]"#), "--actions[0] (leave): `t` must be 0 or more, not -1");
        assert_eq!(error(r#"[{"type": "click"}]"#), "--actions[0] (click): click: needs `node` (or `target`) or x, y");
        assert_eq!(error(r#"[{"type": "click", "node": "A", "x": 1, "y": 1}]"#).split(": ").nth(2), Some("give a node or x, y, not both (use `offset` to move off the node's anchor)"));
        assert_eq!(error(r#"[{"type": "click", "node": 5}]"#), "--actions[0] (click): click: `node` must be a node path string, not a number");
        assert_eq!(error(r#"[{"type": "click", "x": 1, "y": 1, "button": 4}]"#), "--actions[0] (click): `button` must be 1, 2 or 3, not 4");
        assert_eq!(error(r#"[{"type": "drag", "from": "A"}]"#), "--actions[0] (drag): drag: needs `to` (a node path or {\"x\": .., \"y\": ..})");
        assert_eq!(error(r#"[{"type": "drag", "from": "A", "to": "B", "steps": 0}]"#), "--actions[0] (drag): `steps` must be a whole number from 1 to 10000, not 0");
        assert_eq!(error(r#"[{"type": "wheel", "x": 1, "y": 1}]"#).split(": ").nth(1), Some("wheel"));
    }

    #[test]
    fn path_suffixes_start_at_a_name() {
        let plain = "CoreGui.RURStory.StoryTarget.Stack.Button";
        assert!(path_matches(plain, plain, plain));
        assert!(path_matches(plain, plain, "StoryTarget.Stack.Button"));
        assert!(path_matches(plain, plain, "Button"));
        assert!(!path_matches(plain, plain, "tton"));
        assert!(!path_matches(plain, plain, "Stack"));
        let (plain, quoted) = ("CoreGui.RURStory.StoryTarget.Item.Name.Arrow", "CoreGui.RURStory.StoryTarget[\"Item.Name\"].Arrow");
        assert!(path_matches(plain, quoted, "[\"Item.Name\"].Arrow"));
        assert!(path_matches(plain, quoted, "Item.Name.Arrow"));
        assert!(path_matches(plain, quoted, "StoryTarget[\"Item.Name\"].Arrow"));
    }
}
