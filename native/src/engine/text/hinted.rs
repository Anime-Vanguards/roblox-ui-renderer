//! Port of runtime/text/hinted.luau: hinted advances of the truncation glyph (U+2026, else ".") per
//! font file and pixel size. Generated in process with the FreeType compiled into the crate
//! (crate::glyphs::truncation_advances, the port of tools/glyphs/metrics.py; identical advances) and
//! cached as JSON under `<cache>/hinted/<key>.json`, so an engine start reads them instead of loading
//! the glyph 2048 times. The key covers the FreeType version, [`GENERATOR_VERSION`],
//! FREETYPE_PROPERTIES, the size range and the font bytes; it does not depend on Python or the
//! environment otherwise. Fonts are resolved lazily: a font missing from the cache is generated alone.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Bump when the generation (size request, load flags, glyph choice) changes
pub const GENERATOR_VERSION: u32 = 4;
/// The cache file format (version 1 was metrics.py's, keyed by the Python environment)
const CACHE_VERSION: f64 = 2.0;

pub struct HintedData {
    pub glyph: u32,
    pub advances: Vec<f64>,
}

pub struct Hinted {
    cache: String,
    maximum: f64,
    fingerprint: String,
    metrics: HashMap<String, Option<Arc<HintedData>>>,
    failures: HashMap<String, String>,
    by_key: HashMap<String, Option<Arc<HintedData>>>,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Luau `tostring` of an integral number (the only kind the key formats).
fn luau_integer(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value:e}").replace('e', "e+")
    }
}

/// Replaces `destination` with `source`; Windows readers can briefly deny the replacement.
fn replace_file(source: &str, destination: &str) -> std::io::Result<()> {
    let mut attempt = 0;
    loop {
        match std::fs::rename(source, destination) {
            Ok(()) => return Ok(()),
            Err(error) if attempt < 7 && error.kind() == std::io::ErrorKind::PermissionDenied => {
                std::thread::sleep(std::time::Duration::from_millis(10 << attempt.min(3)));
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

impl Hinted {
    /// `root` (the checkout) is no longer needed: generation runs in process.
    pub fn new(_root: &str, cache: &str) -> Hinted {
        let requested = env("RUR_HINTED_MAX_SIZE").and_then(|value| super::num::tonumber(value.as_bytes()));
        let maximum = match requested {
            Some(value) if value >= 1.0 && value < f64::INFINITY => value.floor(),
            _ => 2048.0,
        };
        // FREETYPE_PROPERTIES changes FreeType's defaults (FT_Init_FreeType reads it)
        let parts = serde_json::json!([
            "rur-hinted",
            GENERATOR_VERSION,
            crate::glyphs::freetype_version(),
            env("FREETYPE_PROPERTIES").unwrap_or_default(),
        ]);
        Hinted {
            cache: cache.to_string(),
            maximum,
            fingerprint: sha256_hex(parts.to_string().as_bytes()),
            metrics: HashMap::new(),
            failures: HashMap::new(),
            by_key: HashMap::new(),
        }
    }

    fn load(&self, path: &str, key: &str) -> Option<Arc<HintedData>> {
        let bytes = std::fs::read(path).ok()?;
        let data: Value = serde_json::from_slice(&bytes).ok()?;
        let object = data.as_object()?;
        if object.get("version")?.as_f64()? != CACHE_VERSION
            || object.get("key")?.as_str()? != key
            || object.get("maxSize")?.as_f64()? != self.maximum
        {
            return None;
        }
        let glyph = object.get("glyph")?.as_f64()?;
        if glyph < 0.0 || glyph.fract() != 0.0 || glyph > f64::from(u32::MAX) {
            return None;
        }
        let advances = object.get("advances")?.as_array()?;
        if advances.len() as f64 != self.maximum {
            return None;
        }
        let mut values = Vec::with_capacity(advances.len());
        for value in advances {
            let value = value.as_f64()?;
            if !value.is_finite() {
                return None;
            }
            values.push(value);
        }
        Some(Arc::new(HintedData { glyph: glyph as u32, advances: values }))
    }

    /// Writes a cache file atomically (a temporary file in the same directory, then a rename).
    fn store(&self, path: &str, key: &str, data: &HintedData) -> Result<(), String> {
        let directory = format!("{}/hinted", self.cache);
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let body = serde_json::json!({
            "version": CACHE_VERSION as i64,
            "key": key,
            "maxSize": self.maximum as i64,
            "glyph": data.glyph,
            "advances": data.advances,
            "freetype": crate::glyphs::freetype_version(),
            "generator": GENERATOR_VERSION,
        });
        static WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let write = WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let token = sha256_hex(format!("{:?}:{}:{write}:{path}", std::time::SystemTime::now(), std::process::id()).as_bytes());
        let temporary = format!("{path}.{}.tmp", &token[..16]);
        std::fs::write(&temporary, body.to_string()).map_err(|error| error.to_string())?;
        replace_file(&temporary, path).map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            error.to_string()
        })
    }

    fn prepare(&mut self, path: &str) {
        if self.metrics.contains_key(path) {
            return;
        }
        let result = self.prepare_inner(path);
        match result {
            Ok(data) => {
                self.metrics.insert(path.to_string(), data);
            }
            Err(message) => {
                self.metrics.insert(path.to_string(), None);
                self.failures.insert(path.to_string(), message);
            }
        }
    }

    fn prepare_inner(&mut self, path: &str) -> Result<Option<Arc<HintedData>>, String> {
        let Ok(bytes) = std::fs::read(path) else {
            return Ok(None);
        };
        let magic = bytes.get(0..4).unwrap_or(&[]);
        if magic != b"\0\x01\0\0" && magic != b"OTTO" && magic != b"ttcf" && magic != b"true" {
            return Ok(None);
        }
        let font_hash = sha256_hex(&bytes);
        let key = sha256_hex(format!("2:{}:{}:{font_hash}", self.fingerprint, luau_integer(self.maximum)).as_bytes());
        if let Some(found) = self.by_key.get(&key) {
            if found.is_some() {
                return Ok(found.clone());
            }
        }
        let cache = format!("{}/hinted/{key}.json", self.cache);
        if let Some(data) = self.load(&cache, &key) {
            self.by_key.insert(key, Some(data.clone()));
            return Ok(Some(data));
        }
        let (glyph, advances) = crate::glyphs::truncation_advances(&bytes, self.maximum as u32)
            .map_err(|error| format!("hinted metric generation failed: {error:#}"))?;
        let data = Arc::new(HintedData { glyph, advances });
        // A cache that cannot be written only costs the next start a regeneration
        let _ = self.store(&cache, &key, &data);
        self.by_key.insert(key, Some(data.clone()));
        Ok(Some(data))
    }

    /// `Hinted.Advance`: the hinted advance of `glyph` at physical `size`, or why there is none.
    pub fn advance(&mut self, path: &str, size: f64, glyph: u32) -> Result<f64, String> {
        self.prepare(path);
        let data = match self.metrics.get(path) {
            Some(Some(data)) => data.clone(),
            _ => {
                return Err(self.failures.get(path).cloned().unwrap_or_else(|| "hinted metrics unavailable for this font".into()));
            }
        };
        if size < 1.0 || size > self.maximum || size.fract() != 0.0 {
            return Err(format!(
                "physical text size {} is outside hinted metric range 1..{} (RUR_HINTED_MAX_SIZE)",
                super::layout::luau_number(size),
                luau_integer(self.maximum)
            ));
        }
        if glyph != data.glyph {
            return Err("glyph is not the preloaded truncation glyph".into());
        }
        Ok(data.advances[size as usize - 1])
    }
}
