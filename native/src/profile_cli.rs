//! The engine's `profile diff` and `profile report` commands with standalone output defaults.
//!
//! Left alone, both commands write their pages next to their inputs. That is fine in a checkout,
//! but an installed binary is pointed at profiles from anywhere, and it should not add files to
//! a folder the user only reads. In standalone mode they default to a folder under the state
//! directory instead; `--out <dir>` still names another, relative to the current directory.
use std::path::{Path, PathBuf};

use crate::{engine::profiler, paths};

/// The positional inputs of each profile command, as `(command, input names in order, how many
/// are required)`. `view` and `render` take a second profile to compare with the first. The
/// release tests read this table from the source to build their cases, so keep one entry per line.
const POSITIONAL_PATHS: &[(&str, &[&str], usize)] = &[
    ("report", &["input"], 1),
    ("diff", &["before", "after"], 2),
    ("view", &["input", "after"], 1),
    ("render", &["input", "after"], 1),
];

/// The options of a profile command that are followed by a value, which is not an input.
fn options_with_values(command: &str) -> &'static [&'static str] {
    match command {
        "report" => &["--out"],
        "diff" => &["--out", "--top"],
        "view" | "render" => &[
            "--view", "--views", "--metric", "--select", "--theme", "--keys", "--size", "--out",
            "--scale", "--format",
        ],
        _ => &[],
    }
}

/// The arguments that are inputs: every one that is not a listed option or that option's value.
fn positional_arguments<'a>(argv: &'a [String], valued_options: &[&str]) -> Vec<&'a str> {
    let mut inputs = Vec::new();
    let mut index = 0;
    while index < argv.len() {
        if valued_options.contains(&argv[index].as_str()) {
            // Skip the option's value too
            index += 2;
        } else {
            inputs.push(argv[index].as_str());
            index += 1;
        }
    }
    inputs
}

/// How an input is named in an error, for example `profile diff before`.
fn input_label(command: &str, name: &str) -> String {
    format!("profile {command} {name}")
}

/// Checks the inputs of a profile command in a release binary, before startup creates the state
/// directory and before the command reads anything. An empty input would otherwise select the
/// `profile.json` in the current folder, which the user never named, and a flag in an input's place
/// would be read as a path. A checkout build keeps the engine's handling of both.
///
/// An unknown flag counts as an input, so it is refused like any other value that is not a path.
pub(crate) fn validate_arguments(command: &str, argv: &[String]) -> Result<(), String> {
    if !paths::is_standalone() {
        return Ok(());
    }
    let Some((_, names, required)) = POSITIONAL_PATHS
        .iter()
        .find(|(name, _, _)| *name == command)
    else {
        // Not a command with inputs; its own parser reports what is wrong
        return Ok(());
    };
    // The path options, such as `--out`, follow the same rules as in every other command
    paths::validate_arguments(argv)?;
    let argv = crate::engine::launch::split_equals(argv);
    let inputs = positional_arguments(&argv, options_with_values(command));
    for (position, input) in inputs.iter().enumerate() {
        // An input past the named ones is checked too; the command's own parser may refuse it
        let name = names.get(position).unwrap_or(&"input");
        paths::validate_path_value(&input_label(command, name), input)?;
    }
    if inputs.len() < *required {
        let label = input_label(command, names[inputs.len()]);
        return Err(format!("{label} needs a nonempty value"));
    }
    Ok(())
}

/// `rur-native profile diff <before> <after> [--out <dir>] [--top <n>]`. Only the output default
/// differs from the engine's, so the engine's own command does the work.
pub fn diff(argv: &[String]) -> i32 {
    if !paths::is_standalone() {
        return profiler::diff::cli(argv);
    }
    let mut argv = crate::engine::launch::split_equals(argv);
    if let Err(error) = paths::validate_arguments(&argv) {
        eprintln!("rur-native profile diff: {error}");
        return 2;
    }
    if !argv.iter().any(|arg| arg == "--out") {
        let default_out = paths::standalone_output("profile-diff");
        argv.extend(["--out".into(), default_out.to_string_lossy().into_owned()]);
    }
    profiler::diff::cli(&argv)
}

/// `rur-native profile report <profile dir|file> [--out <dir>]`. The engine's report command has
/// no output option, so in standalone mode this runs its steps with one.
pub fn report(argv: &[String]) -> i32 {
    if !paths::is_standalone() {
        return profiler::diff::report_cli(argv);
    }
    let argv = crate::engine::launch::split_equals(argv);
    let (input, out) = match report_arguments(&argv) {
        Ok(arguments) => arguments,
        Err(message) => {
            eprintln!("{message}");
            return 2;
        }
    };
    match write_report(&input, &out) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("rur-native profile report: {error}");
            1
        }
    }
}

/// The profile to report on and the directory for its page, from the arguments after
/// `profile report`. The first argument that is not `--out` or its value is the profile.
fn report_arguments(argv: &[String]) -> Result<(PathBuf, PathBuf), String> {
    paths::validate_arguments(argv)?;
    let mut input = None;
    let mut out = paths::standalone_output("profile-report");
    let mut index = 0;
    while index < argv.len() {
        if argv[index] == "--out" {
            let path = argv
                .get(index + 1)
                .ok_or("rur-native profile report: --out needs a directory")?;
            out = PathBuf::from(path);
            index += 2;
        } else {
            input.get_or_insert_with(|| Path::new(&argv[index]));
            index += 1;
        }
    }
    let input = input
        .ok_or("usage: rur-native profile report <profile dir|file> [--out <dir>]")?
        .to_path_buf();
    Ok((input, out))
}

/// Writes `report.html` for the profile in `input` (a `profile.json` or the folder holding it)
/// into `out`, prints the profile's summary and says where the page went.
fn write_report(input: &Path, out: &Path) -> anyhow::Result<()> {
    let file = if input.is_file() {
        input.to_path_buf()
    } else {
        input.join("profile.json")
    };
    let text = std::fs::read_to_string(&file)?;
    let profile: serde_json::Value = serde_json::from_str(&text)?;
    std::fs::create_dir_all(out)?;
    let page = out.join("report.html");
    std::fs::write(&page, profiler::report::html(&text))?;
    print!("{}", profile["summary"].as_str().unwrap_or_default());
    eprintln!("Saved {}", page.display());
    Ok(())
}
