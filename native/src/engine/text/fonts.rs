//! Port of runtime/text/fonts.luau: Font (family, weight, style) -> face file -> parsed OpenType font,
//! using the local Studio install for rbxasset:// and the renderer's asset cache for rbxassetid://
//! (cloud) faces, plus the measured fallback order and Enum.Font faces (the emulator's data/font-enums.json, compiled in).
//!
//! Path strings are built exactly like the Luau (`{ContentRoot}/fonts/...`, with the content root as
//! Studio reports it, e.g. `.../content/` giving `.../content//fonts/...`), because face paths are what
//! the scene export writes into its font registry.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

use serde_json::Value;

use crate::resolve::AssetRef;

use super::hinted::Hinted;
use super::opentype::Font;

/// Index of a face in the registry. Faces are distinct objects even when they share a file: each
/// (family, weight, style) request resolves to its own face and the fallbacks are faces of their own
/// (the Luau code compares face tables by identity).
pub type FaceId = u32;

#[derive(Debug)]
pub struct Face {
    pub id: FaceId,
    pub path: Arc<str>,
    pub font: Arc<Font>,
    pub weight: f64,
    pub style: String,
    pub family: String,
    pub substituted: bool,
}

/// Where fonts come from: the repo root, the cache directory (`assets/` cloud fonts, `hinted/`
/// metrics) and the Studio content root. (The Enum.Font faces, data/font-enums.json, are compiled into
/// the engine: roblox_engine_emulator::data::FONT_ENUMS.)
#[derive(Clone, Debug)]
pub struct FontPaths {
    pub root: String,
    pub cache: String,
    /// None: discover like Fonts.ContentRoot (RUR_ROBLOX_CONTENT, the Studio install, LOCALAPPDATA).
    pub content_root: Option<String>,
    /// A frozen font set (RUR_FONT_ROOT; tools/engine/golden.mjs --freeze-fonts) read instead of the Studio
    /// install's fonts and the cache's cloud fonts: `studio/` laid out like the install (`content/fonts`,
    /// `ExtraContent/...`) and `assets/` like the cache. Studio updates and a cleared cache then leave
    /// golden output alone. The content root stays the install's (image variants).
    pub fonts: Option<String>,
}

impl FontPaths {
    /// Paths for a checkout at `root` (forward slashes, as runtime/paths.luau derives them).
    pub fn for_repo(root: &str) -> FontPaths {
        let root = normalize(root);
        let root = root.trim_end_matches('/').to_string();
        FontPaths {
            // The state dir (RUR_STATE_DIR / RUR_CACHE_DIR, default <root>/.cache; crate::state_dir)
            cache: normalize(&crate::state_dir(std::path::Path::new(&root)).to_string_lossy()),
            root,
            content_root: None,
            fonts: std::env::var("RUR_FONT_ROOT").ok().filter(|dir| !dir.is_empty()).map(|dir| normalize(&dir).trim_end_matches('/').to_string()),
        }
    }
}

fn normalize(path: &str) -> String {
    path.replace('\\', "/")
}

fn is_dir(path: &str) -> bool {
    std::fs::metadata(path).map(|meta| meta.is_dir()).unwrap_or(false)
}

fn is_file(path: &str) -> bool {
    std::fs::metadata(path).map(|meta| meta.is_file()).unwrap_or(false)
}

/// A listed entry's kind from the listing itself (no query per entry); a symbolic link is followed,
/// as is_dir / is_file do
fn entry_kind(entry: &std::fs::DirEntry, path: &str) -> (bool, bool) {
    match entry.file_type() {
        Ok(kind) if !kind.is_symlink() => (kind.is_dir(), kind.is_file()),
        _ => (is_dir(path), is_file(path)),
    }
}

/// `Fonts.ContentRoot`: RUR_ROBLOX_CONTENT, then the Studio install (roblox_install, as Lune's
/// `roblox.studioContentPath`), then the newest `%LOCALAPPDATA%/Roblox/Versions/*` with Studio fonts.
pub fn discover_content_root() -> Option<String> {
    if let Ok(path) = std::env::var("RUR_ROBLOX_CONTENT") {
        if is_dir(&path) {
            return Some(normalize(&path));
        }
    }
    static DISCOVERED: OnceLock<Option<String>> = OnceLock::new();
    DISCOVERED
        .get_or_init(|| roblox_engine_emulator::alloc::long_lived(discover_installed_content_root))
        .clone()
}

fn discover_installed_content_root() -> Option<String> {
    if let Ok(studio) = roblox_install::RobloxStudio::locate() {
        let path = studio.content_path().display().to_string();
        if is_dir(&path) {
            return Some(normalize(&path));
        }
    }
    let local = std::env::var("LOCALAPPDATA").ok()?;
    let versions = format!("{}/Roblox/Versions", normalize(&local));
    let mut candidates: Vec<(String, u128)> = Vec::new();
    for entry in std::fs::read_dir(&versions).ok()?.flatten() {
        let base = format!("{versions}/{}", entry.file_name().to_string_lossy());
        if is_file(&format!("{base}/RobloxStudioBeta.exe")) && is_dir(&format!("{base}/content/fonts")) {
            let modified = std::fs::metadata(&base)
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis())
                .unwrap_or(0);
            candidates.push((format!("{base}/content"), modified));
        }
    }
    candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    candidates.into_iter().next().map(|candidate| candidate.0)
}

/// `string.match(ContentRoot, "^(.*)/content/?$")`.
fn version_root(content: &str) -> Option<String> {
    if let Some(stripped) = content.strip_suffix("/content/") {
        return Some(stripped.to_string());
    }
    content.strip_suffix("/content").map(|stripped| stripped.to_string())
}

fn is_font_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".ttf") || lower.ends_with(".otf") || lower.ends_with(".json") || lower.ends_with(".ttc")
}

const FAMILY_REDIRECTS: [(&str, &str); 2] = [
    ("rbxasset://fonts/families/GothamSSm.json", "rbxasset://fonts/families/Montserrat.json"),
    ("rbxasset://fonts/families/Arial.json", "rbxasset://fonts/families/Arimo.json"),
];

/// Measured (layout-probe-fallback 320/320): SourceSans Pro Regular first whatever the requested
/// weight or style, then Arimo before the emoji fonts (see fonts.luau).
const FALLBACK_FILES: [&str; 7] = [
    "rbxasset://fonts/SourceSansPro-Regular.ttf",
    "rbxasset://fonts/Arimo-Regular.ttf",
    "rbxasset://fonts/RobloxEmoji.ttf",
    "rbxasset://fonts/TwemojiMozilla.ttf",
    "rbxasset://fonts/NotoSansThaiUI-Regular.ttf",
    "rbxasset://fonts/NotoNaskhArabicUI-Regular.ttf",
    "rbxasset://fonts/NotoSansDevanagariUI-Regular.ttf",
];

const CLOUD_FALLBACK_FILES: [&str; 1] = ["rbxassetid://123924484074402"];

fn weight_name(name: &str) -> Option<f64> {
    Some(match name {
        "Thin" => 100.0,
        "ExtraLight" => 200.0,
        "Light" => 300.0,
        "Regular" => 400.0,
        "Medium" => 500.0,
        "SemiBold" => 600.0,
        "Bold" => 700.0,
        "ExtraBold" => 800.0,
        "Heavy" => 900.0,
        _ => return None,
    })
}

struct FamilyFace {
    name: String,
    weight: f64,
    style: String,
    asset_id: String,
}

struct Family {
    faces: Vec<FamilyFace>,
}

struct CachedFontDir {
    stamps: Vec<(PathBuf, roblox_engine_emulator::files::Stamp)>,
    files: Arc<HashSet<String>>,
}

fn enum_faces() -> Arc<HashMap<String, Vec<String>>> {
    static FACES: OnceLock<Arc<HashMap<String, Vec<String>>>> = OnceLock::new();
    FACES
        .get_or_init(|| {
            roblox_engine_emulator::alloc::long_lived(|| {
                let mut map = HashMap::new();
                if let Ok(Value::Object(data)) = serde_json::from_slice::<Value>(roblox_engine_emulator::data::FONT_ENUMS) {
                    if let Some(Value::Object(from_enum)) = data.get("fromEnum") {
                        for (name, face) in from_enum {
                            if let Some(parts) = face.as_array() {
                                let parts = parts.iter().map(|part| part.as_str().unwrap_or("").to_string()).collect();
                                map.insert(name.clone(), parts);
                            }
                        }
                    }
                }
                Arc::new(map)
            })
        })
        .clone()
}

fn font_dirs_cache() -> &'static RwLock<HashMap<(String, bool, bool), CachedFontDir>> {
    static CACHE: OnceLock<RwLock<HashMap<(String, bool, bool), CachedFontDir>>> = OnceLock::new();
    CACHE.get_or_init(|| roblox_engine_emulator::alloc::long_lived(|| RwLock::new(HashMap::new())))
}

fn families_cache() -> &'static RwLock<HashMap<String, (roblox_engine_emulator::files::Stamp, Option<Arc<Family>>)>> {
    static CACHE: OnceLock<RwLock<HashMap<String, (roblox_engine_emulator::files::Stamp, Option<Arc<Family>>)>>> = OnceLock::new();
    CACHE.get_or_init(|| roblox_engine_emulator::alloc::long_lived(|| RwLock::new(HashMap::new())))
}

fn parsed_fonts_cache() -> &'static RwLock<HashMap<String, (roblox_engine_emulator::files::Stamp, Result<Arc<Font>, String>)>> {
    static CACHE: OnceLock<RwLock<HashMap<String, (roblox_engine_emulator::files::Stamp, Result<Arc<Font>, String>)>>> = OnceLock::new();
    CACHE.get_or_init(|| roblox_engine_emulator::alloc::long_lived(|| RwLock::new(HashMap::new())))
}

fn scan_font_dir(
    directory: &str,
    recursive: bool,
    skip_fonts_json: bool,
    stamps: &mut Vec<(PathBuf, roblox_engine_emulator::files::Stamp)>,
    files: &mut HashSet<String>,
) {
    let dir_path = Path::new(directory);
    if let Some(stamp) = roblox_engine_emulator::files::stamp(dir_path) {
        stamps.push((dir_path.to_path_buf(), stamp));
    }
    let Ok(entries) = std::fs::read_dir(dir_path) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{directory}/{name}");
        if recursive {
            if entry_kind(&entry, &path).0 {
                scan_font_dir(&path, true, skip_fonts_json, stamps, files);
            } else if (!skip_fonts_json || name != "fonts.json") && is_font_file(&name) {
                files.insert(path);
            }
        } else if (!skip_fonts_json || name != "fonts.json") && is_font_file(&name) && entry_kind(&entry, &path).1 {
            files.insert(path);
        }
    }
}

fn preload_font_dir(directory: &str, recursive: bool, skip_fonts_json: bool) -> Arc<HashSet<String>> {
    let keep = roblox_engine_emulator::lua::modules::keeping_between_runs();
    if keep {
        let cache = font_dirs_cache().read().unwrap();
        if let Some(entry) = cache.get(&(directory.to_string(), recursive, skip_fonts_json)) {
            if entry.stamps.iter().all(|(path, stamp)| roblox_engine_emulator::files::stamp(path) == Some(*stamp)) {
                return entry.files.clone();
            }
        }
    }
    let mut stamps = Vec::new();
    let mut files = HashSet::new();
    if keep {
        roblox_engine_emulator::alloc::long_lived(|| {
            scan_font_dir(directory, recursive, skip_fonts_json, &mut stamps, &mut files);
        });
        let files = roblox_engine_emulator::alloc::long_lived(|| Arc::new(files));
        if !stamps.is_empty() {
            roblox_engine_emulator::alloc::long_lived(|| {
                font_dirs_cache().write().unwrap().insert(
                    (directory.to_string(), recursive, skip_fonts_json),
                    CachedFontDir { stamps, files: files.clone() },
                );
            });
        }
        files
    } else {
        scan_font_dir(directory, recursive, skip_fonts_json, &mut stamps, &mut files);
        Arc::new(files)
    }
}

fn parse_family_file(path: &str) -> Option<Arc<Family>> {
    let bytes = std::fs::read(path).ok()?;
    let Value::Object(data) = serde_json::from_slice::<Value>(&bytes).ok()? else {
        return None;
    };
    let faces = data.get("faces").filter(|faces| !faces.is_null() && faces.as_bool() != Some(false))?;
    let list: Vec<&Value> = match faces {
        Value::Array(items) => items.iter().collect(),
        Value::Object(map) => map.values().collect(),
        _ => Vec::new(),
    };
    let faces = list
        .into_iter()
        .map(|face| FamilyFace {
            name: face.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
            weight: face.get("weight").and_then(Value::as_f64).unwrap_or(f64::NAN),
            style: face.get("style").and_then(Value::as_str).unwrap_or("").to_string(),
            asset_id: face.get("assetId").and_then(Value::as_str).unwrap_or("").to_string(),
        })
        .collect();
    Some(Arc::new(Family { faces }))
}

fn load_family_file(path: &str) -> Option<Arc<Family>> {
    if !roblox_engine_emulator::lua::modules::keeping_between_runs() {
        return parse_family_file(path);
    }
    let Some(stamp) = roblox_engine_emulator::files::stamp(Path::new(path)) else {
        return parse_family_file(path);
    };
    {
        let cache = families_cache().read().unwrap();
        if let Some((cached_stamp, family)) = cache.get(path) {
            if *cached_stamp == stamp {
                return family.clone();
            }
        }
    }
    let family = roblox_engine_emulator::alloc::long_lived(|| parse_family_file(path));
    roblox_engine_emulator::alloc::long_lived(|| {
        families_cache().write().unwrap().insert(path.to_string(), (stamp, family.clone()));
    });
    family
}

fn load_parsed_font(path: &str) -> Result<Arc<Font>, String> {
    let read_and_parse = || match std::fs::read(path) {
        Ok(bytes) => Font::parse(bytes, path).map(Arc::new),
        Err(error) => Err(format!("failed to parse font {path}: {error}")),
    };
    if !roblox_engine_emulator::lua::modules::keeping_between_runs() {
        return read_and_parse();
    }
    let Some(stamp) = roblox_engine_emulator::files::stamp(Path::new(path)) else {
        return read_and_parse();
    };
    {
        let cache = parsed_fonts_cache().read().unwrap();
        if let Some((cached_stamp, result)) = cache.get(path) {
            if *cached_stamp == stamp {
                return result.clone();
            }
        }
    }
    let result = roblox_engine_emulator::alloc::long_lived(read_and_parse);
    roblox_engine_emulator::alloc::long_lived(|| {
        parsed_fonts_cache().write().unwrap().insert(path.to_string(), (stamp, result.clone()));
    });
    result
}

/// CSS-style closeness of a candidate weight to the requested one.
fn weight_distance(requested: f64, candidate: f64) -> f64 {
    if candidate == requested {
        return 0.0;
    }
    if (400.0..=500.0).contains(&requested) {
        if candidate > requested && candidate <= 500.0 {
            return candidate - requested;
        } else if candidate < requested {
            return 1000.0 + (requested - candidate);
        }
        return 2000.0 + (candidate - requested);
    } else if requested < 400.0 {
        if candidate < requested {
            return requested - candidate;
        }
        return 1000.0 + (candidate - requested);
    }
    if candidate > requested {
        return candidate - requested;
    }
    1000.0 + (requested - candidate)
}

pub struct FontRegistry {
    paths: FontPaths,
    content_root: Option<String>,
    /// Where font files are read: the Studio install's content root and version folder, or the frozen
    /// set's `studio/content` and `studio`; `assets` the cloud fonts' directory.
    font_content: Option<String>,
    font_version: Option<String>,
    assets: String,
    preloaded_sets: Vec<Arc<HashSet<String>>>,
    restricted_assets: Option<(String, HashSet<String>)>,
    enum_faces: Arc<HashMap<String, Vec<String>>>,
    family_cache: HashMap<String, Option<Arc<Family>>>,
    parsed_cache: HashMap<String, Option<Arc<Font>>>,
    face_cache: HashMap<(String, u64, String), Option<FaceId>>,
    faces: Vec<Arc<Face>>,
    fallbacks: Option<Vec<FaceId>>,
    cloud_fallbacks: Option<Vec<FaceId>>,
    hinted: Hinted,
    /// `Fonts.Diagnostics`: key -> message.
    pub diagnostics: BTreeMap<String, String>,
    /// `Fonts.MissingAssets`: cloud assets that were needed but are not cached: the id, or for a pinned version
    /// its content id (`rbxassetid://N&version=V`), as download.rs fetch_fonts reads them.
    pub missing_assets: BTreeSet<String>,
    /// Families no face could be read for, not even the Source Sans Pro fallback (no Studio fonts): their text
    /// has no glyphs and is not drawn (the scene's `diagnostics.unavailableFonts`).
    pub unavailable: BTreeSet<String>,
}

impl FontRegistry {
    /// `Fonts.Preload`: indexes every local font / family file and cached cloud font (file contents
    /// are read on first use), and reads the Enum.Font faces (roblox_engine_emulator::data::FONT_ENUMS).
    pub fn new(paths: FontPaths) -> FontRegistry {
        let content_root = paths.content_root.clone().or_else(discover_content_root);
        let font_content = match &paths.fonts {
            Some(fonts) => Some(format!("{fonts}/studio/content")),
            None => content_root.clone(),
        };
        let font_version = font_content.as_deref().and_then(version_root);
        let assets = format!("{}/assets", paths.fonts.as_ref().unwrap_or(&paths.cache));
        let hinted = Hinted::new(&paths.root, &paths.cache);
        let mut registry = FontRegistry {
            paths,
            content_root,
            font_content,
            font_version,
            assets,
            preloaded_sets: Vec::with_capacity(3),
            restricted_assets: None,
            enum_faces: enum_faces(),
            family_cache: HashMap::new(),
            parsed_cache: HashMap::new(),
            face_cache: HashMap::new(),
            faces: Vec::new(),
            fallbacks: None,
            cloud_fallbacks: None,
            hinted,
            diagnostics: BTreeMap::new(),
            missing_assets: BTreeSet::new(),
            unavailable: BTreeSet::new(),
        };
        registry.preload();
        registry
    }

    /// Treats only `keep` (font file paths) as cached cloud font files, as if the others were never
    /// downloaded (cached family files stay): a frozen corpus resolves faces as it was recorded,
    /// whatever the cache gained since. Call before the first lookup.
    pub fn restrict_cached_assets(&mut self, keep: &HashSet<String>) {
        match &mut self.restricted_assets {
            Some((_, existing)) => existing.retain(|path| keep.contains(path)),
            None => self.restricted_assets = Some((format!("{}/", self.assets), keep.clone())),
        }
    }

    fn has_preloaded(&self, path: &str) -> bool {
        if let Some((assets_prefix, keep)) = &self.restricted_assets {
            if path.starts_with(assets_prefix.as_str()) && !path.ends_with(".json") && !keep.contains(path) {
                return false;
            }
        }
        self.preloaded_sets.iter().any(|set| set.contains(path))
    }

    /// The cloud fonts' directory: the cache's `assets`, or the frozen font set's.
    pub fn assets_dir(&self) -> &str {
        &self.assets
    }

    /// The directory Studio's font files are read from (`…/content` of the install or the frozen set).
    pub fn font_content_root(&self) -> Option<&str> {
        self.font_content.as_deref()
    }

    fn preload(&mut self) {
        if let Some(content) = &self.font_content {
            self.preloaded_sets.push(preload_font_dir(&format!("{content}/fonts"), true, false));
        }
        if let Some(version) = &self.font_version {
            self.preloaded_sets.push(preload_font_dir(&format!("{version}/ExtraContent/LuaPackages/Packages/_Index/BuilderIcons"), true, false));
        }
        self.preloaded_sets.push(preload_font_dir(&self.assets, false, true));
    }

    pub fn content_root(&self) -> Option<&str> {
        self.content_root.as_deref()
    }

    pub fn paths(&self) -> &FontPaths {
        &self.paths
    }

    #[inline]
    pub fn face(&self, id: FaceId) -> &Arc<Face> {
        &self.faces[id as usize]
    }

    /// `Fonts.EnumFace`: family, weight and style of an Enum.Font name.
    pub fn enum_face(&self, name: &str) -> Option<(String, f64, String)> {
        let face = self.enum_faces.get(name)?;
        let family = face.first().cloned().unwrap_or_default();
        let weight = face.get(1).and_then(|weight| weight_name(weight)).unwrap_or(400.0);
        let style = face.get(2).map(|style| style.as_str()).unwrap_or("Normal").to_ascii_lowercase();
        Some((family, weight, style))
    }

    /// `Fonts.HasFamily`: whether a family file ships with Studio / is cached (before redirects).
    pub fn has_family(&mut self, uri: &str) -> bool {
        self.resolve_uri(uri).is_some()
    }

    /// `Fonts.ResolveUri`: local path of an rbxasset:// or rbxassetid:// URI.
    pub fn resolve_uri(&mut self, uri: &str) -> Option<String> {
        if let Some(relative) = uri.strip_prefix("rbxasset://") {
            if relative.is_empty() {
                return None;
            }
            let content = self.font_content.as_ref()?;
            let candidate = format!("{content}/{relative}");
            if self.has_preloaded(&candidate) {
                return Some(candidate);
            }
            if let Some(version) = &self.font_version {
                let extra = format!("{version}/ExtraContent/{relative}");
                if self.has_preloaded(&extra) {
                    return Some(extra);
                }
                let platform = format!("{version}/PlatformContent/pc/{relative}");
                if self.has_preloaded(&platform) {
                    return Some(platform);
                }
            }
            return None;
        }
        let asset = font_asset(uri)?;
        let key = asset.key();
        for extension in ["", ".ttf", ".otf", ".ttc", ".json", ".bin"] {
            let candidate = format!("{}/{key}{extension}", self.assets);
            if self.has_preloaded(&candidate) {
                return Some(candidate);
            }
        }
        self.missing_assets.insert(match asset.version() {
            None => asset.id().to_string(),
            Some(_) => asset.uri(),
        });
        None
    }

    fn load_family(&mut self, uri: &str) -> Option<Arc<Family>> {
        if let Some(cached) = self.family_cache.get(uri) {
            return cached.clone();
        }
        let target = FAMILY_REDIRECTS.iter().find(|(from, _)| *from == uri).map(|(_, to)| *to).unwrap_or(uri);
        let family = self.resolve_uri(target).and_then(|path| load_family_file(&path));
        if family.is_none() {
            self.diagnostics.insert(
                uri.to_string(),
                format!("font family {uri} is not available offline (fetch it with rur fetch); using Source Sans Pro metrics"),
            );
        }
        self.family_cache.insert(uri.to_string(), family.clone());
        family
    }

    /// `Fonts.LoadFile`: the parsed font at a resolved path (cached, failures included).
    pub fn load_file(&mut self, path: &str) -> Option<Arc<Font>> {
        if let Some(cached) = self.parsed_cache.get(path) {
            return cached.clone();
        }
        let font = match load_parsed_font(path) {
            Ok(font) => Some(font),
            Err(message) => {
                self.diagnostics.insert(path.to_string(), message);
                None
            }
        };
        self.parsed_cache.insert(path.to_string(), font.clone());
        font
    }

    fn add_face(&mut self, path: String, font: Arc<Font>, weight: f64, style: String, family: String, substituted: bool) -> FaceId {
        let id = self.faces.len() as FaceId;
        self.faces.push(Arc::new(Face { id, path: path.into(), font, weight, style, family, substituted }));
        id
    }

    /// `Fonts.GetFace`: the closest face of a family (style first, then CSS weight distance), else
    /// Source Sans Pro Regular; None, with the family in [`Self::unavailable`], when neither can be read.
    pub fn get_face(&mut self, family: &str, weight: f64, style: &str) -> Option<FaceId> {
        let key = (family.to_string(), weight.to_bits(), style.to_string());
        if let Some(cached) = self.face_cache.get(&key) {
            return *cached;
        }
        let mut result = None;
        if let Some(data) = self.load_family(family) {
            let mut candidates: Vec<&FamilyFace> = data.faces.iter().collect();
            candidates.sort_by(|a, b| {
                let style_a = if a.style == style { 0 } else { 1 };
                let style_b = if b.style == style { 0 } else { 1 };
                style_a.cmp(&style_b).then_with(|| {
                    weight_distance(weight, a.weight).partial_cmp(&weight_distance(weight, b.weight)).unwrap_or(std::cmp::Ordering::Equal)
                })
            });
            for (index, candidate) in candidates.iter().enumerate() {
                let Some(path) = self.resolve_uri(&candidate.asset_id) else {
                    continue;
                };
                let Some(font) = self.load_file(&path) else {
                    continue;
                };
                let first = candidates[0];
                let substituted = index > 0 && (candidate.weight != first.weight || candidate.style != first.style);
                if substituted {
                    self.diagnostics.insert(
                        format!("{family}|{}|{style}", super::layout::luau_number(weight)),
                        format!("{family} {} is not cached offline; measured with {}", first.name, candidate.name),
                    );
                }
                result = Some(self.add_face(path, font, candidate.weight, candidate.style.clone(), family.to_string(), substituted));
                break;
            }
        }
        if result.is_none() {
            if let Some(path) = self.resolve_uri("rbxasset://fonts/SourceSansPro-Regular.ttf") {
                if let Some(font) = self.load_file(&path) {
                    result = Some(self.add_face(path, font, 400.0, "normal".into(), family.to_string(), true));
                }
            }
        }
        if result.is_none() {
            // (in the place of load_family's "using Source Sans Pro metrics": there is no Source Sans Pro either)
            let reason = match &self.font_content {
                None => "no Roblox Studio fonts were found (install Studio, or set RUR_ROBLOX_CONTENT to a Studio content folder)".to_string(),
                Some(content) => format!("{content}/fonts/SourceSansPro-Regular.ttf can't be read"),
            };
            self.diagnostics.insert(family.to_string(), format!("text in {family} is not drawn: no face of it and no Source Sans Pro fallback is available; {reason}"));
            self.unavailable.insert(family.to_string());
        }
        self.face_cache.insert(key, result);
        result
    }

    fn fallback_faces(&mut self, files: &[&str]) -> Vec<FaceId> {
        let mut result = Vec::new();
        for uri in files {
            let Some(path) = self.resolve_uri(uri) else {
                continue;
            };
            let Some(font) = self.load_file(&path) else {
                continue;
            };
            result.push(self.add_face(path, font, 400.0, "normal".into(), uri.to_string(), false));
        }
        result
    }

    /// `Fonts.Fallbacks`.
    pub fn fallbacks(&mut self) -> &[FaceId] {
        if self.fallbacks.is_none() {
            let faces = self.fallback_faces(&FALLBACK_FILES);
            self.fallbacks = Some(faces);
        }
        self.fallbacks.as_deref().unwrap()
    }

    /// The fallback faces once [`Self::fallbacks`] has loaded them (empty before).
    pub fn loaded_fallbacks(&self) -> &[FaceId] {
        self.fallbacks.as_deref().unwrap_or(&[])
    }

    /// `Fonts.CloudFallbacks`: only resolved when no local fallback has a glyph.
    pub fn cloud_fallbacks(&mut self) -> &[FaceId] {
        if self.cloud_fallbacks.is_none() {
            let faces = self.fallback_faces(&CLOUD_FALLBACK_FILES);
            self.cloud_fallbacks = Some(faces);
        }
        self.cloud_fallbacks.as_deref().unwrap()
    }

    /// `Fonts.HintedAdvance`: None leaves the caller's approximation, with a diagnostic.
    pub fn hinted_advance(&mut self, face: FaceId, size: f64, glyph: u32) -> Option<f64> {
        let path = self.faces[face as usize].path.clone();
        match self.hinted.advance(&path, size, glyph) {
            Ok(advance) => Some(advance),
            Err(reason) => {
                self.diagnostics.insert(
                    format!("hinted:{path}"),
                    format!("hinted truncation advance unavailable; using approximate width: {reason}"),
                );
                None
            }
        }
    }
}

/// The cloud asset a family or face URI names, read as an image's is (resolve.rs asset_ref):
/// `rbxassetid://N`, `rbxassetid://N&version=V` (that version, cached as `<id>-v<version>`) and
/// `roblox.com/asset/?id=N`. A bare id names no font, nor does a spelling a Roblox client loads nothing for.
fn font_asset(uri: &str) -> Option<AssetRef> {
    if uri.trim().bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    crate::resolve::asset_ref(uri)?.ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pinned family or face is its own cache file, and a missing one is reported as the content id
    /// download.rs fetch_fonts downloads
    #[test]
    fn cloud_fonts_are_found_by_id_and_version() {
        let cache = std::env::temp_dir().join(format!("rur-font-versions-{}", std::process::id()));
        let assets = cache.join("assets");
        let _ = std::fs::remove_dir_all(&cache);
        std::fs::create_dir_all(&assets).unwrap();
        for name in ["5.json", "5-v2.json", "6-v3.ttf"] {
            std::fs::write(assets.join(name), "{}").unwrap();
        }
        let cache_dir = cache.to_string_lossy().replace('\\', "/");
        let mut fonts = FontRegistry::new(FontPaths { root: cache_dir.clone(), cache: cache_dir.clone(), content_root: None, fonts: None });
        let file = |name: &str| Some(format!("{cache_dir}/assets/{name}"));
        assert_eq!(fonts.resolve_uri("rbxassetid://5"), file("5.json"));
        assert_eq!(fonts.resolve_uri("rbxassetid://5&version=2"), file("5-v2.json"));
        assert_eq!(fonts.resolve_uri("rbxassetid://5&Version=02&x=y"), file("5-v2.json"));
        assert_eq!(fonts.resolve_uri("http://www.roblox.com/asset/?id=5&version=2"), file("5-v2.json"));
        assert_eq!(fonts.resolve_uri("rbxassetid://6&version=3"), file("6-v3.ttf"));
        assert_eq!(fonts.resolve_uri("rbxassetid://6"), None);
        assert_eq!(fonts.resolve_uri("rbxassetid://5&version=4"), None);
        assert_eq!(fonts.resolve_uri("rbxassetid://5?version=2"), None, "a Roblox client loads nothing for it");
        assert_eq!(fonts.resolve_uri("5"), None, "a bare id is no font");
        assert_eq!(fonts.missing_assets.iter().map(String::as_str).collect::<Vec<_>>(), ["6", "rbxassetid://5&version=4"]);
        let _ = std::fs::remove_dir_all(&cache);
    }
}
