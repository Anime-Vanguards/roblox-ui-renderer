use clap::Parser;
use rur_native::NativeOptions;

/// mimalloc instead of the system heap: the same results, faster allocation-heavy story code. What a
/// warm worker or service keeps across runs goes to a heap of its own (roblox_engine_emulator::alloc).
#[cfg(not(feature = "memtrace"))]
#[global_allocator]
static GLOBAL: Allocator = Allocator;

#[cfg(not(feature = "memtrace"))]
struct Allocator;

#[cfg(not(feature = "memtrace"))]
mod heap {
    use std::ffi::c_void;

    unsafe extern "C" {
        pub fn mi_malloc(size: usize) -> *mut c_void;
        pub fn mi_zalloc(size: usize) -> *mut c_void;
        pub fn mi_realloc(p: *mut c_void, size: usize) -> *mut c_void;
        pub fn mi_malloc_aligned(size: usize, alignment: usize) -> *mut c_void;
        pub fn mi_zalloc_aligned(size: usize, alignment: usize) -> *mut c_void;
        pub fn mi_realloc_aligned(p: *mut c_void, size: usize, alignment: usize) -> *mut c_void;
        pub fn mi_free(p: *mut c_void);
        fn mi_heap_new() -> *mut c_void;
        pub fn mi_heap_malloc_aligned(heap: *mut c_void, size: usize, alignment: usize) -> *mut c_void;
        pub fn mi_heap_zalloc_aligned(heap: *mut c_void, size: usize, alignment: usize) -> *mut c_void;
        pub fn mi_heap_realloc_aligned(heap: *mut c_void, p: *mut c_void, size: usize, alignment: usize) -> *mut c_void;
        pub fn mi_option_set_enabled_default(option: i32, enable: bool);
        pub fn mi_option_set_default(option: i32, value: std::ffi::c_long);
        pub fn mi_option_set(option: i32, value: std::ffi::c_long);
        pub fn mi_collect(force: bool);
    }

    /// What the engine's idle collection asks of the allocator, done by mimalloc
    pub fn controls() -> roblox_engine_emulator::alloc::Controls {
        roblox_engine_emulator::alloc::Controls {
            collect: || unsafe { mi_collect(true) },
            set_purge_delay: |milliseconds| unsafe { mi_option_set(MI_OPTION_PURGE_DELAY, milliseconds as std::ffi::c_long) },
        }
    }

    /// mimalloc v2's `mi_option_eager_commit`
    pub const MI_OPTION_EAGER_COMMIT: i32 = 3;
    /// mimalloc v2's `mi_option_purge_delay`
    pub const MI_OPTION_PURGE_DELAY: i32 = 15;

    thread_local! {
        static HEAP: std::cell::Cell<*mut c_void> = const { std::cell::Cell::new(std::ptr::null_mut()) };
    }

    /// This thread's long-lived heap (mimalloc's heaps belong to the thread that made them; any
    /// thread frees their blocks), made on first use
    pub fn long_lived() -> *mut c_void {
        HEAP.with(|heap| {
            if heap.get().is_null() {
                heap.set(unsafe { mi_heap_new() });
            }
            heap.get()
        })
    }

    /// Whether mimalloc's plain functions give this alignment (as the mimalloc crate decides)
    #[inline]
    pub fn plain(size: usize, align: usize) -> bool {
        align <= 16 && align <= size
    }
}

#[cfg(not(feature = "memtrace"))]
unsafe impl std::alloc::GlobalAlloc for Allocator {
    #[inline]
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let (size, align) = (layout.size(), layout.align());
        unsafe {
            if roblox_engine_emulator::alloc::in_long_lived() {
                heap::mi_heap_malloc_aligned(heap::long_lived(), size, align).cast()
            } else if heap::plain(size, align) {
                heap::mi_malloc(size).cast()
            } else {
                heap::mi_malloc_aligned(size, align).cast()
            }
        }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        let (size, align) = (layout.size(), layout.align());
        unsafe {
            if roblox_engine_emulator::alloc::in_long_lived() {
                heap::mi_heap_zalloc_aligned(heap::long_lived(), size, align).cast()
            } else if heap::plain(size, align) {
                heap::mi_zalloc(size).cast()
            } else {
                heap::mi_zalloc_aligned(size, align).cast()
            }
        }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: std::alloc::Layout) {
        unsafe { heap::mi_free(ptr.cast()) }
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
        let align = layout.align();
        unsafe {
            if roblox_engine_emulator::alloc::in_long_lived() {
                heap::mi_heap_realloc_aligned(heap::long_lived(), ptr.cast(), new_size, align).cast()
            } else if heap::plain(new_size, align) {
                heap::mi_realloc(ptr.cast(), new_size).cast()
            } else {
                heap::mi_realloc_aligned(ptr.cast(), new_size, align).cast()
            }
        }
    }
}

/// With feature `memtrace`: the same allocator, counting live bytes per phase (src/memtrace.rs)
#[cfg(feature = "memtrace")]
#[global_allocator]
static GLOBAL: rur_native::memtrace::Tracing = rur_native::memtrace::Tracing(mimalloc::MiMalloc);

extern crate mimalloc;

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
    // Commit mimalloc segments page by page instead of 32 MiB at once and retain freed pages briefly
    // between runs (MIMALLOC_EAGER_COMMIT / MIMALLOC_PURGE_DELAY still override these defaults).
    #[cfg(not(feature = "memtrace"))]
    unsafe {
        heap::mi_option_set_enabled_default(heap::MI_OPTION_EAGER_COMMIT, false);
        heap::mi_option_set_default(heap::MI_OPTION_PURGE_DELAY, 100);
    }
    #[cfg(not(feature = "memtrace"))]
    let _ = roblox_engine_emulator::alloc::set_controls(heap::controls());
    // Before anything else, so asking for the version works anywhere and creates no state directory
    if std::env::args().nth(1).as_deref().is_some_and(|arg| arg == "--version" || arg == "-V") {
        println!("rur-native {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    // Release builds fix their repo, state directory and Vulkan loader defaults for this process, its
    // threads and its children. This is the only place they change the environment: on Linux and macOS
    // that must happen before any other thread can read it (see the `paths` module comment). The Open
    // Cloud API key leaves the environment here too, so no child process inherits it (download.rs).
    // SAFETY: nothing above started a thread, and no engine or GPU library has been loaded yet
    unsafe {
        rur_native::download::take_api_key_from_environment();
        if let Err(error) = rur_native::paths::initialize() {
            return Err(answer_batch_job_startup_error(error));
        }
        rur_native::render::gpu::initialize_loader_environment();
    }
    rur_native::process::configure_rayon();
    let argv: Vec<String> = std::env::args().collect();
    if let Some(command @ ("engine" | "engine-batch" | "engine-batch-job")) = argv.get(1).map(String::as_str) {
        rur_native::process::watch_parent(command);
    }
    // `rur-native engine ...`: the engine CLI every Node host spawns (engine/cli.rs)
    if let Some(code) = rur_native::engine::cli::dispatch() {
        std::process::exit(code);
    }
    // `rur-native render ...`: one `rur render` output directory (story_render.rs)
    match argv.get(1).map(String::as_str) {
        Some("render") => {
            rur_native::process::watch_parent("render");
            std::process::exit(rur_native::story_render::cli(&argv[2..]));
        }
        // `rur-native assets fetch <uri>... | account`: the asset cache and downloads, for the Node CLI (download.rs)
        Some("assets") => {
            rur_native::process::watch_parent("assets");
            std::process::exit(rur_native::download::cli(&argv[2..]));
        }
        // `rur-native doctor [--json]`: what this install uses and is missing, with how to fix it (doctor.rs)
        Some("doctor") => std::process::exit(rur_native::doctor::cli(&argv[2..])),
        // `rur-native debug-report [<story>...]`: every story rendered on its own, written up to share (report.rs)
        Some("debug-report") => {
            rur_native::process::watch_parent("debug-report");
            std::process::exit(rur_native::report::cli(&argv[2..]));
        }
        // `rur-native rojo-mounts <file.project.json>`: the mounts a Rojo project gives a run (rojo.rs)
        Some("rojo-mounts") => {
            rur_native::process::watch_parent("rojo-mounts");
            std::process::exit(rur_native::rojo::mounts_cli(&argv[2..]));
        }
        // `rur-native sequence ...`: a story captured frame by frame (sequence.rs)
        Some("sequence") => {
            rur_native::process::watch_parent("sequence");
            std::process::exit(rur_native::story_render::sequence_cli(&argv[2..]));
        }
        // `rur-native sweep ...` / `diff ...`: every story rendered and reported on (sweep.rs), a source rendered at a git
        // ref and in the working tree and compared (diff.rs); `rur sweep` and `rur diff` forward to them
        Some("sweep") => {
            rur_native::process::watch_parent("sweep");
            std::process::exit(rur_native::sweep::cli(&argv[2..]));
        }
        Some("diff") => {
            rur_native::process::watch_parent("diff");
            std::process::exit(rur_native::diff::cli(&argv[2..]));
        }
        // `rur-native cost ...`: deterministic Roblox UI cost accounting and A/B diff (cost.rs); `rur cost` forwards here
        Some("cost") => {
            rur_native::process::watch_parent("cost");
            std::process::exit(rur_native::cost::cli(&argv[2..]));
        }
        // `rur-native git-ref ...`: a run's sources extracted at a git ref, for `rur render --git-ref` (gitref.rs)
        Some("git-ref") => {
            rur_native::process::watch_parent("git-ref");
            std::process::exit(rur_native::gitref::cli(&argv[2..]));
        }
        // `rur-native serve ...`: the render service cli/client.mjs starts (serve.rs)
        Some("serve") => std::process::exit(rur_native::serve::cli(&argv[2..])),
        // `rur-native record ...`: a harness run recorded as a 3D scene and a playback: the emulator's
        // recorder (`ree record`) with rur's engines (the GUI layer installed)
        Some("record") => {
            rur_native::process::watch_parent("record");
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
        rur_native::process::watch_parent("headless");
        rur_native::headless::run(options)
    } else {
        rur_native::app::run(options)
    }
}
