//! `rur-native doctor [--json]`: what this install uses and what it is missing, each with how to fix it:
//! the build, the state directory, the Roblox Studio install fonts and textures come from, the account
//! private assets are downloaded with (never its secret), and ror, which draws ViewportFrames.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

/// The checks, as `(name, value, problem)`: a problem says what is missing and how to get it
pub(crate) fn checks() -> Vec<(&'static str, String, bool)> {
    let repo = crate::paths::default_repo();
    let build = if crate::paths::is_standalone() { "release build".to_owned() } else { format!("checkout build of {}", repo.display()) };
    let mut checks = vec![("rur", format!("{} ({build})", env!("CARGO_PKG_VERSION")), false)];
    checks.push(("state", crate::state_dir(&repo).display().to_string(), false));
    let content = std::env::var_os("RUR_ROBLOX_CONTENT").filter(|content| !content.is_empty()).map(PathBuf::from);
    checks.push(studio_check(content.as_deref(), crate::resolve::studio_install()));
    let mut resolver = crate::resolve::Resolver::new(&repo, true);
    let account = match resolver.account_access() {
        Some("broker") => Some("an account broker (its token is never shown)"),
        Some(_) => Some("the account cookie (never shown)"),
        None => None,
    };
    checks.push((
        "account",
        match (resolver.downloads_mut().api_key_state(), account) {
            (Some(Ok(())), Some(account)) => format!("an Open Cloud API key (never shown), then {account}"),
            (Some(Ok(())), None) => "an Open Cloud API key (never shown)".to_owned(),
            (Some(Err(reason)), account) => format!("{}; {reason}", account.unwrap_or("none: public assets only")),
            (None, Some(account)) => account.to_owned(),
            (None, None) => "none: public assets only (docs/assets.md says how to add one)".to_owned(),
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

/// The `studio` check: where fonts and built-in textures come from, from `content` (RUR_ROBLOX_CONTENT, when set) and
/// `install` (the Studio install rur finds without it). A `content` that is not a folder, or lacks the `fonts` or
/// `textures` folder rur reads (the font lookup takes any folder it names, so text is then not drawn), is a problem
/// as much as no Studio at all.
fn studio_check(content: Option<&Path>, install: Option<PathBuf>) -> (&'static str, String, bool) {
    const FIX: &str = "set RUR_ROBLOX_CONTENT to a Roblox Studio content folder, the one that holds fonts/ and textures/";
    let Some(content) = content else {
        return match install {
            Some(install) => ("studio", install.display().to_string(), false),
            None => ("studio", "not found: fonts and built-in textures come from a Roblox Studio install (or set RUR_ROBLOX_CONTENT to its content folder)".to_owned(), true),
        };
    };
    let named = format!("RUR_ROBLOX_CONTENT is {}", content.display());
    if !content.is_dir() {
        let instead = match &install {
            Some(install) => format!("fonts and built-in textures come from {} instead", install.display()),
            None => "with no Roblox Studio install either, text is not drawn and built-in textures are missing".to_owned(),
        };
        return ("studio", format!("{named}, which is not a folder: {instead} ({FIX})"), true);
    }
    let missing: Vec<&str> = ["fonts", "textures"].into_iter().filter(|folder| !content.join(folder).is_dir()).collect();
    if !missing.is_empty() {
        let mut effects = Vec::new();
        if missing.contains(&"fonts") {
            effects.push("text is not drawn".to_owned());
        }
        if missing.contains(&"textures") {
            effects.push(match &install {
                Some(install) => format!("built-in textures come from {}", install.display()),
                None => "built-in textures are missing".to_owned(),
            });
        }
        let folders = missing.iter().map(|folder| format!("{folder}/")).collect::<Vec<_>>().join(" or ");
        return ("studio", format!("{named}, which has no {folders} folder: {} ({FIX})", effects.join("; ")), true);
    }
    // (the install folder holding it, as textures are looked up)
    let root = content.parent().filter(|root| !root.as_os_str().is_empty()).unwrap_or(content);
    ("studio", root.display().to_string(), false)
}

/// The worker's version, or why it can't draw ViewportFrames
fn worker_version(worker: &Path) -> Result<String, String> {
    let run = |args: &[&str]| crate::download::without_account(&mut Command::new(worker)).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output();
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A RUR_ROBLOX_CONTENT that is not a folder, or lacks fonts/ or textures/, is a problem, as no Studio at all is;
    /// a whole content folder is not
    #[test]
    fn a_studio_content_folder_that_is_missing_or_lacks_fonts_is_a_problem() {
        let directory = std::env::temp_dir().join(format!("rur-doctor-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let install = directory.join("install");
        let content = directory.join("copy").join("content");
        std::fs::create_dir_all(content.join("textures")).unwrap();

        let (_, value, problem) = studio_check(Some(&directory.join("nowhere")), None);
        assert!(problem && value.contains("which is not a folder") && value.contains("text is not drawn"), "{value}");
        let (_, value, problem) = studio_check(Some(&directory.join("nowhere")), Some(install.clone()));
        assert!(problem && value.contains(&format!("come from {} instead", install.display())), "{value}");

        let (_, value, problem) = studio_check(Some(&content), Some(install.clone()));
        assert!(problem && value.contains("which has no fonts/ folder: text is not drawn") && value.contains("RUR_ROBLOX_CONTENT"), "{value}");
        std::fs::create_dir_all(content.join("fonts")).unwrap();
        assert_eq!(studio_check(Some(&content), Some(install.clone())), ("studio", directory.join("copy").display().to_string(), false));
        std::fs::remove_dir_all(content.join("textures")).unwrap();
        let (_, value, problem) = studio_check(Some(&content), None);
        assert!(problem && value.contains("which has no textures/ folder: built-in textures are missing"), "{value}");

        assert_eq!(studio_check(None, Some(install.clone())), ("studio", install.display().to_string(), false));
        assert!(studio_check(None, None).2);
        let _ = std::fs::remove_dir_all(&directory);
    }
}
