//! The account signed in to Roblox Studio on this machine: its `.ROBLOSECURITY` cookie, read from where Studio
//! keeps it. The account's last source ([`super::Account`]), after a broker and the cookie's variables and files, and
//! only when `ROR_STUDIO_COOKIE` is `1` (any other value is off; roblox-object-renderer reads the same switch).
//!
//! - Windows: the Credential Manager's generic credentials `https://www.roblox.com:RobloxStudioAuthuserid` (the
//!   signed-in account's user id) and `https://www.roblox.com:RobloxStudioAuth.ROBLOSECURITY<user id>` (its
//!   cookie). Without a user id: `...RobloxStudioAuth.ROBLOSECURITY` alone, else the one account with a cookie
//!   there. Older Studio builds kept it in the registry instead (`HKCU\Software\Roblox\RobloxStudioBrowser\roblox.com`,
//!   value `.ROBLOSECURITY`: `SEC::<...>,EXP::<...>,COOK::<cookie>`), read when the Credential Manager has none.
//! - macOS: the `.ROBLOSECURITY` cookie of a `roblox.com` domain in
//!   `~/Library/HTTPStorages/com.Roblox.RobloxStudio.binarycookies`, Apple's binary cookie file.
//! - Elsewhere: not available.
//!
//! The OS calls only hand over the stored bytes; the parsing here is tested on synthetic data. The cookie is kept as
//! the other cookies are ([`super::Account`]'s Debug never shows it), and the bytes it was read from are zeroed. A
//! reason why it could not be read names no value, user id or credential name.

// the Windows readers are tested everywhere, used on Windows alone
#![cfg_attr(not(windows), allow(dead_code))]

/// The switch: `1` reads Studio's account, anything else does not. Not a `RUR_*` name: one switch serves rur and
/// roblox-object-renderer, which draws its ViewportFrames and reads it too.
pub const SWITCH: &str = "ROR_STUDIO_COOKIE";

/// The Credential Manager's target names Studio's start with: `<TARGET>userid` holds the signed-in account's user id,
/// `<TARGET>.ROBLOSECURITY<user id>` its cookie
const TARGET: &str = "https://www.roblox.com:RobloxStudioAuth";

const CREDENTIAL_MANAGER: &str = "Windows Credential Manager";
const REGISTRY: &str = "Windows registry";
#[cfg_attr(windows, allow(dead_code))]
const BINARY_COOKIES: &str = "macOS cookie file";

/// What looking for Studio's cookie found
#[derive(Clone, PartialEq, Eq)]
pub enum StudioCookie {
    /// `ROR_STUDIO_COOKIE` is not `1`
    Off,
    /// No reader on this platform (neither Windows nor macOS)
    Unsupported,
    NotFound,
    /// The cookie, and where it was found ("Windows Credential Manager", "Windows registry", "macOS cookie file")
    Found { cookie: String, store: &'static str },
    /// Why it could not be read (never a value)
    Unreadable(String),
}

/// Never the cookie
impl std::fmt::Debug for StudioCookie {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StudioCookie::Found { store, .. } => write!(formatter, "Found({store})"),
            other => formatter.write_str(&other.status()),
        }
    }
}

impl StudioCookie {
    /// `off (set ROR_STUDIO_COOKIE=1)`, `not found`, `found (<store>)`, `unreadable: <reason>`: never a value
    pub fn status(&self) -> String {
        match self {
            StudioCookie::Off => format!("off (set {SWITCH}=1)"),
            StudioCookie::Unsupported => "not available on this platform".into(),
            StudioCookie::NotFound => "not found".into(),
            StudioCookie::Found { store, .. } => format!("found ({store})"),
            StudioCookie::Unreadable(why) => format!("unreadable: {why}"),
        }
    }
}

/// Whether the switch's value turns the source on: exactly `1`
pub fn enabled(value: Option<&str>) -> bool {
    value == Some("1")
}

/// Studio's cookie when `env`'s `ROR_STUDIO_COOKIE` is `1` (the store is not touched otherwise)
pub fn lookup(env: &dyn Fn(&str) -> Option<String>) -> StudioCookie {
    if !enabled(env(SWITCH).as_deref()) {
        return StudioCookie::Off;
    }
    read()
}

/// Bytes that may hold the cookie: zeroed on drop
struct Blob(Vec<u8>);

impl Drop for Blob {
    fn drop(&mut self) {
        for byte in self.0.iter_mut() {
            // SAFETY: `byte` is a valid, aligned, exclusive reference into the vector
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
    }
}

/// A stored value as text bytes: UTF-8 as it is, UTF-16 (little-endian, as Windows writes wide strings) when every
/// second byte is 0; trailing NULs dropped
fn decoded(stored: &[u8]) -> Blob {
    let wide = stored.len() >= 2 && stored.len().is_multiple_of(2) && stored.iter().skip(1).step_by(2).all(|&byte| byte == 0);
    let mut bytes: Vec<u8> = if wide { stored.iter().step_by(2).copied().collect() } else { stored.to_vec() };
    while bytes.last() == Some(&0) {
        bytes.pop();
    }
    Blob(bytes)
}

/// A cookie value as it goes into the Cookie header: printable ASCII without `;`, `,` or `"`, a `.ROBLOSECURITY=`
/// prefix dropped. None when it is not one.
fn cookie_value(text: &str) -> Option<String> {
    let text = text.trim();
    let value = text.strip_prefix(".ROBLOSECURITY=").unwrap_or(text);
    let valid = !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_graphic() && !matches!(byte, b';' | b',' | b'"'));
    valid.then(|| value.to_string())
}

/// The cookie in a stored value, when it is one
fn cookie_of(stored: &[u8]) -> Option<String> {
    let text = decoded(stored);
    std::str::from_utf8(&text.0).ok().and_then(cookie_value)
}

/// A found value: the cookie, or why the store's entry is not one
fn usable(cookie: Option<String>, store: &'static str) -> StudioCookie {
    match cookie {
        Some(cookie) => StudioCookie::Found { cookie, store },
        None => StudioCookie::Unreadable(format!("the {store} entry holds no usable cookie")),
    }
}

fn digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// The cookie in the Credential Manager. `read` gives a generic credential's stored bytes by its name (None when there
/// is none), `names` the names that match a `<prefix>*` filter; their errors name no value.
///
/// The signed-in account's cookie (its user id in `<TARGET>userid`), else the one without a user id that builds with a
/// single account kept. Only with no user id at all, the cookie of the one account that has one: with several, which
/// is signed in is unknown.
fn from_credentials(read: &dyn Fn(&str) -> Result<Option<Blob>, String>, names: &dyn Fn(&str) -> Result<Vec<String>, String>) -> StudioCookie {
    let prefix = format!("{TARGET}.ROBLOSECURITY");
    let user = match read(&format!("{TARGET}userid")) {
        Ok(stored) => stored.and_then(|stored| std::str::from_utf8(&decoded(&stored.0).0).ok().map(|text| text.trim().to_string())).filter(|id| digits(id)),
        Err(why) => return StudioCookie::Unreadable(why),
    };
    let mut candidates: Vec<String> = user.iter().map(|id| format!("{prefix}{id}")).collect();
    candidates.push(prefix.clone());
    for name in &candidates {
        match read(name) {
            Ok(Some(stored)) => return usable(cookie_of(&stored.0), CREDENTIAL_MANAGER),
            Ok(None) => {}
            Err(why) => return StudioCookie::Unreadable(why),
        }
    }
    if user.is_some() {
        return StudioCookie::NotFound;
    }
    let accounts: Vec<String> = match names(&format!("{prefix}*")) {
        Ok(names) => names.into_iter().filter(|name| name.strip_prefix(&prefix).is_some_and(digits)).collect(),
        Err(why) => return StudioCookie::Unreadable(why),
    };
    match accounts.as_slice() {
        [] => StudioCookie::NotFound,
        [name] => match read(name) {
            Ok(Some(stored)) => usable(cookie_of(&stored.0), CREDENTIAL_MANAGER),
            Ok(None) => StudioCookie::NotFound,
            Err(why) => StudioCookie::Unreadable(why),
        },
        _ => StudioCookie::Unreadable("several Studio accounts have a cookie and none is marked as signed in".into()),
    }
}

/// The cookie in the registry value older Studio builds wrote: comma-separated `KEY::<value>` items, the cookie in
/// `COOK::<...>`
fn from_registry_value(value: &str) -> StudioCookie {
    let cookie = value.split(',').find_map(|item| item.trim().strip_prefix("COOK::")).map(|item| item.strip_prefix('<').and_then(|item| item.strip_suffix('>')).unwrap_or(item));
    match cookie {
        Some(cookie) => usable(cookie_value(cookie), REGISTRY),
        None => StudioCookie::Unreadable(format!("the {REGISTRY} entry holds no cookie")),
    }
}

#[cfg(windows)]
fn read() -> StudioCookie {
    match from_credentials(&os::credential, &os::credential_names) {
        StudioCookie::NotFound => match os::registry_value() {
            Ok(Some(value)) => match std::str::from_utf8(&value.0) {
                Ok(text) => from_registry_value(text),
                Err(_) => StudioCookie::Unreadable(format!("the {REGISTRY} entry is not text")),
            },
            Ok(None) => StudioCookie::NotFound,
            Err(why) => StudioCookie::Unreadable(why),
        },
        other => other,
    }
}

#[cfg(target_os = "macos")]
fn read() -> StudioCookie {
    let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()) else {
        return StudioCookie::Unreadable("HOME is not set".into());
    };
    let file = std::path::PathBuf::from(home).join("Library").join("HTTPStorages").join("com.Roblox.RobloxStudio.binarycookies");
    match std::fs::read(&file) {
        Ok(bytes) => {
            let bytes = Blob(bytes);
            match parse_binary_cookies(&bytes.0) {
                Ok(Some(cookie)) => StudioCookie::Found { cookie, store: BINARY_COOKIES },
                Ok(None) => StudioCookie::NotFound,
                Err(why) => StudioCookie::Unreadable(why.into()),
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => StudioCookie::NotFound,
        Err(error) => StudioCookie::Unreadable(format!("the {BINARY_COOKIES}: {}", error.kind())),
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn read() -> StudioCookie {
    StudioCookie::Unsupported
}

/// The Credential Manager and the registry through windows-sys: the stored bytes only, read here and parsed above
#[cfg(windows)]
mod os {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS, GetLastError};
    use windows_sys::Win32::Security::Credentials::{CRED_TYPE_GENERIC, CREDENTIALW, CredEnumerateW, CredFree, CredReadW};
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};

    use super::Blob;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Copies a credential's stored bytes out and zeroes Windows' copy, which the caller then frees.
    ///
    /// # Safety
    ///
    /// `credential` comes from CredReadW or CredEnumerateW and has not been freed.
    unsafe fn take_blob(credential: &CREDENTIALW) -> Vec<u8> {
        let (blob, size) = (credential.CredentialBlob, credential.CredentialBlobSize as usize);
        if blob.is_null() || size == 0 {
            return Vec::new();
        }
        // SAFETY: the API's credential holds `size` bytes at `blob`, in memory that is the caller's until freed
        unsafe {
            let bytes = std::slice::from_raw_parts(blob, size).to_vec();
            std::ptr::write_bytes(blob, 0, size);
            bytes
        }
    }

    /// A generic credential's stored bytes; None when there is none of that name
    pub(super) fn credential(target: &str) -> Result<Option<Blob>, String> {
        let name = wide(target);
        let mut found: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `name` is NUL-terminated; on success `found` points at one credential, freed below
        if unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut found) } == 0 {
            // SAFETY: no precondition
            let code = unsafe { GetLastError() };
            return if code == ERROR_NOT_FOUND { Ok(None) } else { Err(format!("Windows Credential Manager error {code}")) };
        }
        // SAFETY: CredReadW succeeded, so `found` is valid until CredFree
        let bytes = unsafe { take_blob(&*found) };
        // SAFETY: allocated by CredReadW, freed once
        unsafe { CredFree(found.cast()) };
        Ok(Some(Blob(bytes)))
    }

    /// The names of the generic credentials that match `filter` (`<prefix>*`)
    pub(super) fn credential_names(filter: &str) -> Result<Vec<String>, String> {
        let filter = wide(filter);
        let (mut count, mut list): (u32, *mut *mut CREDENTIALW) = (0, std::ptr::null_mut());
        // SAFETY: `filter` is NUL-terminated; on success `list` holds `count` credentials, freed below
        if unsafe { CredEnumerateW(filter.as_ptr(), 0, &mut count, &mut list) } == 0 {
            // SAFETY: no precondition
            let code = unsafe { GetLastError() };
            return if code == ERROR_NOT_FOUND { Ok(Vec::new()) } else { Err(format!("Windows Credential Manager error {code}")) };
        }
        let mut names = Vec::new();
        for index in 0..count as usize {
            // SAFETY: `list` holds `count` valid credential pointers until CredFree
            let credential = unsafe { &**list.add(index) };
            // every credential the filter matched came with its stored bytes: zero them before they are freed
            // SAFETY: as above
            drop(Blob(unsafe { take_blob(credential) }));
            if credential.Type != CRED_TYPE_GENERIC || credential.TargetName.is_null() {
                continue;
            }
            // SAFETY: TargetName is a NUL-terminated wide string of the credential
            let name = unsafe {
                let length = (0..).take_while(|&at| *credential.TargetName.add(at) != 0).count();
                String::from_utf16_lossy(std::slice::from_raw_parts(credential.TargetName, length))
            };
            names.push(name);
        }
        // SAFETY: allocated by CredEnumerateW, freed once
        unsafe { CredFree(list.cast()) };
        Ok(names)
    }

    /// The registry value older Studio builds kept the cookie in, as UTF-8; None when it is not there
    pub(super) fn registry_value() -> Result<Option<Blob>, String> {
        let key = wide("Software\\Roblox\\RobloxStudioBrowser\\roblox.com");
        let value = wide(".ROBLOSECURITY");
        let mut size: u32 = 0;
        // SAFETY: valid NUL-terminated wide strings; a null data pointer asks for the size
        let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), std::ptr::null_mut(), &mut size) };
        if status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(format!("Windows registry error {status}"));
        }
        if size > 64 * 1024 {
            return Err("the Windows registry entry is too long".into());
        }
        let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
        let mut size = (buffer.len() * 2) as u32;
        // SAFETY: `buffer` holds `size` bytes
        let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buffer.as_mut_ptr().cast(), &mut size) };
        let length = buffer.iter().position(|&unit| unit == 0).unwrap_or(buffer.len());
        let text = (status == ERROR_SUCCESS).then(|| String::from_utf16(&buffer[..length]).ok()).flatten();
        for unit in buffer.iter_mut() {
            // SAFETY: a valid, aligned, exclusive reference into the buffer
            unsafe { std::ptr::write_volatile(unit, 0) };
        }
        match (status, text) {
            (ERROR_SUCCESS, Some(text)) => Ok(Some(Blob(text.into_bytes()))),
            (ERROR_SUCCESS, None) => Err("the Windows registry entry is not text".into()),
            (status, _) => Err(format!("Windows registry error {status}")),
        }
    }
}

/// The `.ROBLOSECURITY` cookie of a `roblox.com` domain in Apple's binary cookie file, the one that expires last when
/// there are several; None when the file holds none. An error says what is malformed, never a value.
///
/// The format: `cook`, the page count and each page's size (big-endian u32s), then the pages. A page: the bytes
/// `00 00 01 00`, its cookie count and each cookie's offset in the page (little-endian u32s from here on). A cookie:
/// its size, at 16, 20 and 28 the offsets in the cookie of its domain, name and value (NUL-terminated strings), at 40
/// its expiry (an f64, seconds since 2001).
#[cfg(any(target_os = "macos", test))]
fn parse_binary_cookies(file: &[u8]) -> Result<Option<String>, &'static str> {
    const TRUNCATED: &str = "the macOS cookie file is cut short or malformed";
    fn word(bytes: &[u8], at: usize, big_endian: bool) -> Result<usize, &'static str> {
        let word: [u8; 4] = bytes.get(at..at.checked_add(4).ok_or(TRUNCATED)?).and_then(|word| word.try_into().ok()).ok_or(TRUNCATED)?;
        Ok(if big_endian { u32::from_be_bytes(word) } else { u32::from_le_bytes(word) } as usize)
    }
    fn string(cookie: &[u8], at: usize) -> Result<&[u8], &'static str> {
        let rest = cookie.get(at..).ok_or(TRUNCATED)?;
        rest.iter().position(|&byte| byte == 0).map(|end| &rest[..end]).ok_or(TRUNCATED)
    }
    fn roblox(domain: &[u8]) -> bool {
        let domain = domain.strip_prefix(b".").unwrap_or(domain);
        const SUFFIX: &[u8] = b".roblox.com";
        domain.eq_ignore_ascii_case(&SUFFIX[1..]) || (domain.len() > SUFFIX.len() && domain[domain.len() - SUFFIX.len()..].eq_ignore_ascii_case(SUFFIX))
    }

    if file.get(..4) != Some(b"cook".as_slice()) {
        return Err("the macOS cookie file is not a binary cookie file");
    }
    let pages = word(file, 4, true)?;
    let mut start = pages.checked_mul(4).and_then(|sizes| sizes.checked_add(8)).filter(|&start| start <= file.len()).ok_or(TRUNCATED)?;
    let mut best: Option<(f64, &[u8])> = None;
    for index in 0..pages {
        let size = word(file, 8 + index * 4, true)?;
        let page = file.get(start..start.checked_add(size).ok_or(TRUNCATED)?).ok_or(TRUNCATED)?;
        start += size;
        if page.get(..4) != Some([0u8, 0, 1, 0].as_slice()) {
            return Err(TRUNCATED);
        }
        let count = word(page, 4, false)?;
        if count.checked_mul(4).and_then(|offsets| offsets.checked_add(8)).is_none_or(|end| end > page.len()) {
            return Err(TRUNCATED);
        }
        for at in 0..count {
            let offset = word(page, 8 + at * 4, false)?;
            let length = word(page, offset, false)?;
            let cookie = page.get(offset..offset.checked_add(length).ok_or(TRUNCATED)?).filter(|cookie| cookie.len() >= 48).ok_or(TRUNCATED)?;
            let (domain, name, value) = (string(cookie, word(cookie, 16, false)?)?, string(cookie, word(cookie, 20, false)?)?, string(cookie, word(cookie, 28, false)?)?);
            let expiry = f64::from_le_bytes(cookie[40..48].try_into().map_err(|_| TRUNCATED)?);
            if name == b".ROBLOSECURITY" && roblox(domain) && best.is_none_or(|(latest, _)| expiry > latest) {
                best = Some((expiry, value));
            }
        }
    }
    match best {
        None => Ok(None),
        Some((_, value)) => std::str::from_utf8(value).ok().and_then(cookie_value).map(Some).ok_or("the macOS cookie file's .ROBLOSECURITY cookie holds no usable value"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const USER: &str = "12345";
    const COOKIE: &str = "dummy-studio-cookie-0123456789";

    /// A fake Credential Manager over `entries` (target name, stored bytes)
    fn lookup_in(entries: &[(String, Vec<u8>)]) -> StudioCookie {
        let store: HashMap<String, Vec<u8>> = entries.iter().cloned().collect();
        let read = |name: &str| -> Result<Option<Blob>, String> { Ok(store.get(name).map(|bytes| Blob(bytes.clone()))) };
        let names = |filter: &str| -> Result<Vec<String>, String> { Ok(store.keys().filter(|name| name.starts_with(filter.trim_end_matches('*'))).cloned().collect()) };
        from_credentials(&read, &names)
    }

    fn utf16(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    fn found(cookie: &str) -> StudioCookie {
        StudioCookie::Found { cookie: cookie.into(), store: CREDENTIAL_MANAGER }
    }

    #[test]
    fn only_one_switches_it_on() {
        assert!(enabled(Some("1")));
        for off in ["0", "true", "yes", " 1", "1 ", ""] {
            assert!(!enabled(Some(off)), "{off:?}");
        }
        assert!(!enabled(None));
        // Off, the store is not asked
        assert_eq!(lookup(&|_| None), StudioCookie::Off);
        assert_eq!(lookup(&|name| (name == SWITCH).then(|| "true".to_string())), StudioCookie::Off);
    }

    #[test]
    fn statuses_never_hold_a_value() {
        assert_eq!(StudioCookie::Off.status(), "off (set ROR_STUDIO_COOKIE=1)");
        assert_eq!(StudioCookie::NotFound.status(), "not found");
        assert_eq!(StudioCookie::Unsupported.status(), "not available on this platform");
        assert_eq!(found(COOKIE).status(), "found (Windows Credential Manager)");
        assert_eq!(format!("{:?}", found(COOKIE)), "Found(Windows Credential Manager)");
    }

    #[test]
    fn the_signed_in_accounts_cookie() {
        let cookie_target = format!("{TARGET}.ROBLOSECURITY{USER}");
        // as Studio writes them, UTF-8; UTF-16 and a trailing NUL read the same
        let entries = [(format!("{TARGET}userid"), USER.as_bytes().to_vec()), (cookie_target.clone(), COOKIE.as_bytes().to_vec())];
        assert_eq!(lookup_in(&entries), found(COOKIE));
        let entries = [(format!("{TARGET}userid"), utf16(USER)), (cookie_target.clone(), utf16(&format!("{COOKIE}\0")))];
        assert_eq!(lookup_in(&entries), found(COOKIE));
        // a `.ROBLOSECURITY=` prefix is dropped, as in env files
        let entries = [(format!("{TARGET}userid"), USER.as_bytes().to_vec()), (cookie_target.clone(), format!(".ROBLOSECURITY={COOKIE}").into_bytes())];
        assert_eq!(lookup_in(&entries), found(COOKIE));
        // another account's cookie is never taken for the signed-in one's
        let entries = [(format!("{TARGET}userid"), USER.as_bytes().to_vec()), (format!("{TARGET}.ROBLOSECURITY999"), COOKIE.as_bytes().to_vec())];
        assert_eq!(lookup_in(&entries), StudioCookie::NotFound);
    }

    #[test]
    fn without_a_user_id_the_one_account() {
        // a build that kept one account without its id
        assert_eq!(lookup_in(&[(format!("{TARGET}.ROBLOSECURITY"), COOKIE.as_bytes().to_vec())]), found(COOKIE));
        // the one account with a cookie; the other entries Studio keeps are not cookies
        let entries = [(format!("{TARGET}.ROBLOSECURITY{USER}"), COOKIE.as_bytes().to_vec()), (format!("{TARGET}accessToken{USER}"), b"other".to_vec())];
        assert_eq!(lookup_in(&entries), found(COOKIE));
        // several: which is signed in is unknown
        let entries = [(format!("{TARGET}.ROBLOSECURITY1"), COOKIE.as_bytes().to_vec()), (format!("{TARGET}.ROBLOSECURITY2"), COOKIE.as_bytes().to_vec())];
        assert!(matches!(lookup_in(&entries), StudioCookie::Unreadable(_)));
        assert_eq!(lookup_in(&[]), StudioCookie::NotFound);
    }

    #[test]
    fn what_cannot_be_read_is_said_without_a_value() {
        let entries = [(format!("{TARGET}userid"), USER.as_bytes().to_vec()), (format!("{TARGET}.ROBLOSECURITY{USER}"), b"dummy value with spaces".to_vec())];
        assert_eq!(lookup_in(&entries).status(), "unreadable: the Windows Credential Manager entry holds no usable cookie");
        let failing = from_credentials(&|_| Err("Windows Credential Manager error 5".into()), &|_| Ok(Vec::new()));
        assert_eq!(failing.status(), "unreadable: Windows Credential Manager error 5");
    }

    #[test]
    fn the_registry_value_of_older_builds() {
        let value = format!("SEC::<YES>,EXP::<2099-01-01T00:00:00Z>,COOK::<{COOKIE}>");
        assert_eq!(from_registry_value(&value), StudioCookie::Found { cookie: COOKIE.into(), store: REGISTRY });
        assert_eq!(from_registry_value("SEC::<YES>,EXP::<2099-01-01>").status(), "unreadable: the Windows registry entry holds no cookie");
        assert_eq!(from_registry_value("COOK::<A B>").status(), "unreadable: the Windows registry entry holds no usable cookie");
    }

    /// One cookie record: the header (size, flags, the strings' offsets, expiry and creation), then the strings
    fn record(domain: &str, name: &str, value: &str, expiry: f64) -> Vec<u8> {
        const HEADER: usize = 56;
        let (mut offsets, mut strings) = (Vec::new(), Vec::new());
        for text in [domain, name, "/", value] {
            offsets.push((HEADER + strings.len()) as u32);
            strings.extend_from_slice(text.as_bytes());
            strings.push(0);
        }
        let mut record = Vec::new();
        for word in [(HEADER + strings.len()) as u32, 0, 1, 0, offsets[0], offsets[1], offsets[2], offsets[3], 0, 0] {
            record.extend(word.to_le_bytes());
        }
        record.extend(expiry.to_le_bytes());
        record.extend(0f64.to_le_bytes());
        record.extend(strings);
        record
    }

    fn page(records: &[Vec<u8>]) -> Vec<u8> {
        let mut page = vec![0, 0, 1, 0];
        page.extend((records.len() as u32).to_le_bytes());
        let mut offset = 8 + 4 * records.len() + 4;
        for record in records {
            page.extend((offset as u32).to_le_bytes());
            offset += record.len();
        }
        page.extend([0; 4]);
        for record in records {
            page.extend(record);
        }
        page
    }

    fn cookie_file(pages: &[Vec<u8>]) -> Vec<u8> {
        let mut file = b"cook".to_vec();
        file.extend((pages.len() as u32).to_be_bytes());
        for page in pages {
            file.extend((page.len() as u32).to_be_bytes());
        }
        for page in pages {
            file.extend(page);
        }
        // the checksum and the footer, which the reader does not need
        file.extend([0; 4]);
        file.extend([0x07, 0x17, 0x20, 0x05, 0, 0, 0, 0x4b]);
        file
    }

    #[test]
    fn the_roblox_cookie_in_a_binary_cookie_file() {
        let parsed = |file: &[u8]| parse_binary_cookies(file).unwrap();
        let file = cookie_file(&[page(&[record(".example.com", ".ROBLOSECURITY", "dummy-other-site", 9e8), record(".roblox.com", "RBXEventTracker", "dummy-not-it", 9e8), record(".roblox.com", ".ROBLOSECURITY", COOKIE, 9e8)])]);
        assert_eq!(parsed(&file).as_deref(), Some(COOKIE));
        // a subdomain counts; lookalike domains do not
        assert_eq!(parsed(&cookie_file(&[page(&[record("www.roblox.com", ".ROBLOSECURITY", COOKIE, 9e8)])])).as_deref(), Some(COOKIE));
        for domain in [".notroblox.com", "roblox.com.example", ".roblox.co"] {
            assert_eq!(parsed(&cookie_file(&[page(&[record(domain, ".ROBLOSECURITY", COOKIE, 9e8)])])), None, "{domain}");
        }
        // on another page, and the one that expires last of two
        let file = cookie_file(&[page(&[record(".roblox.com", ".ROBLOSECURITY", "dummy-older", 8e8)]), page(&[record(".roblox.com", ".ROBLOSECURITY", COOKIE, 9e8)])]);
        assert_eq!(parsed(&file).as_deref(), Some(COOKIE));
        assert_eq!(parsed(&cookie_file(&[])), None);
        assert_eq!(parsed(&cookie_file(&[page(&[])])), None);
    }

    #[test]
    fn a_malformed_binary_cookie_file_is_an_error_not_a_panic() {
        let good = cookie_file(&[page(&[record(".roblox.com", ".ROBLOSECURITY", COOKIE, 9e8)])]);
        assert_eq!(parse_binary_cookies(b"nope").unwrap_err(), "the macOS cookie file is not a binary cookie file");
        // cut anywhere before its page ends, or any byte set to 0xff
        for length in 0..good.len() - 12 {
            let error = parse_binary_cookies(&good[..length]);
            assert!(error.is_err(), "cut at {length}");
            assert!(!format!("{error:?}").contains(COOKIE));
        }
        for at in 4..good.len() {
            let mut broken = good.clone();
            broken[at] = 0xff;
            let _ = parse_binary_cookies(&broken);
        }
        // a value that is no cookie value
        let file = cookie_file(&[page(&[record(".roblox.com", ".ROBLOSECURITY", "A B", 9e8)])]);
        assert_eq!(parse_binary_cookies(&file).unwrap_err(), "the macOS cookie file's .ROBLOSECURITY cookie holds no usable value");
    }

    /// The Windows calls on names no credential has: nothing is found, and nothing real is read
    #[cfg(windows)]
    #[test]
    fn the_credential_manager_answers_a_name_it_lacks() {
        let name = format!("rur-test-absent-{}", std::process::id());
        assert!(os::credential(&name).unwrap().is_none());
        assert!(os::credential_names(&format!("{name}*")).unwrap().is_empty());
    }
}
