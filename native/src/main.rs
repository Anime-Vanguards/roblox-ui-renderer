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

/// A release batch runner reads its child's answer from stdout, so a child that cannot start (no
/// usable state directory, say) answers its job with the error and exits. Any other process
/// returns the error for `main` to report.
fn answer_batch_job_startup_error(error: anyhow::Error) -> anyhow::Error {
    let is_batch_job = std::env::args().nth(1).as_deref() == Some(rur_native::engine::cli::BATCH_JOB_COMMAND);
    if rur_native::paths::is_standalone() && is_batch_job {
        let answer = serde_json::json!({"code": 1, "error": error.to_string()});
        println!("{}{answer}", rur_native::engine::cli::BATCH_ANSWER);
        std::process::exit(1);
    }
    error
}

fn main() -> anyhow::Result<()> {
    // Before anything else, so asking for the version works anywhere and creates no state directory
    if std::env::args().nth(1).as_deref().is_some_and(|arg| arg == "--version" || arg == "-V") {
        println!("rur-native {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    // Release builds fix their repo, state directory and Vulkan loader defaults for this process, its
    // threads and its children. This is the only place they change the environment: on Linux and macOS
    // that must happen before any other thread can read it (see the `paths` module comment).
    // SAFETY: nothing above started a thread, and no engine or GPU library has been loaded yet
    unsafe {
        if let Err(error) = rur_native::paths::initialize() {
            return Err(answer_batch_job_startup_error(error));
        }
        rur_native::render::gpu::initialize_loader_environment();
    }
    // `rur-native engine ...`: the engine CLI every Node host spawns (engine/cli.rs)
    if let Some(code) = rur_native::engine::cli::dispatch() {
        std::process::exit(code);
    }
    // `rur-native render ...`: one `rur render` output directory (story_render.rs)
    let argv: Vec<String> = std::env::args().collect();
    match argv.get(1).map(String::as_str) {
        Some("render") => std::process::exit(rur_native::story_render::cli(&argv[2..])),
        // `rur-native rojo-mounts <file.project.json>`: the mounts a Rojo project gives a run (rojo.rs)
        Some("rojo-mounts") => std::process::exit(rur_native::rojo::mounts_cli(&argv[2..])),
        // `rur-native sequence ...`: a story captured frame by frame (sequence.rs)
        Some("sequence") => std::process::exit(rur_native::story_render::sequence_cli(&argv[2..])),
        // `rur-native serve ...`: the render service cli/client.mjs starts (serve.rs)
        Some("serve") => std::process::exit(rur_native::serve::cli(&argv[2..])),
        // `rur-native record ...`: a harness run recorded as a 3D scene and a playback: the emulator's
        // recorder (`ree record`) with rur's engines (the GUI layer installed)
        Some("record") => {
            let args = rur_native::engine::cli::with_rojo_project(&argv[2..]).and_then(|args| rur_native::paths::record_arguments(&args)).map_err(anyhow::Error::msg)?;
            std::process::exit(rur_native::engine::record::run::cli(&args, &rur_native::engine::host(), true));
        }
        // `rur-native profile diff <before> <after> | report <dir> | view <dir> [<dir2>] | render ...`: the emulator's
        // `ree profile` on the profiles render, sequence and record write (`--profile`); view and render are its profile
        // viewer (the terminal UI, and its screens as PNG, SVG or text)
        Some("profile") => std::process::exit(match argv.get(2).map(String::as_str) {
            Some("diff") => rur_native::profile_cli::diff(&argv[3..]),
            Some("report") => rur_native::profile_cli::report(&argv[3..]),
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
