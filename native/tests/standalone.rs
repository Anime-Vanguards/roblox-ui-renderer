//! Runs the two build modes side by side (see `src/paths.rs`).
//!
//! The tests named `checkout_*` use Cargo's ordinary build of `rur-native`, which keeps its
//! checkout defaults wherever a copy of it sits. They run in every `cargo test`.
//!
//! The tests named `installed_*` use a release build, which is built with `RUR_RELEASE_BUILD=1`
//! and always runs standalone. Cargo cannot build both modes in one test run, so these tests are
//! ignored by default and run only on request: build the marked binary, set
//! `RUR_TEST_STANDALONE_BINARY` to its absolute path, then run this target with `-- --ignored`.
//! The release workflow does this before it packages a build; `tools/release/README.md` has the
//! commands. Only this suite reads that variable, never the renderer.
use std::{path::Path, process::Command};

#[path = "support/process.rs"]
mod process;
use process::{copy_executable, LockedLaunch};

#[path = "support/path_contract.rs"]
mod path_contract;

/// A command for an executable that is run like an installed one. Every variable that could lead
/// back to a checkout or to the real per-user folders is removed or redirected into `cache`, and
/// Node is made missing.
fn installed(exe: &Path, cwd: &Path, cache: &Path) -> Command {
    let mut command = Command::new(exe);
    command
        .current_dir(cwd)
        .env_remove("RUR_STATE_DIR")
        .env_remove("RUR_CACHE_DIR")
        .env_remove("RUR_PROJECT")
        .env_remove("RUR_REPO")
        .env_remove("RUR_ASSET_REPO")
        .env("HOME", cache)
        .env("LOCALAPPDATA", cache)
        .env("XDG_CACHE_HOME", cache)
        .env("RUR_ROBLOX_CONTENT", cache.join("content"))
        .env("RUR_NODE", "missing-node-for-standalone-test");
    command
}

/// A small project with one place, a module in it and a story that requires the module.
/// `Cargo.lock` and `.env` are decoys: neither may make it a checkout or set `RUR_PROJECT`.
fn write_project(project: &Path) {
    std::fs::create_dir_all(project.join("src/ReplicatedStorage")).unwrap();
    std::fs::write(project.join("Cargo.lock"), "").unwrap();
    std::fs::write(project.join(".env"), "RUR_PROJECT=should-not-be-read\n").unwrap();
    std::fs::write(
        project.join("rur.json"),
        r#"{"places":[{"name":"main","roots":["src"]}]}"#,
    )
    .unwrap();
    std::fs::write(
        project.join("src/ReplicatedStorage/Value.luau"),
        "return 42",
    )
    .unwrap();
    std::fs::write(
        project.join("story.luau"),
        "local value = require(game:GetService('ReplicatedStorage').Value)\n\
         assert(value == 42)\n\
         return function(target) local f = Instance.new('Frame'); f.Name = 'Installed'; f.Parent = target end",
    )
    .unwrap();
}

/// Runs `render --no-png` on `story` into `out`, with `more` arguments after them, and returns
/// the scene it exported.
fn render(command: Command, story: &Path, out: &Path, more: &[&str]) -> serde_json::Value {
    render_source(command, "--story", story, out, more)
}

/// Like `render`, for a source named by `source_flag` (`--story` or `--module`).
fn render_source(
    mut command: Command,
    source_flag: &str,
    source: &Path,
    out: &Path,
    more: &[&str],
) -> serde_json::Value {
    command
        .args(["render", "--no-png", source_flag])
        .arg(source)
        .arg("--dir")
        .arg(out)
        .args(more);
    let output = command.output_locked().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let scene = std::fs::read(out.join("scene.json")).unwrap();
    serde_json::from_slice(&scene).unwrap()
}

/// The scene is ok and holds the frame the story made, so the module under the project's place
/// was found.
fn assert_story_ran(scene: &serde_json::Value) {
    assert_eq!(scene["status"], "ok", "{scene}");
    assert!(scene["nodes"].as_array().unwrap().iter().any(|node| {
        node["path"]
            .as_str()
            .is_some_and(|path| path.ends_with(".Installed"))
    }));
}

/// A recording harness for a project with no `rur.json` settings: place id 0, no streaming, and
/// the project's `Value` module. Used where the recorder must not pick up any other configuration.
const PLAIN_PROJECT_HARNESS: &str = "return function()
	assert(game.PlaceId == 0, tostring(game.PlaceId))
	assert(not workspace.StreamingEnabled)
	assert(require(game:GetService('ReplicatedStorage').Value) == 42)
end";

/// The recorder can exit successfully even when its harness failed. Check both summaries so
/// assertions about settings and content in the harness cannot silently pass.
fn assert_recording_ran(output: &std::process::Output, out: &Path, mode: &str) {
    assert!(output.status.success(), "{output:?}");
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok", "{summary}");
    assert_eq!(summary["errors"], 0, "{summary}");
    assert_eq!(summary["mode"], mode, "{summary}");
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("summary.json")).unwrap()).unwrap();
    assert_eq!(saved, summary);
}

/// A copy of `rur-native` placed the way a release leaves it: alone in `dir/bin`, with a project
/// and a redirected per-user cache beside it. The folder is removed when this is dropped.
struct Installation {
    dir: std::path::PathBuf,
    exe: std::path::PathBuf,
    project: std::path::PathBuf,
    cache: std::path::PathBuf,
}

/// The release build that `RUR_TEST_STANDALONE_BINARY` names. Only the ignored `installed_*` tests
/// call this, so a run that asks for them without the variable fails here instead of passing.
fn standalone_binary() -> std::path::PathBuf {
    std::env::var_os("RUR_TEST_STANDALONE_BINARY")
        .map(std::path::PathBuf::from)
        .expect("Set RUR_TEST_STANDALONE_BINARY to a binary built with RUR_RELEASE_BUILD=1")
}

impl Installation {
    /// An installation of the release build.
    fn new(name: &str) -> Self {
        Self::with_binary(name, &standalone_binary())
    }

    /// An installation of Cargo's ordinary build, a copy of a checkout build.
    fn checkout(name: &str) -> Self {
        Self::with_binary(name, Path::new(env!("CARGO_BIN_EXE_rur-native")))
    }

    fn with_binary(name: &str, binary: &Path) -> Self {
        let dir = std::env::temp_dir().join(format!("rur-{name}-{}", std::process::id()));
        let exe = dir.join("bin").join(if cfg!(windows) {
            "rur-native.exe"
        } else {
            "rur-native"
        });
        let project = dir.join("project");
        let cache = dir.join("cache");
        for folder in [exe.parent().unwrap(), &project, &cache] {
            std::fs::create_dir_all(folder).unwrap();
        }
        copy_executable(binary, &exe).unwrap();
        write_project(&project);
        Self {
            dir,
            exe,
            project,
            cache,
        }
    }

    fn command(&self) -> Command {
        installed(&self.exe, &self.project, &self.cache)
    }
}

impl Drop for Installation {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A one-pixel texture at `<install>/<base>/textures/<name>.png`. Its colour tells a render which
/// folder the texture came from.
#[cfg(windows)]
fn write_texture(install: &Path, base: &str, name: &str, color: [u8; 4]) {
    let folder = install.join(base).join("textures");
    std::fs::create_dir_all(&folder).unwrap();
    image::RgbaImage::from_pixel(1, 1, image::Rgba(color))
        .save(folder.join(format!("{name}.png")))
        .unwrap();
}

/// Makes `folder` look like a Studio install to the Versions lookup, which skips a folder without
/// the executable and sorts the rest by modification time. The time is set on the directory
/// itself, after the executable is written, because writing changes it.
#[cfg(windows)]
fn make_studio_install(folder: &Path, modified_seconds: u64) {
    use std::os::windows::fs::OpenOptionsExt;
    use std::time::{Duration, SystemTime};

    // These two flags let a directory be opened for setting its time, with no sleeps and no
    // dependency, so the order of the installs is deterministic.
    const FILE_WRITE_ATTRIBUTES: u32 = 0x100;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    std::fs::write(folder.join("RobloxStudioBeta.exe"), "").unwrap();
    std::fs::OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(folder)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(modified_seconds))
        .unwrap();
}

/// The colour of the first pixel of the image in `file`.
#[cfg(windows)]
fn first_pixel(file: &Path) -> [u8; 4] {
    image::open(file).unwrap().into_rgba8().get_pixel(0, 0).0
}

/// A story that shows one image for each texture the test below makes.
#[cfg(windows)]
const TEXTURE_STORY: &str = "return function(target)
	for _, name in {'shared', 'extra', 'platform', 'older'} do
		local image = Instance.new('ImageLabel')
		image.Image = 'rbxasset://textures/' .. name .. '.png'
		image.Parent = target
	end
end";

/// On Windows a release build looks for Studio's textures in the order a checkout build does:
/// the folder `RUR_ROBLOX_CONTENT` names, then the newest entry of Versions, then the older ones,
/// and only then the install the Studio locator names. The test renders the same story with both
/// builds and expects the same pixels, whichever install the locator points at. The installs are
/// fake and hold only images this test makes.
#[cfg(windows)]
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_studio_textures_keep_checkout_priority() {
    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const GREEN: [u8; 4] = [0, 255, 0, 255];
    const YELLOW: [u8; 4] = [255, 255, 0, 255];

    let install = Installation::new("studio-texture-order");
    let versions = install.cache.join("Roblox/Versions");
    let older = versions.join("version-older");
    let newest = versions.join("version-newest");
    // The install the locator names, which sits outside Versions.
    let located = install.dir.join("located");
    // The folder that `RUR_ROBLOX_CONTENT` names, in the runs that set it.
    let explicit = install.dir.join("explicit");

    // Each texture is made in several places in different colours, so the colour in a render shows
    // which place the lookup used. The expected colours are in `expected_colors` below.
    for (root, base, name, color) in [
        // Everything the newest install has comes before the older install and the locator.
        // Within it the order is content, ExtraContent, then PlatformContent/pc.
        (&newest, "content", "shared", RED),
        (&newest, "ExtraContent", "shared", BLUE),
        (&newest, "PlatformContent/pc", "shared", BLUE),
        (&newest, "ExtraContent", "extra", RED),
        (&newest, "PlatformContent/pc", "extra", BLUE),
        (&newest, "PlatformContent/pc", "platform", RED),
        // A texture only the older install has is still found before the locator's copy.
        (&older, "content", "shared", GREEN),
        (&older, "content", "older", GREEN),
        (&located, "content", "shared", BLUE),
        (&located, "content", "older", BLUE),
        // The explicit folder, when it is set, comes before all of them.
        (&explicit, "content", "shared", YELLOW),
    ] {
        write_texture(root, base, name, color);
    }
    // Any two times do, as long as `newest` is later.
    make_studio_install(&older, 1_000_000_000);
    make_studio_install(&newest, 1_000_000_010);

    let story = install.project.join("textures.luau");
    std::fs::write(&story, TEXTURE_STORY).unwrap();

    // Renders the story with `exe` and returns the file of each texture, after checking its colour.
    let render_textures = |exe: &Path, mode: &str, use_explicit: bool| {
        let mut command = installed(exe, &install.project, &install.cache);
        command
            .env("ROBLOX_STUDIO_PATH", &located)
            .env_remove("RUR_ROBLOX_CONTENT");
        if use_explicit {
            command.env("RUR_ROBLOX_CONTENT", explicit.join("content"));
        }
        let out = install.dir.join(format!("{mode}-{use_explicit}"));
        render(command, &story, &out, &["--offline"]);

        let expected_colors = [
            ("shared", if use_explicit { YELLOW } else { RED }),
            ("extra", RED),
            ("platform", RED),
            ("older", GREEN),
        ];
        let mut files = Vec::new();
        for (name, expected) in expected_colors {
            let file = out.join("assets").join(format!("{name}.png"));
            assert_eq!(
                first_pixel(&file),
                expected,
                "{mode}, explicit={use_explicit}, {name}"
            );
            files.push(std::fs::read(file).unwrap());
        }
        files
    };

    for use_explicit in [false, true] {
        let checkout = render_textures(
            Path::new(env!("CARGO_BIN_EXE_rur-native")),
            "checkout",
            use_explicit,
        );
        let release = render_textures(&install.exe, "installed", use_explicit);
        assert_eq!(release, checkout);
    }

    // On a machine with no Versions folder the locator is the only source, so a release build
    // still uses it.
    let mut command = installed(
        &install.exe,
        &install.project,
        &install.dir.join("no-versions"),
    );
    command
        .env("ROBLOX_STUDIO_PATH", &located)
        .env_remove("RUR_ROBLOX_CONTENT");
    let out = install.dir.join("locator-fallback");
    render(command, &story, &out, &["--offline"]);
    assert_eq!(first_pixel(&out.join("assets/shared.png")), BLUE);
}

/// The rur checkout this test was built in, which an installed copy must not take as its own.
fn build_checkout() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

/// A junction needs no privilege on Windows, unlike a symlink.
fn link_directory(target: &Path, alias: &Path) {
    #[cfg(windows)]
    {
        let output = Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(alias)
            .arg(target)
            .output_locked()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, alias).unwrap();
}

/// A folder shaped like a rur checkout (its CLI and its `native/Cargo.toml`), without any project
/// content. Tests name it with `--repo` or `RUR_ASSET_REPO`; an installed binary must not take it
/// as its own.
fn write_stand_in_checkout(repo: &Path) {
    std::fs::create_dir_all(repo.join("native")).unwrap();
    std::fs::create_dir_all(repo.join("cli")).unwrap();
    std::fs::write(
        repo.join("native/Cargo.toml"),
        "[package]\nname = \"rur-native\"\n",
    )
    .unwrap();
    std::fs::write(repo.join("cli/rur.mjs"), "").unwrap();
}

/// Puts a copy of `exe` where the Node CLI keeps its build, `<cache>/native-app`, and lists it in
/// `manifest.json` like the CLI does. Returns the copy. The binary itself no longer reads the
/// manifest; tests that remove it show the copy does not depend on it.
fn install_cached_copy(install: &Installation, exe: &Path) -> std::path::PathBuf {
    let app = install.cache.join("native-app");
    std::fs::create_dir_all(&app).unwrap();
    let cached = app.join(install.exe.file_name().unwrap());
    copy_executable(exe, &cached).unwrap();
    let manifest = serde_json::json!({
        "schema": 1,
        "executable": cached.file_name().unwrap().to_str().unwrap(),
    });
    std::fs::write(app.join("manifest.json"), manifest.to_string()).unwrap();
    cached
}

/// The answers a batch run printed, one per job: the lines of stdout that start with the batch marker.
fn batch_answers(output: &std::process::Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("@@RUR-BATCH "))
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Starts `rur-native engine-batch` from `command`, sends one job per line and checks that every
/// job answered with exit code 0. Release jobs can share the runner when their variables match.
fn run_batch(mut command: Command, jobs: &[serde_json::Value]) {
    use std::io::Write;
    let mut child = command
        .arg("engine-batch")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn_locked()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for job in jobs {
        writeln!(stdin, "{job}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let answers = batch_answers(&output);
    assert_eq!(answers.len(), jobs.len(), "{output:?}");
    for answer in answers {
        assert_eq!(
            answer["code"],
            0,
            "{answer}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// A story that makes an editable image, which the engine stores in its state directory.
const EDITABLE_IMAGE_STORY: &str = "return function(target)\n\
     local image = game:GetService('AssetService'):CreateEditableImage({Size=Vector2.new(1,1)})\n\
     game:GetService('AssetService'):CreateSurfaceAppearanceAsync({ColorMap=Content.fromObject(image)})\n\
     local f = Instance.new('Frame'); f.Name = 'Installed'; f.Parent = target end";

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_batch_jobs_restore_the_user_cache_and_honor_job_overrides() {
    let install = Installation::new("batch-cache");
    let checkout = install.dir.join("helper");
    write_stand_in_checkout(&checkout);
    let story = install.project.join("story.luau");
    std::fs::write(&story, EDITABLE_IMAGE_STORY).unwrap();
    let job_state = install.dir.join("job-state");
    let job_cache = install.dir.join("job-cache");
    // Each job replaces the variables of the one before. A job with none goes back to the user
    // cache, and RUR_STATE_DIR wins over RUR_CACHE_DIR.
    let job_environments = [
        serde_json::json!({}),
        serde_json::json!({"RUR_STATE_DIR": job_state, "RUR_CACHE_DIR": job_cache}),
        serde_json::json!({"RUR_CACHE_DIR": job_cache}),
        serde_json::json!({}),
    ];
    let jobs: Vec<_> = job_environments
        .into_iter()
        .enumerate()
        .map(|(index, env)| {
            serde_json::json!({
                "argv": ["--repo", checkout, "--project", install.project,
                    "--story", story, "--out", install.dir.join(format!("{index}.json"))],
                "env": env,
            })
        })
        .collect();
    run_batch(install.command(), &jobs);

    let user_cache = if cfg!(target_os = "macos") {
        install.cache.join("Library/Caches/rur")
    } else {
        install.cache.join("rur")
    };
    // Each run leaves one editable image in the state directory it used
    for (state_dir, runs) in [(user_cache, 2), (job_state, 1), (job_cache, 1)] {
        let images: Vec<String> = std::fs::read_dir(state_dir.join("editable-images"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(images.len(), runs, "{images:?}");
        // The engine numbers its images per process, so every job being a process of its own
        // shows as every image ending in 0. A runner that went back to running jobs in its own
        // process, changing its environment between them, would number them 0, 1, 2...
        assert!(
            images.iter().all(|name| name.ends_with("-0.png")),
            "{images:?}"
        );
    }
    // --repo named the stand-in checkout, but it is not the state directory of any job
    assert!(!checkout.join(".cache").exists());
}

/// Like `LOCAL_IMAGE_STORY`, and it also makes an editable image, which the engine stores in its
/// state directory.
const LOCAL_AND_EDITABLE_IMAGE_STORY: &str = "return function(target)\n\
     local image = game:GetService('AssetService'):CreateEditableImageAsync(Content.fromUri('file://local.png'))\n\
     assert(image.Size.X == 1)\n\
     local made = game:GetService('AssetService'):CreateEditableImage({Size=Vector2.new(1,1)})\n\
     game:GetService('AssetService'):CreateSurfaceAppearanceAsync({ColorMap=Content.fromObject(made)})\n\
     local f = Instance.new('Frame'); f.Name = 'Installed'; f.Parent = target end";

#[test]
fn checkout_copies_keep_root_based_assets_and_batch_state_without_startup_cache() {
    // Everything below comes from the live `--repo` and the explicit roots, never from the checkout
    // that was baked in at compile time. So this test also passes when that checkout is gone, which
    // can be checked by building the tests with a compiler wrapper that overrides CARGO_MANIFEST_DIR.
    // A copy of the checkout build, placed where the Node CLI keeps its cached engine
    let install = Installation::checkout("checkout-batch");
    let repo = install.dir.join("repo");
    let root = install.project.join("src");
    std::fs::create_dir_all(&repo).unwrap();
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]))
        .save(install.project.join("local.png"))
        .unwrap();
    // The image sits in the roots' parent, the project. The worker's current folder does not have it.
    let story = install.project.join("story.luau");
    std::fs::write(&story, LOCAL_AND_EDITABLE_IMAGE_STORY).unwrap();
    let cached = install_cached_copy(&install, &install.exe);
    // The copy must not depend on the manifest next to it
    std::fs::remove_file(cached.parent().unwrap().join("manifest.json")).unwrap();
    let engine_arguments = |out: &Path| -> Vec<String> {
        let text = |path: &Path| path.to_string_lossy().into_owned();
        vec![
            "--repo".into(),
            text(&repo),
            "--roots".into(),
            text(&root),
            "--story".into(),
            text(&story),
            "--out".into(),
            text(out),
        ]
    };
    // Single runs and batch jobs get no cache variables at startup. The second round also has
    // no user cache folder to fall back on.
    for without_user_cache_folders in [false, true] {
        let command = || {
            let mut command = installed(&cached, &install.dir, &install.cache);
            command.env("RUR_ASSET_REPO", install.dir.join("missing-helper"));
            if without_user_cache_folders {
                for key in ["HOME", "LOCALAPPDATA", "XDG_CACHE_HOME"] {
                    command.env_remove(key);
                }
            }
            command
        };
        let single_out = install
            .dir
            .join(format!("single-{without_user_cache_folders}.json"));
        let output = command()
            .arg("engine")
            .args(engine_arguments(&single_out))
            .output_locked()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let scene = serde_json::from_slice(&std::fs::read(single_out).unwrap()).unwrap();
        assert_story_ran(&scene);

        // The second job sets a cache folder of its own
        let job_environments = [
            serde_json::json!({}),
            serde_json::json!({"RUR_CACHE_DIR": install.cache}),
        ];
        let jobs: Vec<_> = job_environments
            .into_iter()
            .enumerate()
            .map(|(index, env)| {
                let out = install
                    .dir
                    .join(format!("batch-{without_user_cache_folders}-{index}.json"));
                serde_json::json!({"argv": engine_arguments(&out), "env": env})
            })
            .collect();
        run_batch(command(), &jobs);
    }
    assert!(repo.join(".cache/editable-images").is_dir());
    assert!(install.cache.join("editable-images").is_dir());
    assert!(!install.cache.join("rur").exists());
    assert!(!install.cache.join("Library").exists());
}

#[test]
fn checkout_headless_keeps_the_first_roots_parent_as_the_asset_base() {
    let install = Installation::checkout("checkout-headless");
    let selected = install.dir.join("selected");
    std::fs::create_dir_all(&selected).unwrap();
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]))
        .save(install.project.join("local.png"))
        .unwrap();
    let story = install.project.join("story.luau");
    std::fs::write(&story, LOCAL_IMAGE_STORY).unwrap();
    let out = install.dir.join("headless");
    let output = install
        .command()
        .env("RUR_CACHE_DIR", &install.cache)
        .args(["--headless", "--repo"])
        .arg(&selected)
        .arg("--project")
        .arg(&selected)
        .arg("--roots")
        .arg(install.project.join("src"))
        .arg("--story")
        .arg(story)
        .args(["--viewport", "32x32", "--out"])
        .arg(&out)
        .output_locked()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let scene = serde_json::from_slice(&std::fs::read(out.join("scene.json")).unwrap()).unwrap();
    assert_story_ran(&scene);
}

/// A story that loads `local.png` from the project through a relative `file://` URI.
const LOCAL_IMAGE_STORY: &str = "return function(target)\n\
     local image = game:GetService('AssetService'):CreateEditableImageAsync(Content.fromUri('file://local.png'))\n\
     assert(image.Size.X == 1)\n\
     local f = Instance.new('Frame'); f.Name = 'Installed'; f.Parent = target end";

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_headless_uses_the_selected_project_for_local_images() {
    let install = Installation::new("headless-project");
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]))
        .save(install.project.join("local.png"))
        .unwrap();
    let story = install.project.join("story.luau");
    std::fs::write(&story, LOCAL_IMAGE_STORY).unwrap();
    let out = install.dir.join("headless");
    // The working directory is not the project, so `local.png` is only found through --project
    let output = install
        .command()
        .current_dir(&install.dir)
        .args(["--headless", "--project"])
        .arg(&install.project)
        .arg("--story")
        .arg(&story)
        .args(["--viewport", "64x64", "--out"])
        .arg(&out)
        .output_locked()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let scene = serde_json::from_slice(&std::fs::read(out.join("scene.json")).unwrap()).unwrap();
    assert_story_ran(&scene);
    assert!(out.join("render.png").is_file());
}

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_story_and_module_use_their_place_unless_one_is_named() {
    let install = Installation::new("source-place");
    let project = &install.project;
    // The project's first place ("first") is `src`, where Value is 42. The second place holds
    // its own Value, 84, and the story that requires it.
    std::fs::write(
        project.join("rur.json"),
        r#"{"places":[{"name":"first","roots":["src"]},{"name":"second","roots":["other"]}]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(project.join("other/ReplicatedStorage")).unwrap();
    std::fs::write(
        project.join("other/ReplicatedStorage/Value.luau"),
        "return 84",
    )
    .unwrap();
    let source = project.join("other/ReplicatedStorage/Example.story.luau");
    // The frame is named after the Value the source found, which tells the places apart
    std::fs::write(&source,
        "local value = require(game:GetService('ReplicatedStorage').Value)\n\
         return function(target) local f = Instance.new('Frame'); f.Name = 'Value' .. value; f.Parent = target end").unwrap();

    // (run name, source flag, extra arguments, the frame the run must produce)
    let cases = [
        ("story", "--story", vec![], "Value84"),
        ("module", "--module", vec![], "Value84"),
        (
            "named-place",
            "--story",
            vec!["--place", "first"],
            "Value42",
        ),
    ];
    for (name, source_flag, more, expected) in cases {
        let scene = render_source(
            install.command(),
            source_flag,
            &source,
            &install.dir.join(name),
            &more,
        );
        assert_eq!(scene["status"], "ok", "{scene}");
        assert!(
            scene["nodes"].as_array().unwrap().iter().any(|node| {
                node["path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with(&format!(".{expected}")))
            }),
            "{name}: {scene}"
        );
    }
}

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_helper_checkout_does_not_change_the_project() {
    let install = Installation::new("helper-project");
    let checkout = build_checkout();
    // Naming the checkout, by flag or only for the asset helper, keeps the installed defaults. So
    // does running the release binary where it was built, which may be inside the checkout, with a
    // run-time variable that says otherwise: only the value compiled in counts.
    for name in [
        "no-checkout",
        "repo-flag",
        "asset-repo",
        "run-time-release-variable",
    ] {
        let mut command = install.command();
        let mut more = Vec::new();
        match name {
            "run-time-release-variable" => {
                command = installed(&standalone_binary(), &install.project, &install.cache);
                command.env("RUR_RELEASE_BUILD", "0");
            }
            "repo-flag" => more.extend(["--repo", checkout.to_str().unwrap()]),
            "asset-repo" => {
                command.env("RUR_ASSET_REPO", checkout);
            }
            _ => {}
        }
        let scene = render(
            command,
            Path::new("story.luau"),
            &install.dir.join(name),
            &more,
        );
        assert_story_ran(&scene);
    }
}

/// The Vulkan loader settings a caller can choose, as `render/gpu.rs` lists them, and the two a release
/// build sets when the caller chose none of them.
const LOADER_SETTINGS: [&str; 5] = [
    "VK_LOADER_LAYERS_DISABLE",
    "VK_LOADER_LAYERS_ENABLE",
    "VK_INSTANCE_LAYERS",
    "VK_LOADER_DRIVERS_SELECT",
    "VK_LOADER_DRIVERS_DISABLE",
];
const LOADER_DEFAULTS: [(&str, &str); 2] = [
    ("VK_LOADER_LAYERS_DISABLE", "~implicit~"),
    ("VK_LOADER_DRIVERS_DISABLE", "*dzn*"),
];

/// Stands in for `cli/native-assets.mjs`: leaves a `used` file beside itself, writes the loader settings
/// it was started with to `loader.json` (in the order of `LOADER_SETTINGS`) and refuses every image.
/// `LOADER_SETTINGS_JSON` is replaced by the settings' names.
const ASSET_HELPER_STUB: &str = r#"
import fs from 'node:fs';
import readline from 'node:readline';
const loaderSettings = LOADER_SETTINGS_JSON;
for await (const line of readline.createInterface({ input: process.stdin })) {
  const request = JSON.parse(line);
  if (request.command === 'quit') break;
  fs.writeFileSync(new URL('used', import.meta.url), 'yes');
  const loader = loaderSettings.map(name => process.env[name] ?? null);
  fs.writeFileSync(new URL('loader.json', import.meta.url), JSON.stringify(loader));
  const images = request.images?.map(uri => ({ uri, error: 'test refusal' })) ?? [];
  console.log(JSON.stringify({ id: request.id, images }));
}
"#;

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_binary_runs_the_real_helper_through_a_link() {
    let install = Installation::new("linked-asset-helper");
    let alias = install.dir.join("helper-link");
    link_directory(build_checkout(), &alias);
    let story = install.project.join("story.luau");
    std::fs::write(
        &story,
        "return function(target) local image=Instance.new('ImageLabel'); image.Image='rur-viewport://missing'; image.Parent=target end",
    )
    .unwrap();
    // A viewport image that has no snapshot is answered by the helper alone (no download), so a
    // matching error shows the helper really started, by its real path and through the link.
    for (name, helper) in [("real", build_checkout()), ("linked", alias.as_path())] {
        let mut command = install.command();
        command
            .env("RUR_ASSET_REPO", helper)
            .env("RUR_NODE", "node");
        let out = install.dir.join(name);
        let scene = render(command, &story, &out, &["--offline"]);
        assert_eq!(scene["status"], "ok", "{scene}");
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(out.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(
            manifest["images"]["rur-viewport://missing"]["error"], "Missing ViewportFrame snapshot",
            "{name}: {manifest}"
        );
    }
}

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_binary_runs_the_helper_named_by_rur_asset_repo() {
    let install = Installation::new("asset-helper");
    let helper_repo = install.dir.join("helper");
    let helper_dir = helper_repo.join("cli");
    std::fs::create_dir_all(&helper_dir).unwrap();
    let loader_names = serde_json::to_string(&LOADER_SETTINGS).unwrap();
    std::fs::write(
        helper_dir.join("native-assets.mjs"),
        ASSET_HELPER_STUB.replace("LOADER_SETTINGS_JSON", &loader_names),
    )
    .unwrap();
    // An image that is not cached makes the story ask the helper for a download
    let story_path = install.project.join("story.luau");
    let story = std::fs::read_to_string(&story_path).unwrap();
    let with_image = story.replace(
        "f.Name = 'Installed'",
        "local image = Instance.new('ImageLabel'); image.Image = 'rbxassetid://123456789'; image.Parent = f; f.Name = 'Installed'",
    );
    std::fs::write(&story_path, with_image).unwrap();

    // The startup defaults reach the helper even though this story draws nothing on a GPU. A
    // caller's setting, even an empty one, suppresses both defaults, as it does for a checkout
    // build's Vulkan probe. Run once with no setting, then once with each setting empty.
    let cases = std::iter::once(None).chain((0..LOADER_SETTINGS.len()).map(Some));
    for chosen in cases {
        let mut command = install.command();
        command
            .env("RUR_ASSET_REPO", &helper_repo)
            .env("RUR_NODE", "node");
        for setting in LOADER_SETTINGS {
            command.env_remove(setting);
        }
        // What the helper should see for each setting, in the order of `LOADER_SETTINGS`
        let mut expected = vec![serde_json::Value::Null; LOADER_SETTINGS.len()];
        match chosen {
            Some(index) => {
                command.env(LOADER_SETTINGS[index], "");
                expected[index] = serde_json::json!("");
            }
            None => {
                for (name, value) in LOADER_DEFAULTS {
                    let index = LOADER_SETTINGS.iter().position(|&s| s == name).unwrap();
                    expected[index] = serde_json::json!(value);
                }
            }
        }
        let out_name = chosen.map_or("no-setting", |index| LOADER_SETTINGS[index]);
        let scene = render(
            command,
            Path::new("story.luau"),
            &install.dir.join(out_name),
            &[],
        );
        assert_story_ran(&scene);
        let loader = std::fs::read(helper_dir.join("loader.json")).unwrap();
        let actual: Vec<serde_json::Value> = serde_json::from_slice(&loader).unwrap();
        assert_eq!(actual, expected, "{out_name}");
    }
    assert!(
        helper_dir.join("used").is_file(),
        "the helper in RUR_ASSET_REPO was not called"
    );
}

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_batch_jobs_keep_the_startup_choice() {
    use std::io::Write;
    let install = Installation::new("batch");
    let checkout = build_checkout();
    let out = install.dir.join("batch.json");
    // Each job replaces the environment, and names the checkout like a Node host would. Both
    // must still run standalone, as the process started.
    let job = serde_json::json!({
        "argv": ["--repo", checkout, "--project", install.project,
            "--story", install.project.join("story.luau"), "--out", out],
        "env": {"RUR_CACHE_DIR": install.cache, "RUR_ROBLOX_CONTENT": install.cache.join("content")},
    });
    let mut child = install
        .command()
        .arg("engine-batch")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn_locked()
        .unwrap();
    let bad_arguments = serde_json::json!({"argv": ["--time", "invalid"]});
    let live_session = serde_json::json!({"argv": ["--session", "1"]});
    let mut failing_child = job.clone();
    failing_child["argv"].as_array_mut().unwrap().extend([
        serde_json::json!("--actions"),
        serde_json::json!(install.dir.join("missing-actions.json")),
    ]);
    // Each kind of bad job answers once with its own exit code and leaves the runner able to
    // accept the next job, which is a good one.
    let jobs_and_codes = [
        (bad_arguments, 2),
        (failing_child, 1),
        (job.clone(), 0),
        (live_session, 2),
        (job, 0),
    ];
    let mut stdin = child.stdin.take().unwrap();
    for (job, _) in &jobs_and_codes {
        writeln!(stdin, "{job}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let codes: Vec<_> = batch_answers(&output)
        .iter()
        .map(|answer| answer["code"].as_i64().unwrap())
        .collect();
    let expected_codes: Vec<_> = jobs_and_codes.iter().map(|(_, code)| *code).collect();
    assert_eq!(codes, expected_codes, "{output:?}");
    let scene = serde_json::from_slice(&std::fs::read(out).unwrap()).unwrap();
    assert_story_ran(&scene);
}

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_batch_uses_the_jobs_cache_without_a_startup_user_cache() {
    let install = Installation::new("batch-without-home");
    let mut command = install.command();
    for name in ["HOME", "LOCALAPPDATA", "XDG_CACHE_HOME"] {
        command.env_remove(name);
    }
    let out = install.dir.join("scene.json");
    let job = serde_json::json!({
        "argv": ["--project", install.project, "--story", install.project.join("story.luau"), "--out", out],
        "env": {"RUR_CACHE_DIR": install.cache, "RUR_ROBLOX_CONTENT": install.cache.join("content")},
    });
    run_batch(command, &[job]);
    let scene = serde_json::from_slice(&std::fs::read(out).unwrap()).unwrap();
    assert_story_ran(&scene);
    assert!(!install.cache.join("rur").exists());
}

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_missing_user_cache_reports_an_error_without_panicking() {
    let install = Installation::new("missing-cache");
    // An empty cache location counts as missing
    for empty_variables in [false, true] {
        let mut command = install.command();
        for name in ["HOME", "LOCALAPPDATA", "XDG_CACHE_HOME"] {
            if empty_variables {
                command.env(name, "");
            } else {
                command.env_remove(name);
            }
        }
        let output = command
            .args(["render", "--no-png", "--story", "story.luau"])
            .output_locked()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("Set RUR_STATE_DIR"), "{error}");
        assert!(!error.contains("panicked"), "{error}");
    }
}

#[test]
fn cached_cli_binary_keeps_the_checkouts_project_defaults() {
    let install = Installation::checkout("cached-cli");
    let checkout = build_checkout();
    let built = Path::new(env!("CARGO_BIN_EXE_rur-native"));
    let cached = install_cached_copy(&install, built);

    let mut scenes = Vec::new();
    for (name, exe) in [("built", built), ("cached", cached.as_path())] {
        let mut command = installed(exe, &install.project, &install.cache);
        command.env("RUR_CACHE_DIR", &install.cache);
        let more = [
            "--repo",
            checkout.to_str().unwrap(),
            "--project",
            install.project.to_str().unwrap(),
        ];
        let scene = render(
            command,
            Path::new("story.luau"),
            &install.dir.join(name),
            &more,
        );
        scenes.push(scene);
    }
    let (built_scene, cached_scene) = (&scenes[0], &scenes[1]);
    // With no --roots, a checkout build leaves the roots to the engine's own defaults, which may
    // reject this project. The copy outside the checkout is the same build, so it must end up
    // exactly where the built one does, not with the standalone place roots.
    assert_eq!(cached_scene["status"], built_scene["status"]);
    assert_eq!(cached_scene["failure"], built_scene["failure"]);
}

/// A running `rur-native serve`, stopped when dropped.
struct Service(std::process::Child);

impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A local port nothing is listening on right now.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// The JSON body of `GET /stories` from the service on `port`, once it is up.
fn get_stories(port: u16) -> serde_json::Value {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::time::{Duration, Instant};
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stream = loop {
        if let Ok(stream) = TcpStream::connect_timeout(&address, Duration::from_millis(100)) {
            break stream;
        }
        assert!(Instant::now() < deadline, "service did not start");
        std::thread::sleep(Duration::from_millis(20));
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        stream,
        "GET /stories HTTP/1.1\r\nHost: {address}\r\nX-RUR: 1\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let body = response.split_once("\r\n\r\n").unwrap().1;
    serde_json::from_str(body).unwrap()
}

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_service_stories_use_the_current_or_environment_project() {
    let install = Installation::new("service-project");
    let root = "src";
    let stories = install.project.join(root).join("ReplicatedStorage");
    std::fs::create_dir_all(&stories).unwrap();
    std::fs::write(stories.join("Example.story.luau"), "return function() end").unwrap();
    std::fs::write(
        install.project.join("rur.json"),
        serde_json::json!({"places":[{"name":"main","roots":[root]}]}).to_string(),
    )
    .unwrap();
    // First the service finds the project in its current directory, then through RUR_PROJECT
    // while running from another folder
    for project_from_env in [false, true] {
        let port = free_port();
        let mut command = install.command();
        if project_from_env {
            command
                .current_dir(install.exe.parent().unwrap())
                .env("RUR_PROJECT", &install.project);
        }
        let _service = Service(
            command
                .args(["serve", "--port", &port.to_string(), "--idle", "30"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn_locked()
                .unwrap(),
        );
        let body = get_stories(port);
        assert_eq!(body["stories"].as_array().unwrap().len(), 1, "{body}");
        assert_eq!(
            body["stories"][0]["key"],
            format!("{root}/ReplicatedStorage/Example.story.luau")
        );
    }
}

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_copy_uses_user_cache_and_current_project_without_node() {
    let install = Installation::new("installed");
    let bin = install.exe.parent().unwrap();

    // --version needs no project, state directory or Node
    let version = install.command().arg("--version").output_locked().unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        concat!("rur-native ", env!("CARGO_PKG_VERSION"))
    );

    // The project is the current directory when nothing names one
    let scene = render(
        install.command(),
        Path::new("story.luau"),
        &install.dir.join("cwd"),
        &[],
    );
    assert_story_ran(&scene);

    // --project wins over RUR_PROJECT
    let mut command = install.command();
    command.env("RUR_PROJECT", install.dir.join("missing-project"));
    let story = Path::new("story.luau");
    let scene = render(
        command,
        story,
        &install.dir.join("flag"),
        &["--project", "."],
    );
    assert_story_ran(&scene);

    // RUR_PROJECT is used when the current directory is not the project. The story is then named
    // by its full path.
    let mut command = install.command();
    command
        .current_dir(bin)
        .env("RUR_PROJECT", &install.project);
    let story = install.project.join("story.luau");
    let scene = render(command, &story, &install.dir.join("env"), &[]);
    assert_story_ran(&scene);

    // The state directory is the per-user cache, and nothing else was written
    let state = if cfg!(target_os = "macos") {
        install.cache.join("Library/Caches/rur")
    } else {
        install.cache.join("rur")
    };
    assert!(state.is_dir());
    assert!(!install.project.join(".cache").exists());
    assert_eq!(std::fs::read_dir(bin).unwrap().count(), 1);
}

/// A release build keeps the frame store inside the output directory it was given. A checkout
/// build shares one beside it, which would write outside the directory a user named.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_saved_frames_stay_inside_the_named_output() {
    let install = Installation::new("saved-frames");
    let out = install.dir.join("captures/named");
    let output = install
        .command()
        .args([
            "--headless",
            "--save-frames",
            "--benchmark",
            "1",
            "--warmup",
            "0",
            "--viewport",
            "32x32",
            "--out",
        ])
        .arg(&out)
        .arg("--story")
        .arg(install.project.join("story.luau"))
        .output_locked()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!out.parent().unwrap().join(".frames").exists());
    let stored_files: Vec<_> = std::fs::read_dir(out.join(".frames"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    for extension in ["png", "json"] {
        assert!(
            stored_files
                .iter()
                .any(|path| path.extension().is_some_and(|ext| ext == extension)),
            "{stored_files:?}"
        );
    }
}

/// True when `text` names `path`, whichever way round the platform writes its slashes.
fn mentions_path(text: &str, path: &Path) -> bool {
    let forward = |text: &str| text.replace('\\', "/");
    forward(text).contains(&forward(path.to_str().unwrap()))
}

/// Asserts that a capture's JSON answer names the folder `expected`. The two are compared by
/// their resolved locations: when the working directory is reached through a link (macOS's /var,
/// or a linked temporary folder on Linux), the binary may report the link or its target.
fn assert_reports_output(stdout: &[u8], expected: &Path) {
    let answer: serde_json::Value = serde_json::from_slice(stdout).unwrap();
    let reported = Path::new(answer["output"].as_str().unwrap());
    assert_eq!(
        reported.canonicalize().unwrap(),
        expected.canonicalize().unwrap()
    );
}

/// A capture without `--out` lands under the state directory and leaves the current directory
/// alone. The old default `out/native`, named on the command line, is still relative to the
/// caller: an output the user names is never moved.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_default_captures_and_batch_summaries_stay_in_state() {
    let install = Installation::new("default-output");
    // Run from a folder of its own, so anything written beside the caller shows up in it. It is
    // reached through a link, as a working directory may be (macOS's temporary folders are links).
    let real_cwd = install.dir.join("working");
    let cwd = install.dir.join("cwd");
    std::fs::create_dir_all(&real_cwd).unwrap();
    link_directory(&real_cwd, &cwd);
    let state = install.cache.join("state");
    let story = install.project.join("story.luau");
    for explicit in [false, true] {
        let mut command = install.command();
        command
            .current_dir(&cwd)
            .env("RUR_STATE_DIR", &state)
            .args([
                "--headless",
                "--viewport",
                "32x32",
                "--time",
                "0",
                "--project",
            ])
            .arg(&install.project)
            .arg("--story")
            .arg(&story);
        if explicit {
            command.args([
                "--out",
                "out/native",
                "--benchmark",
                "1",
                "--warmup",
                "0",
                "--save-frames",
            ]);
        }
        let output = command.output_locked().unwrap();
        assert!(output.status.success(), "{output:?}");
        let out = if explicit {
            cwd.join("out/native")
        } else {
            state.join("out/native")
        };
        for file in ["render.png", "scene.json", "benchmark.json"] {
            assert!(out.join(file).is_file(), "{}", out.join(file).display());
        }
        assert_reports_output(&output.stdout, &out);
        if explicit {
            assert!(out.join(".frames").is_dir());
        } else {
            assert_eq!(std::fs::read_dir(&cwd).unwrap().count(), 0);
        }
    }
    // A batch writes its summary under the state directory too, and says where it went
    let batch = install.project.join("batch.json");
    std::fs::write(&batch, "[]").unwrap();
    let output = install
        .command()
        .current_dir(&cwd)
        .env("RUR_STATE_DIR", &state)
        .arg("--batch")
        .arg(&batch)
        .output_locked()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let summary = state.join("out/native/batch.json");
    assert!(summary.is_file());
    assert!(mentions_path(
        &String::from_utf8_lossy(&output.stderr),
        &summary
    ));
    assert!(!install.project.join("out").exists());

    // A checkout build resolves a relative output against --repo. With the (linked) working
    // directory as that repo, it reports the same folder as the installed build does.
    let checkout = Path::new(env!("CARGO_BIN_EXE_rur-native"));
    let output = installed(checkout, &cwd, &install.cache)
        .env("RUR_STATE_DIR", &state)
        .args([
            "--headless",
            "--repo",
            ".",
            "--out",
            "out/native",
            "--viewport",
            "32x32",
            "--time",
            "0",
            "--project",
        ])
        .arg(&install.project)
        .arg("--roots")
        .arg(install.project.join("src"))
        .arg("--story")
        .arg(&story)
        .output_locked()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_reports_output(&output.stdout, &real_cwd.join("out/native"));
}

/// The tail of a story: it draws the frame `assert_story_ran` looks for.
const DRAW_INSTALLED_FRAME: &str = "return function(target) local f=Instance.new('Frame'); f.Name='Installed'; f.Parent=target end";

/// A linked root in the second place must select that place whether the story is named through
/// the link or its target. The desktop and headless worker share this launch and standby key.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_headless_selects_its_place_through_a_linked_root() {
    let install = Installation::new("headless-linked-place");
    let second = install.dir.join("external").join("roots");
    std::fs::create_dir_all(second.join("ReplicatedStorage")).unwrap();
    std::fs::write(second.join("ReplicatedStorage/Value.luau"), "return 99").unwrap();
    let alias = install.project.join("linked");
    link_directory(&second, &alias);
    std::fs::write(
        install.project.join("rur.json"),
        r#"{"places":[{"name":"first","roots":["src"]},{"name":"second","roots":["linked"]}],"placeIds":{"src":1357}}"#,
    ).unwrap();
    std::fs::write(
        second.join("ReplicatedStorage/Example.story.luau"),
        story_expecting(0, &[("Value", 99)]),
    ).unwrap();
    for (name, root) in [("linked", &alias), ("canonical", &second)] {
        let story = root.join("ReplicatedStorage/Example.story.luau");
        let scene = render(
            install.command(), &story, &install.dir.join(format!("render-{name}")), &[],
        );
        assert_story_ran(&scene);
        let out = install.dir.join(format!("capture-{name}"));
        let output = install
            .command()
            .args(["--headless", "--viewport", "32x32", "--time", "0", "--story"])
            .arg(&story)
            .arg("--out")
            .arg(&out)
            .output_locked()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let scene =
            serde_json::from_slice(&std::fs::read(out.join("scene.json")).unwrap()).unwrap();
        assert_story_ran(&scene);
    }
}

/// A story that asserts the place id and what each ReplicatedStorage module returns, then draws
/// the frame `assert_story_ran` looks for.
fn story_expecting(place_id: u32, modules: &[(&str, u32)]) -> String {
    let mut story = format!("assert(game.PlaceId == {place_id}, tostring(game.PlaceId))\n");
    for (module, value) in modules {
        story.push_str(&format!(
            "assert(require(game:GetService('ReplicatedStorage').{module}) == {value})\n"
        ));
    }
    story + DRAW_INSTALLED_FRAME
}

/// An explicit repo makes both probes safe even if they get the mode wrong: checkout writes
/// there, while standalone writes to the redirected user cache despite --repo.
fn assert_binary_mode(install: &Installation, binary: &Path, mode: &str) {
    let dir = install.dir.join(format!("mode-{mode}"));
    let repo = dir.join("repo");
    let cache = dir.join("cache");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    let story = install.project.join("mode.luau");
    std::fs::write(&story, EDITABLE_IMAGE_STORY).unwrap();
    let scene = render(
        installed(binary, &install.project, &cache),
        &story,
        &dir.join("render"),
        &[
            "--repo", repo.to_str().unwrap(),
            "--project", install.project.to_str().unwrap(),
            "--roots", install.project.join("src").to_str().unwrap(),
        ],
    );
    assert_story_ran(&scene);
    let user_state = if cfg!(target_os = "macos") {
        cache.join("Library/Caches/rur")
    } else {
        cache.join("rur")
    };
    let wrote_to_repo = repo.join(".cache/editable-images").is_dir();
    let wrote_to_user_cache = user_state.join("editable-images").is_dir();
    assert_eq!(wrote_to_repo, mode == "checkout", "wrong {mode} mode");
    assert_eq!(wrote_to_user_cache, mode == "installed", "wrong {mode} mode");
}

/// Runs the same project inputs with a marked binary and Cargo's unmarked build. Check the modes
/// first so compiling both binaries with the release marker cannot turn a comparison into a pass.
fn comparison_binaries(install: &Installation) -> [(&'static str, &Path); 2] {
    let binaries = [
        ("checkout", Path::new(env!("CARGO_BIN_EXE_rur-native"))),
        ("installed", install.exe.as_path()),
    ];
    for (mode, binary) in binaries {
        assert_binary_mode(install, binary, mode);
    }
    binaries
}

/// The folder both modes get as `--repo` in a comparison.
fn comparison_repo(install: &Installation) -> std::path::PathBuf {
    let repo = install.dir.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    repo
}

/// A command for one side of a comparison. Both sides share a state directory, so a difference in
/// what they write cannot come from where they write it.
fn comparison_command(install: &Installation, binary: &Path) -> Command {
    let mut command = installed(binary, &install.project, &install.cache);
    command.env("RUR_STATE_DIR", install.cache.join("state"));
    command
}

/// Renders `story` with both modes, given the same repo, project, roots and snapshot choice, and
/// checks that the two scenes are equal. Only the timings differ, so they are left out.
fn compare_renders(install: &Installation, story: &Path, roots: &str, label: &str, more: &[&str]) {
    let repo = comparison_repo(install);
    let mut arguments = vec![
        "--repo",
        repo.to_str().unwrap(),
        "--project",
        install.project.to_str().unwrap(),
        "--roots",
        roots,
        "--no-snapshots",
    ];
    arguments.extend_from_slice(more);

    let mut scenes = Vec::new();
    for (mode, binary) in comparison_binaries(install) {
        let out = install.dir.join(format!("{mode}-{label}"));
        let mut scene = render(comparison_command(install, binary), story, &out, &arguments);
        assert_story_ran(&scene);
        scene["diagnostics"]
            .as_object_mut()
            .unwrap()
            .remove("timings");
        scenes.push(scene);
    }
    assert_eq!(scenes[0], scenes[1], "{label}");
}

/// An explicit empty `--roots` means no roots, in a release binary as in a checkout build, even
/// though the project's place has a root: only omitted roots come from the place. The story and the
/// harness fail if the place's `Value` is mounted anyway.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_explicit_empty_roots_match_checkout() {
    let install = Installation::new("empty-roots");
    let value_is_absent =
        "assert(not game:GetService('ReplicatedStorage'):FindFirstChild('Value'))";
    let story = install.project.join("story.luau");
    std::fs::write(&story, format!("{value_is_absent}\n{DRAW_INSTALLED_FRAME}")).unwrap();
    let harness = install.project.join("record.luau");
    std::fs::write(&harness, format!("return function() {value_is_absent} end")).unwrap();

    // Mounts and instances come on top of the empty roots, and must not bring the place's roots back
    let mount = format!("ReplicatedStorage/Mounted={}", install.project.display());
    let variants = [
        ("without-mounts", vec![]),
        ("with-mount", vec!["--mount", mount.as_str()]),
        (
            "with-instance",
            vec!["--instance", "ReplicatedStorage/Mounted=Folder"],
        ),
    ];
    for (label, more) in variants {
        compare_renders(&install, &story, "", label, &more);
        compare_recordings(&install, &harness, "", label, &more, None);
    }

    // The last `--roots` wins, so one that only holds empty entries empties the earlier list
    let place_roots = install.project.join("src");
    let place_roots = place_roots.to_str().unwrap();
    let empty_last = ["--roots", ";;"];
    compare_renders(&install, &story, place_roots, "empty-last", &empty_last);
    compare_recordings(
        &install,
        &harness,
        place_roots,
        "empty-last",
        &empty_last,
        None,
    );
}

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_overlapping_roots_match_checkout_in_both_orders() {
    let install = Installation::new("overlapping-roots");
    let external = install.dir.join("external/roots");
    let local = install.project.join("src");
    std::fs::create_dir_all(external.join("ReplicatedStorage")).unwrap();
    std::fs::write(external.join("ReplicatedStorage/Value.luau"), "return 99").unwrap();
    std::fs::write(
        external.join("ReplicatedStorage/External.luau"),
        "return 11",
    )
    .unwrap();
    std::fs::write(local.join("ReplicatedStorage/Local.luau"), "return 22").unwrap();
    let override_module = install.project.join("override.luau");
    std::fs::write(&override_module, "return 7").unwrap();
    let mount = format!("ReplicatedStorage/Value={}", override_module.display());
    let story = install.project.join("story.luau");

    // Both roots define `Value`, and a later root overrides an earlier one. A `--mount` applies
    // after every root, so it wins in both orders.
    let external_then_local = format!("{};{}", external.display(), local.display());
    let local_then_external = format!("{};{}", local.display(), external.display());
    let orders = [
        ("external-local", external_then_local, 42),
        ("local-external", local_then_external, 99),
    ];
    for (order, roots, value_from_roots) in orders {
        let variants: [(&str, u32, Vec<&str>); 2] = [
            ("roots", value_from_roots, vec![]),
            ("mount", 7, vec!["--mount", mount.as_str()]),
        ];
        for (variant, value, more) in variants {
            let expected = [("Value", value), ("External", 11), ("Local", 22)];
            std::fs::write(&story, story_expecting(0, &expected)).unwrap();
            let label = format!("{order}-{variant}");
            compare_renders(&install, &story, &roots, &label, &more);
        }
    }
}

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_linked_root_settings_match_checkout() {
    let install = Installation::new("linked-settings");
    let root = install.project.join("src");
    let actual = install.dir.join("external").join("actual");
    std::fs::create_dir_all(actual.parent().unwrap()).unwrap();
    std::fs::rename(&root, &actual).unwrap();
    link_directory(&actual, &root);
    std::fs::write(
        install.project.join("rur.json"),
        r#"{"places":[{"name":"main","roots":["src"]}],"placeIds":{"src":2468,"actual":9753},"streamingTargetRadius":{"src":123}}"#,
    ).unwrap();
    let story = actual.join("ReplicatedStorage/Example.story.luau");
    std::fs::write(
        &story,
        format!(
            "assert(workspace.StreamingEnabled)\n{}",
            story_expecting(2468, &[("Value", 42)])
        ),
    )
    .unwrap();
    for (name, source) in [
        (
            "declared",
            root.join("ReplicatedStorage/Example.story.luau"),
        ),
        ("target", story),
    ] {
        compare_renders(&install, &source, root.to_str().unwrap(), name, &[]);
    }
}

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_data_roots_match_checkout() {
    let install = Installation::new("data-roots");
    let data = install
        .project
        .join("external/library/src/ReplicatedStorage/Data");
    let external = install.dir.join("external/roots");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(external.join("ReplicatedStorage")).unwrap();
    std::fs::write(data.join("Value.luau"), "return 99").unwrap();
    std::fs::write(
        install.project.join(rur_native::engine::project::DATA_ROOTS_FILE),
        serde_json::json!({(rur_native::engine::project::DATA_ROOTS_KEY): ["ReplicatedStorage/Data"]}).to_string(),
    ).unwrap();
    let story = install.project.join("story.luau");
    std::fs::write(
        &story,
        story_expecting(0, &[("Value", 42), ("Data.Value", 99)]),
    )
    .unwrap();
    let roots = format!(
        "{};{}",
        install.project.join("src").display(),
        external.display()
    );
    compare_renders(&install, &story, &roots, "data", &[]);
    let harness = install.project.join("record.luau");
    std::fs::write(
        &harness,
        "return function()
	assert(require(game:GetService('ReplicatedStorage').Data.Value) == 99)
end",
    )
    .unwrap();
    compare_recordings(&install, &harness, &roots, "data", &[], None);
}

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_declared_place_name_matches_checkout() {
    let install = Installation::new("place-name");
    let external = install.dir.join("external/roots");
    std::fs::create_dir_all(external.join("ReplicatedStorage")).unwrap();
    std::fs::write(
        install.project.join("rur.json"),
        serde_json::json!({"places": [
            {"name": "first", "roots": ["src", external]},
            {"name": "selected", "roots": ["src", external]}
        ]})
        .to_string(),
    )
    .unwrap();
    let roots = format!(
        "{};{}",
        install.project.join("src").display(),
        external.display()
    );
    let story = install.project.join("story.luau");
    let harness = install.project.join("record.luau");
    let assertions = "assert(game.Name == 'selected', game.Name)\nassert(require(game:GetService('ReplicatedStorage').Value) == 42)";
    std::fs::write(&story, format!("{assertions}\n{DRAW_INSTALLED_FRAME}")).unwrap();
    std::fs::write(&harness, format!("return function() {assertions} end")).unwrap();
    compare_renders(&install, &story, &roots, "place", &["--place", "selected"]);
    compare_recordings(
        &install,
        &harness,
        &roots,
        "place",
        &["--place", "selected"],
        None,
    );
}

/// The default root is the project itself when it has no places. Its settings live above it.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_project_root_uses_parent_settings_like_checkout() {
    let install = Installation::new("parent-settings");
    std::fs::create_dir_all(install.project.join("ReplicatedStorage")).unwrap();
    std::fs::write(
        install.project.join("ReplicatedStorage/Value.luau"),
        "return 42",
    )
    .unwrap();
    std::fs::write(install.project.join("rur.json"), "{}").unwrap();
    std::fs::write(
        install.dir.join("rur.json"),
        r#"{"placeIds":{"project":2468}}"#,
    )
    .unwrap();
    let story = install.project.join("story.luau");
    std::fs::write(&story, story_expecting(2468, &[("Value", 42)])).unwrap();
    compare_renders(
        &install,
        &story,
        install.project.to_str().unwrap(),
        "parent",
        &[],
    );
    assert_story_ran(&render(
        install.command(),
        &story,
        &install.dir.join("implicit"),
        &[],
    ));
}

/// The client GUI module the project's `rur.json` names: `Mount` adds a ScreenGui called
/// `Configured` to the player's PlayerGui.
const CLIENT_GUI_MODULE: &str = "return {Mount = function()
	local gui = Instance.new('ScreenGui')
	gui.Name = 'Configured'
	gui.Parent = game:GetService('Players').LocalPlayer.PlayerGui
end}";

/// A recording harness that mounts the client GUI itself and then records a part proving it ran.
/// A story gets the GUI mounted for it when it first reaches the PlayerGui; a recording does not.
const CLIENT_GUI_HARNESS: &str = "return function()
	assert(game.PlaceId == 2468, tostring(game.PlaceId))
	local gui = game:GetService('Players').LocalPlayer:WaitForChild('PlayerGui')
	assert(gui:FindFirstChild('Configured') == nil)
	require(game:GetService('ReplicatedStorage').Gui).Mount()
	assert(gui.Configured:IsA('ScreenGui'))
	local part = Instance.new('Part')
	part.Name = 'RecordedWithGui'
	part.Anchored = true
	part.Parent = workspace
end";

/// A project with a `clientGui` setting must give the same story and recording in both modes. The
/// story finds the GUI mounted; the recording needs its harness to mount the GUI and check it.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_record_with_client_gui_matches_checkout() {
    let install = Installation::new("record-client-gui");
    let external = install.dir.join("external/roots");
    std::fs::create_dir_all(external.join("ReplicatedStorage")).unwrap();
    std::fs::write(
        install.project.join("src/ReplicatedStorage/Gui.luau"),
        CLIENT_GUI_MODULE,
    )
    .unwrap();
    std::fs::write(
        install.project.join("rur.json"),
        r#"{"places":[{"name":"main","roots":["src"]}],"placeIds":{"src":2468},"clientGui":{"src":"ReplicatedStorage.Gui"}}"#,
    )
    .unwrap();
    let roots = format!(
        "{};{}",
        install.project.join("src").display(),
        external.display()
    );
    let story = install.project.join("story.luau");
    std::fs::write(
        &story,
        format!(
            "local gui = game:GetService('Players').LocalPlayer:WaitForChild('PlayerGui')\n\
             assert(gui.Configured)\n{DRAW_INSTALLED_FRAME}"
        ),
    )
    .unwrap();
    compare_renders(&install, &story, &roots, "client-gui", &[]);
    let harness = install.project.join("record.luau");
    std::fs::write(&harness, CLIENT_GUI_HARNESS).unwrap();
    compare_recordings(
        &install,
        &harness,
        &roots,
        "client-gui",
        &[],
        Some("RecordedWithGui"),
    );
}

/// Records `harness` with both modes, given the same repo, project and roots, and checks that
/// what they saved is equal. `part` names a part the harness records, which must be in the saved
/// scene.
fn compare_recordings(
    install: &Installation,
    harness: &Path,
    roots: &str,
    label: &str,
    more: &[&str],
    part: Option<&str>,
) {
    let repo = comparison_repo(install);
    let mut recordings = Vec::new();
    for (mode, binary) in comparison_binaries(install) {
        let out = install.dir.join(format!("record-{mode}-{label}"));
        let output = comparison_command(install, binary)
            .args(["record", "--mode", "edit", "--harness"])
            .arg(harness)
            .args(["--seconds", "0.01", "--fps", "1"])
            .args(["--profile", "caches", "--out"])
            .arg(&out)
            .arg("--repo")
            .arg(&repo)
            .arg("--project")
            .arg(&install.project)
            .args(["--roots", roots, "--no-snapshots"])
            .args(more)
            .output_locked()
            .unwrap();
        // The recorder can exit successfully when its harness failed, so check what it saved
        assert_recording_ran(&output, &out, "edit");
        assert!(out.join("profile/profile.json").is_file());
        if let Some(name) = part {
            assert_scene_has_part(&out, name);
        }
        recordings.push(saved_recording(&output, &out));
    }
    assert_eq!(recordings[0], recordings[1], "{label}");
}

/// What a recording saved: the summary it printed and its data files. The summary's `out` is left
/// out because each mode writes to a folder of its own.
fn saved_recording(output: &std::process::Output, out: &Path) -> Vec<serde_json::Value> {
    let mut summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    summary.as_object_mut().unwrap().remove("out");
    let mut saved = vec![summary];
    for file in ["playback.json", "events.json", "camera.json"] {
        saved.push(serde_json::from_slice(&std::fs::read(out.join(file)).unwrap()).unwrap());
    }
    saved
}

/// The recording's `scene.rbxm` holds a Part called `name`.
fn assert_scene_has_part(out: &Path, name: &str) {
    let scene = std::fs::File::open(out.join("scene.rbxm")).unwrap();
    let tree = rbx_binary::from_reader(scene).unwrap();
    let instance = tree
        .descendants()
        .find(|instance| instance.name == name)
        .expect("recorded GUI harness part");
    assert_eq!(instance.class, "Part");
}

/// The smallest profile `profile report` and `profile diff` accept.
fn write_profile(file: &Path) {
    std::fs::write(
        file,
        r#"{"version":1,"run":{"status":"ok","frames":0},"summary":"Example profile"}"#,
    )
    .unwrap();
}

/// `profile report <file>`, or `profile diff <file> <file>`, which compares a profile with itself.
fn profile_command(install: &Installation, subcommand: &str, file: &Path) -> Command {
    let mut command = install.command();
    command.args(["profile", subcommand]).arg(file);
    if subcommand == "diff" {
        command.arg(file);
    }
    command
}

/// The page each profile command writes into its output folder.
fn profile_page(subcommand: &str) -> &'static str {
    if subcommand == "report" {
        "report.html"
    } else {
        "diff.html"
    }
}

/// The ways to leave a path option incomplete: no value, an empty value (separate and `=`
/// spellings), or another flag as its value. The flag case must not turn the next flag's name
/// into a path, even if a later default would make the command succeed. `--roots` accepts an
/// empty value, so only the other spellings are incomplete for it.
fn incomplete_options(flag: &str) -> Vec<Vec<String>> {
    let spellings = [
        vec![flag.into()],
        vec![flag.into(), String::new()],
        vec![format!("{flag}=")],
        vec![flag.into(), "--offline".into()],
        vec![flag.into(), "-h".into()],
    ];
    let gives_empty_value = |arguments: &Vec<String>| {
        arguments[0].ends_with('=') || arguments.get(1).is_some_and(String::is_empty)
    };
    spellings
        .into_iter()
        .filter(|arguments| flag != "--roots" || !gives_empty_value(arguments))
        .collect()
}

/// The command failed with the plain error for `flag`, not a panic.
fn assert_option_error(output: &std::process::Output, flag: &str) {
    assert!(!output.status.success(), "{flag}: {output:?}");
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains(&format!("{flag} needs a nonempty value")),
        "{error}"
    );
    assert!(!error.contains("panicked"), "{error}");
}

/// Checkout builds accept a trailing `--profile-out` and some empty outputs. The release build
/// deliberately refuses them before the recorder or any of its profilers starts.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_record_rejects_incomplete_output_flags() {
    let install = Installation::new("record-incomplete-output");
    let cwd = install.dir.join("cwd");
    let state = install.cache.join("state");
    let out = install.dir.join("recording");
    let harness = install.project.join("record.luau");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::write(&harness, PLAIN_PROJECT_HARNESS).unwrap();
    for flag in [
        "--out",
        "--dir",
        "--dump-scene",
        "--profile-out",
        "--context",
    ] {
        for arguments in incomplete_options(flag) {
            // The profile is requested by the flag, then by the environment
            for profile_from_environment in [false, true] {
                let mut command = install.command();
                command
                    .current_dir(&cwd)
                    .env("RUR_STATE_DIR", &state)
                    .args(["record", "--project"])
                    .arg(&install.project)
                    .arg("--harness")
                    .arg(&harness)
                    .args(["--mode", "edit", "--seconds", "0.01", "--fps", "1", "--out"])
                    .arg(&out);
                if profile_from_environment {
                    command.env("REE_PROFILE", "caches");
                } else {
                    command
                        .env_remove("REE_PROFILE")
                        .args(["--profile", "caches"]);
                }
                assert_option_error(&command.args(&arguments).output_locked().unwrap(), flag);
            }
        }
    }
    // Nothing was written: no recording, no state, no profile beside the project or the binary
    assert!(!out.exists());
    assert!(!state.exists());
    assert_eq!(std::fs::read_dir(&cwd).unwrap().count(), 0);
    assert_eq!(
        std::fs::read_dir(install.exe.parent().unwrap())
            .unwrap()
            .count(),
        1
    );
}

/// The checkout diff command accepts an incomplete `--out` and writes beside its input; report
/// ignores that option. The release commands deliberately require a nonempty output when named.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_profile_rejects_incomplete_output_flags() {
    let install = Installation::new("profile-incomplete-output");
    let cwd = install.dir.join("cwd");
    let inputs = install.dir.join("inputs");
    let state = install.cache.join("state");
    for dir in [&cwd, &inputs] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let file = inputs.join("profile.json");
    write_profile(&file);
    for subcommand in ["diff", "report", "render"] {
        for arguments in incomplete_options("--out") {
            let output = profile_command(&install, subcommand, &file)
                .current_dir(&cwd)
                .env("RUR_STATE_DIR", &state)
                .args(arguments)
                .output_locked()
                .unwrap();
            assert_option_error(&output, "--out");
        }
    }
    assert!(!state.exists());
    assert_eq!(std::fs::read_dir(&inputs).unwrap().count(), 1);
    assert_eq!(std::fs::read_dir(&cwd).unwrap().count(), 0);
}

/// `count` inputs for a profile command: the valid `profile.json` of the current folder, except
/// the one at `bad_position`, which is `value`.
fn profile_inputs(count: usize, bad_position: usize, value: &str) -> Vec<String> {
    let mut inputs = vec!["profile.json".to_string(); count];
    inputs[bad_position] = value.to_string();
    inputs
}

/// Runs `profile <command> <arguments>` in `cwd`, with the state directory at `state`.
fn run_profile(
    install: &Installation,
    cwd: &Path,
    state: &Path,
    command: &str,
    arguments: &[String],
) -> std::process::Output {
    install
        .command()
        .current_dir(cwd)
        .env("RUR_STATE_DIR", state)
        .args(["profile", command])
        .args(arguments)
        .output_locked()
        .unwrap()
}

/// An empty input must not select the current folder's profile, even when it is valid. Every
/// positional path is tried separately, with options before, between and after the inputs.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_commands_reject_unusable_positional_paths_before_work() {
    // Empty, blank, another flag, and the help flag: none can name a profile
    const UNUSABLE_INPUTS: [&str; 4] = ["", " \t ", "--offline", "-h"];
    let install = Installation::new("unusable-positionals");
    let cwd = install.dir.join("cwd");
    let state = install.cache.join("state");
    let out = install.dir.join("output");
    std::fs::create_dir_all(&cwd).unwrap();
    let profile = cwd.join("profile.json");
    write_profile(&profile);
    let original = std::fs::read(&profile).unwrap();
    let out_option = ["--out".to_string(), out.to_string_lossy().into_owned()];
    for (command, names, required) in path_contract::positional_paths() {
        for (position, name) in names.iter().enumerate() {
            for value in UNUSABLE_INPUTS {
                // `--out` goes before the input at `out_position`, or after the last one
                for out_position in 0..=names.len() {
                    let mut arguments = profile_inputs(names.len(), position, value);
                    // The view command has no output option
                    if command != "view" {
                        arguments.splice(out_position..out_position, out_option.clone());
                    }
                    let output = run_profile(&install, &cwd, &state, command, &arguments);
                    assert_option_error(&output, &format!("profile {command} {name}"));
                }
            }
        }
        // Missing required inputs must fail at the same point, before state is created.
        for count in 0..required {
            let arguments = vec!["profile.json".to_string(); count];
            let output = run_profile(&install, &cwd, &state, command, &arguments);
            assert_option_error(&output, &format!("profile {command} {}", names[count]));
        }
    }
    // Nothing was created or written: no state or output folder, the profile as it was, and no
    // other file in the working folder, the user cache or the folder of the binary.
    assert!(!state.exists());
    assert!(!out.exists());
    assert_eq!(std::fs::read(&profile).unwrap(), original);
    assert_eq!(std::fs::read_dir(&cwd).unwrap().count(), 1);
    assert_eq!(std::fs::read_dir(&install.cache).unwrap().count(), 0);
    assert_eq!(
        std::fs::read_dir(install.exe.parent().unwrap())
            .unwrap()
            .count(),
        1
    );
}

/// Ordinary builds keep accepting empty inputs as the current folder, as the pinned engine does.
#[test]
fn checkout_profile_empty_inputs_keep_the_current_folder() {
    let install = Installation::checkout("checkout-empty-profile");
    write_profile(&install.project.join("profile.json"));
    for command in ["report", "diff"] {
        let input_count = if command == "diff" { 2 } else { 1 };
        let output = install
            .command()
            .env("RUR_STATE_DIR", &install.cache)
            .args(["profile", command])
            .args(vec![""; input_count])
            .output_locked()
            .unwrap();
        assert!(output.status.success(), "{command}: {output:?}");
        // The page is written beside the profile that was found in the current folder
        assert!(install.project.join(profile_page(command)).is_file());
    }
}

/// Values that a request or manifest field must not accept as a path: not a string, empty,
/// blank, a flag, or containing a NUL character.
fn unusable_path_values() -> Vec<serde_json::Value> {
    use serde_json::{Value, json};
    vec![
        Value::Null,
        json!(false),
        json!([]),
        json!({}),
        json!(""),
        json!(" "),
        json!("--offline"),
        json!("\0"),
    ]
}

/// A missing `--controls` value must fail before a requested profile can create any files. The
/// profile is asked for by the flag and by the environment, as either would start writing.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_render_rejects_incomplete_controls_before_profiling() {
    let install = Installation::new("render-incomplete-controls");
    let state = install.cache.join("state");
    let out = install.dir.join("rendering");
    for arguments in incomplete_options("--controls") {
        for profile_from_environment in [false, true] {
            let mut command = install.command();
            command
                .env("RUR_STATE_DIR", &state)
                .args(["render", "--no-png", "--project"])
                .arg(&install.project)
                .arg("--story")
                .arg(install.project.join("story.luau"))
                .arg("--dir")
                .arg(&out);
            if profile_from_environment {
                command.env("REE_PROFILE", "caches");
            } else {
                command
                    .env_remove("REE_PROFILE")
                    .args(["--profile", "caches"]);
            }
            let output = command.args(&arguments).output_locked().unwrap();
            assert_option_error(&output, "--controls");
        }
    }
    assert!(!state.exists());
    assert!(!out.exists());
    assert!(!install.project.join("ree-profile").exists());
}

/// `--controls` and `--context` take a file or inline JSON. Both are checked as paths now, which
/// must not turn inline JSON into an error.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_file_or_json_options_keep_inline_json() {
    let install = Installation::new("inline-path-options");
    let out = install.dir.join("rendering");
    let story = install.project.join("story.luau");
    let rendering = render(install.command(), &story, &out, &["--controls", "{}"]);
    assert_story_ran(&rendering);

    let harness = install.project.join("record.luau");
    std::fs::write(&harness, PLAIN_PROJECT_HARNESS).unwrap();
    let out = install.dir.join("recording");
    let recording = install
        .command()
        .args(["record", "--harness"])
        .arg(&harness)
        .args(["--mode", "edit", "--seconds", "0.01", "--fps", "1"])
        .args(["--context", "{}", "--out"])
        .arg(&out)
        .output_locked()
        .unwrap();
    assert_recording_ran(&recording, &out, "edit");
}

/// The manifest is checked in full before a headless batch starts a GPU or writes any entry: the
/// first entry here is fine, so its output would exist if only the bad entry were refused.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_headless_batch_rejects_unusable_paths_before_outputs() {
    let install = Installation::new("batch-unusable-paths");
    let manifest = install.dir.join("batch.json");
    let summary_out = install.dir.join("summary");
    let entry_out = install.dir.join("entry");
    for field in ["scene", "out"] {
        for value in unusable_path_values() {
            let good_entry = serde_json::json!({"scene": "scene.json", "out": entry_out});
            let mut bad_entry = good_entry.clone();
            bad_entry[field] = value;
            let entries = serde_json::json!([good_entry, bad_entry]);
            std::fs::write(&manifest, entries.to_string()).unwrap();

            let output = install
                .command()
                .arg("--batch")
                .arg(&manifest)
                .arg("--out")
                .arg(&summary_out)
                .output_locked()
                .unwrap();
            assert!(!output.status.success(), "{output:?}");
            let error = String::from_utf8_lossy(&output.stderr);
            // A value that is not a string is a missing field, anything else an unusable one
            let refused = error.contains(&format!("{field} needs a nonempty value"))
                || error.contains(&format!("{field} is required"));
            assert!(refused, "{error}");
            assert!(!summary_out.exists());
            assert!(!entry_out.exists());
        }
    }
    assert!(!install.project.join("render.png").exists());
}

/// Some checkout parsers accept empty paths or use a default for an incomplete flag. Release
/// commands refuse those inputs consistently, including repeated options and both spellings.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_commands_reject_incomplete_path_flags() {
    let install = Installation::new("incomplete-paths");
    let cwd = install.dir.join("cwd");
    let state = install.cache.join("state");
    std::fs::create_dir_all(&cwd).unwrap();
    for subcommand in ["engine", "render", "sequence", "serve", "--headless"] {
        for flag in path_contract::path_flags() {
            for arguments in incomplete_options(flag) {
                let output = install
                    .command()
                    .current_dir(&cwd)
                    .env("RUR_STATE_DIR", &state)
                    .arg(subcommand)
                    .args(arguments)
                    .output_locked()
                    .unwrap();
                assert_option_error(&output, flag);
            }
        }
    }
    // Every command stopped before it created its state directory or wrote beside the cwd
    assert!(!state.exists());
    assert_eq!(std::fs::read_dir(&cwd).unwrap().count(), 0);
}

/// In-process jobs bypass startup, so they need the same check before the engine chooses defaults.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_batch_rejects_incomplete_path_flags() {
    use std::io::Write;
    let install = Installation::new("batch-incomplete-paths");
    let mut command = install.command();
    // A job whose variables match the runner's runs in the runner; one that asks for another repo
    // runs in a child process. Both must refuse the arguments before any work.
    let runner_environment = command_environment(&command);
    let child_repo = install.dir.join("child-repo");
    let child_environment = serde_json::json!({"RUR_REPO": child_repo});
    let mut child = command
        .arg("engine-batch")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn_locked()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    // The flag each job is expected to be refused for, in the order the jobs are sent
    let mut expected_flags = Vec::new();
    for flag in path_contract::path_flags() {
        for arguments in incomplete_options(flag) {
            for environment in [&runner_environment, &child_environment] {
                let job = serde_json::json!({"argv": arguments, "env": environment});
                writeln!(stdin, "{job}").unwrap();
                expected_flags.push(flag);
            }
        }
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let answers = batch_answers(&output);
    assert_eq!(answers.len(), expected_flags.len(), "{output:?}");
    for (answer, flag) in answers.iter().zip(expected_flags) {
        assert_eq!(answer["code"], 2, "{answer}");
        assert_eq!(answer["error"], format!("{flag} needs a nonempty value"));
    }
    assert!(!install.project.join("--offline").exists());
    assert!(!child_repo.exists());
}

/// Empty path variables count as unset. They must neither suppress a standalone default nor
/// make the current directory the output of a profile or dump.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_empty_environment_paths_keep_safe_defaults() {
    let install = Installation::new("empty-environment-paths");
    let out = install.dir.join("rendering");
    let mut command = install.command();
    for name in [
        "RUR_STATE_DIR",
        "RUR_CACHE_DIR",
        "RUR_PROJECT",
        "RUR_REPO",
        "RUR_ASSET_REPO",
        "REE_PROFILE_OUT",
        "RUR_BOUNDARY_PROFILE",
        "RUR_LUAU_PROFILE",
        "RUR_LUAU_ALLOC_PROFILE",
        "RUR_ENGINE_DUMP",
    ] {
        command.env(name, "");
    }
    command.env("REE_PROFILE", "caches");
    let scene = render(command, Path::new("story.luau"), &out, &[]);
    assert_story_ran(&scene);
    assert!(out.join("profile/profile.json").is_file());
    let cache = if cfg!(target_os = "macos") {
        install.cache.join("Library/Caches/rur")
    } else {
        install.cache.join("rur")
    };
    assert!(cache.is_dir());
    for name in [
        "ree-profile",
        "ree-boundary.json",
        "profile.json",
        ".cache",
        "out",
    ] {
        assert!(!install.project.join(name).exists(), "{name}");
    }
    assert_eq!(
        std::fs::read_dir(install.exe.parent().unwrap())
            .unwrap()
            .count(),
        1
    );
}

/// A checkout build keeps the engine's defaults: the pages land beside the profile.
#[test]
fn checkout_profile_reports_keep_their_original_output_defaults() {
    let install = Installation::checkout("checkout-profile");
    let file = install.project.join("profile.json");
    write_profile(&file);
    for subcommand in ["report", "diff"] {
        let output = profile_command(&install, subcommand, &file)
            .output_locked()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    for name in ["report.html", "diff.html", "diff.json", "diff.txt"] {
        assert!(install.project.join(name).is_file());
    }
}

/// An installed binary writes profile pages and recordings' profiles under the state directory
/// unless the caller names an output. The profile it reads sits in a folder of its own, and the
/// current directory is another, so a stray file in either shows up.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_profile_defaults_stay_in_state_and_honor_named_outputs() {
    let install = Installation::new("profile-defaults");
    let cwd = install.dir.join("cwd");
    let inputs = install.dir.join("inputs");
    for dir in [&cwd, &inputs] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let file = inputs.join("profile.json");
    write_profile(&file);
    let state = install.cache.join("state");
    for explicit in [false, true] {
        for subcommand in ["report", "diff"] {
            let mut run = profile_command(&install, subcommand, &file);
            run.current_dir(&cwd).env("RUR_STATE_DIR", &state);
            let out = if explicit {
                // A named output is relative to the current directory
                run.args(["--out", subcommand]);
                cwd.join(subcommand)
            } else {
                state.join("out").join(format!("profile-{subcommand}"))
            };
            let output = run.output_locked().unwrap();
            assert!(output.status.success(), "{output:?}");
            assert!(out.join(profile_page(subcommand)).is_file());
        }
        assert_eq!(std::fs::read_dir(&inputs).unwrap().count(), 1);
        if !explicit {
            assert_eq!(std::fs::read_dir(&cwd).unwrap().count(), 0);
        }
    }
    // The pinned recorder starts its profiler before it checks its arguments. This run fails
    // (it has no `--out`), and its profile must still go to the state directory rather than
    // to a `ree-profile` folder beside the caller.
    let harness = install.project.join("record.luau");
    std::fs::write(&harness, PLAIN_PROJECT_HARNESS).unwrap();
    let output = install
        .command()
        .current_dir(&cwd)
        .env("RUR_STATE_DIR", &state)
        .args(["record", "--project"])
        .arg(&install.project)
        .arg("--harness")
        .arg(&harness)
        .args(["--profile", "caches"])
        .output_locked()
        .unwrap();
    assert!(!output.status.success());
    assert!(!cwd.join("ree-profile").exists());
    let profile: serde_json::Value = serde_json::from_slice(
        &std::fs::read(state.join("out/record/profile/profile.json")).unwrap(),
    ).unwrap();
    assert_eq!(profile["run"]["status"], "failed", "{profile}");
    // `--dump-scene` is an alias of `--out` with no profile default of its own: the profile goes
    // below it
    let out = cwd.join("record-alias");
    let output = install
        .command()
        .current_dir(&cwd)
        .env("RUR_STATE_DIR", &state)
        .args(["record", "--project"])
        .arg(&install.project)
        .arg("--harness")
        .arg(&harness)
        .args([
            "--seconds",
            "0.01",
            "--fps",
            "1",
            "--profile",
            "caches",
            "--dump-scene",
        ])
        .arg(&out)
        .output_locked()
        .unwrap();
    assert_recording_ran(&output, &out, "play");
    assert!(out.join("profile/profile.json").is_file());
    assert!(!cwd.join("ree-profile").exists());
}

/// A `record` command for one side of a comparison, run in `cwd` with no profile variable set. It
/// records `harness` with the same repo, project and roots in both modes. The caller adds the
/// profile settings and the recording output, or leaves the output out on purpose.
fn comparison_record_command(
    install: &Installation,
    binary: &Path,
    repo: &Path,
    harness: &Path,
    cwd: &Path,
) -> Command {
    let mut command = comparison_command(install, binary);
    command
        .current_dir(cwd)
        .env_remove("REE_PROFILE")
        .env_remove("REE_PROFILE_OUT")
        .env_remove("RUR_BOUNDARY_PROFILE")
        .args(["record", "--repo"])
        .arg(repo)
        .arg("--project")
        .arg(&install.project)
        .arg("--roots")
        .arg(install.project.join("src"))
        .args(["--no-snapshots", "--harness"])
        .arg(harness)
        .args(["--mode", "edit", "--seconds", "0.01", "--fps", "1"]);
    command
}

/// The recorder profiles before it validates its output arguments. A profile that `REE_PROFILE`
/// turns on must get the same standalone default as one that `--profile` turns on, both when the
/// recording succeeds and when the command fails for want of `--out`.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_record_environment_profiles_use_standalone_defaults() {
    let install = Installation::new("record-env-defaults");
    let repo = comparison_repo(&install);
    let harness = install.project.join("record.luau");
    std::fs::write(&harness, PLAIN_PROJECT_HARNESS).unwrap();
    // The option that names the recording output (none leaves it out, so the command fails), and
    // whether `REE_PROFILE` or `--profile` turns the profile on.
    let cases = [
        (Some("--dump-scene"), true),
        (Some("--out"), true),
        (Some("--dir"), true),
        (None, true),
        (Some("--dump-scene"), false),
    ];
    for (mode, binary) in comparison_binaries(&install) {
        for (output_option, profile_from_environment) in cases {
            let case = format!(
                "{mode}-{}-{profile_from_environment}",
                output_option
                    .unwrap_or("missing-out")
                    .trim_start_matches('-')
            );
            let cwd = install.dir.join(&case);
            let state = install.cache.join(&case);
            let out = cwd.join("recording");
            std::fs::create_dir_all(&cwd).unwrap();
            let mut command = comparison_record_command(&install, binary, &repo, &harness, &cwd);
            command
                .env("RUR_STATE_DIR", &state)
                // The boundary profiler is not running, so this must not stop the default
                .env("RUR_BOUNDARY_PROFILE", cwd.join("unused-boundary.json"));
            if profile_from_environment {
                command.env("REE_PROFILE", "caches");
            } else {
                command.args(["--profile", "caches"]);
            }
            if let Some(option) = output_option {
                command.arg(option).arg(&out);
            }

            let output = command.output_locked().unwrap();
            let recorded = output_option.is_some();
            if recorded {
                assert_recording_ran(&output, &out, "edit");
            } else {
                assert!(!output.status.success(), "{output:?}");
                assert!(
                    String::from_utf8_lossy(&output.stderr).contains("--out <dir> is required")
                );
            }

            // An installed binary puts the profile below the recording, else in the state
            // directory. A checkout build keeps the engine's own `ree-profile` default where the
            // recording output does not carry a profile folder.
            let profile_directory = match (mode, output_option) {
                ("installed", None) => state.join("out/record/profile"),
                ("checkout", None | Some("--dump-scene")) => cwd.join("ree-profile"),
                _ => out.join("profile"),
            };
            let profile: serde_json::Value = serde_json::from_slice(
                &std::fs::read(profile_directory.join("profile.json")).unwrap(),
            )
            .unwrap();
            let expected_status = if recorded { "ok" } else { "failed" };
            assert_eq!(profile["run"]["status"], expected_status);
            assert!(!cwd.join("unused-boundary.json").exists());
            if mode == "installed" {
                // Nothing lands beside the caller (except the recording) or the executable
                assert!(!cwd.join("ree-profile").exists());
                let files_in_cwd = std::fs::read_dir(&cwd).unwrap().count();
                assert_eq!(files_in_cwd, usize::from(recorded));
                let files_beside_exe = std::fs::read_dir(install.exe.parent().unwrap())
                    .unwrap()
                    .count();
                assert_eq!(files_beside_exe, 1);
            }
        }
    }
}

/// An explicit `REE_PROFILE_OUT` (for the engine's profile) or `RUR_BOUNDARY_PROFILE` (for the
/// boundary profile) keeps the path the caller wrote, relative to the current directory or
/// absolute, in both build modes. It also survives a failed recording that has no output of its
/// own.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_record_profiles_honor_environment_outputs() {
    let install = Installation::new("record-env-outputs");
    let repo = comparison_repo(&install);
    let harness = install.project.join("record.luau");
    std::fs::write(&harness, PLAIN_PROJECT_HARNESS).unwrap();
    for (mode, binary) in comparison_binaries(&install) {
        for variable in ["REE_PROFILE_OUT", "RUR_BOUNDARY_PROFILE"] {
            // The engine's profile writes a folder, the boundary profile one JSON file
            let engine_profile = variable == "REE_PROFILE_OUT";
            let chosen_name = if engine_profile {
                "chosen-profile"
            } else {
                "chosen.json"
            };
            for names_recording_output in [false, true] {
                for written_relative in [false, true] {
                    let case =
                        format!("{mode}-{variable}-{names_recording_output}-{written_relative}");
                    let cwd = install.dir.join(&case);
                    std::fs::create_dir_all(&cwd).unwrap();
                    let chosen = cwd.join(chosen_name);
                    let out = cwd.join("recording");
                    let mut command =
                        comparison_record_command(&install, binary, &repo, &harness, &cwd);
                    if written_relative {
                        command.env(variable, chosen_name);
                    } else {
                        command.env(variable, &chosen);
                    }
                    if engine_profile {
                        command.env("REE_PROFILE", "caches");
                    }
                    if names_recording_output {
                        command.arg("--out").arg(&out);
                    }

                    let output = command.output_locked().unwrap();
                    if names_recording_output {
                        assert_recording_ran(&output, &out, "edit");
                    } else {
                        assert!(!output.status.success(), "{output:?}");
                    }
                    // Both profilers start before the arguments are validated, so they write on
                    // a failed command too
                    let written = if engine_profile {
                        chosen.join("profile.json")
                    } else {
                        chosen
                    };
                    assert!(written.is_file(), "{}: {output:?}", written.display());
                    assert!(!out.join("profile").exists());
                    assert!(!cwd.join("ree-profile").exists());
                    assert!(!cwd.join("ree-boundary.json").exists());
                }
            }
        }
    }
}

/// A JSON service request after `get_stories` has waited for startup.
fn post(port: u16, endpoint: &str, body: &serde_json::Value) -> serde_json::Value {
    let (status, answer) = service_request(port, "POST", &format!("/{endpoint}"), body);
    assert_eq!(status, 200, "{answer}");
    answer
}

/// One request to the service, with the status and JSON body of its answer. Unlike `post` it
/// accepts any status, and it reads the body by its length: `/shutdown` exits after answering,
/// and waiting for the connection to end can see a Windows reset instead of the answer.
fn service_request(
    port: u16,
    method: &str,
    path: &str,
    body: &serde_json::Value,
) -> (u16, serde_json::Value) {
    use std::io::{BufRead, BufReader, Read, Write};
    let mut stream =
        std::net::TcpStream::connect(std::net::SocketAddr::from(([127, 0, 0, 1], port))).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .unwrap();
    let body = body.to_string();
    write!(stream, "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-RUR: 1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let status = line.split_whitespace().nth(1).unwrap().parse().unwrap();
    let mut length = None;
    loop {
        line.clear();
        let read = reader.read_line(&mut line).unwrap();
        assert!(read > 0, "incomplete response headers");
        if line == "\r\n" {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length: ") {
            length = Some(value.trim().parse::<usize>().unwrap());
        }
    }
    let mut answer = vec![0; length.unwrap()];
    reader.read_exact(&mut answer).unwrap();
    (status, serde_json::from_slice(&answer).unwrap())
}

/// A request to `endpoint` that must be refused with `error` and a failing status.
fn assert_service_refuses(
    port: u16,
    endpoint: &str,
    body: &serde_json::Value,
    error: impl AsRef<str>,
) {
    let (status, answer) = service_request(port, "POST", endpoint, body);
    assert!(status >= 400, "{endpoint}: {status} {answer}");
    assert_eq!(answer["error"], error.as_ref(), "{endpoint}: {answer}");
}

/// Unusable paths must fail before a VM, a standby or a sequence can write anything. This needs
/// no GPU, `/sequence` included: a refused request never reaches its GPU startup.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_service_rejects_unusable_paths_before_work() {
    use serde_json::Value;
    let install = Installation::new("service-invalid-paths");
    let cwd = install.dir.join("cwd");
    let out = install.dir.join("rendering");
    std::fs::create_dir_all(&cwd).unwrap();
    let port = free_port();
    let _service = Service(
        install
            .command()
            .current_dir(&cwd)
            .args(["serve", "--port", &port.to_string(), "--idle", "120"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn_locked()
            .unwrap(),
    );
    get_stories(port);

    for endpoint in ["/render", "/sequence"] {
        let valid = serde_json::json!({
            "argv": ["--project", install.project, "--story", install.project.join("story.luau")],
            "out": out,
            "png": false,
            "offline": true,
            "env": {"RUR_ROBLOX_CONTENT": install.cache.join("content")},
            "sequence": {"frames": 1, "png": false, "video": false, "scenes": true},
        });
        // A value that is not a string is a missing `out`, any other unusable one a bad `out`
        for value in unusable_path_values() {
            let mut body = valid.clone();
            body["out"] = value.clone();
            let (status, answer) = service_request(port, "POST", endpoint, &body);
            assert!(status >= 400, "{endpoint}: {status} {answer}");
            let error = if value.is_string() {
                "out needs a nonempty value"
            } else {
                "out is required"
            };
            assert_eq!(answer["error"], error, "{endpoint}: {value}: {answer}");
        }
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove("out");
        assert_service_refuses(port, endpoint, &missing, "out is required");

        for flag in path_contract::path_flags() {
            for arguments in incomplete_options(flag) {
                let mut body = valid.clone();
                let argv = body["argv"].as_array_mut().unwrap();
                argv.extend(arguments.into_iter().map(Value::String));
                let error = format!("{flag} needs a nonempty value");
                assert_service_refuses(port, endpoint, &body, error);
            }
        }
    }

    // The query is percent-decoded: `%00` is a NUL character and `+` a space
    for project in ["", "--offline", "%00", "+"] {
        let path = format!("/stories?project={project}");
        let (status, answer) = service_request(port, "GET", &path, &Value::Null);
        assert_eq!(status, 400, "{answer}");
        assert_eq!(answer["error"], "project needs a nonempty value");
    }

    // The refused requests started no render, and the service still answers
    let (status, health) = service_request(port, "GET", "/health", &Value::Null);
    assert_eq!(status, 200);
    assert_eq!(health["renders"], 0);
    assert_eq!(health["kept"], Value::Null);
    assert_eq!(service_request(port, "GET", "/memory", &Value::Null).0, 200);
    let (status, answer) = service_request(port, "POST", "/shutdown", &Value::Null);
    assert_eq!(status, 200);
    assert_eq!(answer["ok"], true);

    assert!(!out.exists());
    assert_eq!(std::fs::read_dir(&cwd).unwrap().count(), 0);
    // Only the executable is beside the executable: no state or log was written there
    let beside_executable = std::fs::read_dir(install.exe.parent().unwrap()).unwrap();
    assert_eq!(beside_executable.count(), 1);
}

/// One request to the render service: its endpoint, the place whose story it names, that story
/// and the number its `Value` module returns there, which names the frame the scene must hold.
struct PlaceRequest {
    endpoint: &'static str,
    place: &'static str,
    story: &'static str,
    expected: i32,
}

const fn request(
    endpoint: &'static str,
    place: &'static str,
    story: &'static str,
    expected: i32,
) -> PlaceRequest {
    PlaceRequest {
        endpoint,
        place,
        story,
        expected,
    }
}

/// A project with two places that give the same module name different values (42 and 84), and
/// two stories in each that draw a frame named after that value.
fn write_two_places(install: &Installation) {
    std::fs::write(
        install.project.join("rur.json"),
        r#"{"places":[{"name":"first","roots":["first"]},{"name":"second","roots":["second"]}]}"#,
    )
    .unwrap();
    let source = "local value=require(game:GetService('ReplicatedStorage').Value)\nreturn function(target) local f=Instance.new('Frame'); f.Name='Value'..value; f.Parent=target end";
    for (place, value) in [("first", 42), ("second", 84)] {
        let root = install.project.join(place).join("ReplicatedStorage");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("Value.luau"), format!("return {value}")).unwrap();
        for name in ["Example", "Other"] {
            std::fs::write(root.join(format!("{name}.story.luau")), source).unwrap();
        }
    }
}

/// Sends `request` to the service and checks the scene it wrote holds the frame of the requested
/// place's value, not the value of a place an earlier request left loaded.
fn assert_request_uses_its_place(
    install: &Installation,
    port: u16,
    index: usize,
    request: &PlaceRequest,
) {
    let out = install.dir.join(format!("request-{index}"));
    let story = install
        .project
        .join(request.place)
        .join(format!("ReplicatedStorage/{}.story.luau", request.story));
    let body = serde_json::json!({
        "argv": ["--project", install.project, "--story", story, "--time", "0", "--viewport", "32x32"],
        "out": out, "png": false, "offline": true,
        "sequence": {"frames": 1, "png": false, "video": false, "scenes": true},
        "env": {"RUR_ROBLOX_CONTENT": install.cache.join("content")},
    });
    let answer = post(port, request.endpoint, &body);
    assert!(answer.get("result").is_some(), "{answer}");
    let scene_file = if request.endpoint == "render" {
        out.join("scene.json")
    } else {
        out.join("scenes/00000.json")
    };
    let scene: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&scene_file)
            .unwrap_or_else(|error| panic!("{}: {error}", scene_file.display())),
    )
    .unwrap();
    assert_eq!(scene["status"], "ok", "{scene}");
    let frame = format!(".Value{}", request.expected);
    assert!(scene_has_node_ending(&scene, &frame), "{scene}");
}

/// Runs `requests` in order against one service. The service keeps a warm standby for the
/// startup arguments it last saw, and a standby is only reused when the request's startup key
/// matches. The key must carry the place and its roots: a request for the other place must
/// not be served from the standby the previous place left.
fn service_place_requests(name: &str, requests: &[PlaceRequest]) {
    let install = Installation::new(name);
    write_two_places(&install);
    let port = free_port();
    let _service = Service(
        install
            .command()
            .args(["serve", "--port", &port.to_string(), "--idle", "60"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn_locked()
            .unwrap(),
    );
    get_stories(port);
    for (index, request) in requests.iter().enumerate() {
        assert_request_uses_its_place(&install, port, index, request);
        // Let the replacement standby's project load before asking for the other place.
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let state = if cfg!(target_os = "macos") {
        install.cache.join("Library/Caches/rur")
    } else {
        install.cache.join("rur")
    };
    let log = std::fs::read_to_string(state.join("serve.log")).unwrap();
    assert!(log.contains("standby"), "no standby was exercised: {log}");
}

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_service_standbys_keep_the_selected_place_for_render() {
    service_place_requests(
        "service-render-places",
        &[
            request("render", "first", "Example", 42),
            request("render", "first", "Other", 42),
            request("render", "second", "Example", 84),
            request("render", "first", "Other", 42),
        ],
    );
}

/// What `rur.json` says when one request of [`service_kept_project_changes`] is sent, and the name
/// of the frame the story must draw then.
struct ProjectChoice {
    place: &'static str,
    roots: &'static [&'static str],
    expected: &'static str,
}

const fn choice(
    place: &'static str,
    roots: &'static [&'static str],
    expected: &'static str,
) -> ProjectChoice {
    ProjectChoice {
        place,
        roots,
        expected,
    }
}

fn write_project_choice(install: &Installation, choice: &ProjectChoice) {
    std::fs::write(
        install.project.join("rur.json"),
        serde_json::json!({"places": [{"name": choice.place, "roots": choice.roots}]}).to_string(),
    )
    .unwrap();
}

/// Whether some node of `scene` has a path that ends with `suffix`.
fn scene_has_node_ending(scene: &serde_json::Value, suffix: &str) -> bool {
    scene["nodes"].as_array().unwrap().iter().any(|node| {
        node["path"]
            .as_str()
            .is_some_and(|path| path.ends_with(suffix))
    })
}

/// Sends one story to the service while `rur.json` changes between the requests. Request `n`
/// asks for second `n` of the same arguments, so the service may go on from the run it kept. It
/// may only do so while the choice in `rur.json` stays the same: a request that spells the same
/// arguments can resolve to another place or root order after the file changes, and the
/// order-independent fingerprint of the project does not see that.
///
/// The first two `choices` must be equal, so the second request has to reuse the first run (the
/// log says "kept run from 0.0000 s"). The last must differ, so the third request has to start
/// again instead of going on from the second run (no "kept run from 1.0000 s").
fn service_kept_project_changes(name: &str, expression: &str, choices: &[ProjectChoice]) {
    let install = Installation::new(name);
    // A second root with the same module name and another value, to put the roots in two orders
    let other = install.project.join("other/ReplicatedStorage");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("Value.luau"), "return 84").unwrap();
    let story = install.project.join("story.luau");
    std::fs::write(
        &story,
        format!("return function(target) local f=Instance.new('Frame'); f.Name={expression}; f.Parent=target end"),
    )
    .unwrap();
    write_project_choice(&install, &choices[0]);
    let state = install.dir.join("state");
    let port = free_port();
    let _service = Service(
        install
            .command()
            .env("RUR_STATE_DIR", &state)
            .args(["serve", "--port", &port.to_string(), "--idle", "60"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn_locked()
            .unwrap(),
    );
    get_stories(port);
    for (index, choice) in choices.iter().enumerate() {
        write_project_choice(&install, choice);
        let out = install.dir.join(format!("request-{index}"));
        let body = serde_json::json!({
            "argv": ["--project", install.project, "--story", story, "--time", index.to_string(), "--viewport", "32x32"],
            "out": out, "png": false, "offline": true,
            "env": {"RUR_ROBLOX_CONTENT": install.cache.join("content")},
        });
        let answer = post(port, "render", &body);
        assert!(answer.get("result").is_some(), "{answer}");
        let scene: serde_json::Value =
            serde_json::from_slice(&std::fs::read(out.join("scene.json")).unwrap()).unwrap();
        assert_eq!(scene["status"], "ok", "{scene}");
        let frame = format!(".StoryTarget.{}", choice.expected);
        assert!(scene_has_node_ending(&scene, &frame), "{scene}");
    }
    let log = std::fs::read_to_string(state.join("serve.log")).unwrap();
    assert!(
        log.contains("kept run from 0.0000 s"),
        "no kept run was exercised: {log}"
    );
    assert!(
        !log.contains("kept run from 1.0000 s"),
        "the changed project kept its old run: {log}"
    );
}

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_service_kept_runs_follow_changed_root_order() {
    service_kept_project_changes(
        "service-kept-root-order",
        "tostring(require(game:GetService('ReplicatedStorage').Value))",
        &[
            choice("main", &["src", "other"], "84"),
            choice("main", &["src", "other"], "84"),
            choice("main", &["other", "src"], "42"),
        ],
    );
}

#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_service_kept_runs_follow_changed_place_name() {
    service_kept_project_changes(
        "service-kept-place-name",
        "game.Name",
        &[
            choice("first", &["src"], "first"),
            choice("first", &["src"], "first"),
            choice("second", &["src"], "second"),
        ],
    );
}

/// Sequences create a GPU device even without PNG output, so this one is its own test: the
/// release workflow skips it on Windows runners, which have no GPU.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_service_standbys_keep_the_selected_place_for_sequence() {
    service_place_requests(
        "service-sequence-places",
        &[
            request("render", "first", "Example", 42),
            request("render", "first", "Other", 42),
            request("sequence", "second", "Example", 84),
            request("sequence", "first", "Example", 42),
            request("sequence", "second", "Other", 84),
        ],
    );
}

/// A story is placed by comparing canonical paths, so a link to the project or to the story still
/// finds the place whose roots hold it. The story sits under `src`, which only the second place
/// lists: the first place is what a failed match would fall back to.
#[cfg(any(windows, unix))]
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_story_selects_its_place_through_a_project_alias() {
    let install = Installation::new("project-alias");
    std::fs::create_dir_all(install.project.join("first/ReplicatedStorage")).unwrap();
    std::fs::write(
        install.project.join("first/ReplicatedStorage/Value.luau"),
        "return 0",
    )
    .unwrap();
    std::fs::write(
        install.project.join("rur.json"),
        r#"{"places":[{"name":"first","roots":["first"]},{"name":"second","roots":["src"]}]}"#,
    )
    .unwrap();
    let story = install.project.join("src/Example.story.luau");
    std::fs::rename(install.project.join("story.luau"), &story).unwrap();
    let alias = install.dir.join("alias");
    link_directory(&install.project, &alias);
    for (name, project, source) in [
        // The project is the link and the story is named by its real path
        ("alias-project", alias.as_path(), story.clone()),
        // The project is real and the story is named through the link
        (
            "alias-story",
            install.project.as_path(),
            alias.join("src/Example.story.luau"),
        ),
    ] {
        let scene = render(
            install.command(),
            &source,
            &install.dir.join(name),
            &["--project", project.to_str().unwrap()],
        );
        assert_story_ran(&scene);
    }
}

/// The `RUR_*` variables a command will start with: its own, and those it inherits unless it
/// removes them.
fn command_environment(command: &Command) -> serde_json::Value {
    let mut environment: serde_json::Map<String, serde_json::Value> = std::env::vars()
        .filter(|(name, _)| name.starts_with("RUR_"))
        .map(|(name, value)| (name, value.into()))
        .collect();
    for (name, value) in command.get_envs() {
        let name = name.to_str().unwrap();
        if name.starts_with("RUR_") {
            if let Some(value) = value {
                environment.insert(name.into(), value.to_str().unwrap().into());
            } else {
                environment.remove(name);
            }
        }
    }
    environment.into()
}

/// A job whose variables match the runner's runs in the runner, and one that differs runs in a
/// child. Either way a failing job gives the same structured error.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_batch_reuses_matching_environments_and_preserves_job_errors() {
    use std::io::Write;
    use std::process::Stdio;
    let install = Installation::new("batch-warm");
    let mut command = install.command();
    let runner_variables = command_environment(&command);
    let mut runner = command
        .arg("engine-batch")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_locked()
        .unwrap();
    // The fixture fails unless the job runs in the runner's own process
    let fixture = install.project.join("process.luau");
    std::fs::write(
        &fixture,
        format!(
            "return function() assert(rur.processId == {}) end",
            runner.id()
        ),
    )
    .unwrap();
    let scene_path = install.dir.join("scene.json");
    let in_runner = serde_json::json!({
        "argv": [
            "--fixture",
            fixture,
            "--test-hooks",
            "1",
            "--out",
            scene_path,
            "--time",
            "0"
        ],
        "env": runner_variables,
    });
    let mut other_variables = runner_variables.clone();
    other_variables["RUR_STATE_DIR"] = install.dir.join("other-state").to_str().unwrap().into();
    let failing_job = |variables: &serde_json::Value| {
        serde_json::json!({
            "argv": ["--actions", install.dir.join("missing-actions.json")],
            "env": variables,
        })
    };
    let jobs = [
        in_runner,
        failing_job(&runner_variables),
        failing_job(&other_variables),
    ];
    let mut input = runner.stdin.take().unwrap();
    for job in &jobs {
        writeln!(input, "{job}").unwrap();
    }
    drop(input);
    let output = runner.wait_with_output().unwrap();
    let answers = batch_answers(&output);
    assert_eq!(answers.len(), 3, "{output:?}");
    assert_eq!(answers[0]["code"], 0, "{answers:?}");
    let scene: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&scene_path).unwrap()).unwrap();
    assert_eq!(
        scene["status"], "ok",
        "job did not run in the runner: {scene}"
    );
    assert_eq!(answers[1]["code"], 1, "{answers:?}");
    assert!(
        answers[1]["error"]
            .as_str()
            .unwrap()
            .contains("missing-actions.json")
    );
    assert_eq!(answers[1], answers[2]);
}

/// A child job that never yields (here a story run for a billion virtual seconds) must still stop
/// when its runner is killed, or it would run on with no one to answer.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_batch_child_stops_when_the_runner_is_killed() {
    use std::{
        io::{Read, Write},
        process::Stdio,
        time::{Duration, Instant},
    };
    let install = Installation::new("batch-cancel");
    // A job with its own state directory runs in a child, and the directory appearing shows that
    // the child started
    let state = install.dir.join("job-state");
    let mut runner = Service(
        install
            .command()
            .arg("engine-batch")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn_locked()
            .unwrap(),
    );
    let mut input = runner.0.stdin.take().unwrap();
    let job = serde_json::json!({
        "argv": ["--story", install.project.join("story.luau"), "--time", "1000000000", "--out", install.dir.join("scene.json")],
        "env": {"RUR_STATE_DIR": state, "RUR_ROBLOX_CONTENT": install.cache.join("content")},
    });
    writeln!(input, "{job}").unwrap();
    let mut stderr = runner.0.stderr.take().unwrap();
    let (done, exited) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        let _ = done.send(bytes);
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while !state.is_dir() {
        assert!(Instant::now() < deadline, "batch child did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
    runner.0.kill().unwrap();
    runner.0.wait().unwrap();
    // The child inherits stderr. EOF arrives only once it too has exited, including if it was
    // busy running virtual time when the runner disappeared.
    exited
        .recv_timeout(Duration::from_secs(5))
        .expect("job survived its runner");
    assert!(!install.dir.join("scene.json").exists());
}

/// Without a user cache folder or a state directory, a child cannot start. The runner still
/// answers the job with that error, which would otherwise be lost on the child's stderr.
#[test]
#[ignore = "requires RUR_TEST_STANDALONE_BINARY from a marked release build"]
fn installed_batch_preserves_child_startup_errors() {
    use std::{io::Write, process::Stdio};
    let install = Installation::new("batch-startup-error");
    let mut command = install.command();
    for name in ["HOME", "LOCALAPPDATA", "XDG_CACHE_HOME"] {
        command.env_remove(name);
    }
    let mut runner = command
        .arg("engine-batch")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_locked()
        .unwrap();
    let mut input = runner.stdin.take().unwrap();
    writeln!(input, "{}", serde_json::json!({"argv": [], "env": {}})).unwrap();
    drop(input);
    let output = runner.wait_with_output().unwrap();
    let answers = batch_answers(&output);
    assert_eq!(answers.len(), 1, "{output:?}");
    assert_eq!(answers[0]["code"], 1);
    assert!(
        answers[0]["error"]
            .as_str()
            .unwrap()
            .contains("Set RUR_STATE_DIR"),
        "{answers:?}"
    );
}
