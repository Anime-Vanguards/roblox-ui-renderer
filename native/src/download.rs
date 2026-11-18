//! Asset downloads: the network half of the asset cache, for the desktop app, headless renders, the
//! render service and the Node CLI (`rur-native assets`). The web build has none (it draws its bundle).
//!
//! - `rbxassetid://N` (and `roblox.com/asset/?id=N`, bare ids) from asset delivery: the public route
//!   first, then with the account. The CDN location it answers is downloaded without credentials. Many
//!   ids are located first through the batch endpoint, 100 per request, and 16 downloads run at once.
//! - `rbxthumb://` from the thumbnail service: its WebP rendition (what Studio draws), decoded to PNG
//!   here, else its PNG rendition. A Blocked avatar thumbnail is Studio's placeholder silhouette.
//! - The cloud fonts a run reported missing.
//! - An image asset delivery refuses (private to its creator) can be read from an open Studio: with a
//!   Studio MCP bridge configured, the checkout's `node cli/studio-images.mjs store` reads the refused
//!   ids once and stores them in the cache (docs/assets.md "Images from Studio").
//!
//! The account is an account broker or a cookie ([`Account`]). The cookie is only ever placed in the
//! Cookie header of requests to `https://assetdelivery.roblox.com`, the broker's token only in the
//! Authorization header of requests to that broker. Neither is printed, logged, cached or written
//! anywhere, sent across a redirect or passed to a child process.
//!
//! The cache is cli/assets.mjs's layout, so caches it filled stay valid: `<state dir>/assets/<id>.<ext>`
//! with a record of each download in `assets/downloaded/<name>.json`, cloud fonts listed in
//! `assets/fonts.json`, thumbnails under `thumb-<type>-<id>-<w>x<h>-webp.png`. A cache file is replaced
//! by rename, never rewritten in place: render directories hard-link cache files (story_render.rs
//! link_or_copy), and a link must keep the content it was made with.
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use serde_json::{Value, json};

use crate::resolve::{Sniffed, sniff};

pub const USER_AGENT: &str = "rur/0.1 (offline Roblox UI renderer)";
const DELIVERY_HOST: &str = "assetdelivery.roblox.com";
/// How long the server may take to accept, answer or send a request (Node's socket timeout was 30 s of
/// silence; a body may take longer as long as it keeps arriving, up to BODY_TIMEOUT)
const TIMEOUT: Duration = Duration::from_secs(30);
const BODY_TIMEOUT: Duration = Duration::from_secs(300);
/// The largest body read (the biggest cloud font face is about 16 MB)
const BODY_LIMIT: u64 = 512 * 1024 * 1024;
/// Downloads at once: located, a download is one CDN round trip, and 40 images took 2.7 s 8 at a time,
/// 1.7 s 16 at a time
pub const CONCURRENCY: usize = 16;
/// Ids per batch location request (asset delivery's limit)
const BATCH: usize = 100;

// HTTP

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

/// One request as it goes out. The transport adds nothing to it and follows no redirect.
#[derive(Clone)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }
}

/// Header names only: a request can carry the cookie or the broker token
impl std::fmt::Debug for Request {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(name, _)| *name).collect();
        formatter.debug_struct("Request").field("method", &self.method).field("url", &self.url).field("headers", &names).finish()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Response {
    pub status: u16,
    /// Names in lower case
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }

    fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// A request that got no answer: why, and the system's short code for it when there is one (ECONNREFUSED)
#[derive(Clone, Debug)]
pub struct TransportError {
    pub message: String,
    pub code: Option<&'static str>,
}

/// Sends one request. The network's is [`Network`]; tests answer with their own.
pub trait Transport: Send + Sync {
    fn send(&self, request: &Request) -> Result<Response, TransportError>;
}

/// HTTPS through ureq (rustls with its bundled roots), one connection pool per process
pub struct Network;

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            // Redirects are followed by Downloader::get, which drops every header across one
            .max_redirects(0)
            .http_status_as_error(false)
            // Requests go straight to the host they name, never through a proxy from the environment
            .proxy(None)
            .timeout_resolve(Some(TIMEOUT))
            .timeout_connect(Some(TIMEOUT))
            .timeout_send_request(Some(TIMEOUT))
            .timeout_send_body(Some(TIMEOUT))
            .timeout_recv_response(Some(TIMEOUT))
            .timeout_recv_body(Some(BODY_TIMEOUT))
            .build()
            .new_agent()
    })
}

impl Transport for Network {
    fn send(&self, request: &Request) -> Result<Response, TransportError> {
        let host = host_of(&request.url).to_string();
        let failed = |error: ureq::Error| {
            let code = match &error {
                ureq::Error::HostNotFound => Some("ENOTFOUND"),
                ureq::Error::Io(error) => match error.kind() {
                    std::io::ErrorKind::ConnectionRefused => Some("ECONNREFUSED"),
                    std::io::ErrorKind::ConnectionReset => Some("ECONNRESET"),
                    std::io::ErrorKind::TimedOut => Some("ETIMEDOUT"),
                    _ => None,
                },
                _ => None,
            };
            let message = match error {
                ureq::Error::Timeout(_) => format!("timeout fetching {host}"),
                error => format!("fetching {host}: {error}"),
            };
            TransportError { message, code }
        };
        let response = match request.method {
            Method::Get => {
                let mut builder = agent().get(&request.url);
                for (name, value) in &request.headers {
                    builder = builder.header(*name, value);
                }
                builder.call()
            }
            Method::Post => {
                let mut builder = agent().post(&request.url);
                for (name, value) in &request.headers {
                    builder = builder.header(*name, value);
                }
                builder.send(&request.body[..])
            }
        }
        .map_err(failed)?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| (name.as_str().to_ascii_lowercase(), String::from_utf8_lossy(value.as_bytes()).into_owned()))
            .collect();
        let mut body = response.into_body();
        let body = body.with_config().limit(BODY_LIMIT).read_to_vec().map_err(failed)?;
        Ok(Response { status, headers, body })
    }
}

/// The host of an absolute URL, for messages
fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    authority.rsplit_once('@').map_or(authority, |(_, host)| host)
}

/// `location` resolved against `base` (a redirect's Location header)
fn join_url(base: &str, location: &str) -> String {
    if location.contains("://") {
        return location.to_string();
    }
    let (scheme, rest) = base.split_once("://").unwrap_or(("https", base));
    if let Some(rest) = location.strip_prefix("//") {
        return format!("{scheme}://{rest}");
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let origin = format!("{scheme}://{}", &rest[..end]);
    if location.starts_with('/') {
        return format!("{origin}{location}");
    }
    let path = rest[end..].split(['?', '#']).next().unwrap_or("");
    let directory = path.rfind('/').map_or("/", |at| &path[..=at]);
    format!("{origin}{directory}{location}")
}

fn is_https(url: &str) -> bool {
    url.get(..8).is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
}

// The account

/// How asset delivery's private assets are asked for. An account broker comes before any cookie: with
/// broker credentials no cookie is read or sent, and a broker address that is refused does not fall
/// back to the cookie.
///
/// - The broker: `ROBLOX_BROKER_URL` with `ROBLOX_BROKER_TOKEN`, else the JSON file
///   `ROBLOX_BROKER_CREDENTIALS` names, else `~/.config/roblox-broker/credentials.json` (`{ "url", "token" }`).
/// - The cookie (`ROBLOX_ASSET_DELIVERY_COOKIE` or `ROBLOSECURITY`, a `.ROBLOSECURITY` value): a checkout
///   build's `.env`, then the environment, then the file `RUR_COOKIE_ENV` names.
#[derive(Clone, PartialEq, Eq)]
pub enum Account {
    Broker { url: String, token: String },
    /// A broker is configured but its address is refused: why
    BrokerRefused(String),
    Cookie(String),
    Public,
}

/// Never the cookie or the token
impl std::fmt::Debug for Account {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Account::Broker { url, .. } => write!(formatter, "Broker({url})"),
            Account::BrokerRefused(error) => write!(formatter, "BrokerRefused({error})"),
            Account::Cookie(_) => formatter.write_str("Cookie"),
            Account::Public => formatter.write_str("Public"),
        }
    }
}

impl Account {
    /// The account `env` (a variable's value, None when unset) configures. `checkout`: the checkout a
    /// checkout build runs from, whose `.env` is read; None for a release build.
    pub fn load(env: &dyn Fn(&str) -> Option<String>, checkout: Option<&Path>) -> Account {
        match load_broker(env) {
            Some(Ok((url, token))) => Account::Broker { url, token },
            Some(Err(error)) => Account::BrokerRefused(error),
            None => load_cookie(env, checkout).map_or(Account::Public, Account::Cookie),
        }
    }

    /// "broker", "cookie", or None (public assets only, or a broker address that is refused)
    pub fn access(&self) -> Option<&'static str> {
        match self {
            Account::Broker { .. } => Some("broker"),
            Account::Cookie(_) => Some("cookie"),
            Account::BrokerRefused(_) | Account::Public => None,
        }
    }
}

/// A variable, None when unset or empty
fn nonempty(env: &dyn Fn(&str) -> Option<String>, name: &str) -> Option<String> {
    env(name).filter(|value| !value.is_empty())
}

/// A cookie value as written in an env file or variable: quotes keep everything inside them, an
/// unquoted value ends at `#`, and a leading `.ROBLOSECURITY=` is dropped
fn cookie_value(raw: &str) -> Option<String> {
    let value = raw.trim();
    let value = match value.chars().next() {
        Some(quote @ ('"' | '\'')) => {
            let inner = &value[1..];
            &inner[..inner.find(quote)?]
        }
        _ => value.split('#').next().unwrap_or_default().trim(),
    };
    let value = value.strip_prefix(".ROBLOSECURITY=").unwrap_or(value);
    (!value.is_empty()).then(|| value.to_string())
}

/// The first cookie an env file sets: `[export] ROBLOX_ASSET_DELIVERY_COOKIE=...` or `ROBLOSECURITY=...`
fn cookie_from_file(file: &Path) -> Option<String> {
    let bytes = std::fs::read(file).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line).trim_start();
        let line = match line.strip_prefix("export") {
            Some(rest) if rest.starts_with(char::is_whitespace) => rest.trim_start(),
            _ => line,
        };
        let Some(rest) = ["ROBLOX_ASSET_DELIVERY_COOKIE", "ROBLOSECURITY"].iter().find_map(|name| line.strip_prefix(name)) else {
            continue;
        };
        let Some(value) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        if let Some(value) = cookie_value(value) {
            return Some(value);
        }
    }
    None
}

fn load_cookie(env: &dyn Fn(&str) -> Option<String>, checkout: Option<&Path>) -> Option<String> {
    if let Some(cookie) = checkout.and_then(|checkout| cookie_from_file(&checkout.join(".env"))) {
        return Some(cookie);
    }
    for name in ["ROBLOX_ASSET_DELIVERY_COOKIE", "ROBLOSECURITY"] {
        if let Some(cookie) = env(name).and_then(|raw| cookie_value(&raw)) {
            return Some(cookie);
        }
    }
    let candidates = [
        nonempty(env, "RUR_COOKIE_ENV").map(PathBuf::from),
    ];
    candidates.into_iter().flatten().find_map(|file| cookie_from_file(&file))
}

/// The broker's address and token, an error for an address that is refused, or None when none is
/// configured
fn load_broker(env: &dyn Fn(&str) -> Option<String>) -> Option<Result<(String, String), String>> {
    let trimmed = |name: &str| env(name).map(|value| value.trim().to_string()).unwrap_or_default();
    let (mut url, mut token) = (trimmed("ROBLOX_BROKER_URL"), trimmed("ROBLOX_BROKER_TOKEN"));
    if url.is_empty() || token.is_empty() {
        (url, token) = (String::new(), String::new());
        let home = if cfg!(windows) { nonempty(env, "USERPROFILE") } else { nonempty(env, "HOME") };
        let file = nonempty(env, "ROBLOX_BROKER_CREDENTIALS")
            .map(PathBuf::from)
            .or_else(|| home.map(|home| PathBuf::from(home).join(".config").join("roblox-broker").join("credentials.json")));
        let saved = file.and_then(|file| std::fs::read(file).ok()).and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        if let Some((saved_url, saved_token)) = saved.as_ref().and_then(|saved| Some((saved["url"].as_str()?, saved["token"].as_str()?))) {
            (url, token) = (saved_url.trim().to_string(), saved_token.trim().to_string());
        }
    }
    if url.is_empty() || token.is_empty() {
        return None;
    }
    Some(broker_address(&url).map(|url| (url, token)))
}

/// The broker's base URL (origin and path, no trailing slash). The token is the whole credential, so
/// it only goes out over TLS, or in plain http to this machine (a tunnel): `127.0.0.1` or `localhost`.
fn broker_address(raw: &str) -> Result<String, String> {
    let not_url = || "the account broker's address is not a URL".to_string();
    let colon = raw.find(':').ok_or_else(not_url)?;
    let scheme = &raw[..colon];
    if !scheme.starts_with(|c: char| c.is_ascii_alphabetic()) || !scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)) {
        return Err(not_url());
    }
    let scheme = scheme.to_ascii_lowercase();
    let special = matches!(scheme.as_str(), "http" | "https" | "ftp" | "ws" | "wss");
    let rest = &raw[colon + 1..];
    let (authority, path) = if special {
        let rest = rest.trim_start_matches(['/', '\\']);
        let end = rest.find(['/', '\\', '?', '#']).unwrap_or(rest.len());
        (&rest[..end], rest[end..].replace('\\', "/"))
    } else if let Some(rest) = rest.strip_prefix("//") {
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        (&rest[..end], rest[end..].to_string())
    } else {
        ("", rest.to_string())
    };
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let (hostname, port) = match host_port.rfind(':') {
        Some(at) if !host_port[at..].contains(']') => (&host_port[..at], &host_port[at + 1..]),
        _ => (host_port, ""),
    };
    // An empty port (`host:`) is no port; a port is digits up to 65535
    let port = match port {
        "" => None,
        digits if digits.chars().all(|c| c.is_ascii_digit()) => Some(digits.parse::<u32>().ok().filter(|port| *port <= 65535).ok_or_else(not_url)?),
        _ => return Err(not_url()),
    };
    if special && hostname.is_empty() {
        return Err(not_url());
    }
    let hostname = hostname.to_ascii_lowercase();
    let default_port = match scheme.as_str() {
        "http" | "ws" => Some(80),
        "https" | "wss" => Some(443),
        "ftp" => Some(21),
        _ => None,
    };
    let host = match port {
        Some(port) if Some(port) != default_port => format!("{hostname}:{port}"),
        _ => hostname.clone(),
    };
    let loopback = hostname == "127.0.0.1" || hostname == "localhost";
    if scheme != "https" && !(scheme == "http" && loopback) {
        return Err(format!("the account broker's address must be https (http only for 127.0.0.1 or localhost), not {scheme}://{host}"));
    }
    let path = path.split(['?', '#']).next().unwrap_or_default();
    Ok(format!("{scheme}://{host}{}", path.trim_end_matches('/')))
}

/// Whether refused images are read from Studio: a Studio MCP bridge is configured (RUR_STUDIO_MCP_URL,
/// or the TCP bridge's port) and RUR_STUDIO_IMAGES is not "0" (cli/assets.mjs studioImagesEnabled)
pub fn studio_images_enabled(env: &dyn Fn(&str) -> Option<String>) -> bool {
    let port = |name: &str| env(name).and_then(|value| value.trim().parse::<f64>().ok()).is_some_and(|port| port != 0.0);
    let bridge = nonempty(env, "RUR_STUDIO_MCP_URL").is_some() || port("RUR_STUDIO_MCP_PORT");
    env("RUR_STUDIO_IMAGES").as_deref() != Some("0") && bridge
}

/// The environment variables that hold the account (any case): kept from child processes that download
/// nothing (cli/assets.mjs withoutCookie)
pub fn without_account(command: &mut Command) -> &mut Command {
    for (name, _) in std::env::vars_os() {
        let text = name.to_string_lossy();
        if ["ROBLOX_ASSET_DELIVERY_COOKIE", "ROBLOSECURITY", "ROBLOX_BROKER_TOKEN"].iter().any(|secret| text.eq_ignore_ascii_case(secret)) {
            command.env_remove(&name);
        }
    }
    command
}

// The cache

/// A resolved asset: its cache file and what the bytes are; `via` the decal / texture model that named it
#[derive(Clone, Debug, PartialEq)]
pub struct Fetched {
    pub file: PathBuf,
    pub kind: &'static str,
    pub mime: &'static str,
    pub via: Option<String>,
}

impl Fetched {
    fn new(file: PathBuf, sniffed: Sniffed) -> Self {
        Self { file, kind: sniffed.kind, mime: sniffed.mime, via: None }
    }
}

/// Asset delivery serves some files gzip-wrapped without a Content-Encoding header (body starts 1f 8b)
fn unwrap(body: Vec<u8>) -> Vec<u8> {
    if body.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        if std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&body[..]), &mut out).is_ok() {
            return out;
        }
    }
    body
}

/// Writes `file` through a temporary file beside it and a rename: a reader never sees half a file, and
/// a render directory's hard link to the old file keeps the old bytes
pub(crate) fn write_atomic(file: &Path, body: &[u8]) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|time| time.as_millis()).unwrap_or_default();
    let temporary = PathBuf::from(format!("{}.{}.{millis}.{}.tmp", file.display(), std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let written = std::fs::write(&temporary, body).and_then(|_| std::fs::rename(&temporary, file));
    written.map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        format!("writing {}: {error}", file.display())
    })
}

/// Records `body` as what rur downloaded (or read from Studio) under `file`'s name
fn record_download(file: &Path, body: &[u8]) -> Result<(), String> {
    let record = crate::resolve::record_file(file);
    if let Some(directory) = record.parent() {
        std::fs::create_dir_all(directory).map_err(|error| format!("creating {}: {error}", directory.display()))?;
    }
    let sha256 = crate::engine::text::hinted::sha256_hex(body);
    write_atomic(&record, format!(r#"{{"sha256":"{sha256}","size":{}}}"#, body.len()).as_bytes())
}

fn md5_hex(bytes: &[u8]) -> String {
    use md5::{Digest, Md5};
    Md5::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The decoded WebP as a PNG (straight RGBA, lossless), as the renderer reads it; None when it does not decode
fn webp_to_png(webp: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory_with_format(webp, image::ImageFormat::WebP).ok()?.to_rgba8();
    let mut png = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut png),
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )
    .ok()?;
    Some(png)
}

/// Measured (fixtures/image-probe-thumb-blocked.luau, out/compare/image-probe-thumb-blocked, notes/images.md
/// round 4): for a user thumbnail the service reports as "Blocked" (its imageUrl is the UnapprovedImage
/// icon, which Studio's log reports as "HttpError: InvalidUrl") Studio draws a placeholder silhouette
/// instead: AvatarHeadShot a 420 x 420 grey head in a circle (the w=60 / 150 / 420 cells all show this
/// one texture, drawn Pixelated: 100.0 / 99.99 / 99.96% of their pixels within 2), Avatar a 720 x 720
/// black R6 figure. Neither is in the Studio install nor served by the thumbnail API (every
/// returnPolicy, format and isCircular gives the UnapprovedImage URL); Studio downloads them, so they are
/// taken from its HTTP cache (%LOCALAPPDATA%/Roblox/rbx-storage, entries "RBXH" + header + body, body
/// length at byte 25), identified by the body's MD5, and kept in the asset cache. The "Error" state (no
/// imageUrl) draws nothing, like Studio.
const BLOCKED_PLACEHOLDERS: [(&str, &str, usize); 2] =
    [("AvatarHeadShot", "2f598ef27b1dddc3c4556aba8c2101c1", 11315), ("Avatar", "5c2d7bd09200921254a731f35c6594e1", 41357)];

/// The UnapprovedImage icon (the PNG rendition the service's Blocked imageUrl serves)
const UNAPPROVED_IMAGE_MD5: &str = "4cbf45bd6a776b83a681b35f937d5641";

/// The body of the entry in Roblox's HTTP cache `storage` whose body has `md5` and `length` bytes
fn storage_entry(storage: &Path, md5: &str, length: usize) -> Option<Vec<u8>> {
    for folder in std::fs::read_dir(storage).ok()?.flatten() {
        let Ok(entries) = std::fs::read_dir(folder.path()) else { continue };
        for entry in entries.flatten() {
            if entry.metadata().map(|metadata| metadata.len()).ok() != Some(length as u64 + 37) {
                continue;
            }
            let Ok(bytes) = std::fs::read(entry.path()) else { continue };
            if bytes.len() < 37 || &bytes[..4] != b"RBXH" || u32::from_le_bytes([bytes[25], bytes[26], bytes[27], bytes[28]]) as usize != length {
                continue;
            }
            if md5_hex(&bytes[37..]) == md5 {
                return Some(bytes[37..].to_vec());
            }
        }
    }
    None
}

/// The thumbnail endpoints by `rbxthumb://` type (an unknown type is an Asset)
fn thumbnail_endpoint(kind: &str, id: &str, size: &str) -> String {
    let (route, ids) = match kind {
        "Avatar" => ("users/avatar", "userIds"),
        "AvatarHeadShot" => ("users/avatar-headshot", "userIds"),
        "AvatarBust" => ("users/avatar-bust", "userIds"),
        "BadgeIcon" => ("badges/icons", "badgeIds"),
        "GamePass" => ("game-passes", "gamePassIds"),
        "GroupIcon" => ("groups/icons", "groupIds"),
        "BundleThumbnail" => ("bundles/thumbnails", "bundleIds"),
        "DeveloperProduct" => ("developer-products/icons", "developerProductIds"),
        _ => ("assets", "assetIds"),
    };
    format!("https://thumbnails.roblox.com/v1/{route}?{ids}={}&size={}", query_text(id), query_text(size))
}

/// A query value: unreserved characters as they are, the rest percent-encoded
fn query_text(value: &str) -> String {
    value.bytes().map(|byte| if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) { (byte as char).to_string() } else { format!("%{byte:02X}") }).collect()
}

// Studio reads

/// What the Studio read stored: the ids now cached, and why Studio could not give each other one
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StudioReads {
    pub stored: HashSet<String>,
    pub errors: HashMap<String, String>,
}

/// Who reads refused images from Studio
enum Studio {
    /// Nobody (`assets fetch --no-studio`)
    Off,
    /// The checkout's `node cli/studio-images.mjs store`, when a bridge is configured
    Node { repo: PathBuf },
    #[cfg(test)]
    Fake(Box<dyn Fn(&[String]) -> Result<StudioReads, String> + Send + Sync>),
}

// The downloader

type Shared = Arc<(Mutex<Option<Result<Fetched, String>>>, Condvar)>;

/// Downloads in flight in this process, by cache path stem: a second request for the same asset waits
/// for the first one's answer
fn in_flight() -> &'static Mutex<HashMap<PathBuf, Shared>> {
    static IN_FLIGHT: OnceLock<Mutex<HashMap<PathBuf, Shared>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(Default::default)
}

/// The download of one asset in flight: when it ends (also by a panic), the requests waiting for it get
/// its answer and the asset can be asked for again
struct Flight {
    key: PathBuf,
    shared: Shared,
    answer: Option<Result<Fetched, String>>,
}

impl Drop for Flight {
    fn drop(&mut self) {
        if let Ok(mut flights) = in_flight().lock() {
            flights.remove(&self.key);
        }
        let (answer, ready) = &*self.shared;
        if let Ok(mut answer) = answer.lock() {
            *answer = Some(self.answer.take().unwrap_or_else(|| Err("the download stopped".into())));
        }
        ready.notify_all();
    }
}

/// Where an asset delivery request goes and what it carries
struct Target {
    url: String,
    headers: Vec<(&'static str, String)>,
    broker: bool,
}

pub struct Downloader {
    assets: PathBuf,
    transport: Arc<dyn Transport>,
    account: OnceLock<Account>,
    load_account: Box<dyn Fn() -> Account + Send + Sync>,
    studio: Studio,
    /// Roblox's HTTP cache (rbx-storage), where the Blocked placeholders are found
    storage: Option<PathBuf>,
}

impl Downloader {
    /// The downloader of `repo`'s asset cache (`<state dir>/assets`), over the network. The account is
    /// read at the first request that needs it, once.
    pub fn new(repo: &Path) -> Self {
        let checkout = (!crate::paths::is_standalone()).then(|| repo.to_path_buf());
        let local = std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty()).map(PathBuf::from).or_else(|| {
            let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
            Some(PathBuf::from(home).join("AppData").join("Local"))
        });
        Self {
            assets: crate::state_dir(repo).join("assets"),
            transport: Arc::new(Network),
            account: OnceLock::new(),
            load_account: Box::new(move || Account::load(&|name| std::env::var(name).ok(), checkout.as_deref())),
            studio: Studio::Node { repo: repo.to_path_buf() },
            storage: local.map(|local| local.join("Roblox").join("rbx-storage")),
        }
    }

    /// Reads refused images from Studio (when a bridge is configured), or never
    pub fn set_studio(&mut self, enabled: bool) {
        if !enabled {
            self.studio = Studio::Off;
        }
    }

    pub fn account(&self) -> &Account {
        self.account.get_or_init(|| (self.load_account)())
    }

    /// "broker", "cookie", or None (public assets only)
    pub fn account_access(&self) -> Option<&'static str> {
        self.account().access()
    }

    fn cached(&self, key: &str) -> Option<PathBuf> {
        if !self.assets.is_dir() {
            return None;
        }
        crate::resolve::cached_file(&self.assets, key)
    }

    /// Whether fetch_asset_id would download `id`: nothing cached under it, or a file rur did not download
    fn needs_download(&self, id: &str) -> bool {
        match self.cached(id) {
            Some(file) => std::fs::read(&file).is_ok_and(|bytes| !crate::resolve::downloaded(&file, &bytes)),
            None => true,
        }
    }

    /// GET `url`, following up to `redirects` redirects. A redirect never carries a header (so never the
    /// cookie or the broker's token). Plain http is only the broker on this machine (`plain_http`).
    fn get(&self, url: &str, headers: Vec<(&'static str, String)>, redirects: u32, plain_http: bool) -> Result<Response, TransportError> {
        if !is_https(url) && !plain_http {
            return Err(TransportError { message: format!("refusing a plain http request to {}", host_of(url)), code: None });
        }
        let mut all = vec![("User-Agent", USER_AGENT.to_string())];
        all.extend(headers);
        let response = self.transport.send(&Request { method: Method::Get, url: url.to_string(), headers: all, body: Vec::new() })?;
        if (300..400).contains(&response.status) && redirects > 0 {
            if let Some(location) = response.header("location").filter(|location| !location.is_empty()) {
                return self.get(&join_url(url, location), Vec::new(), redirects - 1, false);
            }
        }
        Ok(response)
    }

    fn post(&self, target: &Target, body: &str, extra: Option<(&'static str, String)>) -> Result<Response, TransportError> {
        let mut headers = vec![("User-Agent", USER_AGENT.to_string()), ("Content-Type", "application/json".to_string())];
        headers.extend(target.headers.iter().cloned());
        headers.extend(extra);
        self.transport.send(&Request { method: Method::Post, url: target.url.clone(), headers, body: body.as_bytes().to_vec() })
    }

    /// Without the account: asset delivery itself, bare. With it: the same route behind the broker's
    /// `/rbx/<host>` with the token (the broker adds the cookie and answers the CSRF challenge), else asset
    /// delivery with the cookie, else None. An error for a broker address that is refused.
    fn delivery_target(&self, route: &str, with_account: bool) -> Result<Option<Target>, String> {
        let url = format!("https://{DELIVERY_HOST}{route}");
        if !with_account {
            return Ok(Some(Target { url, headers: Vec::new(), broker: false }));
        }
        Ok(match self.account() {
            Account::BrokerRefused(error) => return Err(error.clone()),
            Account::Broker { url: base, token } => {
                Some(Target { url: format!("{base}/rbx/{DELIVERY_HOST}{route}"), headers: vec![("Authorization", format!("Bearer {token}"))], broker: true })
            }
            Account::Cookie(cookie) => Some(Target { url, headers: vec![("Cookie", format!(".ROBLOSECURITY={cookie}"))], broker: false }),
            Account::Public => None,
        })
    }

    /// Why the broker itself refused (an X-Broker-Error answer: signed out, token invalid, rate limited...),
    /// in its own words with the token masked, or None for Roblox's answer passed through
    fn broker_refusal(&self, response: &Response) -> Option<String> {
        let code = response.header("x-broker-error")?;
        let message = serde_json::from_slice::<Value>(&response.body).ok().and_then(|body| body["message"].as_str().filter(|message| !message.is_empty()).map(str::to_string));
        let text = message.unwrap_or_else(|| format!("{code} (HTTP {})", response.status));
        let text = match self.account() {
            Account::Broker { token, .. } if !token.is_empty() => text.replace(token.as_str(), "<token>"),
            _ => text,
        };
        Some(format!("the account broker refused: {text}"))
    }

    /// Where asset delivery has `id`: the public route first, then with the account
    fn delivery_location(&self, id: &str) -> Result<String, String> {
        let route = format!("/v1/assetId/{id}");
        let (mut last_status, mut last_error, mut account_tried, mut refusal) = (0i64, None::<Value>, false, None::<String>);
        for with_account in [false, true] {
            let target = match self.delivery_target(&route, with_account) {
                Ok(Some(target)) => target,
                Ok(None) => continue,
                Err(error) => {
                    refusal = Some(error);
                    break;
                }
            };
            account_tried |= with_account;
            last_error = None;
            let response = match self.get(&target.url, target.headers, 0, target.broker) {
                Ok(response) => response,
                Err(error) => {
                    last_status = -1;
                    if target.broker {
                        refusal = Some(format!("the account broker did not answer{}", error.code.map(|code| format!(" ({code})")).unwrap_or_default()));
                    }
                    continue;
                }
            };
            last_status = response.status.into();
            if target.broker {
                if let Some(text) = self.broker_refusal(&response) {
                    refusal = Some(text);
                    break;
                }
            }
            if let Ok(payload) = serde_json::from_slice::<Value>(&response.body) {
                if let (true, Some(location)) = (response.ok(), payload["location"].as_str()) {
                    return Ok(location.to_string());
                }
                if let Some(first) = payload["errors"].get(0).filter(|first| first.is_object()) {
                    last_error = Some(first.clone());
                }
            }
        }
        if let Some(refusal) = refusal {
            return Err(format!("asset {id} unavailable from asset delivery ({refusal})"));
        }
        if let Some(error) = last_error {
            // [code, message].filter(part => part !== undefined && part !== "").join(": ")
            let detail = ["code", "message"]
                .iter()
                .filter_map(|key| match error.get(*key)? {
                    Value::String(text) if text.is_empty() => None,
                    Value::String(text) => Some(text.clone()),
                    Value::Null => Some(String::new()),
                    Value::Number(number) => Some(number.as_i64().map(|value| value.to_string()).unwrap_or_else(|| crate::glyphs::js_number_string(number.as_f64().unwrap_or(f64::NAN)))),
                    value => Some(value.to_string()),
                })
                .collect::<Vec<_>>()
                .join(": ");
            let private = matches!(error["code"].as_f64(), Some(code) if code == 401.0 || code == 403.0);
            let hint = if private && !account_tried { "; private asset, no asset cookie is configured" } else { "" };
            let detail = if detail.is_empty() { format!("HTTP {last_status}") } else { detail };
            return Err(format!("asset {id} unavailable from asset delivery ({detail}{hint})"));
        }
        Err(format!("asset {id} unavailable from asset delivery (HTTP {last_status})"))
    }

    /// Where many assets are, from asset delivery's batch endpoint: up to 100 per request, through the
    /// broker or with the cookie when there is one (so a private asset costs no second trip). Each asset
    /// through delivery_location costs a round trip (two for a private one) before its download, so a UI
    /// of many images waited on lookups. An id it does not place is left out; its download asks
    /// delivery_location, which reports why.
    fn delivery_locations(&self, ids: &[String]) -> HashMap<String, String> {
        const ROUTE: &str = "/v1/assets/batch";
        let target = match self.delivery_target(ROUTE, true) {
            Ok(Some(target)) => target,
            Ok(None) => match self.delivery_target(ROUTE, false) {
                Ok(Some(target)) => target,
                _ => return HashMap::new(),
            },
            Err(_) => return HashMap::new(),
        };
        let chunks: Vec<&[String]> = ids.chunks(BATCH).collect();
        if chunks.len() == 1 {
            return self.locate(&target, chunks[0]);
        }
        let found = Mutex::new(HashMap::new());
        std::thread::scope(|scope| {
            for chunk in &chunks {
                let (target, found) = (&target, &found);
                scope.spawn(move || {
                    let located = self.locate(target, chunk);
                    found.lock().unwrap().extend(located);
                });
            }
        });
        found.into_inner().unwrap()
    }

    fn locate(&self, target: &Target, chunk: &[String]) -> HashMap<String, String> {
        let mut found = HashMap::new();
        // assetId as written: ids are digit strings, and some are past a double's integers
        let body = format!("[{}]", chunk.iter().enumerate().map(|(index, id)| format!(r#"{{"requestId":"{index}","assetId":{id}}}"#)).collect::<Vec<_>>().join(","));
        let Ok(mut response) = self.post(target, &body, None) else { return found };
        // A request with the cookie is answered once with the CSRF token it must carry (the broker answers its own)
        let token = (!target.broker).then(|| response.header("x-csrf-token").filter(|token| !token.is_empty()).map(str::to_string)).flatten();
        if let (403, Some(token)) = (response.status, token) {
            match self.post(target, &body, Some(("X-CSRF-TOKEN", token))) {
                Ok(again) => response = again,
                Err(_) => return found,
            }
        }
        if response.status != 200 {
            return found;
        }
        let Ok(Value::Array(replies)) = serde_json::from_slice::<Value>(&response.body) else { return found };
        for (position, reply) in replies.iter().enumerate() {
            // Number(reply.requestId ?? position)
            let index = match &reply["requestId"] {
                Value::Null => Some(position as f64),
                Value::String(text) if text.trim().is_empty() => Some(0.0),
                Value::String(text) => text.trim().parse::<f64>().ok(),
                Value::Number(number) => number.as_f64(),
                Value::Bool(value) => Some(f64::from(u8::from(*value))),
                _ => None,
            };
            let id = index.filter(|index| index.fract() == 0.0 && *index >= 0.0).and_then(|index| chunk.get(index as usize));
            if let (Some(id), Some(location)) = (id, reply["location"].as_str().filter(|location| !location.is_empty())) {
                found.insert(id.clone(), location.to_string());
            }
        }
        found
    }

    /// The cloud-font index (`assets/fonts.json`) the runtime preloads. Several processes download at once
    /// (a parallel sweep), so it is merged with the current file and the directory right before writing
    /// and written through a temporary file + rename: a reader never sees a half-written file, and a
    /// writer never drops the entries another process added (a read of a half-written index once reset
    /// it to a single entry).
    fn record_font(&self, file: &Path) {
        let index = self.assets.join("fonts.json");
        let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let mut list = std::fs::read(&index).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok()).and_then(|list| list.as_array().cloned()).unwrap_or_default();
        if list.iter().any(|entry| entry.as_str() == Some(name.as_str())) {
            return;
        }
        // The cached faces, from the directory itself (the index can lag behind it)
        let cached = std::fs::read_dir(&self.assets).into_iter().flatten().flatten().filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let (stem, ext) = name.rsplit_once('.')?;
            let font = !stem.is_empty() && stem.bytes().all(|byte| byte.is_ascii_digit()) && ["ttf", "otf", "ttc"].contains(&ext.to_ascii_lowercase().as_str());
            font.then_some(name)
        });
        for entry in cached.chain(std::iter::once(name)) {
            let entry = Value::String(entry);
            if !list.contains(&entry) {
                list.push(entry);
            }
        }
        let mut seen = Vec::new();
        list.retain(|entry| if seen.contains(entry) { false } else { seen.push(entry.clone()); true });
        let text = serde_json::to_string_pretty(&Value::Array(list)).unwrap_or_default();
        let _ = write_atomic(&index, text.as_bytes());
    }

    /// Downloads (or reads from the cache) an asset id: a decal / texture model resolves to the image it
    /// names when `allow_decal`. `location`: where delivery_locations found it, tried before asking asset
    /// delivery again.
    pub fn fetch_asset_id(&self, asset_id: &str, allow_decal: bool, offline: bool, location: Option<&str>) -> Result<Fetched, String> {
        let id: String = asset_id.chars().filter(char::is_ascii_digit).collect();
        if id.is_empty() {
            return Err(format!("invalid asset id {asset_id}"));
        }
        let mut cached = self.cached(&id);
        if let Some(file) = cached.clone().filter(|file| file.extension().is_some_and(|ext| ext == "bin")) {
            // Cached before gzip bodies were unwrapped: the real file under its own extension
            let body = unwrap(std::fs::read(&file).map_err(|error| error.to_string())?);
            let sniffed = sniff(&body);
            if sniffed.ext != ".bin" {
                let target = self.assets.join(format!("{id}{}", sniffed.ext));
                write_atomic(&target, &body)?;
                let _ = std::fs::remove_file(&file);
                if sniffed.kind == "font" {
                    self.record_font(&target);
                }
                cached = Some(target);
            }
        }
        let cached_bytes = match &cached {
            Some(file) => Some(std::fs::read(file).map_err(|error| error.to_string())?),
            None => None,
        };
        if let (Some(file), Some(bytes)) = (&cached, &cached_bytes) {
            if offline || crate::resolve::downloaded(file, bytes) {
                let sniffed = sniff(bytes);
                // A cached font that dropped out of the index comes back
                if sniffed.kind == "font" {
                    self.record_font(file);
                }
                if sniffed.kind == "model" && allow_decal {
                    return self.model_texture(&id, file, offline);
                }
                return Ok(Fetched::new(file.clone(), sniffed));
            }
        }
        if offline {
            return Err("offline and not cached".into());
        }
        // One download per asset at a time: a second request waits for the first's answer
        let key = self.assets.join(&id);
        let (shared, first) = {
            let mut flights = in_flight().lock().unwrap();
            match flights.get(&key) {
                Some(shared) => (shared.clone(), false),
                None => {
                    let shared: Shared = Arc::new((Mutex::new(None), Condvar::new()));
                    flights.insert(key.clone(), shared.clone());
                    (shared, true)
                }
            }
        };
        if !first {
            let (answer, ready) = &*shared;
            let mut answer = answer.lock().unwrap();
            while answer.is_none() {
                answer = ready.wait(answer).unwrap();
            }
            return answer.clone().unwrap();
        }
        let mut flight = Flight { key, shared, answer: None };
        let result = self.download_asset(&id, allow_decal, location, cached, cached_bytes);
        flight.answer = Some(result.clone());
        result
    }

    fn download_asset(&self, id: &str, allow_decal: bool, location: Option<&str>, cached: Option<PathBuf>, cached_bytes: Option<Vec<u8>>) -> Result<Fetched, String> {
        let body = (|| -> Result<Vec<u8>, String> {
            let mut download = location.and_then(|location| self.get(location, Vec::new(), 5, false).ok());
            if !download.as_ref().is_some_and(Response::ok) {
                let location = self.delivery_location(id)?;
                download = Some(self.get(&location, Vec::new(), 5, false).map_err(|error| error.message)?);
            }
            let download = download.unwrap();
            if !download.ok() {
                return Err(format!("asset {id} download failed (HTTP {})", download.status));
            }
            Ok(unwrap(download.body))
        })()
        .map_err(|error| match &cached {
            Some(file) => format!(
                "{error}; the cached {} is not rur's download of it (no matching record in assets/downloaded) and is not used",
                file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
            ),
            None => error,
        })?;
        let sniffed = sniff(&body);
        std::fs::create_dir_all(&self.assets).map_err(|error| format!("creating {}: {error}", self.assets.display()))?;
        let file = self.assets.join(format!("{id}{}", sniffed.ext));
        // The same bytes keep their file (and render directories' links to it); other bytes replace it
        if cached.as_ref() != Some(&file) || cached_bytes.as_deref() != Some(&body[..]) {
            write_atomic(&file, &body)?;
        }
        if let Some(old) = cached.filter(|old| *old != file) {
            let _ = std::fs::remove_file(old);
        }
        record_download(&file, &body)?;
        if sniffed.kind == "font" {
            self.record_font(&file);
        }
        if sniffed.kind == "model" && allow_decal {
            return self.model_texture(id, &file, false);
        }
        Ok(Fetched::new(file, sniffed))
    }

    /// A decal / texture model asset (e.g. a Decal id used as an image) -> the image it references
    fn model_texture(&self, id: &str, file: &Path, offline: bool) -> Result<Fetched, String> {
        let text: String = std::fs::read(file).map_err(|error| error.to_string())?.iter().map(|&byte| byte as char).collect();
        match crate::resolve::model_texture(&text) {
            Some(inner) if inner != id => {
                let mut fetched = self.fetch_asset_id(&inner, false, offline, None)?;
                fetched.via = Some(id.to_string());
                Ok(fetched)
            }
            _ => Err(format!("asset {id} is a model without an image texture")),
        }
    }

    /// Studio's placeholder for a Blocked thumbnail of `kind`, from the asset cache, else from Roblox's
    /// HTTP cache (then kept in the asset cache)
    fn blocked_placeholder(&self, kind: &str) -> Option<(PathBuf, Vec<u8>)> {
        let &(_, md5, length) = BLOCKED_PLACEHOLDERS.iter().find(|(name, ..)| *name == kind)?;
        let file = self.assets.join(format!("placeholder-{md5}.png"));
        if file.exists() {
            return std::fs::read(&file).ok().map(|bytes| (file, bytes));
        }
        let body = storage_entry(self.storage.as_deref()?, md5, length)?;
        std::fs::create_dir_all(&self.assets).ok()?;
        write_atomic(&file, &body).ok()?;
        Some((file, body))
    }

    /// An `rbxthumb://` thumbnail (`query`: what follows the scheme). Measured (fixtures/image-probe-thumb.luau,
    /// tools/analysis/image_thumb.py): Studio draws the thumbnail service's WebP rendition (a 60 x 60
    /// headshot's texels: 100% identical to the decoded WebP, 36% to the PNG rendition, which is another
    /// encode); it is cached as PNG under a "-webp" key
    pub fn fetch_thumbnail(&self, query: &str, offline: bool) -> Result<Fetched, String> {
        let query = crate::resolve::ThumbnailQuery::parse(query);
        let key = query.key();
        if let Some(cached) = self.cached(&key) {
            let bytes = std::fs::read(&cached).map_err(|error| error.to_string())?;
            // A cache entry written before the placeholder rule holds the UnapprovedImage icon the service
            // returns for a Blocked thumbnail; Studio draws the placeholder there (17 such entries were found
            // in one cache; fixtures/image-probe-thumb-blocked.luau Blocked150)
            if md5_hex(&bytes) == UNAPPROVED_IMAGE_MD5 {
                if let Some((file, placeholder)) = self.blocked_placeholder(&query.kind) {
                    return Ok(Fetched::new(file, sniff(&placeholder)));
                }
            }
            return Ok(Fetched::new(cached, sniff(&bytes)));
        }
        if offline {
            return Err("offline and not cached".into());
        }
        let (kind, id) = (&query.kind, &query.id);
        let endpoint = thumbnail_endpoint(kind, id, &format!("{}x{}", query.w, query.h));
        let download = |format: &str| -> Result<Vec<u8>, String> {
            let meta = self.get(&format!("{endpoint}&format={format}"), Vec::new(), 5, false).map_err(|error| error.message)?;
            if !meta.ok() {
                return Err(format!("thumbnail metadata failed (HTTP {})", meta.status));
            }
            let payload: Value = serde_json::from_slice(&meta.body).map_err(|error| format!("thumbnail metadata: {error}"))?;
            let first = &payload["data"][0];
            if first["state"] == "Blocked" {
                if let Some((_, placeholder)) = self.blocked_placeholder(kind) {
                    return Ok(placeholder);
                }
            }
            let Some(image_url) = first["imageUrl"].as_str().filter(|url| !url.is_empty()) else {
                return Err(format!("no thumbnail for {kind} {id}"));
            };
            let image = self.get(image_url, Vec::new(), 5, false).map_err(|error| error.message)?;
            if !image.ok() {
                return Err(format!("thumbnail download failed (HTTP {})", image.status));
            }
            let body = unwrap(image.body);
            if sniff(&body).kind != "image" {
                return Err(format!("invalid thumbnail image for {kind} {id}"));
            }
            Ok(body)
        };
        std::fs::create_dir_all(&self.assets).map_err(|error| format!("creating {}: {error}", self.assets.display()))?;
        // Some thumbnail endpoints reject WebP; that must still reach the PNG fallback
        if let Ok(webp) = download("Webp") {
            if sniff(&webp).ext == ".webp" {
                if let Some(png) = webp_to_png(&webp) {
                    let file = self.assets.join(format!("{key}.png"));
                    write_atomic(&file, &png)?;
                    return Ok(Fetched::new(file, sniff(&png)));
                }
            }
        }
        // No WebP, or one that does not decode: the PNG rendition
        let body = download("Png")?;
        let sniffed = sniff(&body);
        let file = self.assets.join(format!("{key}{}", sniffed.ext));
        write_atomic(&file, &body)?;
        Ok(Fetched::new(file, sniffed))
    }

    /// One URI that needs the network: a thumbnail or an asset id. Local URIs (`rbxasset://`, `file://`)
    /// are the resolver's (resolve.rs).
    fn resolve_uri(&self, uri: &str, offline: bool, locations: &HashMap<String, String>) -> Result<Fetched, String> {
        let trimmed = uri.trim();
        if trimmed.to_ascii_lowercase().starts_with("rbxthumb://") {
            return self.fetch_thumbnail(&trimmed["rbxthumb://".len()..], offline);
        }
        match crate::resolve::asset_id(trimmed) {
            Some(id) if offline && self.cached(&id).is_none() => Err("offline and not cached".into()),
            Some(id) => self.fetch_asset_id(&id, true, offline, locations.get(&id).map(String::as_str)),
            None => Err(format!("unsupported content uri: {uri}")),
        }
    }

    /// Resolves many URIs, 16 at a time; the asset ids to download are located in batches first. Online,
    /// the images asset delivery refuses are then read from Studio when a bridge is configured.
    pub fn resolve_all(&self, uris: &[String], offline: bool) -> BTreeMap<String, Result<Fetched, String>> {
        let mut seen = HashSet::new();
        let queue: Vec<&String> = uris.iter().filter(|uri| seen.insert(uri.as_str())).collect();
        let ids: Vec<String> = if offline {
            Vec::new()
        } else {
            let mut seen = HashSet::new();
            queue.iter().filter_map(|uri| crate::resolve::asset_id(uri)).filter(|id| seen.insert(id.clone()) && self.needs_download(id)).collect()
        };
        let locations = if ids.len() > 1 { self.delivery_locations(&ids) } else { HashMap::new() };
        let results = Mutex::new(BTreeMap::new());
        let next = AtomicUsize::new(0);
        let work = || {
            while let Some(uri) = queue.get(next.fetch_add(1, Ordering::Relaxed)) {
                let result = self.resolve_uri(uri, offline, &locations);
                results.lock().unwrap().insert((*uri).clone(), result);
            }
        };
        if queue.len() <= 1 {
            work();
        } else {
            std::thread::scope(|scope| {
                for _ in 0..CONCURRENCY.min(queue.len()) {
                    scope.spawn(work);
                }
            });
        }
        let mut results = results.into_inner().unwrap();
        if !offline {
            self.read_refused_from_studio(&mut results);
        }
        results
    }

    /// Downloads the cloud font assets a run reported missing (diagnostics.missingFontAssets) into the asset
    /// cache, where the engine finds them on its next start. Whether any arrived: a font face, or a family's
    /// JSON (its faces are asked for on the next run). One that is unavailable keeps the run's fallback
    /// metrics and is reported as missing.
    pub fn fetch_fonts(&self, diagnostics: &Value) -> bool {
        let mut fetched = false;
        for font in diagnostics["missingFontAssets"].as_array().into_iter().flatten() {
            let id = font.as_str().map_or_else(|| font.to_string(), str::to_string);
            if let Ok(result) = self.fetch_asset_id(&id, false, false, None) {
                fetched |= result.kind == "font" || result.kind == "json";
            }
        }
        fetched
    }

    /// The asset ids asset delivery would not give (a private image), read from an open Studio when a
    /// bridge is configured; an id Studio cannot read either keeps its error, with Studio's reason
    fn read_refused_from_studio(&self, results: &mut BTreeMap<String, Result<Fetched, String>>) {
        let refused: Vec<(String, String, String)> = results
            .iter()
            .filter_map(|(uri, result)| {
                let error = result.as_ref().err().filter(|error| !error.contains("without an image texture"))?;
                Some((uri.clone(), crate::resolve::asset_id(uri)?, error.clone()))
            })
            .collect();
        if refused.is_empty() {
            return;
        }
        let mut ids: Vec<String> = Vec::new();
        for (_, id, _) in &refused {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        let reads = match &self.studio {
            Studio::Off => return,
            Studio::Node { repo } => {
                if !studio_images_enabled(&|name| std::env::var(name).ok()) {
                    return;
                }
                self.studio_node(repo, &ids)
            }
            #[cfg(test)]
            Studio::Fake(read) => read(&ids),
        };
        let reads = match reads {
            Ok(reads) => reads,
            Err(reason) => StudioReads { stored: HashSet::new(), errors: ids.iter().map(|id| (id.clone(), format!("not read from Studio: {reason}"))).collect() },
        };
        for (uri, id, error) in refused {
            let answer = if reads.stored.contains(&id) {
                self.fetch_asset_id(&id, true, true, None).map_err(|reason| format!("{error}; read from Studio but not cached: {reason}"))
            } else if let Some(reason) = reads.errors.get(&id) {
                Err(format!("{error}; {reason}"))
            } else {
                continue;
            };
            results.insert(uri, answer);
        }
    }

    /// `node cli/studio-images.mjs store` of the checkout: reads `ids` from Studio and stores each image
    /// it gives in this cache (cli/assets.mjs storeStudioImage, recorded with "source": "studio")
    fn studio_node(&self, repo: &Path, ids: &[String]) -> Result<StudioReads, String> {
        let script = crate::paths::checkout_script(repo, "cli/studio-images.mjs")?;
        let state = self.assets.parent().unwrap_or(&self.assets);
        let mut command = Command::new(std::env::var_os("RUR_NODE").unwrap_or_else(|| "node".into()));
        command
            .arg(&script)
            .arg("store")
            .current_dir(script.parent().and_then(Path::parent).unwrap_or(repo))
            .env("RUR_STATE_DIR", state)
            .env_remove("RUR_CACHE_DIR")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // It reads Studio, never Roblox: no account
        without_account(&mut command);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().map_err(|error| format!("starting node for cli/studio-images.mjs: {error}"))?;
        if let Some(mut input) = child.stdin.take() {
            let _ = input.write_all(ids.join("\n").as_bytes());
        }
        let output = child.wait_with_output().map_err(|error| error.to_string())?;
        let reply = serde_json::from_slice::<Value>(&output.stdout).ok().filter(|_| output.status.success());
        let Some(reply) = reply else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let last = stderr.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or_default().trim().to_string();
            return Err(if last.is_empty() { format!("cli/studio-images.mjs exited with {}", output.status) } else { last });
        };
        Ok(StudioReads {
            stored: reply["stored"].as_array().into_iter().flatten().filter_map(|id| id.as_str().map(str::to_string)).collect(),
            errors: reply["errors"].as_object().into_iter().flatten().filter_map(|(id, reason)| Some((id.clone(), reason.as_str()?.to_string()))).collect(),
        })
    }
}

// The command

const USAGE: &str = "usage: rur-native assets fetch [--repo <dir>] [--offline] [--fonts] [--no-studio] [--json] [--stdin] <uri>...\n       rur-native assets account [--repo <dir>] [--json]";

/// `rur-native assets fetch <uri>...`: resolves content URIs as a render does (the asset cache, downloads,
/// Studio's install, local files, Studio reads), and prints each one's file or error. `--fonts`: the
/// arguments are cloud font asset ids (a run's missingFontAssets). `--stdin`: more URIs, one per line.
/// `--json`: an array of `{uri, file, kind, mime}` / `{uri, error}`. Exits 1 when any failed.
///
/// `rur-native assets account`: how private assets are asked for: "broker", "cookie" or "none" (`--json`:
/// `{"access": ...}`). Neither the cookie nor the token is printed.
pub fn cli(args: &[String]) -> i32 {
    let (mut repo, mut offline, mut fonts, mut studio, mut json_output, mut stdin) = (None, false, false, true, false, false);
    let mut uris = Vec::new();
    let mut words = args.iter().skip(1);
    while let Some(word) = words.next() {
        match word.as_str() {
            "--repo" => match words.next() {
                Some(value) => repo = Some(PathBuf::from(value)),
                None => {
                    eprintln!("{USAGE}");
                    return 2;
                }
            },
            value if value.starts_with("--repo=") => repo = Some(PathBuf::from(&value["--repo=".len()..])),
            "--offline" => offline = true,
            "--fonts" => fonts = true,
            "--no-studio" => studio = false,
            "--json" => json_output = true,
            "--stdin" => stdin = true,
            value if value.starts_with("--") => {
                eprintln!("rur-native assets: unknown option {value}\n{USAGE}");
                return 2;
            }
            uri => uris.push(uri.to_string()),
        }
    }
    let repo = repo.unwrap_or_else(crate::paths::default_repo);
    let mut resolver = crate::resolve::Resolver::new(&repo, offline);
    match args.first().map(String::as_str) {
        Some("account") => {
            let access = resolver.account_access();
            if json_output {
                println!("{}", json!({ "access": access }));
            } else {
                println!("{}", access.unwrap_or("none"));
            }
            0
        }
        Some("fetch") => {
            if stdin {
                let mut text = String::new();
                if std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).is_err() {
                    eprintln!("rur-native assets: reading stdin failed");
                    return 1;
                }
                uris.extend(text.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string));
            }
            resolver.downloads_mut().set_studio(studio);
            let results: Vec<(String, Result<Fetched, String>)> = if fonts {
                let downloads = resolver.downloads_mut();
                uris.iter().map(|id| (id.clone(), downloads.fetch_asset_id(id, false, offline, None))).collect()
            } else {
                let mut resolved = resolver.resolve_all(&uris);
                let mut seen = HashSet::new();
                uris.iter()
                    .filter(|uri| seen.insert(uri.as_str()))
                    .map(|uri| {
                        let answer = resolved.remove(uri).unwrap_or_else(|| Err("unresolved".into())).and_then(|file| {
                            let head = std::fs::File::open(&file).and_then(|mut handle| {
                                let mut head = vec![0; 64];
                                let read = std::io::Read::read(&mut handle, &mut head)?;
                                head.truncate(read);
                                Ok(head)
                            });
                            head.map(|head| Fetched::new(file.clone(), sniff(&head))).map_err(|error| format!("{}: {error}", file.display()))
                        });
                        (uri.clone(), answer)
                    })
                    .collect()
            };
            let failed = results.iter().filter(|(_, result)| result.is_err()).count();
            if json_output {
                let rows: Vec<Value> = results
                    .iter()
                    .map(|(uri, result)| match result {
                        Ok(fetched) => json!({ "uri": uri, "file": fetched.file.to_string_lossy(), "kind": fetched.kind, "mime": fetched.mime }),
                        Err(error) => json!({ "uri": uri, "error": error }),
                    })
                    .collect();
                println!("{}", Value::Array(rows));
            } else {
                for (uri, result) in &results {
                    match result {
                        Ok(fetched) => println!("{uri}\t{}", fetched.file.display()),
                        Err(error) => println!("{uri}\terror: {error}"),
                    }
                }
            }
            i32::from(failed > 0)
        }
        _ => {
            eprintln!("{USAGE}");
            2
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A request as a test sees it
    #[derive(Clone, Debug)]
    pub(crate) struct Seen {
        pub method: Method,
        pub url: String,
        pub cookie: Option<String>,
        pub authorization: Option<String>,
        pub csrf: Option<String>,
        pub content_type: Option<String>,
        pub user_agent: Option<String>,
        pub body: String,
    }

    type Answer = Box<dyn Fn(&Seen) -> Result<Response, String> + Send + Sync>;

    /// A transport that answers from a function and remembers every request
    pub(crate) struct Fake {
        answer: Mutex<Answer>,
        seen: Mutex<Vec<Seen>>,
    }

    impl Fake {
        pub fn new(answer: impl Fn(&Seen) -> Result<Response, String> + Send + Sync + 'static) -> Arc<Fake> {
            Arc::new(Fake { answer: Mutex::new(Box::new(answer)), seen: Mutex::new(Vec::new()) })
        }
        pub fn answer(&self, answer: impl Fn(&Seen) -> Result<Response, String> + Send + Sync + 'static) {
            *self.answer.lock().unwrap() = Box::new(answer);
        }
        pub fn seen(&self) -> Vec<Seen> {
            self.seen.lock().unwrap().clone()
        }
        pub fn urls(&self) -> Vec<String> {
            self.seen().into_iter().map(|seen| seen.url).collect()
        }
        pub fn clear(&self) {
            self.seen.lock().unwrap().clear();
        }
    }

    impl Transport for Fake {
        fn send(&self, request: &Request) -> Result<Response, TransportError> {
            let header = |name: &str| request.header(name).map(str::to_string);
            let seen = Seen {
                method: request.method,
                url: request.url.clone(),
                cookie: header("Cookie"),
                authorization: header("Authorization"),
                csrf: header("X-CSRF-TOKEN"),
                content_type: header("Content-Type"),
                user_agent: header("User-Agent"),
                body: String::from_utf8_lossy(&request.body).into_owned(),
            };
            self.seen.lock().unwrap().push(seen.clone());
            (self.answer.lock().unwrap())(&seen).map_err(|message| TransportError { message, code: Some("ECONNREFUSED") })
        }
    }

    pub(crate) fn ok(body: impl Into<Vec<u8>>) -> Result<Response, String> {
        Ok(Response { status: 200, headers: Vec::new(), body: body.into() })
    }

    pub(crate) fn status(status: u16, headers: &[(&str, &str)], body: impl Into<Vec<u8>>) -> Result<Response, String> {
        Ok(Response { status, headers: headers.iter().map(|(name, value)| (name.to_ascii_lowercase(), value.to_string())).collect(), body: body.into() })
    }

    /// A 1 x 1 PNG (the Node tests' image)
    pub(crate) fn png() -> Vec<u8> {
        let mut png = Vec::new();
        image::ImageEncoder::write_image(image::codecs::png::PngEncoder::new(&mut png), &[255, 0, 0, 255], 1, 1, image::ExtendedColorType::Rgba8).unwrap();
        png
    }

    fn path(url: &str) -> &str {
        let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
        rest.find('/').map_or("/", |at| &rest[at..])
    }

    impl Downloader {
        /// A downloader over `assets` through `transport`, with no account (Account::Public), no Studio
        /// reads and no Roblox HTTP cache
        pub(crate) fn for_tests(assets: &Path, transport: Arc<dyn Transport>) -> Self {
            Self {
                assets: assets.to_path_buf(),
                transport,
                account: OnceLock::new(),
                load_account: Box::new(|| Account::Public),
                studio: Studio::Off,
                storage: None,
            }
        }

        fn with_account(self, account: Account) -> Self {
            let account_cell = OnceLock::new();
            let _ = account_cell.set(account);
            Self { account: account_cell, ..self }
        }
    }

    /// A temporary folder, removed when dropped
    struct Temp(PathBuf);

    impl Temp {
        fn new(name: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!("rur-download-{name}-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Temp(dir)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect();
        move |name: &str| map.get(name).cloned()
    }

    fn sha256(bytes: &[u8]) -> String {
        crate::engine::text::hinted::sha256_hex(bytes)
    }

    fn record(assets: &Path, name: &str) -> Value {
        serde_json::from_slice(&std::fs::read(assets.join("downloaded").join(format!("{name}.json"))).unwrap()).unwrap()
    }

    /// The former tools/tests/asset_cookie.mjs: where the cookie comes from and in which order, how its files are read,
    /// that it is read once, and that only asset delivery's lookup with the account carries it
    #[test]
    fn the_cookie_comes_from_its_sources_in_order() {
        let temp = Temp::new("cookie");
        let checkout = temp.0.join("checkout").join("renderer");
        std::fs::create_dir_all(&checkout).unwrap();
        let local = checkout.join(".env");
        let override_file = temp.0.join("override.env").to_string_lossy().into_owned();
        let set = |file: &Path, text: Option<&str>| match text {
            Some(text) => {
                std::fs::create_dir_all(file.parent().unwrap()).unwrap();
                std::fs::write(file, text).unwrap();
            }
            None => {
                let _ = std::fs::remove_file(file);
            }
        };
        let case = |env: &[(&str, &str)], local_text: Option<&str>, override_text: Option<&str>| -> Option<String> {
            set(&local, local_text);
            set(Path::new(&override_file), override_text);
            match Account::load(&env_of(env), Some(&checkout)) {
                Account::Cookie(cookie) => Some(cookie),
                Account::Public => None,
                other => panic!("no broker is configured: {other:?}"),
            }
        };
        let cookie = |value: &str| Some(value.to_string());
        let env_both = [("ROBLOX_ASSET_DELIVERY_COOKIE", "dummy-inherited"), ("ROBLOSECURITY", "dummy-inherited-alias"), ("RUR_COOKIE_ENV", override_file.as_str())];
        // The checkout's .env: comments, quotes, export, the alias and literal text
        assert_eq!(case(&[], Some("ROBLOX_ASSET_DELIVERY_COOKIE=dummy-local # comment\n"), None), cookie("dummy-local"));
        assert_eq!(case(&[], Some("export ROBLOX_ASSET_DELIVERY_COOKIE=\".ROBLOSECURITY=dummy-quoted\" # comment\r\n"), None), cookie("dummy-quoted"));
        assert_eq!(case(&[], Some("ROBLOSECURITY='dummy-alias'\n"), None), cookie("dummy-alias"));
        assert_eq!(case(&[], Some("ROBLOX_ASSET_DELIVERY_COOKIE=\"dummy-$(literal)-${UNEXPANDED}\"\n"), None), cookie("dummy-$(literal)-${UNEXPANDED}"));
        // The checkout's .env comes first, then the environment, then RUR_COOKIE_ENV's file
        assert_eq!(case(&env_both, Some("ROBLOX_ASSET_DELIVERY_COOKIE=dummy-local"), Some("ROBLOX_ASSET_DELIVERY_COOKIE=dummy-override")), cookie("dummy-local"));
        assert_eq!(case(&[("ROBLOSECURITY", ".ROBLOSECURITY=dummy-inherited-alias")], Some("ROBLOX_ASSET_DELIVERY_COOKIE=dummy-local\r\n"), None), cookie("dummy-local"));
        assert_eq!(case(&[("ROBLOX_ASSET_DELIVERY_COOKIE", "dummy-inherited")], Some("ROBLOSECURITY='dummy-local-alias'\n"), None), cookie("dummy-local-alias"));
        assert_eq!(case(&[("RUR_COOKIE_ENV", override_file.as_str())], Some("ROBLOX_ASSET_DELIVERY_COOKIE=dummy-local"), Some("ROBLOX_ASSET_DELIVERY_COOKIE=dummy-override")), cookie("dummy-local"));
        assert_eq!(case(&env_both, None, Some("ROBLOX_ASSET_DELIVERY_COOKIE=dummy-override")), cookie("dummy-inherited"));
        assert_eq!(case(&[("ROBLOSECURITY", ".ROBLOSECURITY=dummy-inherited-alias")], None, None), cookie("dummy-inherited-alias"));
        // Empty, unrelated and unterminated entries are skipped
        assert_eq!(case(&[("ROBLOX_ASSET_DELIVERY_COOKIE", "dummy-inherited")], Some("ROBLOX_ASSET_DELIVERY_COOKIE=\r\nROBLOSECURITY=''\r\n"), None), cookie("dummy-inherited"));
        assert_eq!(case(&[("ROBLOX_ASSET_DELIVERY_COOKIE", "dummy-inherited")], Some("# cookie not configured\nOTHER_OPTION=dummy-unrelated\n"), None), cookie("dummy-inherited"));
        assert_eq!(case(&[("ROBLOX_ASSET_DELIVERY_COOKIE", "dummy-inherited")], Some("ROBLOX_ASSET_DELIVERY_COOKIE=\"unterminated\n"), None), cookie("dummy-inherited"));
        let alias_over_file = [("ROBLOX_ASSET_DELIVERY_COOKIE", ""), ("ROBLOSECURITY", "dummy-inherited-alias"), ("RUR_COOKIE_ENV", override_file.as_str())];
        assert_eq!(case(&alias_over_file, None, Some("ROBLOX_ASSET_DELIVERY_COOKIE=dummy-override")), cookie("dummy-inherited-alias"));
        assert_eq!(case(&[("RUR_COOKIE_ENV", override_file.as_str())], None, Some("ROBLOX_ASSET_DELIVERY_COOKIE=dummy-override")), cookie("dummy-override"));
        assert_eq!(case(&[], None, None), None);
        // A release build reads no checkout .env
        set(&local, Some("ROBLOX_ASSET_DELIVERY_COOKIE=dummy-local"));
        assert_eq!(Account::load(&env_of(&[]), None), Account::Public);

        // Read once: a downloader keeps the account it read first
        let fake = Fake::new(|seen| match seen.url.as_str() {
            url if url.starts_with("https://assetdelivery.roblox.com/") && seen.cookie.is_none() => status(403, &[], ""),
            url if url.starts_with("https://assetdelivery.roblox.com/") => ok(r#"{"location":"https://cdn.example/image"}"#),
            "https://cdn.example/image" => status(302, &[("location", "https://other.example/image")], ""),
            _ => ok(png()),
        });
        let assets = temp.0.join("state").join("assets");
        let mut downloader = Downloader::for_tests(&assets, fake.clone());
        let (env_checkout, env_local) = (checkout.clone(), local.clone());
        downloader.load_account = Box::new(move || Account::load(&env_of(&[]), Some(&env_checkout)));
        assert_eq!(downloader.account_access(), Some("cookie"));
        std::fs::write(&env_local, "ROBLOX_ASSET_DELIVERY_COOKIE=dummy-changed").unwrap();
        let fetched = downloader.fetch_asset_id("1", true, false, None).unwrap();
        let seen = fake.seen();
        assert_eq!(seen.len(), 4);
        assert_eq!(seen[0].cookie, None, "public delivery is attempted first");
        assert_eq!(seen[1].url, "https://assetdelivery.roblox.com/v1/assetId/1");
        assert_eq!(seen[1].cookie.as_deref(), Some(".ROBLOSECURITY=dummy-local"), "the cookie read first is kept");
        assert!(seen[2..].iter().all(|seen| seen.cookie.is_none()), "CDN and redirects must receive no cookie");
        assert!(seen.iter().all(|seen| seen.user_agent.as_deref() == Some(USER_AGENT)));
        assert_eq!(fetched.mime, "image/png");
        assert!(!format!("{fetched:?}").contains("dummy-local"), "result metadata must not contain the cookie");
        assert_eq!(std::fs::read(&fetched.file).unwrap(), png(), "only asset bytes enter the download cache");
        assert!(!format!("{:?}", downloader.account()).contains("dummy"), "the account's Debug never shows the cookie");
        // Without a cookie: the public refusal, and no second request
        fake.clear();
        let public = Downloader::for_tests(&assets, fake.clone());
        assert_eq!(public.account_access(), None);
        let refused = public.fetch_asset_id("2", true, false, None).unwrap_err();
        assert!(refused.contains("HTTP 403"), "{refused}");
        assert_eq!(fake.seen().len(), 1);
    }

    /// The former tools/tests/asset_broker.mjs: an account broker replaces the cookie
    #[test]
    fn an_account_broker_replaces_the_cookie() {
        let temp = Temp::new("broker");
        let home = temp.0.join("home");
        let home_file = home.join(".config").join("roblox-broker").join("credentials.json");
        let named_file = temp.0.join("named.json");
        let home_text = home.to_string_lossy().into_owned();
        let named_text = named_file.to_string_lossy().into_owned();
        let set = |file: &Path, text: Option<&str>| match text {
            Some(text) => {
                std::fs::create_dir_all(file.parent().unwrap()).unwrap();
                std::fs::write(file, text).unwrap();
            }
            None => {
                let _ = std::fs::remove_file(file);
            }
        };
        let named = r#"{ "url": "https://named.example/", "token": "dummy-named-token", "name": "ignored" }"#;
        let saved = r#"{ "url": "https://saved.example/base", "token": "dummy-saved-token" }"#;
        let account = |env: &[(&str, &str)], named_text_value: Option<&str>, saved_text: Option<&str>| {
            set(&named_file, named_text_value);
            set(&home_file, saved_text);
            let mut env: Vec<(&str, &str)> = env.to_vec();
            env.extend([("USERPROFILE", home_text.as_str()), ("HOME", home_text.as_str())]);
            Account::load(&env_of(&env), None)
        };
        let broker = |url: &str, token: &str| Account::Broker { url: url.into(), token: token.into() };
        let from_env = [("ROBLOX_BROKER_URL", "https://env.example"), ("ROBLOX_BROKER_TOKEN", "dummy-env-token")];
        let cookie = ("ROBLOX_ASSET_DELIVERY_COOKIE", "dummy-cookie");
        // Discovery: the environment pair, then the named file, then the one under the home folder; a
        // cookie that is also configured is never used
        let credentials = ("ROBLOX_BROKER_CREDENTIALS", named_text.as_str());
        assert_eq!(account(&[from_env[0], from_env[1], cookie, credentials], Some(named), Some(saved)), broker("https://env.example", "dummy-env-token"));
        assert_eq!(account(&[cookie, credentials], Some(named), Some(saved)), broker("https://named.example", "dummy-named-token"));
        assert_eq!(account(&[cookie], Some(named), Some(saved)), broker("https://saved.example/base", "dummy-saved-token"));
        // A token without an address (or the reverse) in the environment is not a pair: the file is used
        assert_eq!(account(&[from_env[1]], None, Some(saved)), broker("https://saved.example/base", "dummy-saved-token"));
        assert_eq!(account(&[from_env[0]], None, Some(saved)), broker("https://saved.example/base", "dummy-saved-token"));
        // A named file that is absent or incomplete does not fall through to the home folder's
        for content in [None, Some("not json"), Some(r#"{ "url": "https://named.example" }"#), Some(r#"{ "token": "dummy-named-token" }"#)] {
            let found = account(&[credentials], content, Some(saved));
            assert_eq!(found.access(), None, "{content:?}");
        }
        // No broker: the cookie, as before
        assert_eq!(account(&[cookie], None, None).access(), Some("cookie"));
        // The address: https, or http to this machine only
        assert_eq!(account(&[("ROBLOX_BROKER_URL", "http://127.0.0.1:18080/"), from_env[1]], None, None), broker("http://127.0.0.1:18080", "dummy-env-token"));
        assert_eq!(account(&[], None, Some(r#"{ "url": "http://localhost:18080", "token": "dummy-saved-token" }"#)), broker("http://localhost:18080", "dummy-saved-token"));
        assert_eq!(broker_address("HTTPS://Env.Example:443/a/b//").unwrap(), "https://env.example/a/b");
        for (url, reason) in [
            ("http://broker.example", "must be https (http only for 127.0.0.1 or localhost), not http://broker.example"),
            ("http://127.0.0.1.example", "must be https"),
            ("ftp://127.0.0.1", "must be https (http only for 127.0.0.1 or localhost), not ftp://127.0.0.1"),
            ("broker.example", "is not a URL"),
        ] {
            let refused = account(&[("ROBLOX_BROKER_URL", url), from_env[1], cookie], None, None);
            assert!(matches!(&refused, Account::BrokerRefused(error) if error.contains(reason)), "{url}: {refused:?}");
            assert_eq!(refused.access(), None);
        }

        // The requests: asset delivery refuses every bare request (the assets are private); the broker places
        // what it is asked for on the CDN unless the test gives its answer; the CDN serves the image, once
        // through a redirect
        let broker_answer: Arc<Mutex<Option<Result<Response, String>>>> = Arc::default();
        let answer = broker_answer.clone();
        let fake = Fake::new(move |seen| {
            let host = host_of(&seen.url);
            let route = path(&seen.url);
            if host == DELIVERY_HOST {
                return status(401, &[], r#"{"errors":[{"code":0,"message":"Authentication required to access Asset."}]}"#);
            }
            if host == "cdn.example" {
                let elsewhere = format!("https://other.example{route}");
                return if route.ends_with('7') { status(302, &[("location", elsewhere.as_str())], "") } else { ok(png()) };
            }
            if host == "other.example" {
                return ok(png());
            }
            if let Some(answer) = answer.lock().unwrap().clone() {
                return answer;
            }
            if seen.method == Method::Post {
                let entries: Vec<Value> = serde_json::from_str(&seen.body).unwrap();
                let replies: Vec<Value> = entries.iter().map(|entry| json!({"requestId": entry["requestId"], "location": format!("https://cdn.example/{}", entry["assetId"])})).collect();
                return ok(Value::Array(replies).to_string());
            }
            ok(json!({"location": format!("https://cdn.example/{}", route.rsplit('/').next().unwrap())}).to_string())
        });
        let assets = temp.0.join("state").join("assets");
        let through = |account: Account| Downloader::for_tests(&assets, fake.clone()).with_account(account);
        let check = |downloader: &Downloader, id: &str, base: &str, token: &str| {
            fake.clear();
            let fetched = downloader.resolve_uri(&format!("rbxassetid://{id}"), false, &HashMap::new()).unwrap();
            assert_eq!(std::fs::read(&fetched.file).unwrap(), png());
            let seen = fake.seen();
            assert_eq!(
                seen.iter().map(|seen| format!("{:?} {}", seen.method, seen.url)).collect::<Vec<_>>(),
                [
                    format!("Get https://assetdelivery.roblox.com/v1/assetId/{id}"),
                    format!("Get {base}/rbx/assetdelivery.roblox.com/v1/assetId/{id}"),
                    format!("Get https://cdn.example/{id}"),
                ]
            );
            assert_eq!(seen.iter().map(|seen| seen.authorization.clone()).collect::<Vec<_>>(), [None, Some(format!("Bearer {token}")), None], "only the broker receives the token");
            assert!(seen.iter().all(|seen| seen.cookie.is_none()), "no Cookie header in broker mode");
            assert!(!format!("{fetched:?}").contains(token));
        };
        check(&through(broker("https://env.example", "dummy-env-token")), "110", "https://env.example", "dummy-env-token");
        check(&through(broker("https://saved.example/base", "dummy-saved-token")), "120", "https://saved.example/base", "dummy-saved-token");
        check(&through(broker("http://127.0.0.1:18080", "dummy-env-token")), "130", "http://127.0.0.1:18080", "dummy-env-token");
        // No broker: the cookie, as before
        fake.clear();
        let _ = through(Account::Cookie("dummy-cookie".into())).resolve_uri("rbxassetid://91", false, &HashMap::new());
        let seen = fake.seen();
        assert_eq!(
            seen[..2].iter().map(|seen| (seen.url.as_str(), seen.cookie.as_deref(), seen.authorization.as_deref())).collect::<Vec<_>>(),
            [("https://assetdelivery.roblox.com/v1/assetId/91", None, None), ("https://assetdelivery.roblox.com/v1/assetId/91", Some(".ROBLOSECURITY=dummy-cookie"), None)]
        );

        // The token never follows a redirect, from the CDN or from the broker itself
        let env_broker = || through(broker("https://env.example", "dummy-env-token"));
        fake.clear();
        let fetched = env_broker().resolve_uri("rbxassetid://97", false, &HashMap::new()).unwrap();
        assert_eq!(std::fs::read(fetched.file).unwrap(), png());
        assert_eq!(
            fake.seen().iter().map(|seen| (host_of(&seen.url).to_string(), seen.authorization.clone())).collect::<Vec<_>>(),
            [
                ("assetdelivery.roblox.com".to_string(), None),
                ("env.example".to_string(), Some("Bearer dummy-env-token".to_string())),
                ("cdn.example".to_string(), None),
                ("other.example".to_string(), None),
            ]
        );
        fake.clear();
        *broker_answer.lock().unwrap() = Some(status(302, &[("location", "https://other.example/stolen")], ""));
        let error = env_broker().resolve_uri("rbxassetid://98", false, &HashMap::new()).unwrap_err();
        assert!(error.contains("HTTP 302"), "{error}");
        assert!(fake.urls().iter().all(|url| host_of(url) != "other.example"), "a redirect from the broker is not followed");
        *broker_answer.lock().unwrap() = None;

        // The batch lookup: one POST to the broker with the token, no cookie, and no CSRF round of its own
        fake.clear();
        let results = env_broker().resolve_all(&["rbxassetid://201".into(), "rbxassetid://202".into()], false);
        for id in ["201", "202"] {
            assert_eq!(std::fs::read(&results[&format!("rbxassetid://{id}")].as_ref().unwrap().file).unwrap(), png());
        }
        let mut lines: Vec<String> = fake.seen().iter().map(|seen| format!("{:?} {}", seen.method, seen.url)).collect();
        lines.sort();
        assert_eq!(lines, ["Get https://cdn.example/201", "Get https://cdn.example/202", "Post https://env.example/rbx/assetdelivery.roblox.com/v1/assets/batch"]);
        let seen = fake.seen();
        let batch = seen.iter().find(|seen| seen.method == Method::Post).unwrap();
        assert_eq!(
            (batch.authorization.as_deref(), batch.cookie.as_deref(), batch.csrf.as_deref(), batch.content_type.as_deref()),
            (Some("Bearer dummy-env-token"), None, None, Some("application/json"))
        );
        assert_eq!(batch.body, r#"[{"requestId":"0","assetId":201},{"requestId":"1","assetId":202}]"#);
        assert!(seen.iter().filter(|seen| seen.method == Method::Get).all(|seen| seen.authorization.is_none() && seen.cookie.is_none()));
        fake.clear();
        *broker_answer.lock().unwrap() = Some(status(403, &[("x-csrf-token", "dummy-csrf")], ""));
        env_broker().resolve_all(&["rbxassetid://211".into(), "rbxassetid://212".into()], false);
        assert_eq!(fake.seen().iter().filter(|seen| seen.method == Method::Post).count(), 1, "the broker answers the CSRF challenge: the batch is not repeated");
        assert!(fake.seen().iter().all(|seen| seen.csrf.is_none()));
        *broker_answer.lock().unwrap() = None;

        // A refused address is never contacted, says why, and does not fall back to the cookie
        for (url, reason) in [("http://broker.example", "must be https"), ("broker.example", "is not a URL")] {
            let Account::BrokerRefused(error) = account(&[("ROBLOX_BROKER_URL", url), from_env[1], cookie], None, None) else { panic!() };
            let refused = through(Account::BrokerRefused(error));
            fake.clear();
            let failed = refused.resolve_uri("rbxassetid://301", false, &HashMap::new()).unwrap_err();
            assert!(failed.contains(reason), "{failed}");
            refused.resolve_all(&["rbxassetid://302".into(), "rbxassetid://303".into()], false);
            let urls = fake.urls();
            assert!(!urls.is_empty() && urls.iter().all(|url| url.starts_with("https://assetdelivery.roblox.com/v1/assetId/")), "only the bare lookups go out: {urls:?}");
            assert!(fake.seen().iter().all(|seen| seen.authorization.is_none() && seen.cookie.is_none()));
        }

        // The broker's own refusal reads as its message, not a bare status; Roblox's answer through it is Roblox's
        let refusal = |answer: Result<Response, String>, id: &str| {
            *broker_answer.lock().unwrap() = Some(answer);
            env_broker().resolve_uri(&format!("rbxassetid://{id}"), false, &HashMap::new()).unwrap_err()
        };
        let signed_out = refusal(status(503, &[("x-broker-error", "account_signed_out")], r#"{"error":"account_signed_out","message":"the shared account is signed out; the owner has been alerted"}"#), "401");
        assert_eq!(signed_out, "asset 401 unavailable from asset delivery (the account broker refused: the shared account is signed out; the owner has been alerted)");
        let invalid = refusal(status(401, &[("x-broker-error", "token_invalid")], r#"{"error":"token_invalid","message":"token dummy-env-token is unknown or revoked"}"#), "402");
        assert!(invalid.contains("the account broker refused: token <token> is unknown or revoked"), "{invalid}");
        assert!(!invalid.contains("dummy-env-token"), "error text never contains the token");
        let limited = refusal(status(429, &[("x-broker-error", "rate_limited"), ("retry-after", "3")], "not json"), "403");
        assert!(limited.contains("the account broker refused: rate_limited (HTTP 429)"), "{limited}");
        let roblox = refusal(status(403, &[], r#"{"errors":[{"code":403,"message":"User is not authorized to access Asset."}]}"#), "404");
        assert!(roblox.ends_with("asset 404 unavailable from asset delivery (403: User is not authorized to access Asset.)"), "{roblox}");
        let silent = refusal(Err("connect refused".into()), "405");
        assert!(silent.contains("the account broker did not answer (ECONNREFUSED)"), "{silent}");
    }

    /// The former tools/tests/asset_records.mjs: the cache serves only what rur downloaded
    #[test]
    fn downloads_are_recorded_and_stand_ins_downloaded_again() {
        let temp = Temp::new("records");
        let assets = temp.0.join("state").join("assets");
        let real = png();
        let mut stand_in = real.clone();
        stand_in.extend_from_slice(b"stand-in");
        let body: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(real.clone()));
        let refuse = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (answer_body, answer_refuse) = (body.clone(), refuse.clone());
        let fake = Fake::new(move |seen| {
            assert_eq!(seen.cookie, None, "no cookie is configured");
            if seen.url.contains("/v1/assetId/") {
                return if answer_refuse.load(Ordering::Relaxed) {
                    ok(r#"{"errors":[{"code":401,"message":"Authentication required to access Asset."}]}"#)
                } else {
                    ok(r#"{"location":"https://cdn.example/hash"}"#)
                };
            }
            ok(answer_body.lock().unwrap().clone())
        });
        let downloader = Downloader::for_tests(&assets, fake.clone());
        let resolve = |id: &str, offline: bool| {
            fake.clear();
            downloader.resolve_uri(&format!("rbxassetid://{id}"), offline, &HashMap::new())
        };
        // A download is recorded, and the next resolve is a cache hit
        let first = resolve("501", false).unwrap();
        assert_eq!(std::fs::read(&first.file).unwrap(), real);
        assert_eq!(record(&assets, "501.png"), json!({"sha256": sha256(&real), "size": real.len()}));
        assert_eq!(resolve("501", false).unwrap().file, first.file);
        assert!(fake.seen().is_empty(), "a recorded download is served from the cache");

        // A stand-in another tool wrote under an asset's name is replaced by the asset, by rename: a render
        // directory's link to the stand-in keeps it
        let foreign = assets.join("502.png");
        std::fs::write(&foreign, &stand_in).unwrap();
        let render_link = temp.0.join("render-link.png");
        std::fs::hard_link(&foreign, &render_link).unwrap();
        let replaced = resolve("502", false).unwrap();
        assert_eq!(fake.seen().len(), 2, "an unrecorded cache file is downloaded again");
        assert_eq!(replaced.file, foreign);
        assert_eq!(std::fs::read(&foreign).unwrap(), real);
        assert_eq!(std::fs::read(&render_link).unwrap(), stand_in, "the stand-in was replaced, not written into");
        assert_eq!(record(&assets, "502.png")["sha256"], sha256(&real));

        // A stand-in under another extension goes when the asset arrives (it would be found first)
        std::fs::write(assets.join("504.png"), &stand_in).unwrap();
        *body.lock().unwrap() = [&[0xff, 0xd8, 0xff][..], b"jpeg"].concat();
        assert_eq!(resolve("504", false).unwrap().file, assets.join("504.jpg"));
        assert!(!assets.join("504.png").exists());
        *body.lock().unwrap() = real.clone();

        // A recorded file changed in place is downloaded again
        std::fs::write(&first.file, &stand_in).unwrap();
        assert_eq!(std::fs::read(resolve("501", false).unwrap().file).unwrap(), real);
        assert_eq!(fake.seen().len(), 2);

        // An unrecorded file holding the asset's bytes keeps its file (render directories' links stay
        // current) and gets its record
        let legacy = assets.join("505.png");
        std::fs::write(&legacy, &real).unwrap();
        let legacy_link = temp.0.join("legacy-link.png");
        std::fs::hard_link(&legacy, &legacy_link).unwrap();
        resolve("505", false).unwrap();
        std::fs::write(&legacy_link, b"through the link").unwrap();
        assert_eq!(std::fs::read(&legacy).unwrap(), b"through the link", "the same file: not replaced");
        assert_eq!(record(&assets, "505.png")["sha256"], sha256(&real));

        // A stand-in for an asset delivery refuses is not drawn online, and the refusal says why; offline the
        // cache is used as it is, without a request
        std::fs::write(assets.join("503.png"), &stand_in).unwrap();
        refuse.store(true, Ordering::Relaxed);
        let refused = resolve("503", false).unwrap_err();
        assert_eq!(
            refused,
            "asset 503 unavailable from asset delivery (401: Authentication required to access Asset.; private asset, no asset cookie is configured); the cached 503.png is not rur's download of it (no matching record in assets/downloaded) and is not used"
        );
        let offline = resolve("503", true).unwrap();
        assert!(fake.seen().is_empty());
        assert_eq!(std::fs::read(offline.file).unwrap(), stand_in);
    }

    /// The former tools/tests/asset_batch.mjs: ids to download are located in one batch request (with the cookie and the
    /// CSRF token it asks for), then downloaded from the CDN without the cookie; an id the batch does not
    /// place, or whose located download fails, is looked up alone
    #[test]
    fn many_ids_are_located_in_one_batch() {
        let temp = Temp::new("batch");
        let assets = temp.0.join("state").join("assets");
        const COOKIE: &str = "dummy-batch";
        // The batch endpoint places 11 and 12 (13 is "not found"); the per-asset route places the rest. The
        // CDN serves every file but 12's batch location (an expired link).
        let fake = Fake::new(|seen| {
            let route = path(&seen.url);
            if seen.method == Method::Post {
                assert_eq!(seen.url, "https://assetdelivery.roblox.com/v1/assets/batch");
                if seen.csrf.is_none() {
                    return status(403, &[("x-csrf-token", "dummy-token")], "");
                }
                return ok(r#"[{"requestId":"0","location":"https://cdn.example/11"},{"requestId":"1","location":"https://cdn.example/12-expired"},{"requestId":"2","errors":[{"code":404,"message":"Request asset was not found"}]}]"#);
            }
            if host_of(&seen.url) == "cdn.example" {
                return if route.ends_with("-expired") { status(403, &[], "") } else { ok(png()) };
            }
            ok(json!({"location": format!("https://cdn.example/{}-lookup", route.rsplit('/').next().unwrap())}).to_string())
        });
        let downloader = Downloader::for_tests(&assets, fake.clone()).with_account(Account::Cookie(COOKIE.into()));
        // 14 is cached already
        downloader.resolve_all(&["rbxassetid://14".into()], false);
        fake.clear();
        let uris: Vec<String> = ["11", "12", "13", "14", "11"].iter().map(|id| format!("rbxassetid://{id}")).collect();
        let results = downloader.resolve_all(&uris, false);
        let seen = fake.seen();
        let posts: Vec<&Seen> = seen.iter().filter(|seen| seen.method == Method::Post).collect();
        assert_eq!(posts.len(), 2, "one batch request, repeated once with the CSRF token");
        assert_eq!(posts.iter().map(|post| post.csrf.as_deref()).collect::<Vec<_>>(), [None, Some("dummy-token")]);
        assert!(posts.iter().all(|post| post.cookie.as_deref() == Some(".ROBLOSECURITY=dummy-batch")), "the batch request carries the cookie");
        assert_eq!(posts[0].body, r#"[{"requestId":"0","assetId":11},{"requestId":"1","assetId":12},{"requestId":"2","assetId":13}]"#);
        for id in ["11", "12", "13", "14"] {
            assert_eq!(std::fs::read(&results[&format!("rbxassetid://{id}")].as_ref().unwrap().file).unwrap(), png(), "asset {id}");
        }
        let mut gets: Vec<String> = seen.iter().filter(|seen| seen.method == Method::Get).map(|seen| format!("{}{}", host_of(&seen.url), path(&seen.url))).collect();
        gets.sort();
        assert_eq!(
            gets,
            [
                "assetdelivery.roblox.com/v1/assetId/12", // its located download failed
                "assetdelivery.roblox.com/v1/assetId/13", // the batch did not place it
                "cdn.example/11",
                "cdn.example/12-expired",
                "cdn.example/12-lookup",
                "cdn.example/13-lookup",
            ],
            "a located asset is not looked up again; the cached one is not fetched"
        );
        assert!(seen.iter().filter(|seen| host_of(&seen.url) != DELIVERY_HOST).all(|seen| seen.cookie.is_none()), "the CDN never receives the cookie");
        // Cached, or offline: no batch request
        fake.clear();
        downloader.resolve_all(&["rbxassetid://11".into(), "rbxassetid://12".into()], false);
        downloader.resolve_all(&["rbxassetid://21".into(), "rbxassetid://22".into()], true);
        assert!(fake.seen().is_empty());
        // Past 100 ids: a request per 100, ids written as digit strings (past a double's integers too)
        fake.answer(|seen| if seen.method == Method::Post { ok("[]") } else { status(404, &[], "") });
        let many: Vec<String> = (0..150).map(|index| format!("rbxassetid://{}", 90071992547409930u64 + index)).collect();
        downloader.resolve_all(&many, false);
        let bodies: Vec<String> = fake.seen().into_iter().filter(|seen| seen.method == Method::Post).map(|seen| seen.body).collect();
        assert_eq!(bodies.len(), 2);
        let counts: Vec<usize> = bodies.iter().map(|body| body.matches("assetId").count()).collect();
        assert!(counts.contains(&100) && counts.contains(&50), "{counts:?}");
        assert!(bodies.iter().any(|body| body.contains(r#""assetId":90071992547409930}"#)));
    }

    /// Offline nothing goes out; cached thumbnails,
    /// nested decals offline and duplicate requests
    #[test]
    fn offline_requests_never_reach_the_network() {
        let temp = Temp::new("offline");
        let assets = temp.0.join("assets");
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(assets.join("123.png"), png()).unwrap();
        std::fs::write(assets.join("thumb-AvatarHeadShot-42-150x150-webp.png"), png()).unwrap();
        std::fs::write(assets.join("999.rbxmx"), "<roblox><url>rbxassetid://124</url></roblox>").unwrap();
        let fake = Fake::new(|_| panic!("NETWORK WAS ATTEMPTED"));
        let downloader = Downloader::for_tests(&assets, fake.clone()).with_account(Account::Cookie("dummy-native-assets".into()));
        let uris: Vec<String> = ["rbxassetid://123", "rbxassetid://123", "rbxthumb://type=AvatarHeadShot&id=42&w=150&h=150", "rbxassetid://999", "rbxassetid://404"].iter().map(|uri| uri.to_string()).collect();
        let results = downloader.resolve_all(&uris, true);
        assert_eq!(results.len(), 4, "duplicate image requirements are resolved once");
        assert_eq!(std::fs::read(&results["rbxassetid://123"].as_ref().unwrap().file).unwrap(), png());
        assert!(results["rbxthumb://type=AvatarHeadShot&id=42&w=150&h=150"].is_ok(), "cached thumbnails work offline");
        assert!(results["rbxassetid://999"].as_ref().unwrap_err().contains("offline"), "nested decal images preserve offline mode");
        assert!(results["rbxassetid://404"].as_ref().unwrap_err().contains("offline"));
        assert!(!format!("{results:?}").contains("dummy-native-assets"), "credentials never enter the answers");
        assert!(fake.seen().is_empty());
    }

    /// cli/assets.mjs's in-place migrations: a pre-unwrap `.bin` entry becomes its real file, a gzip body is
    /// unwrapped, a cached font is listed in fonts.json, and a decal model resolves to its texture
    #[test]
    fn bin_entries_gzip_bodies_fonts_and_decals() {
        let temp = Temp::new("migrations");
        let assets = temp.0.join("assets");
        std::fs::create_dir_all(&assets).unwrap();
        fn gzip(bytes: &[u8]) -> Vec<u8> {
            let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(bytes).unwrap();
            encoder.finish().unwrap()
        }
        let font = [&[0u8, 1, 0, 0][..], b"font"].concat();
        std::fs::write(assets.join("7.bin"), gzip(&font)).unwrap();
        std::fs::write(assets.join("fonts.json"), "[\n  \"3.ttf\"\n]").unwrap();
        let fake = Fake::new(|seen| match path(&seen.url) {
            "/v1/assetId/8" => ok(r#"{"location":"https://cdn.example/8"}"#),
            "/v1/assetId/9" => ok(r#"{"location":"https://cdn.example/9"}"#),
            "/8" => ok(gzip(b"<roblox><Content name=\"Texture\"><url>http://www.roblox.com/asset/?id=9</url></Content></roblox>")),
            "/9" => ok(png()),
            other => panic!("unexpected request {other}"),
        });
        let downloader = Downloader::for_tests(&assets, fake.clone());
        // Offline: the .bin entry is migrated and indexed without a request
        let migrated = downloader.fetch_asset_id("7", false, true, None).unwrap();
        assert_eq!(migrated.file, assets.join("7.ttf"));
        assert_eq!(std::fs::read(&migrated.file).unwrap(), font);
        assert!(!assets.join("7.bin").exists());
        let index: Value = serde_json::from_slice(&std::fs::read(assets.join("fonts.json")).unwrap()).unwrap();
        assert_eq!(index, json!(["3.ttf", "7.ttf"]), "merged with the index and the directory");
        assert!(fake.seen().is_empty());
        // A decal model (gzip-wrapped) resolves to its image, and both are recorded
        let decal = downloader.fetch_asset_id("8", true, false, None).unwrap();
        assert_eq!((decal.file.clone(), decal.via.as_deref()), (assets.join("9.png"), Some("8")));
        assert!(std::fs::read_to_string(assets.join("8.rbxmx")).unwrap().starts_with("<roblox>"));
        assert_eq!(record(&assets, "9.png")["size"], png().len());
        assert!(assets.join("downloaded").join("8.rbxmx.json").is_file());
        // A model without a texture of its own
        std::fs::write(assets.join("10.rbxmx"), "<roblox><url>rbxassetid://10</url></roblox>").unwrap();
        assert_eq!(downloader.fetch_asset_id("10", true, true, None).unwrap_err(), "asset 10 is a model without an image texture");
    }

    /// A font request: a font face or a family's JSON counts as arrived;
    /// what is unavailable does not, and offline nothing is asked
    #[test]
    fn missing_cloud_fonts_are_downloaded() {
        let temp = Temp::new("fonts");
        let assets = temp.0.join("assets");
        let fake = Fake::new(|seen| match path(&seen.url) {
            "/v1/assetId/12345" => ok(r#"{"location":"https://cdn.example/face"}"#),
            "/v1/assetId/678" => ok(r#"{"location":"https://cdn.example/family"}"#),
            "/face" => ok([&[0u8, 1, 0, 0][..], b"face"].concat()),
            "/family" => ok(r#"{"name":"Family","faces":[]}"#),
            _ => status(404, &[], r#"{"errors":[{"code":404,"message":"Request asset was not found"}]}"#),
        });
        let downloader = Downloader::for_tests(&assets, fake.clone());
        assert!(!downloader.fetch_fonts(&json!({ "missingFontAssets": [] })));
        assert!(!downloader.fetch_fonts(&json!({ "missingFontAssets": ["rbxassetid://5"] })), "unavailable: nothing arrived");
        assert!(downloader.fetch_fonts(&json!({ "missingFontAssets": [12345] })));
        assert!(assets.join("12345.ttf").is_file());
        assert_eq!(serde_json::from_slice::<Value>(&std::fs::read(assets.join("fonts.json")).unwrap()).unwrap(), json!(["12345.ttf"]));
        assert!(downloader.fetch_fonts(&json!({ "missingFontAssets": ["rbxassetid://678"] })), "a family's JSON counts");
        assert!(assets.join("678.json").is_file());
        // The resolver's offline mode asks nothing
        fake.clear();
        let mut offline = crate::resolve::tests::resolver_with(&assets, true, fake.clone());
        assert!(!offline.fetch_fonts(&json!({ "missingFontAssets": ["rbxassetid://9"] })).unwrap());
        assert!(fake.seen().is_empty());
    }

    /// The former tools/analysis/image_assets_test.mjs: thumbnails take the WebP rendition decoded to PNG, fall back to
    /// the PNG rendition on every failure, are cached, and a failed download leaves nothing in the cache
    #[test]
    fn thumbnails_take_webp_then_fall_back_to_png() {
        let temp = Temp::new("thumbnails");
        let assets = temp.0.join("assets");
        let pixels = [10u8, 20, 30, 0, 200, 100, 50, 255];
        let mut webp = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut webp).encode(&pixels, 2, 1, image::ExtendedColorType::Rgba8).unwrap();
        // The WebP rendition, decoded to PNG with its colour under alpha 0
        let webp_body = webp.clone();
        let fake = Fake::new(move |seen| {
            if seen.url.starts_with("https://thumbnails.roblox.com/v1/users/avatar-headshot?userIds=1&size=60x60&format=") {
                assert!(seen.url.ends_with("format=Webp"), "{}", seen.url);
                return ok(r#"{"data":[{"state":"Completed","imageUrl":"https://cdn.example/webp"}]}"#);
            }
            ok(webp_body.clone())
        });
        let downloader = Downloader::for_tests(&assets, fake.clone());
        let fetched = downloader.fetch_thumbnail("type=AvatarHeadShot&id=1&w=60&h=60", false).unwrap();
        assert_eq!(fetched.file, assets.join("thumb-AvatarHeadShot-1-60x60-webp.png"));
        assert_eq!(fetched.mime, "image/png");
        let decoded = image::open(&fetched.file).unwrap().to_rgba8();
        assert_eq!((decoded.width(), decoded.height(), decoded.as_raw().as_slice()), (2, 1, &pixels[..]));
        assert_eq!(fake.seen().len(), 2);

        // Every failure of the WebP attempt reaches the PNG rendition
        let failures: Vec<Vec<Result<Response, String>>> = vec![
            vec![status(400, &[], r#"{"errors":[{"message":"Unsupported format"}]}"#)],
            vec![Err("WebP request failed".into())],
            vec![ok(r#"{"data":[{"imageUrl":"https://cdn.example/webp"}]}"#), status(503, &[], "unavailable")],
            vec![ok("invalid metadata")],
            vec![ok(r#"{"data":[{"imageUrl":"https://cdn.example/webp"}]}"#), ok("not an image")],
            vec![ok(r#"{"data":[{"imageUrl":"https://cdn.example/webp"}]}"#), ok(b"RIFF\0\0\0\0WEBPbroken".to_vec())],
        ];
        for (index, failure) in failures.into_iter().enumerate() {
            let mut answers = failure;
            answers.push(ok(r#"{"data":[{"imageUrl":"https://cdn.example/png"}]}"#));
            answers.push(ok(png()));
            let queue = Arc::new(Mutex::new(std::collections::VecDeque::from(answers)));
            let remaining = queue.clone();
            fake.answer(move |seen| remaining.lock().unwrap().pop_front().unwrap_or_else(|| panic!("unexpected request {}", seen.url)));
            fake.clear();
            let query = format!("type=AvatarHeadShot&id={}&w=150&h=150", index + 10);
            let fetched = downloader.fetch_thumbnail(&query, false).unwrap();
            assert_eq!((fetched.mime, std::fs::read(&fetched.file).unwrap()), ("image/png", png()), "case {index}");
            assert!(fake.urls().iter().any(|url| url.ends_with("format=Png")));
            assert!(queue.lock().unwrap().is_empty());
            let count = fake.seen().len();
            assert_eq!(downloader.fetch_thumbnail(&query, false).unwrap(), fetched);
            assert_eq!(fake.seen().len(), count, "a cached thumbnail is not downloaded again");
        }
        let entries = std::fs::read_dir(&assets).unwrap().count();
        let queue = Arc::new(Mutex::new(std::collections::VecDeque::from(vec![Err("WebP failed".to_string()), ok(r#"{"data":[{"imageUrl":"https://cdn.example/png"}]}"#), status(503, &[], "unavailable")])));
        fake.answer(move |_| queue.lock().unwrap().pop_front().unwrap());
        let failed = downloader.fetch_thumbnail("type=AvatarHeadShot&id=99", false).unwrap_err();
        assert!(failed.contains("HTTP 503"), "{failed}");
        assert_eq!(std::fs::read_dir(&assets).unwrap().count(), entries, "failed downloads must not enter the cache");
        assert!(downloader.fetch_thumbnail("type=AvatarHeadShot&id=98", true).unwrap_err().contains("offline"));
    }

    /// A Blocked avatar thumbnail is Studio's placeholder, found in Roblox's HTTP cache by MD5 and kept
    #[test]
    fn blocked_thumbnails_draw_the_placeholder() {
        let temp = Temp::new("blocked");
        let assets = temp.0.join("assets");
        // Roblox's HTTP cache: entries "RBXH" + header + body, the body's length at byte 25
        let storage = temp.0.join("rbx-storage");
        let entry = |body: &[u8]| {
            let mut bytes = b"RBXH".to_vec();
            bytes.resize(37, 0);
            bytes[25..29].copy_from_slice(&(body.len() as u32).to_le_bytes());
            bytes.extend_from_slice(body);
            bytes
        };
        let body = png();
        std::fs::create_dir_all(storage.join("ab")).unwrap();
        std::fs::write(storage.join("ab").join("other"), entry(b"something else of its own")).unwrap();
        std::fs::write(storage.join("ab").join("match"), entry(&body)).unwrap();
        assert_eq!(storage_entry(&storage, &md5_hex(&body), body.len()), Some(body.clone()));
        assert_eq!(storage_entry(&storage, "00000000000000000000000000000000", body.len()), None);
        // The measured placeholders: found in the asset cache when kept there
        let fake = Fake::new(|seen| {
            assert!(!seen.url.contains("cdn"), "a Blocked thumbnail's imageUrl is not downloaded");
            ok(r#"{"data":[{"state":"Blocked","imageUrl":"https://cdn.example/unapproved"}]}"#)
        });
        let downloader = Downloader::for_tests(&assets, fake.clone());
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(assets.join("placeholder-2f598ef27b1dddc3c4556aba8c2101c1.png"), &body).unwrap();
        let fetched = downloader.fetch_thumbnail("type=AvatarHeadShot&id=5&w=60&h=60", false).unwrap();
        assert_eq!(std::fs::read(&fetched.file).unwrap(), body);
        // Another type has no placeholder: the Blocked answer's image is fetched as any other
        fake.answer(|seen| if seen.url.contains("format=") { ok(r#"{"data":[{"state":"Blocked","imageUrl":"https://cdn.example/x"}]}"#) } else { ok(png()) });
        assert!(downloader.fetch_thumbnail("type=GroupIcon&id=5", false).is_ok());
    }

    /// The resolver part of tools/tests/asset_studio.mjs (before the downloader moved here): refused images are read from Studio when it is
    /// enabled, a cached read is not read again, offline never reads, and an id Studio cannot read keeps
    /// its error with Studio's reason
    #[test]
    fn refused_images_are_read_from_studio() {
        let temp = Temp::new("studio");
        let assets = temp.0.join("state").join("assets");
        let fake = Fake::new(|_| ok(r#"{"errors":[{"code":401,"message":"Authentication required to access Asset."}]}"#));
        let calls: Arc<Mutex<Vec<Vec<String>>>> = Arc::default();
        let (seen_calls, store) = (calls.clone(), assets.clone());
        let mut downloader = Downloader::for_tests(&assets, fake.clone());
        downloader.studio = Studio::Fake(Box::new(move |ids| {
            seen_calls.lock().unwrap().push(ids.to_vec());
            let mut reads = StudioReads::default();
            for id in ids {
                if id == "33" {
                    reads.errors.insert(id.clone(), "Studio (Test Place) could not read it: no permission to load asset".into());
                } else {
                    // What storeStudioImage writes: the PNG and its record with "source": "studio"
                    std::fs::create_dir_all(store.join("downloaded")).unwrap();
                    let file = store.join(format!("{id}.png"));
                    std::fs::write(&file, png()).unwrap();
                    std::fs::write(crate::resolve::record_file(&file), json!({"sha256": sha256(&png()), "size": png().len(), "source": "studio"}).to_string()).unwrap();
                    reads.stored.insert(id.clone());
                }
            }
            Ok(reads)
        }));
        let uris: Vec<String> = ["31", "32", "33", "31"].iter().map(|id| format!("rbxassetid://{id}")).collect();
        let results = downloader.resolve_all(&uris, false);
        assert_eq!(*calls.lock().unwrap(), [vec!["31".to_string(), "32".into(), "33".into()]], "the refused ids in one read");
        for id in ["31", "32"] {
            assert_eq!(results[&format!("rbxassetid://{id}")].as_ref().unwrap().file, assets.join(format!("{id}.png")));
        }
        let error = results["rbxassetid://33"].as_ref().unwrap_err();
        assert!(error.contains("Authentication required") && error.ends_with("; Studio (Test Place) could not read it: no permission to load asset"), "{error}");
        // Read once: cached afterwards; offline never asks Studio
        calls.lock().unwrap().clear();
        let cached = downloader.resolve_all(&["rbxassetid://31".into(), "rbxassetid://32".into()], false);
        assert!(cached.values().all(Result::is_ok));
        downloader.resolve_all(&["rbxassetid://34".into()], true);
        assert!(calls.lock().unwrap().is_empty(), "a cached Studio read and an offline render ask Studio nothing");
        // A model without a texture is not Studio's to read; a failed read keeps every error, with its reason
        downloader.studio = Studio::Fake(Box::new(|_| Err("cli/studio-images.mjs exited with 1".into())));
        let failed = downloader.resolve_all(&["rbxassetid://35".into()], false);
        assert!(failed["rbxassetid://35"].as_ref().unwrap_err().ends_with("; not read from Studio: cli/studio-images.mjs exited with 1"));
        // When it is enabled: a bridge, unless RUR_STUDIO_IMAGES=0
        assert!(!studio_images_enabled(&env_of(&[])));
        assert!(studio_images_enabled(&env_of(&[("RUR_STUDIO_MCP_URL", "http://127.0.0.1:58741")])));
        assert!(!studio_images_enabled(&env_of(&[("RUR_STUDIO_MCP_URL", "http://127.0.0.1:58741"), ("RUR_STUDIO_IMAGES", "0")])));
        assert!(studio_images_enabled(&env_of(&[("RUR_STUDIO_MCP_PORT", "44755")])));
        assert!(!studio_images_enabled(&env_of(&[("RUR_STUDIO_MCP_PORT", "0")])));
    }

    #[test]
    fn redirects_resolve_and_plain_http_is_refused() {
        assert_eq!(join_url("https://a.example/x/y?q", "/z"), "https://a.example/z");
        assert_eq!(join_url("https://a.example/x/y?q", "z"), "https://a.example/x/z");
        assert_eq!(join_url("https://a.example/x", "//b.example/c"), "https://b.example/c");
        assert_eq!(join_url("https://a.example/x", "https://c.example/"), "https://c.example/");
        let temp = Temp::new("redirects");
        let fake = Fake::new(|seen| match seen.url.as_str() {
            "https://cdn.example/1" => status(301, &[("location", "http://cdn.example/1")], ""),
            _ => ok(png()),
        });
        let downloader = Downloader::for_tests(&temp.0, fake.clone());
        let error = downloader.get("https://cdn.example/1", vec![("Cookie", "x".into())], 5, false).unwrap_err();
        assert!(error.message.contains("plain http"), "{}", error.message);
        assert_eq!(fake.urls(), ["https://cdn.example/1"]);
        assert!(downloader.get("http://cdn.example/2", Vec::new(), 5, false).is_err());
        assert_eq!(fake.urls().len(), 1, "a plain http URL is never requested");
    }

    /// One download per asset at a time: concurrent requests for the same id share it
    #[test]
    fn concurrent_requests_share_one_download() {
        let temp = Temp::new("in-flight");
        let assets = temp.0.join("assets");
        let fake = Fake::new(|seen| {
            if seen.url.contains("/v1/assetId/") {
                std::thread::sleep(Duration::from_millis(100));
                return ok(r#"{"location":"https://cdn.example/1"}"#);
            }
            ok(png())
        });
        let downloader = Downloader::for_tests(&assets, fake.clone());
        let uris: Vec<String> = vec!["rbxassetid://61".into(), "http://www.roblox.com/asset/?id=61".into(), "61".into()];
        let results = downloader.resolve_all(&uris, false);
        assert!(results.values().all(Result::is_ok));
        assert_eq!(fake.urls().iter().filter(|url| url.contains("/v1/assetId/61")).count(), 1);
    }
}
