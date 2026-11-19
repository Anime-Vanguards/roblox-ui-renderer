//! `rur-native diff` (`rur diff`): the same source rendered as it is at a git ref (before) and in the working tree
//! (after), then their layout and pixels compared.
//!
//!   rur-native diff --git-ref <ref> --story <file> | --module | --model | --fixture <file> [--out dir] [--json]
//!                   [the render options of `rur render`]
//!
//! Writes `<out>/before/` and `<out>/after/` (each what `rur render` writes) and `<out>/diff.png` (unchanged pixels in
//! grey, changed ones from yellow to red by how much they changed); `<out>` is `--out`, else `<out dir>/<name>-diff`
//! (`--tag`, `--keep` as for a render). stdout holds the result as JSON (`--json`: one line): both renders' summaries,
//! the layout changes (`- removed <path>`, `+ added <path>`, `~ <path>` with each side's description; the first 200)
//! and the pixel counts. Exit code 2 when either render's story failed, 64 for a bad option, 1 when it could not run.
//! The two renders run at the same time, each in a `rur-native render` process of its own.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    js::Json,
    story_args::{Args, Context, Failure},
    summary::scene_diagnostics,
};

/// A loaded render: its directory, nodes, each node's unambiguous path and notes, and its scale
struct Loaded {
    dir: PathBuf,
    nodes: Vec<Json>,
    paths: Vec<String>,
    notes: HashMap<usize, Vec<String>>,
    scale: f64,
}

fn load(target: &Path) -> Result<Loaded, String> {
    let file = if target.is_dir() { target.join("scene.json") } else { target.to_path_buf() };
    let scene = Json::read(&file).ok_or_else(|| format!("{}: no scene", file.display()))?;
    let diagnostics = scene_diagnostics(&scene);
    let scale = scene.get("viewport").get("scale").as_f64().filter(|scale| *scale != 0.0 && !scale.is_nan()).unwrap_or(1.0);
    let Json::Object(mut entries) = scene else { return Err(format!("{}: not a scene", file.display())) };
    let nodes = match entries.iter().position(|(key, _)| key == "nodes").map(|at| entries.swap_remove(at).1) {
        Some(Json::Array(nodes)) => nodes,
        _ => Vec::new(),
    };
    let paths = nodes.iter().enumerate().map(|(index, node)| Some(diagnostics.paths[index].clone()).filter(|path| !path.is_empty()).unwrap_or_else(|| node.get("path").text())).collect();
    Ok(Loaded { dir: file.parent().map(Path::to_path_buf).unwrap_or_default(), nodes, paths, notes: diagnostics.notes, scale })
}

/// `Math.round(value * 100) / 100` (halves round up, as JavaScript's Math.round does)
fn round(value: f64) -> String {
    crate::glyphs::js_number_string((value * 100.0 + 0.5).floor() / 100.0)
}

fn number(value: &Json) -> f64 {
    value.as_f64().unwrap_or(f64::NAN)
}

/// A value as a template literal prints it (a missing one is `undefined`)
fn shown(node: &Json, key: &str) -> String {
    if node.has(key) { node.get(key).text() } else { "undefined".into() }
}

/// `#rrggbb` of a 0-1 color, nothing without one
fn hex(color: &Json) -> String {
    if !color.truthy() {
        return String::new();
    }
    let channels: Vec<String> = color
        .items()
        .iter()
        .map(|channel| {
            let value = (number(channel) * 255.0 + 0.5).floor();
            if value.is_nan() { "NaN".into() } else { format!("{:02x}", value as i64) }
        })
        .collect();
    format!("#{}", channels.join(""))
}

/// One line describing a node's visuals (cli/inspect.mjs describe), with its diagnostic notes
fn describe(node: &Json, notes: Option<&Vec<String>>, scale: f64) -> String {
    let name = node.get("name").text();
    let name = if name.contains(['.', '[', ']', '"']) { serde_json::to_string(&name).expect("a string") } else { name };
    let rect: Vec<String> = node.get("rect").items().iter().map(|value| round(number(value))).collect();
    let mut parts = vec![format!("{} {name}", shown(node, "class")), format!("[{}]", rect.join(", "))];
    if node.get("visible") == &Json::Bool(false) {
        parts.push("hidden".into());
    }
    // LayoutOrder / ZIndex when not the default (0 / 1)
    if node.get("layoutOrder").truthy() {
        parts.push(format!("layoutOrder={}", node.get("layoutOrder").text()));
    }
    if node.has("zIndex") && node.get("zIndex") != &Json::Number(1.0) && !node.has("displayOrder") {
        parts.push(format!("z={}", node.get("zIndex").text()));
    }
    if node.get("rotation").truthy() {
        parts.push(format!("rot={}", round(number(node.get("rotation")))));
    }
    let background = node.get("bg");
    if background.truthy() && number(background.get("transparency")) < 1.0 {
        let transparency = background.get("transparency");
        let alpha = if transparency.truthy() { format!("@{}", round(1.0 - number(transparency))) } else { String::new() };
        parts.push(format!("bg={}{alpha}", hex(background.get("color"))));
    }
    let corner = node.get("corner");
    if corner.truthy() {
        let value = match corner {
            Json::Array(items) => items.iter().map(|item| round(number(item))).collect::<Vec<_>>().join("/"),
            other => round(number(other)),
        };
        parts.push(format!("corner={value}"));
    }
    for stroke in node.get("strokes").items() {
        let text = if stroke.get("mode").as_str() == Some("Contextual") && node.get("text").truthy() { "text:" } else { "" };
        let transparency = stroke.get("transparency");
        let alpha = if transparency.truthy() { format!("@{}", round(1.0 - number(transparency))) } else { String::new() };
        parts.push(format!("stroke={text}{}{}{alpha}", round(number(stroke.get("thickness"))), hex(stroke.get("color"))));
    }
    if node.get("gradient").truthy() {
        parts.push("gradient".into());
    }
    for shadow in node.get("shadows").items() {
        parts.push(format!("shadow=blur{}{}", round(number(shadow.get("blur"))), hex(shadow.get("color"))));
    }
    let image = node.get("image");
    if image.truthy() {
        let scale_type = image.get("scaleType");
        let scaled = if scale_type.truthy() && scale_type.as_str() != Some("Stretch") { format!(" {}", scale_type.text()) } else { String::new() };
        parts.push(format!("image={}{scaled}", shown(image, "uri")));
    }
    let group = node.get("group");
    if group.truthy() {
        let transparency = group.get("transparency");
        parts.push(format!("group{}", if transparency.truthy() { format!("@{}", round(1.0 - number(transparency))) } else { String::new() }));
    }
    let text = node.get("text");
    if text.truthy() {
        let shown_text = node.get("props").get("text");
        let value = if matches!(shown_text, Json::Null) { String::new() } else { shown_text.text() }.replace('\n', "\\n");
        let length: usize = value.chars().map(char::len_utf16).sum();
        let cut = if length > 60 { format!("{}...", crate::js::slice_start(&value, 57)) } else { value };
        parts.push(format!(
            "text={} size={} bounds={}x{}",
            serde_json::to_string(&cut).expect("a string"),
            shown(text, "textSize"),
            round(number(text.get("width")) / scale),
            round(number(text.get("height")) / scale)
        ));
    }
    let path = node.get("path2d");
    if path.truthy() {
        parts.push(format!(
            "path={}{} segments={}{}",
            round(number(path.get("thickness"))),
            hex(path.get("color")),
            path.get("segments").items().len(),
            if path.get("closed").truthy() { " closed" } else { "" }
        ));
    }
    let scroll = node.get("scroll");
    if scroll.truthy() {
        let canvas: Vec<String> = scroll.get("canvas").items().iter().skip(2).map(|value| round(number(value))).collect();
        parts.push(format!("scroll canvas={}", canvas.join("x")));
    }
    for note in notes.into_iter().flatten() {
        parts.push(format!("[{note}]"));
    }
    parts.join(" ")
}

/// The layout and pixel differences between two renders: the change lines, and the pixel counts (None when either
/// render has no render.png) with the difference image written to `out`
pub fn diff_renders(a: &Path, b: &Path, out: &Path) -> Result<(Vec<String>, Json), String> {
    let (a, b) = (load(a)?, load(b)?);
    // (both sides' text bounds in the second render's scale, as the Node inspector printed them)
    let scale = b.scale;
    let mut by_path: Vec<(String, Vec<usize>)> = Vec::new();
    for (index, path) in b.paths.iter().enumerate() {
        match by_path.iter_mut().find(|(known, _)| known == path) {
            Some((_, list)) => list.push(index),
            None => by_path.push((path.clone(), vec![index])),
        }
    }
    let mut used: HashMap<String, usize> = HashMap::new();
    let mut changes = Vec::new();
    for (index, node) in a.nodes.iter().enumerate() {
        let path = &a.paths[index];
        let taken = used.entry(path.clone()).or_insert(0);
        let k = *taken;
        *taken += 1;
        let Some(other) = by_path.iter().find(|(known, _)| known == path).and_then(|(_, list)| list.get(k)).copied() else {
            changes.push(format!("- removed {path}"));
            continue;
        };
        let other_node = &b.nodes[other];
        let other_rect = other_node.get("rect").items();
        let moved = node.get("rect").items().iter().enumerate().any(|(at, value)| (number(value) - other_rect.get(at).map_or(f64::NAN, number)).abs() > 0.01);
        let before = describe(node, a.notes.get(&index), scale);
        let after = describe(other_node, b.notes.get(&other), scale);
        if moved || before != after {
            changes.push(format!("~ {path}\n    A: {before}\n    B: {after}"));
        }
    }
    for (path, list) in &by_path {
        let k = used.get(path).copied().unwrap_or(0);
        for &index in list.iter().skip(k) {
            changes.push(format!("+ added {}\n    {}", b.paths[index], describe(&b.nodes[index], b.notes.get(&index), scale)));
        }
    }
    let (file_a, file_b) = (a.dir.join("render.png"), b.dir.join("render.png"));
    if !file_a.exists() || !file_b.exists() {
        return Ok((changes, Json::Null));
    }
    let read = |file: &Path| image::open(file).map(|image| image.to_rgba8()).map_err(|error| format!("{}: {error}", file.display()));
    let (pa, pb) = (read(&file_a)?, read(&file_b)?);
    if pa.dimensions() != pb.dimensions() {
        let message = format!("pixels: sizes differ ({}x{} vs {}x{})", pa.width(), pa.height(), pb.width(), pb.height());
        return Ok((changes, Json::object().with("changed", Json::Null).with("total", Json::Null).with("file", Json::Null).with("message", message)));
    }
    let mut picture = Vec::with_capacity(pa.as_raw().len());
    let mut changed = 0usize;
    for (x, y) in pa.as_raw().chunks_exact(4).zip(pb.as_raw().chunks_exact(4)) {
        let d = (0..3).map(|channel| (x[channel] as i32 - y[channel] as i32).abs()).max().unwrap();
        if d > 0 {
            changed += 1;
        }
        let gray = ((x[0] as f64 + x[1] as f64 + x[2] as f64) / 12.0 + 0.5).floor() as u8;
        picture.extend(if d > 0 { [255, (255 - d * 4).max(0) as u8, 0, 255] } else { [gray, gray, gray, 255] });
    }
    crate::story_render::write_png(out, &picture, pa.width(), pa.height()).map_err(|error| format!("{error:#}"))?;
    let total = (pa.width() * pa.height()) as usize;
    let message = format!("pixels: {changed} of {total} differ ({}%) → {}", crate::js::to_fixed(changed as f64 / total as f64 * 100.0, 3), out.display());
    Ok((changes, Json::object().with("changed", changed).with("total", total).with("file", out).with("message", message)))
}

/// The diff: both renders, their summaries and the differences
pub fn diff(context: &Context, args: &Args) -> Result<Json, Failure> {
    if args.value("--git-ref").is_none() {
        return Err(Failure::Error("rur diff needs --git-ref <ref> (the 'before' commit)".into()));
    }
    let source = ["--story", "--module", "--model", "--fixture"]
        .iter()
        .find_map(|key| args.value(key))
        .ok_or_else(|| Failure::Error("rur diff needs --story, --module, --model or --fixture".into()))?;
    let name = crate::story_args::source_name(&source, false);
    let out = context.render_out_dir(args, &format!("{name}-diff"));
    let mut own = args.clone();
    own.remove("--tag");
    own.remove("--keep");
    let mut before_args = own.clone();
    before_args.set_value("--out", out.join("before").display().to_string());
    let mut after_args = own;
    after_args.remove("--git-ref");
    after_args.set_value("--out", out.join("after").display().to_string());
    let git_ref = std::sync::Mutex::new(crate::gitref::GitRef::new(&args.value("--git-ref").unwrap_or_default(), &context.state));
    let before = context.prepare_render(&before_args, Some(&git_ref))?;
    let after = context.prepare_render(&after_args, None)?;
    // Both at once: the before render's stderr as it comes, the after render's once it is done
    let (before_outcome, after_outcome) = std::thread::scope(|scope| {
        let later = scope.spawn(|| context.run_render(&after, false, None));
        let first = context.run_render(&before, true, None);
        (first, later.join().expect("the after render's thread"))
    });
    eprint!("{}", after_outcome.stderr);
    let before_summary = context.finish_render(&before, &before_outcome)?;
    let after_summary = context.finish_render(&after, &after_outcome)?;
    let (changes, pixels) = diff_renders(&before.out, &after.out, &out.join("diff.png"))?;
    Ok(Json::object()
        .with("before", before_summary)
        .with("after", after_summary)
        .with("changes", changes)
        .with("pixels", pixels)
        .with("out", out.as_path()))
}

/// `rur-native diff ...`: the diff's result as JSON on stdout (the first 200 changes); returns the exit code
pub fn cli(argv: &[String]) -> i32 {
    let run = || -> Result<(Json, bool), Failure> {
        let (context, args) = Context::new(argv)?;
        let mut result = diff(&context, &args)?;
        let changes: Vec<Json> = result.get("changes").items().iter().take(200).cloned().collect();
        result.set("changes", changes);
        Ok((result, args.truthy("--json")))
    };
    match run() {
        Ok((result, json)) => {
            println!("{}", result.stringify(if json { 0 } else { 2 }));
            if ["before", "after"].iter().any(|side| result.get(side).get("status").as_str() == Some("error")) { 2 } else { 0 }
        }
        Err(failure) => {
            eprintln!("rur diff: {}", failure.message());
            failure.code()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(dir: &Path, nodes: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("scene.json"), format!(r#"{{"viewport": {{"width": 100, "height": 50, "scale": 2}}, "nodes": [{nodes}]}}"#)).unwrap();
    }

    fn png(dir: &Path, pixels: &[[u8; 4]]) {
        let rgba: Vec<u8> = pixels.iter().flatten().copied().collect();
        crate::story_render::write_png(&dir.join("render.png"), &rgba, pixels.len() as u32, 1).unwrap();
    }

    /// Nodes are matched by path (in order where paths repeat): one moved, one removed, one added, one unchanged; the
    /// pixels that differ are counted and drawn
    #[test]
    fn renders_differ_by_layout_and_pixels() {
        let dir = std::env::temp_dir().join(format!("rur-diff-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (a, b) = (dir.join("a"), dir.join("b"));
        scene(
            &a,
            r#"{"id": 1, "name": "Gui", "path": "Gui", "class": "ScreenGui", "rect": [0, 0, 100, 50]},
               {"id": 2, "parent": 1, "name": "Title", "path": "Gui.Title", "class": "TextLabel", "rect": [0, 0, 40, 10], "text": {"textSize": 14, "width": 60, "height": 20}, "props": {"text": "Hi"}},
               {"id": 3, "parent": 1, "name": "Gone", "path": "Gui.Gone", "class": "Frame", "rect": [0, 0, 1, 1], "bg": {"color": [1, 0, 0.5], "transparency": 0.25}}"#,
        );
        scene(
            &b,
            r#"{"id": 1, "name": "Gui", "path": "Gui", "class": "ScreenGui", "rect": [0, 0, 100, 50]},
               {"id": 2, "parent": 1, "name": "Title", "path": "Gui.Title", "class": "TextLabel", "rect": [0, 5, 40, 10], "text": {"textSize": 14, "width": 60, "height": 20}, "props": {"text": "Hi"}},
               {"id": 4, "parent": 1, "name": "New.One", "path": "Gui.New.One", "class": "Frame", "rect": [1, 2, 3, 4], "zIndex": 2}"#,
        );
        png(&a, &[[10, 20, 30, 255], [0, 0, 0, 255], [9, 9, 9, 255]]);
        png(&b, &[[10, 20, 30, 255], [0, 5, 0, 255], [9, 9, 9, 255]]);
        let (changes, pixels) = diff_renders(&a, &b, &dir.join("diff.png")).unwrap();
        assert_eq!(
            changes,
            [
                "~ Gui.Title\n    A: TextLabel Title [0, 0, 40, 10] text=\"Hi\" size=14 bounds=30x10\n    B: TextLabel Title [0, 5, 40, 10] text=\"Hi\" size=14 bounds=30x10",
                "- removed Gui.Gone",
                "+ added Gui[\"New.One\"]\n    Frame \"New.One\" [1, 2, 3, 4] z=2",
            ]
        );
        assert_eq!(pixels.get("changed").as_f64(), Some(1.0));
        assert_eq!(pixels.get("total").as_f64(), Some(3.0));
        assert!(pixels.get("message").text().starts_with("pixels: 1 of 3 differ (33.333%) → "));
        let picture = image::open(dir.join("diff.png")).unwrap().to_rgba8();
        assert_eq!(picture.as_raw()[..12], [5, 5, 5, 255, 255, 235, 0, 255, 2, 2, 2, 255]);
        let node = Json::parse(r#"{"class": "Frame", "name": "Box", "rect": [0.005, 1.234, 2, 3], "bg": {"color": [1, 0, 0.5], "transparency": 0.25}, "corner": [4, 4.5], "rotation": 12.345}"#).unwrap();
        assert_eq!(describe(&node, Some(&vec!["culled, not drawn: x".into()]), 1.0), "Frame Box [0.01, 1.23, 2, 3] rot=12.35 bg=#ff0080@0.75 corner=4/4.5 [culled, not drawn: x]");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
