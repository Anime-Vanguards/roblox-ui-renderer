//! Inventories the parsers, so that a path option, positional input or request field cannot be added
//! without the release binary's path check covering it.
//!
//! A hand-kept list of path options kept missing ones. This module finds the options and request
//! fields in the source instead, and fails until each one is either a checked path or listed here
//! as a value that is not a path. The installed tests take their option list from the production
//! table (`path_flags`), never from a copy.
use std::{collections::BTreeSet, path::Path};

/// The string literals of `source`: the pieces between an opening and a closing quote.
fn quoted(source: &str) -> impl Iterator<Item = &str> {
    source.split('"').skip(1).step_by(2)
}

/// The text of a source file before its unit tests, which spell options that no parser accepts.
fn without_tests(source: &str) -> &str {
    source.split("#[cfg(test)]").next().unwrap()
}

/// The production table of path options, read from the source that checks them.
pub fn path_flags() -> Vec<&'static str> {
    let source = include_str!("../../src/paths.rs");
    let table = source
        .split_once("const PATH_OPTIONS:")
        .unwrap()
        .1
        .split_once("];")
        .unwrap()
        .0;
    quoted(table).collect()
}

/// One profile command's positional inputs: its name, the names of its inputs in order, and how
/// many of them are required.
type PositionalPaths<'a> = (&'a str, Vec<&'a str>, usize);

/// The production table of positional profile paths, read from the source that checks them. Each
/// entry is one line, for example `("diff", &["before", "after"], 2),`.
pub fn positional_paths() -> Vec<PositionalPaths<'static>> {
    read_positional_paths(include_str!("../../src/profile_cli.rs"))
}

/// The table of positional paths in `source`, which may have either kind of line ending: GitHub's
/// Windows runners check the source out with CRLF.
fn read_positional_paths(source: &str) -> Vec<PositionalPaths<'_>> {
    let table = source
        .split_once("const POSITIONAL_PATHS:")
        .unwrap()
        .1
        .split_once("];")
        .unwrap()
        .0;
    // End the table at `];` and let `lines` take the line endings off. Ending it at `\n];` would
    // leave a `\r` after the last entry on a CRLF checkout, and the count would not parse.
    let entries = table
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('('));
    entries
        .map(|entry| {
            // The command comes first among the strings, then the input names
            let mut strings = quoted(entry);
            let command = strings.next().unwrap();
            let names = strings.collect();
            // The required count follows the names: `&["before", "after"], 2),`
            let required = entry
                .rsplit_once("], ")
                .unwrap()
                .1
                .trim_end_matches("),")
                .parse()
                .unwrap();
            (command, names, required)
        })
        .collect()
}

/// A parser's body, through its closing brace at the same indentation as its declaration.
/// These are ordinary Rust functions; nested blocks close at a deeper indentation.
fn parser_body<'a>(source: &'a str, declaration: &str) -> &'a str {
    let start = source
        .find(declaration)
        .unwrap_or_else(|| panic!("missing parser: {declaration}"));
    let indentation = source[..start].rsplit('\n').next().unwrap();
    assert!(
        indentation.trim().is_empty(),
        "parser declaration must start a line"
    );
    let body = &source[start..];
    let closing = format!("\n{indentation}}}");
    let end = body.find(&closing).expect("parser closing brace") + closing.len();
    &body[..end]
}

/// A fingerprint of `source` that ignores whitespace, so that reformatting a parser does not change
/// it. This is FNV-1a, written out because the standard library's hashers do not promise the same
/// values from one Rust version to the next.
fn parser_fingerprint(source: &str) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    source
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .fold(OFFSET_BASIS, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(PRIME)
        })
}

/// Which crate a reviewed parser's file is in.
enum Crate {
    /// `rur-native`
    Native,
    /// The pinned engine, which `rur-native` hands the command line on to
    Engine,
}

/// The parsers and dispatchers whose positional inputs were reviewed, as `(crate, file,
/// declaration, fingerprint of the body)`. Positional reads have no option name to discover, so
/// a change to any of these fails the inventory until someone reviews it again. That includes a
/// new command, and a positional read in a parser that used to accept only options.
const REVIEWED_PARSERS: &[(Crate, &str, &str, u64)] = &[
    (
        Crate::Native,
        "src/main.rs",
        "fn main()",
        0xef48f7ff2870c5f9,
    ),
    (
        Crate::Native,
        "src/engine/cli.rs",
        "pub fn dispatch()",
        0xec582eeea95ba2d,
    ),
    (
        Crate::Native,
        "src/engine/cli.rs",
        "pub fn parse(",
        0xd64899b928ddd817,
    ),
    (
        Crate::Native,
        "src/serve.rs",
        "pub fn cli(",
        0xe9cede2cfe5e04f5,
    ),
    (
        Crate::Native,
        "src/story_render.rs",
        "pub fn cli(",
        0x44483cf7af4097e0,
    ),
    (
        Crate::Native,
        "src/story_render.rs",
        "pub fn sequence_cli(",
        0x82d6f9190b44a8c7,
    ),
    (
        Crate::Native,
        "src/story_render.rs",
        "fn profile_options(",
        0x7ae20c86eff1512,
    ),
    (
        Crate::Native,
        "src/paths.rs",
        "pub fn record_arguments(",
        0x3fb3894075e89463,
    ),
    (
        Crate::Native,
        "src/profile_cli.rs",
        "pub fn diff(",
        0x315b2700a4d46ad,
    ),
    (
        Crate::Native,
        "src/profile_cli.rs",
        "pub fn report(",
        0x1c651aebf155a92d,
    ),
    (
        Crate::Native,
        "src/profile_cli.rs",
        "fn report_arguments(",
        0x2b9dc3541d2284e1,
    ),
    (
        Crate::Engine,
        "src/launch.rs",
        "pub fn parse(",
        0x73d661f9db8ad9ac,
    ),
    (
        Crate::Engine,
        "src/launch.rs",
        "pub fn accept(",
        0xe6c9d18304265f16,
    ),
    (
        Crate::Engine,
        "src/record/run.rs",
        "pub fn parse(",
        0x33d01c615a8b3286,
    ),
    (
        Crate::Engine,
        "src/profiler/diff.rs",
        "pub fn cli(",
        0x27935c9b0915f010,
    ),
    (
        Crate::Engine,
        "src/profiler/diff.rs",
        "pub fn report_cli(",
        0x820ad33259ceb03f,
    ),
    (
        Crate::Engine,
        "profile-view/src/lib.rs",
        "pub fn main(",
        0x579bd64fad9d3b67,
    ),
    (
        Crate::Engine,
        "profile-view/src/cli.rs",
        "pub fn parse(",
        0x94f13f9205e7b0a4,
    ),
];

/// The profile commands' inputs are the only positional paths in the reviewed parsers.
fn assert_profile_commands_are_the_positional_paths() {
    assert_eq!(
        positional_paths(),
        vec![
            ("report", vec!["input"], 1),
            ("diff", vec!["before", "after"], 2),
            ("view", vec!["input", "after"], 1),
            ("render", vec!["input", "after"], 1),
        ]
    );
}

/// The desktop command line is parsed by Clap, which generates its positionals from the
/// declaration. It has none; a new one needs a path check.
fn assert_clap_declares_no_positionals() {
    use clap::CommandFactory;
    assert!(
        rur_native::NativeOptions::command()
            .get_positionals()
            .next()
            .is_none(),
        "classify new desktop positional arguments"
    );
}

/// Every reviewed parser still has the body that was reviewed.
fn assert_reviewed_parsers_are_unchanged() {
    let native = Path::new(env!("CARGO_MANIFEST_DIR"));
    let engine = engine_dir(native);
    for (location, file, declaration, expected) in REVIEWED_PARSERS {
        let root = match location {
            Crate::Native => native,
            Crate::Engine => engine.as_path(),
        };
        let source = std::fs::read_to_string(root.join(file)).unwrap();
        let actual = parser_fingerprint(parser_body(&source, declaration));
        assert_eq!(
            actual, *expected,
            "{file}: {declaration} changed; review its positional paths and update their validation and inventory (fingerprint {actual:#018x})"
        );
    }
}

/// The table above only protects anything if startup runs the check.
fn assert_startup_checks_the_profile_commands() {
    let startup = include_str!("../../src/paths.rs");
    assert!(startup.contains("crate::profile_cli::validate_arguments(command, &argv[3..])"));
}

#[test]
fn every_parser_positional_has_a_path_classification() {
    // The profile paths are the only positional paths. Every other reviewed parser ignores
    // unknown words, rejects them (Clap), or passes them to one of the inventoried parsers.
    assert_profile_commands_are_the_positional_paths();
    assert_clap_declares_no_positionals();
    assert_reviewed_parsers_are_unchanged();
    assert_startup_checks_the_profile_commands();
}

/// Every `--option` spelled as a word between quotes in `source`.
fn flags(source: &str) -> BTreeSet<String> {
    let is_option = |word: &&str| {
        word.starts_with("--")
            && word.len() > 2
            && word[2..]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    };
    source
        .split('"')
        .filter(is_option)
        .map(str::to_string)
        .collect()
}

/// The options spelled in every `.rs` file below `dir`, outside unit tests.
fn source_flags(dir: &Path, options: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            source_flags(&path, options);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let mut source = std::fs::read_to_string(&path).unwrap();
            // The path table cannot count as evidence that a parser accepts an option, or the
            // check could never find an option the table has but the parsers lack.
            if let Some((before, table)) = source.split_once("const PATH_OPTIONS:") {
                source = format!("{before}{}", table.split_once("];").unwrap().1);
            }
            options.extend(flags(without_tests(&source)));
        }
    }
}

/// The options of `NativeOptions`. Clap's derive can add options without spelling their flags
/// anywhere in the source.
fn clap_flags(options: &mut BTreeSet<String>) {
    use clap::CommandFactory;
    for arg in rur_native::NativeOptions::command().get_arguments() {
        if let Some(long) = arg.get_long() {
            options.insert(format!("--{long}"));
        }
        for alias in arg.get_all_aliases().into_iter().flatten() {
            options.insert(format!("--{alias}"));
        }
    }
}

/// The folder of the pinned engine's source. Cargo says where the dependency it resolved is, so
/// a pin update also updates this inventory.
fn engine_dir(native: &Path) -> std::path::PathBuf {
    let output = std::process::Command::new(env!("CARGO"))
        .current_dir(native)
        .args(["metadata", "--locked", "--format-version", "1"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let engine = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["name"] == "roblox-engine-emulator")
        .unwrap();
    Path::new(engine["manifest_path"].as_str().unwrap())
        .parent()
        .unwrap()
        .to_path_buf()
}

/// The options that the pinned engine's launch, recorder and profile commands parse. `rur-native`
/// hands the command line on to them, so they take paths from it too.
fn engine_flags(engine: &Path, options: &mut BTreeSet<String>) {
    let parsers = [
        "src/launch.rs",
        "src/record/run.rs",
        "src/profiler/mod.rs",
        "src/profiler/diff.rs",
    ];
    for file in parsers {
        let source = std::fs::read_to_string(engine.join(file)).unwrap();
        options.extend(flags(without_tests(&source)));
    }
    source_flags(&engine.join("src/record"), options);
    // The profile viewer strips the dashes before it matches an option's name, so its names are
    // read from the arms of that match.
    let viewer = std::fs::read_to_string(engine.join("profile-view/src/cli.rs")).unwrap();
    let arms = viewer
        .split_once("match name {")
        .unwrap()
        .1
        .split_once("_ =>")
        .unwrap()
        .0;
    for line in arms.lines() {
        if let Some((arm, _)) = line.split_once("=>") {
            options.extend(quoted(arm).map(|name| format!("--{name}")));
        }
    }
}

/// Parsed options whose value is not a path: numbers, booleans, selectors and labels. The
/// file-or-JSON options (`--controls`, `--context`) are paths, so they are in the production table
/// instead.
const NON_PATH_OPTIONS: &[&str] = &[
    "--all-layers",
    "--assets",
    "--assets-at",
    "--background",
    "--benchmark",
    "--camera",
    "--code",
    "--crop",
    "--device",
    "--dt",
    "--duration",
    "--format",
    "--fps",
    "--frame-hashes",
    "--frames",
    "--from",
    "--headless",
    "--help",
    "--idle",
    "--keys",
    "--margin",
    "--max-script-timeouts",
    "--max-wall-seconds",
    "--metric",
    "--mode",
    "--no-parent-watch",
    "--no-png",
    "--no-snapshots",
    "--no-video",
    "--offline",
    "--pad",
    "--place-id",
    "--placeholders",
    "--player",
    "--port",
    "--profile",
    "--profile-alloc-step",
    "--profile-counters",
    "--profile-phase",
    "--profile-top",
    "--rotation-clipping",
    "--run-context",
    "--save-frames",
    "--scale",
    "--scenes",
    "--script-timeout",
    "--seconds",
    "--select",
    "--session",
    "--size",
    "--streaming",
    "--strict-assets",
    "--studio",
    "--test-hooks",
    "--theme",
    "--time",
    "--timing-check",
    "--to",
    "--top",
    "--topbar-inset",
    "--verbose",
    "--version",
    "--view",
    "--viewport",
    "--views",
    "--vsync",
    "--warmup",
];

#[test]
fn every_parser_option_has_a_path_classification() {
    let native = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut options = BTreeSet::new();
    source_flags(&native.join("src"), &mut options);
    clap_flags(&mut options);
    engine_flags(&engine_dir(native), &mut options);

    let paths = path_flags();
    let unclassified: Vec<_> = options
        .iter()
        .filter(|option| {
            !paths.contains(&option.as_str()) && !NON_PATH_OPTIONS.contains(&option.as_str())
        })
        .collect();
    assert!(
        unclassified.is_empty(),
        "unclassified parser options: {unclassified:?}"
    );
    assert!(
        paths.iter().all(|option| options.contains(*option)),
        "path check has an option absent from the parsers"
    );
}

/// How one source file reads the fields of a request or entry.
struct FieldReads {
    file: &'static str,
    /// The text before a field's name: `input["` for `input["out"]`.
    readers: &'static [&'static str],
    /// Fields that are not paths. `argv` is checked as command-line options, `env` is applied by
    /// the startup and child-process defaults, and `sequence` holds only numbers and flags.
    non_paths: &'static [&'static str],
}

const FIELD_READS: &[FieldReads] = &[
    FieldReads {
        file: "serve.rs",
        readers: &["input[\"", "input.get(\"", "request.query.get(\""],
        non_paths: &[
            "env",
            "argv",
            "png",
            "offline",
            "placeholders",
            "pixels",
            "sequence",
            "collect",
        ],
    },
    FieldReads {
        file: "engine/cli.rs",
        readers: &["job[\"", "job.get(\""],
        non_paths: &["argv", "env"],
    },
    FieldReads {
        file: "headless.rs",
        readers: &["entry[\"", "entry.get(\""],
        non_paths: &[],
    },
    FieldReads {
        file: "sequence.rs",
        readers: &["value.get(\"", "number(\"", "flag(\""],
        non_paths: &[
            "fps", "from", "frames", "to", "duration", "crop", "pad", "png", "scenes", "video",
        ],
    },
];

/// Whether `source` checks the path field `field` before it is used. These are the only path
/// fields: the two work endpoints' `out` are counted separately in the test below.
fn is_checked_path_field(file: &str, source: &str, field: &str) -> bool {
    match (file, field) {
        ("serve.rs", "project") => source.contains("validate_path_value(\"project\", project)"),
        ("headless.rs", "scene" | "out") => {
            source.contains("request_path(entry, field)")
                && source.contains("for field in [\"scene\", \"out\"]")
        }
        _ => false,
    }
}

/// The service's endpoints: the `("METHOD", "/path")` arms of its request match.
fn service_endpoints(service: &str) -> BTreeSet<&str> {
    let mut endpoints = BTreeSet::new();
    for (arm, _) in service.lines().filter_map(|line| line.split_once("=>")) {
        let literals: Vec<_> = quoted(arm).collect();
        if let [method, path] = literals[..] {
            if method.bytes().all(|byte| byte.is_ascii_uppercase()) && path.starts_with('/') {
                endpoints.insert(path);
            }
        }
    }
    endpoints
}

/// The inventories read production source, and a checkout may give that source either line
/// ending, so each of them must read the same from LF and from CRLF text.
#[test]
fn source_inventory_ignores_line_endings() {
    // A small service with one flag, one endpoint, a parser and a test module to leave out
    const SERVICE: &str = concat!(
        "fn dispatch() {\n",
        "    let flag = \"--offline\";\n",
        "    match request {\n",
        "        (\"GET\", \"/health\") => (),\n",
        "    }\n",
        "}\n",
        "#[cfg(test)]\n",
        "mod tests { const FLAG: &str = \"--test-only\"; }"
    );
    let profile_source = include_str!("../../src/profile_cli.rs").replace("\r\n", "\n");
    let expected_paths = read_positional_paths(&profile_source);
    for ending in ["\n", "\r\n"] {
        // A file may or may not end with a line ending
        for final_ending in [false, true] {
            let mut profile = profile_source.trim_end().replace('\n', ending);
            if final_ending {
                profile.push_str(ending);
            }
            assert_eq!(read_positional_paths(&profile), expected_paths);
        }
        let service = SERVICE.replace('\n', ending);
        assert_eq!(
            flags(without_tests(&service)),
            flags(without_tests(SERVICE))
        );
        assert_eq!(service_endpoints(&service), service_endpoints(SERVICE));
        assert_eq!(
            parser_fingerprint(parser_body(&service, "fn dispatch()")),
            parser_fingerprint(parser_body(SERVICE, "fn dispatch()"))
        );
    }
}

/// Inventories the fields that are read, not a second list of path fields: an added field fails
/// until its classification is reviewed. A path field must be read through `request_path`, or
/// validated before it is converted to a path.
#[test]
fn every_request_field_has_a_path_classification() {
    let native = Path::new(env!("CARGO_MANIFEST_DIR"));
    for reads in FIELD_READS {
        let source = std::fs::read_to_string(native.join("src").join(reads.file)).unwrap();
        let source = without_tests(&source);
        for reader in reads.readers {
            for after_reader in source.split(reader).skip(1) {
                let field = after_reader.split('"').next().unwrap();
                if reads.non_paths.contains(&field) {
                    continue;
                }
                assert!(
                    is_checked_path_field(reads.file, source, field),
                    "{}: unchecked request field {field}",
                    reads.file
                );
            }
        }
    }

    // Both work endpoints must read their required output through the checked path reader.
    let service = include_str!("../../src/serve.rs");
    assert_eq!(service.matches("request_path(input, \"out\")").count(), 2);
    // A new endpoint needs its fields classified above before it can be added here.
    assert_eq!(
        service_endpoints(service),
        BTreeSet::from([
            "/health",
            "/memory",
            "/stories",
            "/render",
            "/sequence",
            "/shutdown"
        ])
    );
}
