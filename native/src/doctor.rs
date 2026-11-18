//! `rur-native doctor [--json]`: what this install uses and what it is missing, each with how to fix it:
//! the build, the state directory, the Roblox Studio install fonts and textures come from, the account
//! private assets are downloaded with (never its secret), and ror, which draws ViewportFrames.
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

/// The checks, as `(name, value, problem)`: a problem says what is missing and how to get it
fn checks() -> Vec<(&'static str, String, bool)> {
    let repo = crate::paths::default_repo();
    let build = if crate::paths::is_standalone() { "release build".to_owned() } else { format!("checkout build of {}", repo.display()) };
    let mut checks = vec![("rur", format!("{} ({build})", env!("CARGO_PKG_VERSION")), false)];
    checks.push(("state", crate::state_dir(&repo).display().to_string(), false));
    checks.push(match crate::resolve::studio_content() {
        Some(content) => ("studio", content.display().to_string(), false),
        None => ("studio", "not found: fonts and built-in textures come from a Roblox Studio install (or set RUR_ROBLOX_CONTENT to its content folder)".to_owned(), true),
    });
    let account = crate::resolve::Resolver::new(&repo, true).account_access();
    checks.push((
        "account",
        match account {
            Some("broker") => "an account broker (its token is never shown)".to_owned(),
            Some(_) => "the account cookie (never shown)".to_owned(),
            None => "none: public assets only (docs/assets.md says how to add one)".to_owned(),
        },
        false,
    ));
    checks.push(match crate::viewports::executable(&repo) {
        Ok(worker) => match worker_version(&worker) {
            Ok(version) => ("viewports", format!("{} ({version})", worker.display()), false),
            Err(problem) => ("viewports", problem, true),
        },
        Err(missing) => ("viewports", missing, true),
    });
    checks
}

/// The worker's version, or why it can't draw ViewportFrames
fn worker_version(worker: &Path) -> Result<String, String> {
    let run = |args: &[&str]| Command::new(worker).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output();
    let version = run(&["--version"]).ok().map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned()).filter(|version| !version.is_empty());
    let version = version.unwrap_or_else(|| "version unknown".into());
    match run(&["viewport-worker", "--help"]) {
        Ok(output) if output.status.success() => Ok(version),
        Ok(_) => Err(format!("{} ({version}) has no viewport worker: ViewportFrames need ror 0.5.0 or later", worker.display())),
        Err(error) => Err(format!("{} can't be started: {error}", worker.display())),
    }
}

pub fn cli(args: &[String]) -> i32 {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("usage: rur-native doctor [--json]\n\nWhat this install uses and what it is missing: the build, the state directory, the Roblox\nStudio install, the account for private assets and ror, which draws ViewportFrames.");
        return 0;
    }
    if let Some(unknown) = args.iter().find(|arg| *arg != "--json") {
        eprintln!("rur-native doctor: unknown argument {unknown} (usage: rur-native doctor [--json])");
        return 2;
    }
    let checks = checks();
    if args.iter().any(|arg| arg == "--json") {
        let object: serde_json::Map<String, Value> = checks.iter().map(|(name, value, _)| (name.to_string(), json!(value))).collect();
        println!("{}", Value::Object(object));
    } else {
        for (name, value, problem) in &checks {
            println!("{}{name:<10} {value}", if *problem { "! " } else { "  " });
        }
    }
    0
}
