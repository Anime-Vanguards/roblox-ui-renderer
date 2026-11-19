//! `rur-native debug-report [<story>...]`: a report to send rur's maintainers when stories fail. It renders each story as
//! `rur-native render --story <file>` would, in a child process of its own (so a crash or a hang is reported too), and
//! writes one Markdown file: the setup, and per story its status, failure, traceback, errors and warnings, the engine
//! members it used that rur doesn't emulate, the images and fonts that failed, drawing errors and what rur printed.
//!
//! It is safe to share by default: paths are made relative to the project, the user name, home folder and computer
//! name are replaced, the project folder's own name is left out, and no source code, `print` output or credentials are
//! included. `--source` adds the line of each frame of a failure's traceback, `--prints` the stories' `print` output.
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

const USAGE: &str = "usage: rur-native debug-report [<story file>...] [--out <file>] [--source] [--prints] [--limit <n>]
                               [--timeout <seconds>] [--offline] [--time <s>] [--viewport WxH]
                               [--rojo <file>] [--project <dir>] [--roots <list>] [--place <name>] [--mount <spec>]...

Renders each story (every story of the project when none is named) as `rur-native render` does, each in a process
of its own, and writes a Markdown report to send rur's maintainers (default rur-report.md). Paths are made relative to
the project and your user name, home folder and computer name are replaced; no source code, print output or
credentials are included unless asked:
  --source      the source line of each frame of a failure's traceback (your code)
  --prints      what the stories print with print()
  --limit <n>   at most n stories when none is named (default 100)
  --timeout <s> a story still running after s seconds is stopped and reported (default 120)";

/// The options `debug-report` passes on to each `render` unchanged, with a value
const FORWARDED: [&str; 7] = ["--time", "--viewport", "--rojo", "--project", "--roots", "--place", "--mount"];

#[derive(Debug)]
struct Options {
    stories: Vec<PathBuf>,
    out: PathBuf,
    source: bool,
    prints: bool,
    limit: usize,
    timeout: Duration,
    /// The arguments every render gets besides --story and --dir
    forwarded: Vec<String>,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        stories: Vec::new(),
        out: PathBuf::from("rur-report.md"),
        source: false,
        prints: false,
        limit: 100,
        timeout: Duration::from_secs(120),
        forwarded: Vec::new(),
    };
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        let value = || args.get(index + 1).cloned().filter(|value| !value.is_empty()).ok_or_else(|| format!("{arg} needs a value"));
        match arg {
            "--out" => options.out = PathBuf::from(value()?),
            "--limit" => options.limit = value()?.parse().map_err(|_| "--limit takes a number".to_string())?,
            "--timeout" => options.timeout = Duration::from_secs_f64(value()?.parse().map_err(|_| "--timeout takes seconds".to_string())?),
            "--source" | "--prints" | "--offline" => {
                match arg {
                    "--source" => options.source = true,
                    "--prints" => options.prints = true,
                    _ => options.forwarded.push(arg.into()),
                }
                index += 1;
                continue;
            }
            option if FORWARDED.contains(&option) => options.forwarded.extend([arg.to_string(), value()?]),
            option if option.starts_with('-') => return Err(format!("unknown option {option}")),
            story => {
                options.stories.push(PathBuf::from(story));
                index += 1;
                continue;
            }
        }
        index += 2;
    }
    Ok(options)
}

/// The value after `key` among the forwarded options
fn forwarded<'a>(options: &'a Options, key: &str) -> Option<&'a str> {
    options.forwarded.iter().position(|arg| arg == key).and_then(|index| options.forwarded.get(index + 1)).map(String::as_str)
}

/// The project folder: --project (a folder, or a project file's), --rojo's folder, else the current directory
fn project_dir(options: &Options, cwd: &Path) -> PathBuf {
    let named = forwarded(options, "--project").or_else(|| forwarded(options, "--rojo")).map(|path| cwd.join(path));
    let dir = match named {
        Some(path) if path.is_file() => path.parent().map(Path::to_path_buf).unwrap_or(path),
        Some(path) => path,
        None => cwd.to_path_buf(),
    };
    crate::paths::canonical_path(&dir).unwrap_or(dir)
}

/// Replaces what identifies the person or machine in a text: the project folder (as `<project>`), the report's
/// temporary folder, the state folder, the home folder (`~`), and the user and computer names
pub struct Redactor {
    pairs: Vec<(String, String)>,
}

impl Redactor {
    pub fn new(project: &Path, scratch: &Path, state: &Path) -> Redactor {
        let mut pairs: Vec<(String, String)> = Vec::new();
        let mut add = |from: &Path, to: &str| {
            let text = from.to_string_lossy().trim_start_matches(r"\\?\").trim_end_matches(['\\', '/']).to_string();
            if text.len() > 3 {
                pairs.push((text.clone(), to.into()));
                pairs.push((text.replace('\\', "/"), to.into()));
                pairs.push((text.replace('\\', "\\\\"), to.into()));
            }
        };
        add(scratch, "<report>");
        add(state, "<rur state>");
        add(project, "<project>");
        if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
            add(Path::new(&home), "~");
        }
        for (variable, to) in [("USERNAME", "<user>"), ("USER", "<user>"), ("COMPUTERNAME", "<computer>"), ("HOSTNAME", "<computer>")] {
            if let Some(value) = std::env::var(variable).ok().filter(|value| value.len() >= 3) {
                pairs.push((value, to.into()));
            }
        }
        // The longest first, so a folder is replaced before the home folder it is in
        pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        Redactor { pairs }
    }

    pub fn apply(&self, text: &str) -> String {
        let mut text = text.to_string();
        for (from, to) in &self.pairs {
            text = replace_ignoring_case(&text, from, to);
        }
        redact_tokens(&text)
    }
}

fn replace_ignoring_case(text: &str, from: &str, to: &str) -> String {
    let lower = text.to_lowercase();
    let needle = from.to_lowercase();
    // (lower-casing keeps byte offsets only for ASCII; other texts are replaced as they are)
    if lower.len() != text.len() || needle.len() != from.len() {
        return text.replace(from, to);
    }
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(&needle) {
        out.push_str(&text[at..at + found]);
        out.push_str(to);
        at += found + needle.len();
    }
    out.push_str(&text[at..]);
    out
}

/// A run of 200 or more hex digits, as a Roblox cookie's token is, should one ever reach a message (hashes rur prints
/// are 64 digits)
fn redact_tokens(text: &str) -> String {
    const LONGEST_KEPT: usize = 199;
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    for c in text.chars() {
        if c.is_ascii_hexdigit() {
            run.push(c);
            continue;
        }
        out.push_str(if run.len() > LONGEST_KEPT { "<token>" } else { &run });
        run.clear();
        out.push(c);
    }
    out.push_str(if run.len() > LONGEST_KEPT { "<token>" } else { &run });
    out
}

/// One story's render
struct Run {
    file: PathBuf,
    seconds: f64,
    exit: Option<i32>,
    timed_out: bool,
    scene: Option<Value>,
    manifest: Option<Value>,
    result: Option<Value>,
    stderr: String,
}

impl Run {
    /// The script errors of a run whose mount went well (a React tree's render errors, which React catches)
    fn script_errors(&self) -> usize {
        let output = self.scene.as_ref().and_then(|scene| scene["diagnostics"]["output"].as_array().cloned()).unwrap_or_default();
        output.iter().filter(|entry| entry["kind"] == "error").count()
    }

    fn status(&self) -> String {
        if self.timed_out {
            return "timed out".into();
        }
        match (&self.scene, self.exit) {
            (Some(scene), Some(0)) if scene["status"] == "ok" && self.script_errors() > 0 => {
                format!("ok, with {} script error{}", self.script_errors(), if self.script_errors() == 1 { "" } else { "s" })
            }
            (Some(scene), Some(0)) => scene["status"].as_str().unwrap_or("?").to_string(),
            (Some(scene), _) => format!("{} (rur-native exited with {})", scene["status"].as_str().unwrap_or("?"), self.exit.map_or("no code".into(), |code| code.to_string())),
            (None, Some(code)) => format!("crashed (exit code {code})"),
            (None, None) => "crashed".into(),
        }
    }

    fn ok(&self) -> bool {
        self.status() == "ok"
    }
}

fn render(story: &Path, dir: &Path, options: &Options) -> Run {
    let started = Instant::now();
    let _ = std::fs::create_dir_all(dir);
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("rur-native"));
    let mut command = Command::new(exe);
    command.arg("render").arg("--story").arg(story).arg("--dir").arg(dir).args(&options.forwarded);
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).env("NO_COLOR", "1");
    let mut run = Run { file: story.to_path_buf(), seconds: 0.0, exit: None, timed_out: false, scene: None, manifest: None, result: None, stderr: String::new() };
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            run.stderr = format!("could not start rur-native render: {error}");
            return run;
        }
    };
    // (both pipes are read on threads, so a chatty render can't fill one and stall)
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out_thread = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let err_thread = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                run.exit = status.code();
                break;
            }
            Ok(None) if started.elapsed() > options.timeout => {
                let _ = child.kill();
                let _ = child.wait();
                run.timed_out = true;
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => break,
        }
    }
    let stdout = out_thread.join().unwrap_or_default();
    run.stderr = err_thread.join().unwrap_or_default();
    run.result = stdout.lines().rev().find_map(|line| serde_json::from_str::<Value>(line).ok());
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok().and_then(|text| serde_json::from_str::<Value>(&text).ok());
    run.scene = read("scene.json");
    run.manifest = read("manifest.json");
    run.seconds = started.elapsed().as_secs_f64();
    run
}

/// `ReplicatedStorage.Shared.Button:12 function Render` -> (chunk, line)
fn frame_location(frame: &str) -> Option<(&str, usize)> {
    let location = frame.trim().split(' ').next()?;
    let (chunk, line) = location.rsplit_once(':')?;
    Some((chunk, line.parse().ok()?))
}

/// The file a chunk (a ModuleScript's full name) was loaded from, through the run's Rojo mounts and roots
fn chunk_file(chunk: &str, story: &Path, mounts: &[(String, PathBuf)], roots: &[PathBuf]) -> Option<PathBuf> {
    let names: Vec<&str> = chunk.split('.').collect();
    let stem = story.file_name()?.to_string_lossy().trim_end_matches(".luau").trim_end_matches(".lua").to_string();
    if chunk == stem || chunk.ends_with(&format!(".{stem}")) {
        return Some(story.to_path_buf());
    }
    let candidates = |dir: PathBuf, rest: &[&str]| -> Option<PathBuf> {
        let base = rest.iter().fold(dir, |dir, name| dir.join(name));
        let name = base.file_name()?.to_string_lossy().into_owned();
        let parent = base.parent()?.to_path_buf();
        [
            parent.join(format!("{name}.luau")),
            parent.join(format!("{name}.lua")),
            base.join("init.luau"),
            base.join("init.lua"),
            parent.join(format!("{name}.server.luau")),
            parent.join(format!("{name}.client.luau")),
        ]
        .into_iter()
        .find(|file| file.is_file())
    };
    for (at, source) in mounts {
        let at: Vec<&str> = at.split('/').collect();
        if names.len() >= at.len() && names.iter().zip(&at).all(|(name, mounted)| name == mounted) {
            let rest = &names[at.len()..];
            if rest.is_empty() {
                if source.is_file() {
                    return Some(source.clone());
                }
                return candidates(source.clone(), &[]).or_else(|| [source.join("init.luau"), source.join("init.lua")].into_iter().find(|file| file.is_file()));
            }
            if let Some(file) = candidates(source.clone(), rest) {
                return Some(file);
            }
        }
    }
    roots.iter().find_map(|root| candidates(root.clone(), &names))
}

/// The source line of each frame of a traceback, where the frame's chunk maps to a file
fn with_source(trace: &str, story: &Path, mounts: &[(String, PathBuf)], roots: &[PathBuf]) -> String {
    let mut out = String::new();
    for frame in trace.lines().filter(|frame| !frame.trim().is_empty()) {
        out.push_str(frame.trim_end());
        out.push('\n');
        let Some((chunk, line)) = frame_location(frame) else { continue };
        if chunk.starts_with("rur.") || line == 0 {
            continue;
        }
        let Some(file) = chunk_file(chunk, story, mounts, roots) else { continue };
        if let Some(code) = std::fs::read_to_string(&file).ok().and_then(|text| text.lines().nth(line - 1).map(str::to_string)) {
            out.push_str(&format!("    | {}\n", code.trim()));
        }
    }
    out
}

/// The run's Rojo mounts and roots as `render` resolves them for this story (for --source)
fn run_sources(story: &Path, options: &Options) -> (Vec<(String, PathBuf)>, Vec<PathBuf>) {
    let mut argv: Vec<String> = vec!["--story".into(), story.to_string_lossy().into_owned()];
    argv.extend(options.forwarded.iter().cloned());
    let Ok(resolved) = crate::rojo::resolve_arguments(&argv) else { return (Vec::new(), Vec::new()) };
    let mounts = resolved.project.map(|(_, _, mounts)| mounts.into_iter().filter(|mount| !mount.instance).map(|mount| (mount.path, PathBuf::from(mount.value))).collect()).unwrap_or_default();
    let roots = forwarded(options, "--roots").map(|roots| roots.split(';').filter(|root| !root.is_empty()).map(PathBuf::from).collect()).unwrap_or_default();
    (mounts, roots)
}

fn code_block(out: &mut String, text: &str) {
    out.push_str("```text\n");
    out.push_str(text.trim_end());
    out.push_str("\n```\n");
}

/// The Markdown of one story's run
fn story_section(run: &Run, name: &str, options: &Options, redact: &Redactor) -> String {
    let mut out = format!("### `{name}`: {}\n\n", run.status());
    let diagnostics = run.scene.as_ref().map(|scene| scene["diagnostics"].clone()).unwrap_or(Value::Null);
    let (mounts, roots) = if options.source { run_sources(&run.file, options) } else { (Vec::new(), Vec::new()) };
    let trace_text = |trace: &str| if options.source { with_source(trace, &run.file, &mounts, &roots) } else { trace.to_string() };
    if let Some(failure) = run.scene.as_ref().and_then(|scene| scene["failure"].as_str()) {
        out.push_str(&format!("**Failure:** `{}`\n\n", redact.apply(failure).replace('`', "'")));
    }
    if let Some(hint) = diagnostics["hint"].as_str() {
        out.push_str(&format!("**rur's hint:** {}\n\n", redact.apply(hint)));
    }
    let entries: Vec<&Value> = diagnostics["output"].as_array().into_iter().flatten().collect();
    let shown: Vec<&&Value> = entries.iter().filter(|entry| options.prints || entry["kind"] != "print").collect();
    if !shown.is_empty() {
        out.push_str(&format!("**{}** ({}):\n\n", if options.prints { "Script output" } else { "Errors and warnings" }, shown.len()));
        let mut text = String::new();
        for entry in shown {
            text.push_str(&format!("[{} at {:.2}s] {}\n", entry["kind"].as_str().unwrap_or("?"), entry["time"].as_f64().unwrap_or(0.0), entry["message"].as_str().unwrap_or("")));
            if let Some(trace) = entry["trace"].as_str() {
                for line in trace_text(trace).lines() {
                    text.push_str(&format!("    {line}\n"));
                }
            }
        }
        code_block(&mut out, &redact.apply(&text));
        out.push('\n');
    }
    let prints = entries.iter().filter(|entry| entry["kind"] == "print").count();
    if prints > 0 && !options.prints {
        out.push_str(&format!("{prints} print lines left out (--prints adds them).\n\n"));
    }
    let unemulated: Vec<String> = diagnostics["unemulated"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| format!("`{}` x{}", entry["member"].as_str().unwrap_or("?"), entry["count"]))
        .collect();
    if !unemulated.is_empty() {
        out.push_str(&format!("**Engine members rur doesn't emulate, as used:** {}\n\n", redact.apply(&unemulated.join(", "))));
    }
    let studio = &diagnostics["studioOnly"];
    if studio["count"].as_u64().is_some_and(|count| count > 0) {
        out.push_str(&format!("**Studio-only placeholders:** {} (used: {})\n\n", studio["count"], redact.apply(&studio["used"].to_string())));
    }
    let images: Vec<String> = run
        .manifest
        .as_ref()
        .and_then(|manifest| manifest["images"].as_object())
        .into_iter()
        .flatten()
        .filter_map(|(uri, entry)| entry.get("error").map(|error| format!("- `{uri}`: {}", error.as_str().map(str::to_string).unwrap_or_else(|| error.to_string()))))
        .collect();
    if !images.is_empty() {
        out.push_str(&format!("**Images that failed** ({}):\n\n{}\n\n", images.len(), redact.apply(&images.join("\n"))));
    }
    let fonts: Vec<String> = diagnostics["fonts"].as_array().into_iter().flatten().map(|entry| format!("- {}: {}", entry["key"].as_str().unwrap_or("?"), entry["message"].as_str().unwrap_or(""))).collect();
    let missing_fonts = diagnostics["missingFontAssets"].as_array().map(Vec::len).unwrap_or(0);
    if !fonts.is_empty() || missing_fonts > 0 {
        out.push_str(&format!("**Fonts:** {missing_fonts} cloud font assets missing\n{}\n\n", redact.apply(&fonts.join("\n"))));
    }
    let drawing: Vec<String> = run.result.as_ref().map(|result| result["renderErrors"].as_array().into_iter().flatten().filter_map(|error| error.as_str().map(str::to_string)).collect()).unwrap_or_default();
    if !drawing.is_empty() {
        out.push_str("**Drawing errors:**\n\n");
        code_block(&mut out, &redact.apply(&drawing.join("\n")));
        out.push('\n');
    }
    if let Some(timings) = diagnostics.get("timings") {
        out.push_str(&format!("Timings (s): project {:.2}, story {:.2}, run {:.2}; the whole render took {:.1} s.\n\n", timings["mount"].as_f64().unwrap_or(0.0), timings["story"].as_f64().unwrap_or(0.0), timings["run"].as_f64().unwrap_or(0.0), run.seconds));
    }
    if !run.ok() || run.stderr.lines().count() > 2 {
        out.push_str("<details><summary>What rur-native render printed</summary>\n\n");
        code_block(&mut out, &redact.apply(&run.stderr));
        out.push_str("\n</details>\n\n");
    }
    out
}

/// The setup: rur's version and build, the platform, and doctor's checks (what they found, never paths or secrets)
fn setup_section(redact: &Redactor, project: &Path, options: &Options) -> String {
    let mut out = String::from("## Setup\n\n");
    let build = if crate::paths::is_standalone() { "release build" } else { "checkout build" };
    out.push_str(&format!("- rur-native {} ({build}), {} {}\n", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH));
    for (name, value, problem) in crate::doctor::checks() {
        if name == "rur" || name == "state" {
            continue;
        }
        let value = match name {
            // (where Studio is installed says nothing rur needs; whether it is found does)
            "studio" if !problem => "found".to_string(),
            _ => redact.apply(&value),
        };
        out.push_str(&format!("- {name}: {}{value}\n", if problem { "problem: " } else { "" }));
    }
    let project_files = crate::rojo::project_files_in(project);
    let names: Vec<String> = project_files.iter().filter_map(|file| file.file_name()).map(|name| name.to_string_lossy().into_owned()).collect();
    out.push_str(&format!(
        "- project: {}{}\n",
        if names.is_empty() { "no Rojo project file".to_string() } else { format!("Rojo project files {}", names.join(", ")) },
        if crate::rojo::declares_places(project) { "; rur.json declares places" } else { "" }
    ));
    if let Ok(Some(file)) = crate::rojo::project_file_in(project) {
        if let Ok(mappings) = crate::rojo::mappings(&file) {
            out.push_str("- what it maps:\n");
            for (at, source, exists) in mappings {
                out.push_str(&format!("  - {} = {}{}\n", crate::rojo::dotted(&at), redact.apply(&source.to_string_lossy()), if exists { "" } else { " (doesn't exist)" }));
            }
        }
    }
    if !options.forwarded.is_empty() {
        out.push_str(&format!("- options: `{}`\n", redact.apply(&options.forwarded.join(" "))));
    }
    out.push('\n');
    out
}

pub fn cli(args: &[String]) -> i32 {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return 0;
    }
    let options = match parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("rur-native debug-report: {error}\n\n{USAGE}");
            return 2;
        }
    };
    let Ok(cwd) = std::env::current_dir() else {
        eprintln!("rur-native debug-report: no current directory");
        return 1;
    };
    let project = project_dir(&options, &cwd);
    let mut stories: Vec<PathBuf> = options.stories.iter().map(|story| cwd.join(story)).collect();
    if let Some(missing) = stories.iter().find(|story| !story.is_file()) {
        eprintln!("rur-native debug-report: {} is not a file", missing.display());
        return 2;
    }
    let listed = stories.is_empty();
    if listed {
        if project.parent().is_none() {
            eprintln!("rur-native debug-report: {} is the root of a drive; run it in your project folder, or name stories", project.display());
            return 2;
        }
        match crate::catalog::scan(&project, forwarded(&options, "--roots")) {
            Ok(entries) => stories = entries.into_iter().map(|entry| entry.file).collect(),
            Err(error) => {
                eprintln!("rur-native debug-report: {error}");
                return 1;
            }
        }
        if stories.is_empty() {
            eprintln!("rur-native debug-report: no *.story.luau or *.story.lua files in {}; run it in your project folder, or name stories", project.display());
            return 2;
        }
    }
    let total = stories.len();
    stories.truncate(options.limit.max(1));
    let scratch = std::env::temp_dir().join(format!("rur-debug-report-{}", std::process::id()));
    let state = crate::state_dir(&crate::paths::default_repo());
    let redact = Redactor::new(&project, &scratch, &state);
    let name_of = |story: &Path| -> String {
        let relative = story.strip_prefix(&project).map(Path::to_path_buf).unwrap_or_else(|_| story.to_path_buf());
        redact.apply(&relative.to_string_lossy().replace('\\', "/"))
    };
    let mut runs = Vec::new();
    for (index, story) in stories.iter().enumerate() {
        let run = render(story, &scratch.join(index.to_string()), &options);
        crate::term::note(&format!("[{}/{}] {}: {}", index + 1, stories.len(), name_of(story), crate::term::status(&run.status())));
        runs.push(run);
    }
    let _ = std::fs::remove_dir_all(&scratch);

    let mut report = String::from("# rur debug report\n\n");
    report.push_str(&format!(
        "Written by `rur-native debug-report`. Paths are relative to the project (`<project>`), and the user name, home folder (`~`) and computer name are replaced. It holds no source code{}, no `print` output{} and no credentials. Read it before you share it.\n\n",
        if options.source { " except the traceback lines --source asked for" } else { "" },
        if options.prints { " except what --prints asked for" } else { "" },
    ));
    report.push_str(&setup_section(&redact, &project, &options));
    let failed = runs.iter().filter(|run| !run.ok()).count();
    report.push_str(&format!("## Stories\n\n{} run{}, {} ok, {failed} not ok.\n\n", runs.len(), if listed && total > runs.len() { format!(" (the first {} of {total}; --limit runs more)", runs.len()) } else { String::new() }, runs.len() - failed));
    // Every unemulated member across the stories: what rur's maintainers would add first
    let mut members: Vec<(String, u64)> = Vec::new();
    for run in &runs {
        for entry in run.scene.as_ref().and_then(|scene| scene["diagnostics"]["unemulated"].as_array()).into_iter().flatten() {
            let member = entry["member"].as_str().unwrap_or("?").to_string();
            let count = entry["count"].as_u64().unwrap_or(0);
            match members.iter_mut().find(|(name, _)| *name == member) {
                Some(existing) => existing.1 += count,
                None => members.push((member, count)),
            }
        }
    }
    members.sort_by(|a, b| b.1.cmp(&a.1));
    if !members.is_empty() {
        let listed: Vec<String> = members.iter().map(|(member, count)| format!("`{member}` x{count}")).collect();
        report.push_str(&format!("Engine members rur doesn't emulate, across the stories: {}\n\n", redact.apply(&listed.join(", "))));
    }
    report.push_str("| Story | Status | Seconds |\n|---|---|---|\n");
    for run in &runs {
        report.push_str(&format!("| `{}` | {} | {:.1} |\n", name_of(&run.file), run.status(), run.seconds));
    }
    report.push('\n');
    // The stories that aren't ok first, then the ok ones with something to say
    let mut ordered: Vec<&Run> = runs.iter().filter(|run| !run.ok()).collect();
    ordered.extend(runs.iter().filter(|run| run.ok()));
    for run in ordered {
        let section = story_section(run, &name_of(&run.file), &options, &redact);
        let notable = !run.ok() || section.lines().filter(|line| !line.trim().is_empty()).count() > 2;
        if notable {
            report.push_str(&section);
        }
    }
    let out = cwd.join(&options.out);
    if let Err(error) = std::fs::write(&out, &report) {
        eprintln!("rur-native debug-report: {}: {error}", out.display());
        return 1;
    }
    crate::term::note(&format!("{} stories, {failed} not ok -> {}", runs.len(), out.display()));
    crate::term::note("Read the report before you share it: it has no source code, paths outside the project or credentials.");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_replaces_folders_and_names_case_blind() {
        let redact = Redactor {
            pairs: vec![
                (r"D:\Profiles\Someone\work\game".into(), "<project>".into()),
                ("D:/Profiles/Someone/work/game".into(), "<project>".into()),
                (r"D:\Profiles\Someone".into(), "~".into()),
                ("Someone".into(), "<user>".into()),
            ],
        };
        assert_eq!(redact.apply(r"d:\profiles\someone\work\game\src\A.luau and D:\Profiles\Someone\.rokit"), r"<project>\src\A.luau and ~\.rokit");
        assert_eq!(redact.apply("D:/Profiles/Someone/work/game/x by someone"), "<project>/x by <user>");
        let token = "A1".repeat(150);
        let digest = "f".repeat(64);
        assert_eq!(redact.apply(&format!("value {token} end {digest}")), format!("value <token> end {digest}"));
    }

    #[test]
    fn options_split_into_own_and_forwarded() {
        let args: Vec<String> = ["a.story.luau", "--source", "--rojo", "x.project.json", "--offline", "--out", "r.md", "--limit", "5"].iter().map(|s| s.to_string()).collect();
        let options = parse(&args).unwrap();
        assert_eq!(options.stories, [PathBuf::from("a.story.luau")]);
        assert!(options.source && !options.prints);
        assert_eq!(options.out, PathBuf::from("r.md"));
        assert_eq!(options.limit, 5);
        assert_eq!(options.forwarded, ["--rojo", "x.project.json", "--offline"]);
        assert!(parse(&["--bogus".to_string()]).unwrap_err().contains("unknown option --bogus"));
    }

    #[test]
    fn a_frame_maps_to_its_line_through_a_mount() {
        let dir = std::env::temp_dir().join(format!("rur-report-frames-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Packages/Lib")).unwrap();
        std::fs::write(dir.join("Packages/Lib/init.luau"), "local a = 1\nreturn a < nil\n").unwrap();
        let story = dir.join("Button.story.luau");
        std::fs::write(&story, "return 1\n").unwrap();
        let mounts = vec![("ReplicatedStorage/Packages".to_string(), dir.join("Packages"))];
        let out = with_source("ReplicatedStorage.Packages.Lib:2 function f\nrur.prelude:10\nReplicatedStorage.RURFixtures.Button.story:1\n", &story, &mounts, &[]);
        assert_eq!(out, "ReplicatedStorage.Packages.Lib:2 function f\n    | return a < nil\nrur.prelude:10\nReplicatedStorage.RURFixtures.Button.story:1\n    | return 1\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
