//! Where the process's memory is: live heap bytes by the work that allocated them, and the
//! process's own totals.
//!
//! [`process_bytes`] (private bytes and working set) is always there: the session reports it with
//! every reply (`timings.processBytes`, the profiler's `processMB` counter) next to `instances` and
//! `luauHeapBytes`.
//!
//! The breakdown needs a build with feature `memtrace` (`cargo build --release --features
//! memtrace`): the global allocator then puts a 16-byte header before every block, holding its size
//! and the tag of the scope that allocated it ([`scope`]), and keeps a live total per tag. Freed
//! blocks leave the tag they were allocated under, whoever frees them, so a total that keeps
//! growing names the phase whose data is being kept. `RUR_MEMTRACE=1` adds the totals to every
//! session reply (`timings.memory`) and headless benchmark row (`memory`). Without the feature the
//! scopes compile to nothing and the allocator is plain mimalloc.

/// What allocated a block. Scopes nest; a block belongs to the innermost one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Tag {
    /// Anything outside a scope (startup, the UI, worker threads)
    Other = 0,
    /// The engine thread outside the scopes below: scripts (the Luau heap is part of this; see
    /// `luauHeapBytes`), the Instance API, signals, the scheduler
    Script,
    Layout,
    /// The retained export's cache and the one-shot export
    Export,
    /// Loading the project and its modules (sources, bytecode)
    Project,
    /// The display side's scene store
    Scene,
    /// The scroll projection
    Presentation,
    /// Draw preparation (records, raster layers, caches)
    Prepare,
    Gpu,
    Assets,
}

pub const TAGS: [(Tag, &str); 10] = [
    (Tag::Other, "other"),
    (Tag::Script, "script"),
    (Tag::Layout, "layout"),
    (Tag::Export, "export"),
    (Tag::Project, "project"),
    (Tag::Scene, "scene"),
    (Tag::Presentation, "presentation"),
    (Tag::Prepare, "prepare"),
    (Tag::Gpu, "gpu"),
    (Tag::Assets, "assets"),
];

/// (private bytes, working set) of this process; None where it is not measured.
pub fn process_bytes() -> Option<(u64, u64)> {
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Default)]
        struct Counters {
            cb: u32,
            page_fault_count: u32,
            peak_working_set_size: usize,
            working_set_size: usize,
            quota_peak_paged_pool_usage: usize,
            quota_paged_pool_usage: usize,
            quota_peak_non_paged_pool_usage: usize,
            quota_non_paged_pool_usage: usize,
            pagefile_usage: usize,
            peak_pagefile_usage: usize,
            private_usage: usize,
        }
        unsafe extern "system" {
            fn GetCurrentProcess() -> isize;
            fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, size: u32) -> i32;
        }
        let mut counters = Counters { cb: std::mem::size_of::<Counters>() as u32, ..Default::default() };
        let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
        if ok == 0 {
            return None;
        }
        Some((counters.private_usage as u64, counters.working_set_size as u64))
    }
    #[cfg(target_os = "linux")]
    {
        let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
        let mut fields = statm.split_whitespace().map(|field| field.parse::<u64>().ok());
        let (_size, resident, shared) = (fields.next()??, fields.next()??, fields.next()??);
        Some(((resident - shared.min(resident)) * 4096, resident * 4096))
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        None
    }
}

#[cfg(feature = "memtrace")]
mod imp {
    use super::{TAGS, Tag};
    use std::alloc::{GlobalAlloc, Layout};
    use std::cell::Cell;
    use std::sync::atomic::{AtomicIsize, Ordering};

    thread_local! {
        static CURRENT: Cell<u8> = const { Cell::new(0) };
    }
    static LIVE: [AtomicIsize; TAGS.len()] = [const { AtomicIsize::new(0) }; TAGS.len()];

    pub struct Scope(u8);

    impl Drop for Scope {
        fn drop(&mut self) {
            CURRENT.with(|current| current.set(self.0));
        }
    }

    #[inline]
    pub fn scope(tag: Tag) -> Scope {
        Scope(CURRENT.with(|current| current.replace(tag as u8)))
    }

    pub fn live() -> Vec<(&'static str, i64)> {
        TAGS.iter().map(|&(tag, name)| (name, LIVE[tag as usize].load(Ordering::Relaxed) as i64)).collect()
    }

    /// mimalloc with a header per block: the block's tag (the header is as large as the block's
    /// alignment, 16 bytes at least; its last byte before the block is the tag).
    pub struct Tracing(pub mimalloc::MiMalloc);

    #[inline]
    fn header(layout: Layout) -> usize {
        layout.align().max(16)
    }

    unsafe impl GlobalAlloc for Tracing {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let tag = CURRENT.try_with(|current| current.get()).unwrap_or(0);
            unsafe { self.alloc_tagged(layout, tag, false) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let tag = CURRENT.try_with(|current| current.get()).unwrap_or(0);
            unsafe { self.alloc_tagged(layout, tag, true) }
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            let header = header(layout);
            unsafe {
                let tag = *pointer.sub(1);
                LIVE[tag as usize].fetch_sub(layout.size() as isize, Ordering::Relaxed);
                let whole = Layout::from_size_align_unchecked(layout.size() + header, layout.align().max(16));
                self.0.dealloc(pointer.sub(header), whole);
            }
        }

        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            // (a block keeps the tag it was first allocated under)
            unsafe {
                let tag = *pointer.sub(1);
                let new = self.alloc_tagged(Layout::from_size_align_unchecked(size, layout.align()), tag, false);
                if !new.is_null() {
                    std::ptr::copy_nonoverlapping(pointer, new, layout.size().min(size));
                    self.dealloc(pointer, layout);
                }
                new
            }
        }
    }

    impl Tracing {
        unsafe fn alloc_tagged(&self, layout: Layout, tag: u8, zeroed: bool) -> *mut u8 {
            let header = header(layout);
            unsafe {
                let whole = Layout::from_size_align_unchecked(layout.size() + header, layout.align().max(16));
                let base = if zeroed { self.0.alloc_zeroed(whole) } else { self.0.alloc(whole) };
                if base.is_null() {
                    return base;
                }
                let pointer = base.add(header);
                *pointer.sub(1) = tag;
                LIVE[tag as usize].fetch_add(layout.size() as isize, Ordering::Relaxed);
                pointer
            }
        }
    }
}

#[cfg(feature = "memtrace")]
pub use imp::{Scope, Tracing, live, scope};

#[cfg(not(feature = "memtrace"))]
pub struct Scope;

/// Blocks allocated on this thread until the scope ends belong to `tag`.
#[cfg(not(feature = "memtrace"))]
#[inline(always)]
pub fn scope(_tag: Tag) -> Scope {
    Scope
}

/// Live bytes per tag (empty without feature `memtrace`).
#[cfg(not(feature = "memtrace"))]
pub fn live() -> Vec<(&'static str, i64)> {
    Vec::new()
}

/// Whether replies and benchmark rows carry the per-tag totals (`RUR_MEMTRACE=1`, feature `memtrace`).
pub fn reporting() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    cfg!(feature = "memtrace") && *ON.get_or_init(|| std::env::var("RUR_MEMTRACE").as_deref() == Ok("1"))
}

/// The per-tag totals as a JSON object (bytes).
pub fn live_json() -> serde_json::Value {
    serde_json::Value::Object(live().into_iter().map(|(name, bytes)| (name.to_string(), serde_json::json!(bytes))).collect())
}
