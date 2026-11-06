# Third-party notices

roblox-ui-renderer is licensed under the GNU General Public License, version 2 or (at your option) any later version
(see [LICENSE](LICENSE)), GPL-2.0-or-later for short. The programs it builds
contain third-party code under the licenses below. This repository ships no fonts, textures or other
Roblox content: fonts and `rbxasset://` textures are read at run time from the user's own Roblox Studio
install, and `rbxassetid://` assets are downloaded into a local cache.

## Compiled into `rur-native` and `rur-web`

### FreeType 2.14.2

The glyph rasterizer compiles FreeType, which `tools/native-host/source.mjs` downloads at build time
(the official `VER-2-14-2` archive, checked by its SHA-256) into `<state dir>/native-app/source/`. We use
it under the GNU General Public License, version 2 or later, one of the two licenses FreeType offers (the
other, the FreeType License, isn't compatible with version 2 of the GPL); its text is `docs/GPLv2.TXT` in that
source tree (<https://gitlab.freedesktop.org/freetype/freetype/-/blob/master/docs/GPLv2.TXT>).

> Portions of this software are copyright © 2025 The FreeType Project (https://freetype.org). All rights reserved.

The FreeType build also compiles its BDF and PCF font drivers (MIT-style licenses, in `src/bdf/README`
and `src/pcf/README` of the FreeType tree) and its bundled copy of zlib (zlib license, `src/gzip/`).

### The engine: roblox-engine-emulator and what it vendors

The engine core is [roblox-engine-emulator](https://github.com/Anime-Vanguards/roblox-engine-emulator)
(GPL-2.0-or-later), a Cargo git dependency. It brings in:

| Component | License | License text |
|---|---|---|
| Luau (Roblox's Lua dialect and VM), through its vendored `luau0-src` crate, patched | MIT | `vendor/luau0-src/LICENSE` in roblox-engine-emulator (covers Luau, Lua 5.1 and the luau-src crate) |
| `lune-roblox` and `lune-utils` from [Lune](https://github.com/lune-org/lune), vendored and patched | MPL-2.0 | <https://www.mozilla.org/en-US/MPL/2.0/>; the vendored sources keep their headers |

### egui's default fonts

The desktop storybook embeds the fonts of the `epaint_default_fonts` crate (egui's UI font, the Hack
monospace font, Noto Emoji and an emoji icon font) under their licenses: `(MIT OR Apache-2.0) AND
OFL-1.1 AND Ubuntu-font-1.0`. The license files are in that crate's source package.

### Rust crates

Every crate in `native/Cargo.lock` is compiled in. Their licenses are listed below as each crate's
`Cargo.toml` declares them (generated from `native/Cargo.lock` on 2026-11-04; the git packages are the
engine's, above). Each crate's license text is in its source package on crates.io. Where a crate offers a
choice of licenses we use it under MIT when offered, else under the first option compatible with the GPL.

Licenses other than MIT and Apache-2.0 among them: Unicode-3.0 (ICU4X data crates), Zlib, BSD-2-Clause,
BSD-3-Clause (including zstd through `zstd-sys`), BSL-1.0, ISC, CC0-1.0, BSD-2-Clause-Patent and MPL-2.0
(`roblox_install`, `option-ext`; used unmodified).

## Distributing a binary

rur's own code is GPL-2.0-or-later. A built rur also links Rust crates that are licensed under Apache-2.0
alone (among them winit, glutin, ab_glyph, and wgpu's codespan-reporting and spirv). The Free Software
Foundation considers Apache-2.0 compatible with version 3 of the GPL but not version 2, so distribute a
binary under GPL-3.0-or-later, which the "or later" allows. Include its text
(https://www.gnu.org/licenses/gpl-3.0.txt), the corresponding source of the whole work and the crates'
license notices. The source itself can be used under version 2 or later.

## npm packages (installed by `npm install`, not bundled)

| Package | License |
|---|---|
| opentype.js | MIT |
| pngjs | MIT |
| tiny-inflate | MIT |
| string.prototype.codepointat | MIT |

## Data in this repository

`data/studio/*.json` holds numbers measured in Roblox Studio (text bounds, glyph advances, wrap and
truncation widths) for the tests. `data/studio/dotted-glyphs.json` also holds small alpha crops of two
glyphs as Studio drew them in Montserrat and Fredoka One, both under the SIL Open Font License 1.1. No
font files are included.

## Crate list

| Crate | Version | License |
|---|---|---|
| ab_glyph | 0.2.32 | Apache-2.0 |
| ab_glyph_rasterizer | 0.1.10 | Apache-2.0 |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| ahash | 0.8.12 | MIT OR Apache-2.0 |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 |
| android-activity | 0.6.1 | MIT OR Apache-2.0 |
| android-properties | 0.2.2 | MIT |
| anstream | 0.6.21 | MIT OR Apache-2.0 |
| anstyle | 1.0.14 | MIT OR Apache-2.0 |
| anstyle-parse | 0.2.7 | MIT OR Apache-2.0 |
| anstyle-query | 1.1.5 | MIT OR Apache-2.0 |
| anstyle-wincon | 3.0.11 | MIT OR Apache-2.0 |
| anyhow | 1.0.100 | MIT OR Apache-2.0 |
| arboard | 3.6.1 | MIT OR Apache-2.0 |
| arrayref | 0.3.9 | BSD-2-Clause |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 |
| as-raw-xcb-connection | 1.0.1 | MIT OR Apache-2.0 |
| ash | 0.38.0+1.3.281 | MIT OR Apache-2.0 |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT |
| autocfg | 1.5.1 | Apache-2.0 OR MIT |
| base64 | 0.13.1 | MIT/Apache-2.0 |
| base64 | 0.22.1 | MIT OR Apache-2.0 |
| bit-set | 0.8.0 | Apache-2.0 OR MIT |
| bit-vec | 0.8.0 | Apache-2.0 OR MIT |
| bitflags | 1.3.2 | MIT/Apache-2.0 |
| bitflags | 2.13.2 | MIT OR Apache-2.0 |
| blake3 | 1.5.0 | CC0-1.0 OR Apache-2.0 |
| block | 0.1.6 | MIT |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 |
| block2 | 0.5.1 | MIT |
| bstr | 1.13.1 | MIT OR Apache-2.0 |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 |
| bytemuck | 1.24.0 | Zlib OR Apache-2.0 OR MIT |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT |
| byteorder | 1.5.0 | Unlicense OR MIT |
| byteorder-lite | 0.1.0 | Unlicense OR MIT |
| bytes | 1.12.1 | MIT |
| calloop | 0.13.0 | MIT |
| calloop | 0.14.4 | MIT |
| calloop-wayland-source | 0.3.0 | MIT |
| calloop-wayland-source | 0.4.1 | MIT |
| cc | 1.5.1 | MIT OR Apache-2.0 |
| cfg_aliases | 0.2.2 | MIT |
| cfg-if | 0.1.10 | MIT/Apache-2.0 |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 |
| cgl | 0.3.2 | MIT / Apache-2.0 |
| chrono | 0.4.45 | MIT OR Apache-2.0 |
| clap | 4.5.50 | MIT OR Apache-2.0 |
| clap_builder | 4.5.50 | MIT OR Apache-2.0 |
| clap_derive | 4.5.49 | MIT OR Apache-2.0 |
| clap_lex | 0.7.7 | MIT OR Apache-2.0 |
| clipboard-win | 5.4.1 | BSL-1.0 |
| codespan-reporting | 0.12.0 | Apache-2.0 |
| color_quant | 1.1.0 | MIT |
| colorchoice | 1.0.5 | MIT OR Apache-2.0 |
| combine | 4.6.8 | MIT |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT |
| console | 0.16.6 | MIT |
| constant_time_eq | 0.3.1 | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| core_maths | 0.1.1 | MIT |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 |
| core-foundation | 0.9.4 | MIT OR Apache-2.0 |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 |
| core-graphics | 0.23.2 | MIT OR Apache-2.0 |
| core-graphics-types | 0.1.3 | MIT OR Apache-2.0 |
| core-graphics-types | 0.2.0 | MIT OR Apache-2.0 |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 |
| crc32fast | 1.5.2 | MIT OR Apache-2.0 |
| crossbeam-deque | 0.8.8 | MIT OR Apache-2.0 |
| crossbeam-epoch | 0.9.21 | MIT OR Apache-2.0 |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 |
| crunchy | 0.2.4 | MIT |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 |
| cty | 0.2.2 | MIT OR Apache-2.0 |
| cursor-icon | 1.2.0 | MIT OR Apache-2.0 OR Zlib |
| data-url | 0.3.2 | MIT OR Apache-2.0 |
| digest | 0.10.7 | MIT OR Apache-2.0 |
| dirs | 2.0.2 | MIT OR Apache-2.0 |
| dirs | 5.0.1 | MIT OR Apache-2.0 |
| dirs-sys | 0.3.7 | MIT OR Apache-2.0 |
| dirs-sys | 0.4.1 | MIT OR Apache-2.0 |
| dispatch | 0.2.0 | MIT |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 |
| dlib | 0.5.3 | MIT |
| document-features | 0.2.12 | MIT OR Apache-2.0 |
| downcast-rs | 1.2.1 | MIT/Apache-2.0 |
| dpi | 0.1.2 | Apache-2.0 AND MIT |
| dunce | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| ecolor | 0.33.3 | MIT OR Apache-2.0 |
| eframe | 0.33.3 | MIT OR Apache-2.0 |
| egui | 0.33.3 | MIT OR Apache-2.0 |
| egui_glow | 0.33.3 | MIT OR Apache-2.0 |
| egui-wgpu | 0.33.3 | MIT OR Apache-2.0 |
| egui-winit | 0.33.3 | MIT OR Apache-2.0 |
| either | 1.18.0 | MIT OR Apache-2.0 |
| emath | 0.33.3 | MIT OR Apache-2.0 |
| encode_unicode | 1.0.0 | Apache-2.0 OR MIT |
| epaint | 0.33.3 | MIT OR Apache-2.0 |
| epaint_default_fonts | 0.33.3 | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 |
| equivalent | 1.0.2 | Apache-2.0 OR MIT |
| erased-serde | 0.4.10 | MIT OR Apache-2.0 |
| errno | 0.3.14 | MIT OR Apache-2.0 |
| error-code | 3.4.0 | BSL-1.0 |
| euclid | 0.22.14 | MIT OR Apache-2.0 |
| fax | 0.2.7 | MIT |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 |
| find-msvc-tools | 0.1.14 | MIT OR Apache-2.0 |
| flate2 | 1.1.10 | MIT OR Apache-2.0 |
| float-cmp | 0.9.0 | MIT |
| foldhash | 0.1.5 | Zlib |
| foldhash | 0.2.0 | Zlib |
| fontconfig-parser | 0.5.8 | MIT |
| fontdb | 0.23.0 | MIT |
| foreign-types | 0.5.0 | MIT/Apache-2.0 |
| foreign-types-macros | 0.2.4 | MIT/Apache-2.0 |
| foreign-types-shared | 0.3.1 | MIT/Apache-2.0 |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 |
| futures-core | 0.3.34 | MIT OR Apache-2.0 |
| futures-task | 0.3.34 | MIT OR Apache-2.0 |
| futures-util | 0.3.34 | MIT OR Apache-2.0 |
| generic-array | 0.14.7 | MIT |
| gethostname | 1.1.0 | Apache-2.0 |
| getrandom | 0.2.17 | MIT OR Apache-2.0 |
| getrandom | 0.3.4 | MIT OR Apache-2.0 |
| getrandom | 0.4.3 | MIT OR Apache-2.0 |
| gif | 0.13.3 | MIT OR Apache-2.0 |
| gl_generator | 0.14.0 | Apache-2.0 |
| glam | 0.30.10 | MIT OR Apache-2.0 |
| glow | 0.16.0 | MIT OR Apache-2.0 OR Zlib |
| glutin | 0.32.3 | Apache-2.0 |
| glutin_egl_sys | 0.7.1 | Apache-2.0 |
| glutin_glx_sys | 0.6.1 | Apache-2.0 |
| glutin_wgl_sys | 0.6.1 | Apache-2.0 |
| glutin-winit | 0.5.0 | MIT |
| gpu-alloc | 0.6.2 | MIT OR Apache-2.0 |
| gpu-alloc-types | 0.3.1 | MIT OR Apache-2.0 |
| gpu-allocator | 0.27.0 | MIT OR Apache-2.0 |
| gpu-descriptor | 0.3.2 | MIT OR Apache-2.0 |
| gpu-descriptor-types | 0.2.0 | MIT OR Apache-2.0 |
| half | 2.7.1 | MIT OR Apache-2.0 |
| hashbrown | 0.15.5 | MIT OR Apache-2.0 |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 |
| heck | 0.5.0 | MIT OR Apache-2.0 |
| hermit-abi | 0.5.3 | MIT OR Apache-2.0 |
| hexf-parse | 0.2.1 | CC0-1.0 |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 |
| iana-time-zone-haiku | 0.1.2 | MIT OR Apache-2.0 |
| icu_collections | 2.3.0 | Unicode-3.0 |
| icu_locale_core | 2.3.0 | Unicode-3.0 |
| icu_normalizer | 2.3.0 | Unicode-3.0 |
| icu_normalizer_data | 2.3.0 | Unicode-3.0 |
| icu_properties | 2.3.0 | Unicode-3.0 |
| icu_properties_data | 2.3.0 | Unicode-3.0 |
| icu_provider | 2.3.1 | Unicode-3.0 |
| idna | 1.1.0 | MIT OR Apache-2.0 |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT |
| image | 0.25.8 | MIT OR Apache-2.0 |
| image-webp | 0.2.4 | MIT OR Apache-2.0 |
| imagesize | 0.13.0 | MIT |
| indexmap | 2.14.2 | Apache-2.0 OR MIT |
| is_terminal_polyfill | 1.70.2 | MIT OR Apache-2.0 |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| jni | 0.22.4 | MIT OR Apache-2.0 |
| jni-macros | 0.22.4 | MIT OR Apache-2.0 |
| jni-sys | 0.3.1 | MIT OR Apache-2.0 |
| jni-sys | 0.4.1 | MIT OR Apache-2.0 |
| jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 |
| jobserver | 0.1.35 | MIT OR Apache-2.0 |
| js-sys | 0.3.106 | MIT OR Apache-2.0 |
| khronos_api | 3.1.0 | Apache-2.0 |
| khronos-egl | 6.0.0 | MIT/Apache-2.0 |
| kurbo | 0.11.3 | Apache-2.0 OR MIT |
| lazy_static | 1.5.0 | MIT OR Apache-2.0 |
| libc | 0.2.189 | MIT OR Apache-2.0 |
| libloading | 0.8.9 | ISC |
| libm | 0.2.16 | MIT |
| libmimalloc-sys | 0.1.49 | MIT |
| libredox | 0.1.25 | MIT |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| linux-raw-sys | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| litemap | 0.8.3 | Unicode-3.0 |
| litrs | 1.0.0 | MIT OR Apache-2.0 |
| lock_api | 0.4.14 | MIT OR Apache-2.0 |
| log | 0.4.34 | MIT OR Apache-2.0 |
| lz4_flex | 0.11.6 | MIT |
| malloc_buf | 0.0.6 | MIT |
| memchr | 2.8.3 | Unlicense OR MIT |
| memmap2 | 0.9.11 | MIT OR Apache-2.0 |
| memoffset | 0.9.1 | MIT |
| metal | 0.32.0 | MIT OR Apache-2.0 |
| mimalloc | 0.1.52 | MIT |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 |
| mlua | 0.11.4 | MIT |
| mlua-sys | 0.8.3 | MIT |
| moxcms | 0.7.11 | BSD-3-Clause OR Apache-2.0 |
| naga | 27.0.3 | MIT OR Apache-2.0 |
| ndk | 0.9.0 | MIT OR Apache-2.0 |
| ndk-context | 0.1.1 | MIT OR Apache-2.0 |
| ndk-sys | 0.6.0+11769913 | MIT OR Apache-2.0 |
| nohash-hasher | 0.2.0 | Apache-2.0 OR MIT |
| num_enum | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 |
| num_enum_derive | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| objc | 0.2.7 | MIT |
| objc-sys | 0.3.5 | MIT |
| objc2 | 0.5.2 | MIT |
| objc2 | 0.6.4 | MIT |
| objc2-app-kit | 0.2.2 | MIT |
| objc2-app-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-cloud-kit | 0.2.2 | MIT |
| objc2-contacts | 0.2.2 | MIT |
| objc2-core-data | 0.2.2 | MIT |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-core-graphics | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-core-image | 0.2.2 | MIT |
| objc2-core-location | 0.2.2 | MIT |
| objc2-encode | 4.1.0 | MIT |
| objc2-foundation | 0.2.2 | MIT |
| objc2-foundation | 0.3.2 | MIT |
| objc2-io-surface | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-link-presentation | 0.2.2 | MIT |
| objc2-metal | 0.2.2 | MIT |
| objc2-quartz-core | 0.2.2 | MIT |
| objc2-symbols | 0.2.2 | MIT |
| objc2-ui-kit | 0.2.2 | MIT |
| objc2-uniform-type-identifiers | 0.2.2 | MIT |
| objc2-user-notifications | 0.2.2 | MIT |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| once_cell_polyfill | 1.70.2 | MIT OR Apache-2.0 |
| option-ext | 0.2.0 | MPL-2.0 |
| orbclient | 0.3.55 | MIT |
| ordered-float | 2.10.1 | MIT |
| ordered-float | 5.5.0 | MIT |
| os_str_bytes | 7.2.1 | MIT OR Apache-2.0 |
| owned_ttf_parser | 0.25.1 | Apache-2.0 |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 |
| paste | 1.0.15 | MIT OR Apache-2.0 |
| path-clean | 1.0.1 | MIT OR Apache-2.0 |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 |
| pico-args | 0.5.0 | MIT |
| pin-project | 1.1.13 | Apache-2.0 OR MIT |
| pin-project-internal | 1.1.13 | Apache-2.0 OR MIT |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 |
| plain | 0.2.3 | MIT/Apache-2.0 |
| png | 0.17.16 | MIT OR Apache-2.0 |
| png | 0.18.1 | MIT OR Apache-2.0 |
| polling | 3.11.0 | Apache-2.0 OR MIT |
| pollster | 0.4.0 | Apache-2.0/MIT |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT |
| portable-atomic-util | 0.2.8 | Apache-2.0 OR MIT |
| potential_utf | 0.1.6 | Unicode-3.0 |
| pp-rs | 0.2.1 | BSD-3-Clause |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 |
| presser | 0.3.1 | MIT OR Apache-2.0 |
| proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| profiling | 1.0.18 | MIT OR Apache-2.0 |
| profiling-procmacros | 1.0.18 | MIT OR Apache-2.0 |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 |
| quick-error | 2.0.1 | MIT/Apache-2.0 |
| quick-xml | 0.41.0 | MIT |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| rand | 0.8.8 | MIT OR Apache-2.0 |
| rand | 0.9.5 | MIT OR Apache-2.0 |
| rand_chacha | 0.3.1 | MIT OR Apache-2.0 |
| rand_chacha | 0.9.0 | MIT OR Apache-2.0 |
| rand_core | 0.6.4 | MIT OR Apache-2.0 |
| rand_core | 0.9.5 | MIT OR Apache-2.0 |
| range-alloc | 0.1.5 | MIT OR Apache-2.0 |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib |
| rayon | 1.11.0 | MIT OR Apache-2.0 |
| rayon-core | 1.13.0 | MIT OR Apache-2.0 |
| rbx_binary | 2.0.1 | MIT |
| rbx_dom_weak | 4.2.0 | MIT |
| rbx_reflection | 6.1.0 | MIT |
| rbx_reflection_database | 2.0.2+roblox-700 | MIT |
| rbx_types | 3.1.0 | MIT |
| rbx_xml | 2.0.1 | MIT |
| redox_syscall | 0.4.1 | MIT |
| redox_syscall | 0.5.18 | MIT |
| redox_syscall | 0.9.4 | MIT |
| redox_users | 0.4.6 | MIT |
| renderdoc-sys | 1.1.0 | MIT OR Apache-2.0 |
| resvg | 0.45.1 | Apache-2.0 OR MIT |
| rgb | 0.8.53 | MIT |
| rmp | 0.8.15 | MIT |
| rmp-serde | 1.3.1 | MIT |
| roblox_install | 1.2.0 | MPL-2.0 |
| roxmltree | 0.20.0 | MIT OR Apache-2.0 |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 |
| rustc-hash | 1.1.0 | Apache-2.0/MIT |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT |
| rustix | 0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| rustversion | 1.0.23 | MIT OR Apache-2.0 |
| rustybuzz | 0.20.1 | MIT |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 |
| same-file | 1.0.6 | Unlicense/MIT |
| scoped-tls | 1.0.1 | MIT/Apache-2.0 |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 |
| semver | 1.0.28 | MIT OR Apache-2.0 |
| serde | 1.0.228 | MIT OR Apache-2.0 |
| serde_core | 1.0.228 | MIT OR Apache-2.0 |
| serde_derive | 1.0.228 | MIT OR Apache-2.0 |
| serde_json | 1.0.145 | MIT OR Apache-2.0 |
| serde-value | 0.7.0 | MIT |
| sha2 | 0.10.9 | MIT OR Apache-2.0 |
| shlex | 2.0.1 | MIT OR Apache-2.0 |
| simd_cesu8 | 1.2.0 | Apache-2.0 OR MIT |
| simd-adler32 | 0.3.10 | MIT |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 |
| simplecss | 0.2.2 | Apache-2.0 OR MIT |
| siphasher | 1.0.4 | MIT OR Apache-2.0 |
| slab | 0.4.12 | MIT |
| slotmap | 1.1.1 | Zlib |
| smallvec | 1.16.2 | MIT OR Apache-2.0 |
| smithay-client-toolkit | 0.19.2 | MIT |
| smithay-client-toolkit | 0.20.0 | MIT |
| smithay-clipboard | 0.7.3 | MIT |
| smol_str | 0.2.2 | MIT OR Apache-2.0 |
| spirv | 0.3.0+sdk-1.3.268.0 | Apache-2.0 |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 |
| strict-num | 0.1.1 | MIT |
| strsim | 0.11.1 | MIT |
| svgtypes | 0.15.3 | Apache-2.0 OR MIT |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| syn | 3.0.6 | MIT OR Apache-2.0 |
| synstructure | 0.14.0 | MIT |
| termcolor | 1.4.1 | Unlicense OR MIT |
| thiserror | 1.0.69 | MIT OR Apache-2.0 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 |
| tiff | 0.10.3 | MIT |
| tiny-skia | 0.11.4 | BSD-3-Clause |
| tiny-skia-path | 0.11.4 | BSD-3-Clause |
| tinystr | 0.8.4 | Unicode-3.0 |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT |
| tokio | 1.48.0 | MIT |
| toml_datetime | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_edit | 0.25.15+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_parser | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 |
| tracing | 0.1.44 | MIT |
| tracing-core | 0.1.36 | MIT |
| ttf-parser | 0.25.1 | MIT OR Apache-2.0 |
| twox-hash | 2.1.4 | MIT |
| type-map | 0.5.1 | MIT/Apache-2.0 |
| typeid | 1.0.3 | MIT OR Apache-2.0 |
| typenum | 1.20.1 | MIT OR Apache-2.0 |
| unicode-bidi | 0.3.18 | MIT OR Apache-2.0 |
| unicode-bidi-mirroring | 0.4.0 | MIT/Apache-2.0 |
| unicode-ccc | 0.4.0 | MIT/Apache-2.0 |
| unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| unicode-properties | 0.1.4 | MIT/Apache-2.0 |
| unicode-script | 0.5.8 | MIT OR Apache-2.0 |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 |
| unicode-vo | 0.1.0 | MIT/Apache-2.0 |
| unicode-width | 0.2.2 | MIT OR Apache-2.0 |
| unicode-xid | 0.2.6 | MIT OR Apache-2.0 |
| url | 2.5.8 | MIT OR Apache-2.0 |
| ustr | 1.1.0 | BSD-2-Clause-Patent |
| usvg | 0.45.1 | Apache-2.0 OR MIT |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT |
| utf8parse | 0.2.2 | Apache-2.0 OR MIT |
| version_check | 0.9.5 | MIT/Apache-2.0 |
| walkdir | 2.5.0 | Unlicense/MIT |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wasip2 | 1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wasm-bindgen | 0.2.129 | MIT OR Apache-2.0 |
| wasm-bindgen-futures | 0.4.79 | MIT OR Apache-2.0 |
| wasm-bindgen-macro | 0.2.129 | MIT OR Apache-2.0 |
| wasm-bindgen-macro-support | 0.2.129 | MIT OR Apache-2.0 |
| wasm-bindgen-shared | 0.2.129 | MIT OR Apache-2.0 |
| wayland-backend | 0.3.17 | MIT |
| wayland-client | 0.31.15 | MIT |
| wayland-csd-frame | 0.3.0 | MIT |
| wayland-cursor | 0.31.14 | MIT |
| wayland-protocols | 0.32.13 | MIT |
| wayland-protocols-experimental | 20250721.0.1 | MIT |
| wayland-protocols-misc | 0.3.12 | MIT |
| wayland-protocols-plasma | 0.3.12 | MIT |
| wayland-protocols-wlr | 0.3.12 | MIT |
| wayland-scanner | 0.31.11 | MIT |
| wayland-sys | 0.31.11 | MIT |
| web-sys | 0.3.106 | MIT OR Apache-2.0 |
| web-time | 1.1.0 | MIT OR Apache-2.0 |
| webbrowser | 1.2.4 | MIT OR Apache-2.0 |
| weezl | 0.1.12 | MIT OR Apache-2.0 |
| wgpu | 27.0.1 | MIT OR Apache-2.0 |
| wgpu-core | 27.0.3 | MIT OR Apache-2.0 |
| wgpu-core-deps-apple | 27.0.0 | MIT OR Apache-2.0 |
| wgpu-core-deps-emscripten | 27.0.0 | MIT OR Apache-2.0 |
| wgpu-core-deps-windows-linux-android | 27.0.0 | MIT OR Apache-2.0 |
| wgpu-hal | 27.0.4 | MIT OR Apache-2.0 |
| wgpu-types | 27.0.1 | MIT OR Apache-2.0 |
| winapi | 0.3.9 | MIT/Apache-2.0 |
| winapi-i686-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 |
| winapi-util | 0.1.11 | Unlicense OR MIT |
| winapi-x86_64-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 |
| windows | 0.58.0 | MIT OR Apache-2.0 |
| windows_aarch64_gnullvm | 0.48.5 | MIT OR Apache-2.0 |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_aarch64_gnullvm | 0.53.1 | MIT OR Apache-2.0 |
| windows_aarch64_msvc | 0.48.5 | MIT OR Apache-2.0 |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 |
| windows_aarch64_msvc | 0.53.1 | MIT OR Apache-2.0 |
| windows_i686_gnu | 0.48.5 | MIT OR Apache-2.0 |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_gnu | 0.53.1 | MIT OR Apache-2.0 |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_gnullvm | 0.53.1 | MIT OR Apache-2.0 |
| windows_i686_msvc | 0.48.5 | MIT OR Apache-2.0 |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_msvc | 0.53.1 | MIT OR Apache-2.0 |
| windows_x86_64_gnu | 0.48.5 | MIT OR Apache-2.0 |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_gnu | 0.53.1 | MIT OR Apache-2.0 |
| windows_x86_64_gnullvm | 0.48.5 | MIT OR Apache-2.0 |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_gnullvm | 0.53.1 | MIT OR Apache-2.0 |
| windows_x86_64_msvc | 0.48.5 | MIT OR Apache-2.0 |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_msvc | 0.53.1 | MIT OR Apache-2.0 |
| windows-core | 0.58.0 | MIT OR Apache-2.0 |
| windows-core | 0.62.2 | MIT OR Apache-2.0 |
| windows-implement | 0.58.0 | MIT OR Apache-2.0 |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 |
| windows-interface | 0.58.0 | MIT OR Apache-2.0 |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| windows-result | 0.2.0 | MIT OR Apache-2.0 |
| windows-result | 0.4.1 | MIT OR Apache-2.0 |
| windows-strings | 0.1.0 | MIT OR Apache-2.0 |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 |
| windows-sys | 0.48.0 | MIT OR Apache-2.0 |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 |
| windows-sys | 0.59.0 | MIT OR Apache-2.0 |
| windows-sys | 0.60.2 | MIT OR Apache-2.0 |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 |
| windows-targets | 0.48.5 | MIT OR Apache-2.0 |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 |
| windows-targets | 0.53.5 | MIT OR Apache-2.0 |
| winit | 0.30.13 | Apache-2.0 |
| winnow | 1.0.4 | MIT |
| winreg | 0.6.2 | MIT |
| wit-bindgen | 0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| writeable | 0.6.4 | Unicode-3.0 |
| x11-dl | 2.21.0 | MIT |
| x11rb | 0.13.2 | MIT OR Apache-2.0 |
| x11rb-protocol | 0.13.2 | MIT OR Apache-2.0 |
| xcursor | 0.3.11 | MIT |
| xkbcommon-dl | 0.4.2 | MIT |
| xkeysym | 0.2.1 | MIT OR Apache-2.0 OR Zlib |
| xml-rs | 0.8.29 | MIT |
| xmlwriter | 0.1.0 | MIT |
| yoke | 0.8.3 | Unicode-3.0 |
| yoke-derive | 0.8.3 | Unicode-3.0 |
| zerocopy | 0.8.59 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerocopy-derive | 0.8.59 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerofrom | 0.1.8 | Unicode-3.0 |
| zerofrom-derive | 0.1.8 | Unicode-3.0 |
| zerotrie | 0.2.5 | Unicode-3.0 |
| zerovec | 0.11.8 | Unicode-3.0 |
| zerovec-derive | 0.11.6 | Unicode-3.0 |
| zlib-rs | 0.6.8 | Zlib |
| zstd | 0.13.3 | MIT |
| zstd-safe | 7.3.0 | BSD-3-Clause |
| zstd-sys | 2.1.0+zstd.1.5.7 | BSD-3-Clause |
| zune-core | 0.4.12 | MIT OR Apache-2.0 OR Zlib |
| zune-jpeg | 0.4.21 | MIT OR Apache-2.0 OR Zlib |
