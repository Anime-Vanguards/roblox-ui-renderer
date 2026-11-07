use clap::Parser;
use rur_native::NativeOptions;

/// mimalloc instead of the system heap: the same results, faster allocation-heavy story code
#[cfg(not(feature = "memtrace"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// With feature `memtrace`: the same allocator, counting live bytes per phase (src/memtrace.rs)
#[cfg(feature = "memtrace")]
#[global_allocator]
static GLOBAL: rur_native::memtrace::Tracing = rur_native::memtrace::Tracing(mimalloc::MiMalloc);

fn main() -> anyhow::Result<()> {
    // `rur-native engine ...`: the engine CLI every Node host spawns (engine/cli.rs)
    if let Some(code) = rur_native::engine::cli::dispatch() {
        std::process::exit(code);
    }
    // `rur-native render ...`: one `rur render` output directory (story_render.rs)
    let argv: Vec<String> = std::env::args().collect();
    match argv.get(1).map(String::as_str) {
        Some("render") => std::process::exit(rur_native::story_render::cli(&argv[2..])),
        // `rur-native sequence ...`: a story captured frame by frame (sequence.rs)
        Some("sequence") => std::process::exit(rur_native::story_render::sequence_cli(&argv[2..])),
        // `rur-native serve ...`: the render service cli/client.mjs starts (serve.rs)
        Some("serve") => std::process::exit(rur_native::serve::cli(&argv[2..])),
        // `rur-native record ...`: a harness run recorded as a 3D scene and a playback: the emulator's
        // recorder (`ree record`) with rur's engines (the GUI layer installed)
        Some("record") => std::process::exit(rur_native::engine::record::run::cli(&argv[2..], &rur_native::engine::host(), true)),
        // `rur-native profile diff <before> <after> | report <dir> | view <dir> [<dir2>] | render ...`: the emulator's
        // `ree profile` on the profiles render, sequence and record write (`--profile`); view and render are its profile
        // viewer (the terminal UI, and its screens as PNG, SVG or text)
        Some("profile") => std::process::exit(match argv.get(2).map(String::as_str) {
            Some("diff") => rur_native::engine::profiler::diff::cli(&argv[3..]),
            Some("report") => rur_native::engine::profiler::diff::report_cli(&argv[3..]),
            Some("view" | "render") => ree_profile_view::main("rur profile", &argv[2..]),
            _ => {
                eprintln!("usage: rur-native profile diff <before dir> <after dir> [--out <dir>] [--top <n>] | report <profile dir>\n       rur-native profile view <dir> [<dir2>] | render <dir> [<dir2>] --view <name|all> --out <file|dir|->");
                2
            }
        }),
        _ => {}
    }
    let options = NativeOptions::parse().resolve()?;
    if options.headless {
        rur_native::headless::run(options)
    } else {
        rur_native::app::run(options)
    }
}
