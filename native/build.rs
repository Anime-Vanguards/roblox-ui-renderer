//! Compiles FreeType 2.14.2 (the release Studio's glyphs match; notes/text.md "FreeType") and
//! src/glyphs/rur_glyph.c, the C port of tools/glyphs/rasterize.py, into the crate (src/glyphs.rs).
//! The source is the pinned official archive tools/native-host/source.mjs extracts to
//! `.cache/native-app/source/freetype-2.14.2` (next to the Lune and Luau sources Cargo.toml uses),
//! or RUR_FREETYPE_SRC.
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/glyphs/rur_glyph.c");
    println!("cargo:rerun-if-env-changed=RUR_FREETYPE_SRC");
    // src/paths.rs reads RUR_RELEASE_BUILD at compile time; rebuild when it changes
    println!("cargo:rerun-if-env-changed=RUR_RELEASE_BUILD");
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let source = std::env::var_os("RUR_FREETYPE_SRC")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../.cache/native-app/source/freetype-2.14.2"));
    if !source.join("include/ft2build.h").is_file() {
        panic!(
            "FreeType source not found at {} (run `node tools/native-app/build.mjs --prepare` or set RUR_FREETYPE_SRC)",
            source.display()
        );
    }
    println!("cargo:rerun-if-changed={}", source.join("include/freetype/freetype.h").display());
    // The CMake build's source list (CMakeLists.txt BASE_SRCS) with the default module set of
    // include/freetype/config/ftmodule.h and the default ftoption.h (no HarfBuzz, PNG, Brotli or bzip2,
    // like tools/glyphs/build_freetype.mjs's build)
    let files = [
        "src/autofit/autofit.c",
        "src/base/ftbase.c",
        "src/base/ftbbox.c",
        "src/base/ftbdf.c",
        "src/base/ftbitmap.c",
        "src/base/ftcid.c",
        "src/base/ftfstype.c",
        "src/base/ftgasp.c",
        "src/base/ftglyph.c",
        "src/base/ftgxval.c",
        "src/base/ftinit.c",
        "src/base/ftmm.c",
        "src/base/ftotval.c",
        "src/base/ftpatent.c",
        "src/base/ftpfr.c",
        "src/base/ftstroke.c",
        "src/base/ftsynth.c",
        "src/base/ftsystem.c",
        "src/base/fttype1.c",
        "src/base/ftwinfnt.c",
        "src/base/ftdebug.c",
        "src/bdf/bdf.c",
        "src/bzip2/ftbzip2.c",
        "src/cache/ftcache.c",
        "src/cff/cff.c",
        "src/cid/type1cid.c",
        "src/gzip/ftgzip.c",
        "src/lzw/ftlzw.c",
        "src/pcf/pcf.c",
        "src/pfr/pfr.c",
        "src/psaux/psaux.c",
        "src/pshinter/pshinter.c",
        "src/psnames/psnames.c",
        "src/raster/raster.c",
        "src/sdf/sdf.c",
        "src/sfnt/sfnt.c",
        "src/smooth/smooth.c",
        "src/svg/svg.c",
        "src/truetype/truetype.c",
        "src/type1/type1.c",
        "src/type42/type42.c",
        "src/winfonts/winfnt.c",
    ];
    let mut build = cc::Build::new();
    build
        .include(source.join("include"))
        .define("FT2_BUILD_LIBRARY", None)
        .warnings(false)
        .opt_level(3);
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        build.define("_CRT_SECURE_NO_WARNINGS", None);
    }
    for file in files {
        build.file(source.join(file));
    }
    build.file(manifest.join("src/glyphs/rur_glyph.c"));
    let emscripten = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("emscripten");
    if emscripten {
        println!("cargo:rerun-if-changed=web/egl_shim.c");
        build.file(manifest.join("web/egl_shim.c"));
    }
    build.compile("rurfreetype");

    // The browser module (native/web/host.js loads it): a modular factory, growable memory, a large
    // stack for deep Luau / layout recursion, WebGL2 for wgpu's GLES backend, catchable C++ exceptions
    // (Luau errors unwind as C++ exceptions through Rust frames), the in-memory file system the page
    // fills with its bundle. The exceptions are WebAssembly's own: Rust 1.93+ unwinds with them on
    // Emscripten (1.98 removed the switch back to JavaScript exceptions; 1.99 passes -fwasm-exceptions
    // itself), and Emscripten links only one model, so every C/C++ object (Luau, zstd, FreeType and the
    // EGL shim) is compiled with -fwasm-exceptions too (tools/native-app/build.mjs WEB_C_FLAGS)
    if emscripten {
        for arg in [
            "-sMODULARIZE=1",
            "-sEXPORT_NAME=createRurWeb",
            "-sENVIRONMENT=web,worker",
            "-sALLOW_MEMORY_GROWTH=1",
            "-sINITIAL_MEMORY=128MB",
            "-sMAXIMUM_MEMORY=4GB",
            "-sSTACK_SIZE=16MB",
            "-sFORCE_FILESYSTEM=1",
            "-sMIN_WEBGL_VERSION=2",
            "-sMAX_WEBGL_VERSION=2",
            "-sFULL_ES3=1",
            "-fwasm-exceptions",
            "-sEXPORTED_FUNCTIONS=_main,_rur_web_init,_rur_web_frame,_rur_web_scene,_rur_web_drop,_rur_web_gpu,_malloc,_free",
            "-sEXPORTED_RUNTIME_METHODS=FS,cwrap,UTF8ToString",
        ] {
            println!("cargo:rustc-link-arg-bin=rur-web={arg}");
        }
    }
}
