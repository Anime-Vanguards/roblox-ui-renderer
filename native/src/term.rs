//! rur-native's own stderr lines, labelled and colored: `rur: error:` red, `rur: warning:` yellow, `rur: hint:` cyan, a
//! run's status by outcome. The colors reach a terminal only (anstream drops them in a pipe or a file, and with
//! NO_COLOR; CLICOLOR_FORCE=1 keeps them), so what programs read is the plain text.
use std::io::Write;

use anstyle::{AnsiColor, Style};

const ERROR: Style = AnsiColor::Red.on_default().bold();
const WARNING: Style = AnsiColor::Yellow.on_default().bold();
const HINT: Style = AnsiColor::Cyan.on_default().bold();
const OK: Style = AnsiColor::Green.on_default().bold();

fn print(text: std::fmt::Arguments) {
    let _ = writeln!(anstream::stderr(), "{text}");
}

fn labelled(label: &str, style: Style, text: &str) {
    print(format_args!("rur: {style}{label}:{style:#} {text}"));
}

/// `rur: error: <text>`
pub fn error(text: &str) {
    labelled("error", ERROR, text);
}

/// `rur: warning: <text>`
pub fn warning(text: &str) {
    labelled("warning", WARNING, text);
}

/// `rur: hint: <text>`
pub fn hint(text: &str) {
    labelled("hint", HINT, text);
}

/// `rur: <text>`
pub fn note(text: &str) {
    print(format_args!("rur: {text}"));
}

/// A line that already starts with `rur: `: a project's note (`rur: Rojo project ...`) as it is, anything else (a
/// `$path` that doesn't exist, several project files) as a warning
pub fn project_line(line: &str) {
    let text = line.strip_prefix("rur: ").unwrap_or(line);
    if text.starts_with("Rojo project ") {
        note(text);
    } else {
        warning(text);
    }
}

/// A run's status word in its color (`ok` green, `error` red, anything else yellow)
pub fn status(status: &str) -> String {
    let style = match status {
        "ok" => OK,
        "error" | "failed" => ERROR,
        _ => WARNING,
    };
    format!("{style}{status}{style:#}")
}

/// `rur: <status>; <rest>` with the status in its color
pub fn summary(status_word: &str, rest: &str) {
    print(format_args!("rur: {}; {rest}", status(status_word)));
}
