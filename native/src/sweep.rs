//! `rur-native sweep` (`rur sweep`): every story of the project rendered and reported on.
//!
//!   rur-native sweep [--filter a|b] [--jobs N] [--timeout s] [--no-png] [--out out/sweep] [--place <name>|all]
//!                    [--failed] [--limit N] [--retries 1] [--summarize] [the render options below]
//!
//! Each story renders in a `rur-native render` process of its own, `--jobs` at a time (a third of the CPUs, at most
//! 8): a story that runs away is killed with everything it started after `--timeout` seconds (90), and one that crashes
//! its process is recorded as a crash and, with `--retries` (1), run again at the end. Its output directory is
//! `<out>/stories/<story>/` (what `rur render` writes, summary.json included). `report.json` holds a record per story
//! and the totals (failure causes grouped, unemulated members, missing images and fonts, Studio-only instances, the
//! slowest stories), `summary.md` the same as tables; stdout the totals' headline as JSON. `--failed` re-runs the
//! stories the last report has as not clean and merges them into it, `--limit N` runs the first N, `--summarize`
//! rebuilds the totals and summary.md from report.json without rendering.
//!
//! The render options passed to every story: `--viewport` (1200x900), `--time` (1), `--scale` (1.5), `--mode`,
//! `--player`, `--background`, `--project`, `--roots`, `--place-id`, `--rotation-clipping`, `--device`,
//! `--topbar-inset`, `--rojo`, `--rojo-at`, `--git-ref`, `--plugin`, `--mount`, `--offline`.
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use crate::{
    js::{self, Json},
    story_args::{Args, Context, Failure, Opt},
};

/// One story to sweep: its file, the place it runs in, its key (its path in the project) and its output folder's name
#[derive(Clone, Debug)]
pub struct Entry {
    pub file: PathBuf,
    pub place: String,
    pub key: String,
    pub slug: String,
}

/// `dir` as it is listed: on Windows a verbatim path (`\\?\C:\...`), which keeps a folder name that ends in a dot or a
/// space (path normalization would drop it and miss the folder; Node's readdir reads it)
fn listed(dir: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let text = dir.to_string_lossy();
        if dir.is_absolute() && !text.starts_with(r"\\") {
            return PathBuf::from(format!(r"\\?\{}", text.replace('/', r"\")));
        }
    }
    dir.to_path_buf()
}

/// The stories (`*.story.luau` / `*.story.lua`) below `dir`; `skip` leaves out dot-folders and node_modules. Links to
/// folders are not followed.
fn walk(dir: &Path, skip: bool, results: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(listed(dir)).map_err(|error| format!("{}: {error}", dir.display()))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if skip && (name.starts_with('.') || name == "node_modules") {
            continue;
        }
        let full = dir.join(&name);
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            walk(&full, skip, results)?;
        } else if name.ends_with(".story.luau") || name.ends_with(".story.lua") {
            results.push(full);
        }
    }
    Ok(())
}

/// The storybook stories of a run's project (its roots, the folders it mounts, and the Rojo project's own folder),
/// `--filter` applied (case-insensitive substrings, `a|b` either), sorted
pub fn list_stories(context: &Context, args: &Args) -> Result<Vec<PathBuf>, String> {
    let mut results = Vec::new();
    for root in context.project_roots(args) {
        walk(Path::new(&root), false, &mut results)?;
    }
    if context.has_mounts(args) {
        for mount in context.run_mounts(args, &mut |path| Ok(path))? {
            let value = Path::new(&mount.value);
            if !mount.instance && listed(value).is_dir() {
                walk(value, false, &mut results)?;
            }
        }
    }
    if let Some(file) = context.rojo_file(args) {
        if let Some(dir) = context.resolve(&file).parent() {
            walk(dir, true, &mut results)?;
        }
    }
    let mut seen = HashSet::new();
    // (the folders walked are absolute already: each file is as path.resolve gives it)
    let mut files: Vec<PathBuf> = results.into_iter().filter(|file| seen.insert(file.clone())).collect();
    let filters: Vec<String> = args.value("--filter").map(|filter| filter.to_lowercase().split('|').filter(|part| !part.is_empty()).map(str::to_string).collect()).unwrap_or_default();
    files.retain(|file| filters.is_empty() || filters.iter().any(|filter| file.to_string_lossy().to_lowercase().contains(filter.as_str())));
    files.sort_by(|a, b| a.to_string_lossy().encode_utf16().cmp(b.to_string_lossy().encode_utf16()));
    Ok(files)
}

/// `file` relative to `project` (forward slashes) when it is inside it, else the whole path with forward slashes
fn story_key(project: &Path, file: &Path) -> String {
    let same = |a: &std::path::Component, b: &std::path::Component| {
        let (a, b) = (a.as_os_str().to_string_lossy(), b.as_os_str().to_string_lossy());
        if cfg!(windows) { a.to_lowercase() == b.to_lowercase() } else { a == b }
    };
    let mut parts = file.components();
    for part in project.components() {
        match parts.next() {
            Some(other) if same(&part, &other) => {}
            _ => return file.to_string_lossy().replace('\\', "/"),
        }
    }
    parts.as_path().to_string_lossy().replace('\\', "/")
}

/// A story's output folder name: its key's folders joined with dots (short, so Windows paths stay well under MAX_PATH)
fn story_slug(key: &str) -> String {
    let mut slug = key.to_string();
    if slug.len() >= 2 && slug.as_bytes()[0].is_ascii_alphabetic() && slug.as_bytes()[1] == b':' {
        slug.drain(..2);
    }
    if let Some(stem) = slug.strip_suffix(".story.luau").or_else(|| slug.strip_suffix(".story.lua")) {
        slug = stem.to_string();
    }
    slug.split('/').filter(|part| !part.is_empty()).collect::<Vec<_>>().join(".")
}

/// Every story of the project with the place whose roots mount it (a story under a root two places share is listed
/// by both and runs with the first), sorted by key
pub fn story_entries(context: &Context, args: &Args) -> Result<Vec<Entry>, String> {
    let project = context.project(args);
    let requested = args.value("--place").unwrap_or_else(|| "all".into());
    let names: Vec<String> = crate::places::places(&project).into_iter().map(|place| place.name).collect();
    let places: Vec<String> = if args.truthy("--roots") {
        vec![if requested == "all" { names[0].clone() } else { requested }]
    } else if requested == "all" {
        names
    } else {
        vec![requested]
    };
    let mut seen: Vec<(PathBuf, String)> = Vec::new();
    for place in places {
        let mut place_args = args.clone();
        place_args.set_value("--place", place.clone());
        for file in list_stories(context, &place_args)? {
            if !seen.iter().any(|(known, _)| *known == file) {
                seen.push((file, place.clone()));
            }
        }
    }
    let mut entries: Vec<Entry> = seen
        .into_iter()
        .map(|(file, place)| {
            let key = story_key(&project, &file);
            let slug = story_slug(&key);
            Entry { file, place, key, slug }
        })
        .collect();
    entries.sort_by(|a, b| js::locale_compare(&a.key, &b.key));
    Ok(entries)
}

/// The render options a sweep passes to each story's render
const PASSED: [&str; 16] = [
    "--viewport", "--time", "--scale", "--mode", "--player", "--background", "--project", "--roots", "--place-id", "--rotation-clipping", "--device",
    "--topbar-inset", "--rojo", "--rojo-at", "--git-ref", "--plugin",
];

/// A story's render options: the story, its output folder and place, and the sweep's render options
fn story_args(entry: &Entry, dir: &Path, args: &Args, with_png: bool) -> Args {
    let mut story = Args::default();
    story.set_value("--story", entry.file.display().to_string());
    story.set_value("--out", dir.display().to_string());
    story.set_value("--place", entry.place.clone());
    if !with_png {
        story.set("--no-png", Opt::Flag);
    }
    for key in PASSED {
        if let Some(value) = args.given(key) {
            story.set_value(key, value);
        }
    }
    let mounts = args.list("--mount");
    if !mounts.is_empty() {
        story.set("--mount", Opt::List(mounts));
    }
    if args.get("--plugin") == Some(&Opt::Flag) {
        story.set("--plugin", Opt::Flag);
    }
    if args.truthy("--offline") {
        story.set("--offline", Opt::Flag);
    }
    story
}

/// The line of a (possibly nested, traceback-laden) Luau error that states what went wrong
pub fn primary_message(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').map(|line| line.trim()).filter(|line| !line.is_empty()).collect();
    let traceback = |line: &str| {
        // [string "..."]:12 or [string "..."]:12 function name
        let Some(rest) = line.strip_prefix("[string \"") else { return false };
        rest.match_indices("\"]:").any(|(at, _)| {
            let after = &rest[at + 3..];
            let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            digits > 0 && (after.len() == digits || after[digits..].starts_with(" function "))
        })
    };
    let meaningful: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| !line.to_lowercase().starts_with("error while running") && !traceback(line) && !line.starts_with("Stack Begin") && !line.starts_with("Stack End"))
        .collect();
    let sited = |line: &str| {
        line.match_indices(':').any(|(at, _)| {
            let after = &line[at + 1..];
            let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            digits > 0 && after[digits..].starts_with(": ")
        })
    };
    meaningful.iter().find(|line| sited(line)).or(meaningful.first()).or(lines.first()).map_or_else(String::new, |line| line.to_string())
}

/// `[string "A.B"]:12: message` / `A.B:12: message` as (`A.B:12`, `message`)
pub fn split_site(message: &str) -> (Option<String>, String) {
    let site_and_rest = |name: &str, after: &str| {
        let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        (digits > 0 && after[digits..].starts_with(": ")).then(|| (format!("{name}:{}", &after[..digits]), after[digits + 2..].to_string()))
    };
    if let Some(rest) = message.strip_prefix("[string \"") {
        if let Some(close) = rest.find('"') {
            if close > 0 && rest[close..].starts_with("\"]:") {
                if let Some((site, text)) = site_and_rest(&rest[..close], &rest[close + 3..]) {
                    return (Some(site), text);
                }
            }
        }
        return (None, message.to_string());
    }
    // ([\w.\- ]+?):(\d+): — the name has no colon, so it ends at the first one
    if let Some(colon) = message.find(':') {
        let name = &message[..colon];
        if !name.is_empty() && name.chars().all(|c| js::is_word(c) || matches!(c, '.' | '-' | ' ')) {
            if let Some((site, text)) = site_and_rest(name, &message[colon + 1..]) {
                return (Some(site), text);
            }
        }
    }
    (None, message.to_string())
}

/// A message with what differs between stories (instance paths, numbers) replaced, so equal causes group
pub fn cause_key(message: &str) -> String {
    // (valid member of \w+) "[^"]*"  ->  $1 "…"
    let mut text = String::new();
    let mut rest = message;
    while let Some(at) = rest.find("valid member of ") {
        let after = &rest[at + "valid member of ".len()..];
        let word = after.len() - after.trim_start_matches(js::is_word).len();
        let quoted = &after[word..];
        let close = quoted.strip_prefix(" \"").and_then(|inner| inner.find('"').map(|end| word + 2 + end + 1));
        match close {
            Some(end) if word > 0 => {
                text.push_str(&rest[..at + "valid member of ".len() + word]);
                text.push_str(" \"…\"");
                rest = &after[end..];
            }
            _ => {
                text.push_str(&rest[..at + "valid member of ".len()]);
                rest = after;
            }
        }
    }
    text.push_str(rest);
    // (Infinite yield possible on ')[^']*(')  ->  $1…$2
    let mut yielded = String::new();
    let mut rest = text.as_str();
    const YIELD: &str = "Infinite yield possible on '";
    while let Some(at) = rest.find(YIELD) {
        let after = &rest[at + YIELD.len()..];
        match after.find('\'') {
            Some(end) => {
                yielded.push_str(&rest[..at + YIELD.len()]);
                yielded.push_str("…'");
                rest = &after[end + 1..];
            }
            None => {
                yielded.push_str(&rest[..at + YIELD.len()]);
                rest = after;
            }
        }
    }
    yielded.push_str(rest);
    // \b\d+(\.\d+)?\b  ->  N
    let chars: Vec<char> = yielded.chars().collect();
    let mut numbered = String::new();
    let mut index = 0;
    let boundary_after = |at: usize| at >= chars.len() || !js::is_word(chars[at]);
    while index < chars.len() {
        let starts = chars[index].is_ascii_digit() && (index == 0 || !js::is_word(chars[index - 1]));
        if starts {
            let mut end = index;
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
            }
            let mut decimal = None;
            if end + 1 < chars.len() && chars[end] == '.' && chars[end + 1].is_ascii_digit() {
                let mut fraction = end + 1;
                while fraction < chars.len() && chars[fraction].is_ascii_digit() {
                    fraction += 1;
                }
                if boundary_after(fraction) {
                    decimal = Some(fraction);
                }
            }
            if let Some(stop) = decimal.or(boundary_after(end).then_some(end)) {
                numbered.push('N');
                index = stop;
                continue;
            }
        }
        numbered.push(chars[index]);
        index += 1;
    }
    js::slice_start(&numbered, 240).to_string()
}

/// Runs one story into `<out>/stories/<slug>` and returns its record
fn sweep_one(context: &Context, entry: &Entry, args: &Args, out_root: &Path, timeout: f64, with_png: bool, git_ref: Option<&Mutex<crate::gitref::GitRef>>) -> Json {
    let dir = out_root.join("stories").join(&entry.slug);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let started = Instant::now();
    // The render, as a child `rur render` of the Node CLI ran it: what it printed, and its summary when it finished
    let (timed_out, code, stderr, summary) = match context.prepare_render(&story_args(entry, &dir, args, with_png), git_ref) {
        Err(failure) => {
            let printed = match &failure {
                Failure::Usage(message) => format!("rur: {message}\n"),
                Failure::Error(message) => format!("Error: {message}\n"),
            };
            (false, Some(failure.code()), printed, None)
        }
        Ok(render) => {
            let outcome = context.run_render(&render, false, Some(Duration::from_secs_f64(timeout)));
            if outcome.timed_out {
                (true, outcome.code, outcome.stderr, None)
            } else {
                match context.finish_render(&render, &outcome) {
                    Ok(summary) => (false, outcome.code, outcome.stderr, Some(summary)),
                    Err(error) => (false, Some(1), format!("{}Error: {error}\n", outcome.stderr), None),
                }
            }
        }
    };
    let stderr = js::slice_end(&stderr, 8000).to_string();
    let mut record = Json::object()
        .with("story", entry.key.as_str())
        .with("place", entry.place.as_str())
        .with("out", format!("stories/{}", entry.slug))
        .with("status", "crash")
        .with("clean", false)
        .with("failure", Json::Null)
        .with("errors", Json::Array(Vec::new()))
        .with("scriptErrors", 0.0)
        .with("warnings", 0.0)
        .with("warningMessages", Json::Array(Vec::new()))
        .with("unemulated", Json::Array(Vec::new()))
        .with("missingImages", Json::Array(Vec::new()))
        .with("missingFonts", Json::Array(Vec::new()))
        .with("renderErrors", Json::Array(Vec::new()))
        .with("nodes", 0.0)
        .with("placeholders", 0.0)
        .with("placeholderRoots", Json::Array(Vec::new()))
        .with("timings", Json::Null)
        .with("seconds", 0.0);
    if timed_out {
        record.set("status", "timeout");
        record.set("failure", format!("timed out after {}s", crate::glyphs::js_number_string(timeout)));
    } else if let Some(summary) = summary {
        let scene = Json::read(&dir.join("scene.json")).unwrap_or_else(Json::object);
        let diagnostics = scene.get("diagnostics");
        let output = diagnostics.get("output").items();
        let messages = |kind: &str| -> Vec<Json> {
            output.iter().filter(|item| item.get("kind").as_str() == Some(kind)).map(|item| Json::from(primary_message(&message_text(item.get("message"))))).take(20).collect()
        };
        let status = summary.get("status");
        record.set("status", if status.truthy() { status.clone() } else { Json::from("unknown") });
        let failure = summary.get("failure");
        record.set("failure", if failure.truthy() { Json::from(primary_message(&failure.text())) } else { Json::Null });
        record.set("errors", messages("error"));
        record.set("scriptErrors", summary.get("scriptErrors").clone());
        record.set("warnings", summary.get("warnings").clone());
        record.set("warningMessages", messages("warn"));
        record.set("unemulated", diagnostics.get("unemulated").items().to_vec());
        record.set("missingImages", summary.get("missingImages").clone());
        record.set("missingFonts", summary.get("missingFonts").clone());
        record.set("nodes", summary.get("nodes").clone());
        let timings = diagnostics.get("timings");
        record.set("timings", if timings.truthy() { timings.clone() } else { Json::Null });
        // Studio-only instances the story touched (created from the place hierarchy, default properties)
        let studio_only = diagnostics.get("studioOnly");
        let count = match studio_only.get("count") {
            Json::Number(number) if !number.is_nan() => *number,
            Json::String(text) => Some(js::number(text)).filter(|number| !number.is_nan()).unwrap_or(0.0),
            _ => 0.0,
        };
        record.set("placeholders", count);
        record.set("placeholderRoots", studio_only.get("used").items().iter().take(10).cloned().collect::<Vec<_>>());
        record.set("renderErrors", summary.get("renderErrors").items().to_vec());
        let clean = record.get("status").as_str() == Some("ok") && record.get("scriptErrors") == &Json::Number(0.0) && record.get("renderErrors").items().is_empty();
        record.set("clean", clean);
    } else {
        // The run failed: the engine says why ("rur-native render: ..."), then the error it failed with
        let lines: Vec<&str> = stderr
            .split('\n')
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("at ") && !line.starts_with("rur: ") && *line != "[Stack Begin]" && *line != "[Stack End]" && !line.starts_with("Script '"))
            .collect();
        let failure = lines.iter().find(|line| !line.starts_with("Error: runtime failed")).or(lines.first()).map_or_else(|| format!("exit code {}", code.map_or("null".to_string(), |code| code.to_string())), |line| line.to_string());
        record.set("failure", failure);
        record.set("detail", js::slice_end(&stderr, 2000));
    }
    record.set("seconds", started.elapsed().as_millis() as f64 / 1000.0);
    record
}

/// An output entry's message as text (`String(message)`)
fn message_text(message: &Json) -> String {
    match message {
        Json::Null => String::new(),
        other => other.text(),
    }
}

/// The problem a progress line or the story table shows: the failure, else the first error or render error
fn first_problem(record: &Json) -> String {
    let failure = record.get("failure");
    if failure.truthy() {
        return failure.text();
    }
    for list in ["errors", "renderErrors"] {
        if let Some(first) = record.get(list).items().first().filter(|first| first.truthy()) {
            return first.text();
        }
    }
    String::new()
}

/// Instance paths (below the DataModel, dot-joined) known to exist in each place's Studio hierarchy, so failures on
/// them count as Studio-only content, not emulation
fn studio_hierarchies(context: &Context, args: &Args) -> HashMap<String, HashSet<String>> {
    let _ = (context, args);
    HashMap::new()
}

/// "X is not a valid member of Class "A.B"" where A.B.X is in the place's Studio hierarchy
fn studio_only_member(message: &str, place: &str, hierarchies: &HashMap<String, HashSet<String>>) -> Option<String> {
    let set = hierarchies.get(place)?;
    const MEMBER: &str = " is not a valid member of ";
    for (at, _) in message.match_indices(MEMBER) {
        let before = &message[..at];
        let name_start = before.rfind(|c: char| js::is_space(c) || c == '"').map_or(0, |index| index + before[index..].chars().next().unwrap().len_utf8());
        let name = &before[name_start..];
        if name.is_empty() {
            continue;
        }
        let after = &message[at + MEMBER.len()..];
        let class = after.len() - after.trim_start_matches(js::is_word).len();
        let Some(quoted) = after[class..].strip_prefix(" \"") else { continue };
        let Some(end) = quoted.find('"').filter(|end| class > 0 && *end > 0) else { continue };
        let full = format!("{}.{name}", &quoted[..end]);
        return set.contains(&full).then_some(full);
    }
    None
}

/// Counts of each key over the records, a record counting a key once, in the order first seen
fn count_by(records: &[Json], key_of: impl Fn(&Json) -> Vec<Json>) -> Vec<(Json, usize)> {
    let mut counts: Vec<(Json, usize)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for record in records {
        let mut seen = HashSet::new();
        for key in key_of(record) {
            let id = key.stringify(0);
            if !seen.insert(id.clone()) {
                continue;
            }
            match index.get(&id) {
                Some(&at) => counts[at].1 += 1,
                None => {
                    index.insert(id, counts.len());
                    counts.push((key, 1));
                }
            }
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    counts
}

/// The report's totals over `records` (which gain `studioOnly` where a failure is Studio-only content)
pub fn aggregate(records: &mut [Json], hierarchies: &HashMap<String, HashSet<String>>) -> Json {
    let mut status: Vec<(String, usize)> = Vec::new();
    for record in records.iter() {
        let name = record.get("status").text();
        match status.iter_mut().find(|(known, _)| *known == name) {
            Some(entry) => entry.1 += 1,
            None => status.push((name, 1)),
        }
    }
    // Failure causes: mount failures, script errors and timeouts / crashes, counted once per story
    struct Cause {
        cause: String,
        stories: Vec<String>,
        sites: Vec<(String, usize)>,
        examples: Vec<String>,
    }
    let mut causes: Vec<Cause> = Vec::new();
    for record in records.iter_mut() {
        let mut found: Vec<(String, String)> = Vec::new();
        let status = record.get("status").text();
        let failure = record.get("failure");
        if status == "timeout" || status == "crash" {
            found.push((if failure.truthy() { failure.text() } else { status.clone() }, status.clone()));
        } else if status == "error" && failure.truthy() {
            found.push((failure.text(), "mount".into()));
        }
        for message in record.get("errors").items() {
            let message = message.text();
            if status == "error" && failure.as_str() == Some(message.as_str()) {
                continue;
            }
            found.push((message, "script".into()));
        }
        if status == "yielded" {
            found.push(("story mount never finished (yielded)".into(), "yielded".into()));
        }
        let story = record.get("story").text();
        let place = record.get("place").text();
        for (message, mut kind) in found {
            let (site, text) = split_site(&message);
            if let Some(studio_only) = studio_only_member(&text, &place, hierarchies) {
                if !record.get("studioOnly").truthy() {
                    record.set("studioOnly", studio_only);
                }
                kind = format!("{kind} (Studio-only instance)");
            }
            let key = format!("{kind}: {}", cause_key(&text));
            let at = match causes.iter().position(|cause| cause.cause == key) {
                Some(at) => at,
                None => {
                    causes.push(Cause { cause: key, stories: Vec::new(), sites: Vec::new(), examples: Vec::new() });
                    causes.len() - 1
                }
            };
            let cause = &mut causes[at];
            if !cause.stories.contains(&story) {
                if cause.examples.len() < 5 {
                    cause.examples.push(story.clone());
                }
                cause.stories.push(story.clone());
            }
            if let Some(site) = site {
                match cause.sites.iter_mut().find(|(known, _)| *known == site) {
                    Some(entry) => entry.1 += 1,
                    None => cause.sites.push((site, 1)),
                }
            }
        }
    }
    causes.sort_by(|a, b| b.stories.len().cmp(&a.stories.len()));
    let causes: Vec<Json> = causes
        .into_iter()
        .map(|mut cause| {
            cause.sites.sort_by(|a, b| b.1.cmp(&a.1));
            let sites: Vec<String> = cause.sites.iter().take(4).map(|(site, count)| format!("{site} ({count})")).collect();
            Json::object().with("cause", cause.cause).with("stories", cause.stories.len()).with("sites", sites).with("examples", cause.examples)
        })
        .collect();
    let mut unemulated: Vec<(Json, f64, f64)> = Vec::new();
    for record in records.iter() {
        for entry in record.get("unemulated").items() {
            let member = entry.get("member");
            let count = entry.get("count").as_f64().unwrap_or(f64::NAN);
            match unemulated.iter_mut().find(|(known, _, _)| known == member) {
                Some(item) => {
                    item.1 += 1.0;
                    item.2 += count;
                }
                None => unemulated.push((member.clone(), 1.0, count)),
            }
        }
    }
    unemulated.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal)));
    let warnings = count_by(records, |record| record.get("warningMessages").items().iter().map(|message| Json::from(cause_key(&split_site(&message.text()).1))).collect());
    let images = count_by(records, |record| record.get("missingImages").items().to_vec());
    let placeholders = count_by(records, |record| record.get("placeholderRoots").items().to_vec());
    let fonts = count_by(records, |record| record.get("missingFonts").items().to_vec());
    let mut seconds: Vec<f64> = records.iter().map(|record| record.get("seconds").as_f64().unwrap_or(f64::NAN)).collect();
    seconds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let percentile = |p: f64| if seconds.is_empty() { 0.0 } else { seconds[(seconds.len() - 1).min((p * seconds.len() as f64).floor() as usize)] };
    let mut slowest: Vec<&Json> = records.iter().collect();
    slowest.sort_by(|a, b| {
        let (a, b) = (a.get("seconds").as_f64().unwrap_or(f64::NAN), b.get("seconds").as_f64().unwrap_or(f64::NAN));
        b.partial_cmp(&a).unwrap_or(std::cmp::Ordering::Equal)
    });
    let listed = |counts: Vec<(Json, usize)>, name: &str| -> Vec<Json> { counts.into_iter().map(|(key, stories)| Json::object().with(name, key).with("stories", stories)).collect() };
    Json::object()
        .with("stories", records.len())
        .with("clean", records.iter().filter(|record| record.get("clean").truthy()).count())
        .with("studioOnly", records.iter().filter(|record| !record.get("clean").truthy() && record.get("studioOnly").truthy()).count())
        .with("status", Json::Object(status.into_iter().map(|(name, count)| (name, Json::from(count))).collect()))
        .with("causes", causes)
        .with("unemulated", unemulated.into_iter().map(|(member, stories, calls)| Json::object().with("member", member).with("stories", stories).with("calls", calls)).collect::<Vec<_>>())
        .with("warnings", listed(warnings, "message"))
        .with("missingImages", listed(images, "image"))
        .with("placeholders", listed(placeholders, "instance"))
        .with("missingFonts", listed(fonts, "font"))
        .with(
            "seconds",
            Json::object()
                .with("total", seconds.iter().sum::<f64>())
                .with("median", percentile(0.5))
                .with("p90", percentile(0.9))
                .with("max", seconds.last().copied().unwrap_or(0.0)),
        )
        .with(
            "slowest",
            slowest
                .into_iter()
                .take(15)
                .map(|record| Json::object().with("story", record.get("story").clone()).with("seconds", record.get("seconds").clone()).with("status", record.get("status").clone()))
                .collect::<Vec<_>>(),
        )
}

/// A table cell's text, cut to 200 characters before escaping (a cut cannot leave half an escape), with backslashes
/// escaped before pipes (text ending in a backslash cannot unescape the cell's own `|`)
pub fn markdown_cell(value: &str) -> String {
    let one_line = value.replace("\r\n", " ").replace('\n', " ");
    js::slice_start(&one_line, 200).replace('\\', "\\\\").replace('|', "\\|")
}

/// `String(value ?? "")` for a table cell
fn cell(value: &Json) -> String {
    markdown_cell(&match value {
        Json::Null => String::new(),
        other => other.text(),
    })
}

/// A number with `digits` decimals (`toFixed`); NaN for a value that is not a number
fn fixed(value: &Json, digits: usize) -> String {
    js::to_fixed(value.as_f64().unwrap_or(f64::NAN), digits)
}

/// summary.md: the report as tables
pub fn markdown(report: &Json) -> String {
    let totals = report.get("totals");
    let options = report.get("options");
    let records = report.get("records").items();
    let mut lines: Vec<String> = vec!["# Story sweep".into(), String::new()];
    lines.push(format!(
        "{} · {} stories · {} clean · jobs {} · timeout {}s · viewport {} @{}x · time {}s · wall {}s",
        report.get("finished").text(),
        totals.get("stories").text(),
        totals.get("clean").text(),
        options.get("jobs").text(),
        options.get("timeout").text(),
        options.get("viewport").text(),
        options.get("scale").text(),
        options.get("time").text(),
        fixed(report.get("wallSeconds"), 0)
    ));
    lines.push(String::new());
    lines.extend(["Clean = mounted without error, no script errors, no render errors.".into(), String::new()]);
    lines.extend(["## Status".into(), String::new(), "| status | stories |".into(), "|---|---|".into()]);
    let mut status: Vec<&(String, Json)> = totals.get("status").entries().iter().collect();
    status.sort_by(|a, b| b.1.as_f64().unwrap_or(0.0).partial_cmp(&a.1.as_f64().unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));
    for (name, count) in status {
        lines.push(format!("| {name} | {} |", count.text()));
    }
    let with_errors = records.iter().filter(|record| record.get("status").as_str() == Some("ok") && record.get("scriptErrors").as_f64().unwrap_or(0.0) > 0.0).count();
    lines.push(format!("| ok with script errors | {with_errors} |"));
    lines.push(format!("| not clean, needs Studio-only content (instances the place has but the repo does not) | {} |", totals.get("studioOnly").text()));
    let with_placeholders = records.iter().filter(|record| record.get("placeholders").as_f64().unwrap_or(0.0) > 0.0).count();
    lines.push(format!("| rendered with Studio-only placeholder instances (default properties) | {with_placeholders} |"));
    lines.push(String::new());
    let changes = report.get("changes");
    if changes.truthy() {
        lines.extend(["## Changes vs previous report".into(), String::new()]);
        let (fixed_list, regressed) = (changes.get("fixed").items(), changes.get("regressed").items());
        lines.push(format!("Fixed (now clean): {} · regressed (no longer clean): {}", fixed_list.len(), regressed.len()));
        lines.push(String::new());
        for story in regressed.iter().take(50) {
            lines.push(format!("- regressed: {}", story.text()));
        }
        if !regressed.is_empty() {
            lines.push(String::new());
        }
    }
    lines.extend(["## Failure causes (stories affected)".into(), String::new(), "| stories | cause | top sites | example |".into(), "|---|---|---|---|".into()]);
    for cause in totals.get("causes").items().iter().take(40) {
        let sites: Vec<String> = cause.get("sites").items().iter().map(Json::text).collect();
        let example = cause.get("examples").items().first().cloned().unwrap_or(Json::Null);
        lines.push(format!("| {} | {} | {} | {} |", cause.get("stories").text(), cell(cause.get("cause")), markdown_cell(&sites.join(", ")), cell(&example)));
    }
    lines.extend([String::new(), "## Unemulated members (stories / calls)".into(), String::new(), "| member | stories | calls |".into(), "|---|---|---|".into()]);
    for item in totals.get("unemulated").items().iter().take(40) {
        lines.push(format!("| {} | {} | {} |", cell(item.get("member")), item.get("stories").text(), item.get("calls").text()));
    }
    lines.extend([String::new(), "## Studio-only instances used (placeholders with default properties)".into(), String::new(), "| stories | instance |".into(), "|---|---|".into()]);
    for item in totals.get("placeholders").items().iter().take(25) {
        lines.push(format!("| {} | {} |", item.get("stories").text(), cell(item.get("instance"))));
    }
    lines.extend([String::new(), "## Warnings (stories)".into(), String::new(), "| stories | warning |".into(), "|---|---|".into()]);
    for item in totals.get("warnings").items().iter().take(20) {
        lines.push(format!("| {} | {} |", item.get("stories").text(), cell(item.get("message"))));
    }
    let images = totals.get("missingImages").items();
    lines.extend([String::new(), format!("## Missing images ({} unique)", images.len()), String::new(), "| stories | image |".into(), "|---|---|".into()]);
    for item in images.iter().take(15) {
        lines.push(format!("| {} | {} |", item.get("stories").text(), cell(item.get("image"))));
    }
    let fonts = totals.get("missingFonts").items();
    lines.extend([String::new(), format!("## Missing fonts ({} unique)", fonts.len()), String::new()]);
    for item in fonts.iter().take(15) {
        lines.push(format!("- {} stories: {}", item.get("stories").text(), item.get("font").text()));
    }
    let seconds = totals.get("seconds");
    lines.extend([
        String::new(),
        "## Time".into(),
        String::new(),
        format!("Total {}s · median {}s · p90 {}s · max {}s", fixed(seconds.get("total"), 0), fixed(seconds.get("median"), 1), fixed(seconds.get("p90"), 1), fixed(seconds.get("max"), 1)),
        String::new(),
    ]);
    for item in totals.get("slowest").items() {
        lines.push(format!("- {}s {} ({})", fixed(item.get("seconds"), 1), item.get("story").text(), item.get("status").text()));
    }
    lines.extend([
        String::new(),
        "## Stories".into(),
        String::new(),
        "| story | place | status | errors | first problem | unemulated | missing images | nodes | s |".into(),
        "|---|---|---|---|---|---|---|---|---|".into(),
    ]);
    for record in records {
        let unemulated: Vec<String> = record.get("unemulated").items().iter().take(3).map(|entry| entry.get("member").text()).collect();
        lines.push(format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            cell(record.get("story")),
            record.get("place").text(),
            record.get("status").text(),
            record.get("scriptErrors").text(),
            markdown_cell(&first_problem(record)),
            markdown_cell(&unemulated.join(", ")),
            record.get("missingImages").items().len(),
            record.get("nodes").text(),
            fixed(record.get("seconds"), 1)
        ));
    }
    lines.join("\n") + "\n"
}

/// `items.slice(0, Number(limit))`
fn limited<T>(mut items: Vec<T>, limit: f64) -> Vec<T> {
    let count = if limit.is_nan() {
        0
    } else if limit < 0.0 {
        items.len().saturating_sub((-limit).trunc() as usize)
    } else {
        (limit.trunc() as usize).min(items.len())
    };
    items.truncate(count);
    items
}

fn write(file: &Path, text: &str) -> Result<(), String> {
    std::fs::write(file, text).map_err(|error| format!("{}: {error}", file.display()))
}

/// The sweep: renders the stories, writes report.json and summary.md, and returns the report
pub fn sweep(context: &Context, args: &Args) -> Result<Json, Failure> {
    let out_root = match args.value("--out") {
        Some(out) => context.resolve(&out),
        None => context.out.join("sweep"),
    };
    let cpus = std::thread::available_parallelism().map_or(1, |count| count.get());
    let jobs = Some(args.number("--jobs")).filter(|jobs| *jobs != 0.0 && !jobs.is_nan()).unwrap_or((cpus / 3).clamp(1, 8) as f64).max(1.0);
    let timeout = Some(args.number("--timeout")).filter(|timeout| *timeout != 0.0 && !timeout.is_nan()).unwrap_or(90.0).max(5.0);
    let with_png = !args.truthy("--no-png");
    let or = |value: f64, default: f64| if value == 0.0 || value.is_nan() { default } else { value };
    // Studio runs at 1.5x DPI here; sweep renders match it unless --scale says otherwise
    let viewport = args.value("--viewport").unwrap_or_else(|| "1200x900".into());
    let (scale, time) = (or(args.number("--scale"), 1.5), or(args.number("--time"), 1.0));
    let filter = match args.get("--filter") {
        Some(Opt::Flag) => Json::Bool(true),
        _ => args.value("--filter").map_or(Json::Null, Json::from),
    };
    let options = Json::object()
        .with("jobs", jobs)
        .with("timeout", timeout)
        .with("viewport", viewport.as_str())
        .with("scale", scale)
        .with("time", time)
        .with("png", with_png)
        .with("filter", filter);
    let mut child_args = args.clone();
    child_args.set_value("--viewport", viewport.clone());
    child_args.set_value("--scale", crate::glyphs::js_number_string(scale));
    child_args.set_value("--time", crate::glyphs::js_number_string(time));
    std::fs::create_dir_all(&out_root).map_err(|error| format!("{}: {error}", out_root.display()))?;
    let report_file = out_root.join("report.json");
    let summary_file = out_root.join("summary.md");
    let previous = Json::read(&report_file);

    // --summarize: rebuild the totals and summary.md from the existing report without rendering
    if args.truthy("--summarize") {
        let Some(mut previous) = previous else { return Err(Failure::Error(format!("no report at {}", report_file.display()))) };
        let mut records = previous.get("records").items().to_vec();
        let totals = aggregate(&mut records, &studio_hierarchies(context, args));
        previous.set("totals", totals);
        if previous.has("records") {
            previous.set("records", records);
        }
        write(&report_file, &previous.stringify(1))?;
        write(&summary_file, &markdown(&previous))?;
        return Ok(previous);
    }

    let mut entries = story_entries(context, args)?;
    if let (true, Some(previous)) = (args.truthy("--failed"), &previous) {
        let not_clean: HashSet<String> = previous.get("records").items().iter().filter(|record| !record.get("clean").truthy()).map(|record| record.get("story").text()).collect();
        entries.retain(|entry| not_clean.contains(&entry.key));
    }
    if args.truthy("--limit") {
        entries = limited(entries, args.number("--limit"));
    }
    let shown_jobs = crate::glyphs::js_number_string(jobs);
    let shown_timeout = crate::glyphs::js_number_string(timeout);
    eprintln!("rur sweep: {} stories, {shown_jobs} jobs, timeout {shown_timeout}s{} -> {}", entries.len(), if with_png { "" } else { ", no png" }, out_root.display());

    let git_ref = args.value("--git-ref").map(|reference| Mutex::new(crate::gitref::GitRef::new(&reference, &context.state)));
    let started = Instant::now();
    let run = |list: &[Entry], progress: bool| -> Vec<Json> {
        let next = AtomicUsize::new(0);
        let done = Mutex::new(0usize);
        let records: Mutex<Vec<Json>> = Mutex::new(Vec::new());
        let workers = (jobs.min(list.len() as f64)).trunc() as usize;
        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    loop {
                        let index = next.fetch_add(1, Ordering::SeqCst);
                        let Some(entry) = list.get(index) else { break };
                        let record = sweep_one(context, entry, &child_args, &out_root, timeout, with_png, git_ref.as_ref());
                        if progress {
                            let mut done = done.lock().unwrap();
                            *done += 1;
                            let problem = if record.get("clean").truthy() { String::new() } else { format!(" {}", js::slice_start(&first_problem(&record), 140)) };
                            let errors = record.get("scriptErrors").as_f64().unwrap_or(0.0);
                            eprintln!(
                                "[{}/{}] {}{} {}s {}{problem}",
                                *done,
                                list.len(),
                                record.get("status").text(),
                                if errors != 0.0 { format!(" ({} errors)", crate::glyphs::js_number_string(errors)) } else { String::new() },
                                fixed(record.get("seconds"), 1),
                                entry.key
                            );
                        }
                        records.lock().unwrap().push(record);
                    }
                });
            }
        });
        records.into_inner().unwrap()
    };
    let mut records = run(&entries, true);

    // A crash is run once more at the end: a runtime edited while a sweep runs (another agent's work, a half-written
    // file) crashes every story that starts during the edit
    let retries = if args.present("--retries") { args.number("--retries") } else { 1.0 };
    let mut attempt = 0.0;
    while attempt < retries {
        attempt += 1.0;
        let crashed: Vec<String> = records.iter().filter(|record| record.get("status").as_str() == Some("crash")).map(|record| record.get("story").text()).collect();
        if crashed.is_empty() {
            break;
        }
        eprintln!("rur sweep: re-running {} crashed stories", crashed.len());
        let retry: Vec<Entry> = crashed.iter().filter_map(|story| entries.iter().find(|entry| entry.key == *story).cloned()).collect();
        for record in run(&retry, false) {
            let story = record.get("story").text();
            if let Some(at) = records.iter().position(|known| known.get("story").text() == story) {
                records[at] = record;
            }
        }
    }
    // --failed re-runs merge into the previous report so it stays complete
    let mut all_records = records;
    if let (true, Some(previous)) = (args.truthy("--failed"), &previous) {
        let rerun: HashMap<String, Json> = all_records.into_iter().map(|record| (record.get("story").text(), record)).collect();
        all_records = previous.get("records").items().iter().map(|record| rerun.get(&record.get("story").text()).cloned().unwrap_or_else(|| record.clone())).collect();
    }
    all_records.sort_by(|a, b| js::locale_compare(&a.get("story").text(), &b.get("story").text()));

    let changes = match &previous {
        Some(previous) if matches!(previous.get("records"), Json::Array(_)) => {
            let before: HashMap<String, &Json> = previous.get("records").items().iter().map(|record| (record.get("story").text(), record)).collect();
            let (mut fixed_list, mut regressed) = (Vec::new(), Vec::new());
            for record in &all_records {
                let story = record.get("story").text();
                let Some(old) = before.get(&story) else { continue };
                let (now, then) = (record.get("clean").truthy(), old.get("clean").truthy());
                if now && !then {
                    fixed_list.push(story.clone());
                }
                if !now && then {
                    regressed.push(story);
                }
            }
            Json::object().with("fixed", fixed_list).with("regressed", regressed)
        }
        _ => Json::Null,
    };
    let wall = started.elapsed().as_millis() as f64 / 1000.0;
    let totals = aggregate(&mut all_records, &studio_hierarchies(context, args));
    let report = Json::object()
        .with("finished", chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
        .with("wallSeconds", wall)
        .with("options", options)
        .with("totals", totals)
        .with("changes", changes)
        .with("records", all_records);
    write(&report_file, &report.stringify(1))?;
    write(&summary_file, &markdown(&report))?;
    Ok(report)
}

/// `rur-native sweep ...`: the sweep, then the totals' headline as JSON on stdout; returns the exit code (64 for a bad
/// option, 1 when it could not run)
pub fn cli(argv: &[String]) -> i32 {
    let run = || -> Result<Json, Failure> {
        let (context, args) = Context::new(argv)?;
        let report = sweep(&context, &args)?;
        let out_root = match args.value("--out") {
            Some(out) => context.resolve(&out),
            None => context.out.join("sweep"),
        };
        let totals = report.get("totals");
        let top = |list: &str, name: &str| -> Vec<String> { totals.get(list).items().iter().take(10).map(|item| format!("{} {}", item.get("stories").text(), item.get(name).text())).collect() };
        Ok(Json::object()
            .with("stories", totals.get("stories").clone())
            .with("clean", totals.get("clean").clone())
            .with("status", totals.get("status").clone())
            .with("topCauses", top("causes", "cause"))
            .with("topUnemulated", top("unemulated", "member"))
            .with("seconds", totals.get("seconds").clone())
            .with("report", out_root.join("summary.md").as_path()))
    };
    match run() {
        Ok(headline) => {
            println!("{}", headline.stringify(2));
            0
        }
        Err(failure) => {
            eprintln!("rur sweep: {}", failure.message());
            failure.code()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_group_by_cause_and_site() {
        assert_eq!(primary_message("error while running\n[string \"A.B\"]:3 function x\nA.B:12: attempt to index nil\nStack Begin"), "A.B:12: attempt to index nil");
        assert_eq!(primary_message("  only line  "), "only line");
        assert_eq!(primary_message(""), "");
        assert_eq!(split_site("[string \"A.B\"]:12: boom"), (Some("A.B:12".to_string()), "boom".to_string()));
        assert_eq!(split_site("Shared.Module-x:7: bad: thing"), (Some("Shared.Module-x:7".to_string()), "bad: thing".to_string()));
        assert_eq!(split_site("no site here"), (None, "no site here".to_string()));
        assert_eq!(split_site("A:12:3: x"), (None, "A:12:3: x".to_string()));
        assert_eq!(cause_key("Frame is not a valid member of Folder \"Gui.Main\""), "Frame is not a valid member of Folder \"…\"");
        assert_eq!(cause_key("Infinite yield possible on 'ReplicatedStorage:WaitForChild(\"X\")'"), "Infinite yield possible on '…'");
        assert_eq!(cause_key("got 12 and 1.5x and v2 and 3.25 (7)"), "got N and N.5x and v2 and N (N)");
        let hierarchies = HashMap::from([("lobby".to_string(), HashSet::from(["Gui.Main.Frame".to_string()]))]);
        assert_eq!(studio_only_member("Frame is not a valid member of Folder \"Gui.Main\"", "lobby", &hierarchies).as_deref(), Some("Gui.Main.Frame"));
        assert_eq!(studio_only_member("Other is not a valid member of Folder \"Gui.Main\"", "lobby", &hierarchies), None);
        assert_eq!(markdown_cell("a|b"), "a\\|b");
        assert_eq!(markdown_cell("ends with \\"), "ends with \\\\");
        assert_eq!(markdown_cell(&("x".repeat(199) + "\\|")), "x".repeat(199) + "\\\\");
        assert_eq!(markdown_cell("two\r\nlines"), "two lines");
        assert_eq!(story_slug("C:/elsewhere/stories/App.story.luau"), "elsewhere.stories.App");
        assert_eq!(limited(vec![1, 2, 3], f64::NAN), Vec::<i32>::new());
        assert_eq!(limited(vec![1, 2, 3], -1.0), [1, 2]);
        assert_eq!(limited(vec![1, 2, 3], 2.7), [1, 2]);
    }

    /// A folder whose name ends in a dot (which Windows' path normalization drops, so a plain listing misses the folder)
    /// is walked as Node's readdir walks it; the stories keep the paths as they are spelt
    #[test]
    fn stories_are_found_in_folders_whose_names_end_in_a_dot() {
        let dir = std::env::temp_dir().join(format!("rur-sweep-walk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(listed(&dir));
        let inner = dir.join("Water.").join("Inner");
        std::fs::create_dir_all(listed(&inner)).unwrap();
        std::fs::write(listed(&inner.join("Deep.story.luau")), "").unwrap();
        std::fs::write(dir.join("Top.story.luau"), "").unwrap();
        std::fs::write(dir.join("Other.luau"), "").unwrap();
        let mut found = Vec::new();
        walk(&dir, false, &mut found).unwrap();
        found.sort();
        assert_eq!(found, [dir.join("Top.story.luau"), inner.join("Deep.story.luau")]);
        std::fs::remove_dir_all(listed(&dir)).unwrap();
    }

    /// The totals of a report: statuses, causes grouped across stories (once per story, a mount error not counted
    /// again as a script error), unemulated members, warnings and the time percentiles
    #[test]
    fn records_aggregate_into_totals() {
        let record = |story: &str, status: &str, failure: Option<&str>, errors: &[&str], seconds: f64| {
            Json::object()
                .with("story", story)
                .with("place", "lobby")
                .with("status", status)
                .with("clean", status == "ok" && errors.is_empty())
                .with("failure", failure.map(Json::from).unwrap_or(Json::Null))
                .with("errors", errors.iter().map(|error| Json::from(*error)).collect::<Vec<_>>())
                .with("warningMessages", vec![Json::from("A:1: careful 3")])
                .with("unemulated", vec![Json::object().with("member", "X.Y").with("count", 2.0)])
                .with("missingImages", Json::Array(Vec::new()))
                .with("missingFonts", Json::Array(Vec::new()))
                .with("placeholderRoots", Json::Array(Vec::new()))
                .with("seconds", seconds)
        };
        let mut records = vec![
            record("a", "ok", None, &[], 1.0),
            record("b", "error", Some("M:4: boom 1"), &["M:4: boom 1", "S:2: nil 5"], 3.0),
            record("c", "ok", None, &["S:9: nil 7", "S:9: nil 8"], 2.0),
            record("d", "crash", Some("rur-native render: died"), &[], 4.0),
        ];
        let totals = aggregate(&mut records, &HashMap::new());
        assert_eq!(totals.get("status").stringify(0), r#"{"ok":2,"error":1,"crash":1}"#);
        assert_eq!(
            totals.get("causes").stringify(0),
            concat!(
                r#"[{"cause":"script: nil N","stories":2,"sites":["S:9 (2)","S:2 (1)"],"examples":["b","c"]},"#,
                r#"{"cause":"mount: boom N","stories":1,"sites":["M:4 (1)"],"examples":["b"]},"#,
                r#"{"cause":"crash: rur-native render: died","stories":1,"sites":[],"examples":["d"]}]"#
            )
        );
        assert_eq!(totals.get("unemulated").stringify(0), r#"[{"member":"X.Y","stories":4,"calls":8}]"#);
        assert_eq!(totals.get("warnings").stringify(0), r#"[{"message":"careful N","stories":4}]"#);
        assert_eq!(totals.get("seconds").stringify(0), r#"{"total":10,"median":3,"p90":4,"max":4}"#);
        assert_eq!(totals.get("clean").as_f64(), Some(1.0));
    }
}
