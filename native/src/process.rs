//! Process and thread hygiene shared by every native entry point:
//!
//! - [`configure_rayon`]: caps the global rayon pool to the process's OS CPU affinity mask (at most
//!   8 threads, matching `rur-native serve`), respecting `RAYON_NUM_THREADS`.
//! - [`watch_parent`]: delegates to the emulator's parent-process watchdog so headless CLI children
//!   exit promptly if their caller disappears (`RUR_PARENT_WATCH=0` / `REE_PARENT_WATCH=0` opts out).
//! - [`attach_child`]: binds spawned helper children (viewport workers, batch children, ffmpeg) to a
//!   Windows Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, so no worker can linger after its
//!   parent exits or is terminated.
use std::process::Child;

/// Logical processors available to this process, respecting the OS affinity mask on Windows.
pub fn affinity_cores() -> usize {
    #[cfg(windows)]
    {
        unsafe extern "system" {
            fn GetCurrentProcess() -> isize;
            fn GetProcessAffinityMask(
                process: isize,
                process_affinity_mask: *mut usize,
                system_affinity_mask: *mut usize,
            ) -> i32;
        }
        let mut process_mask: usize = 0;
        let mut system_mask: usize = 0;
        // SAFETY: out-pointers to locals on the current process pseudo-handle
        if unsafe { GetProcessAffinityMask(GetCurrentProcess(), &mut process_mask, &mut system_mask) } != 0
            && process_mask != 0
        {
            return process_mask.count_ones() as usize;
        }
    }
    std::thread::available_parallelism().map_or(1, usize::from)
}

/// Sizes rayon's global pool once at process startup: UI rasters and directory walks gain nothing
/// past 8 threads, while spawning one thread per hardware core explodes thread count when several
/// engine processes run side by side. Leaves `RAYON_NUM_THREADS` untouched when the caller set it.
pub fn configure_rayon() {
    if std::env::var_os("RAYON_NUM_THREADS").is_some() {
        return;
    }
    let threads = affinity_cores().clamp(1, 8);
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .thread_name(|index| format!("rur-rayon-{index}"))
        .build_global();
}

/// Starts the parent-process watchdog for a CLI command unless `RUR_PARENT_WATCH=0` or
/// `REE_PARENT_WATCH=0`. Long-lived background daemons (`serve`) and interactive apps do not call
/// this.
pub fn watch_parent(command: &str) {
    if std::env::var("RUR_PARENT_WATCH").is_ok_and(|value| value == "0")
        || std::env::var("REE_PARENT_WATCH").is_ok_and(|value| value == "0")
    {
        return;
    }
    crate::engine::watchdog::watch_parent(command);
}

/// Assigns `child` to a process-wide Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` on
/// Windows so exiting (including `std::process::exit` or termination) immediately tears down every
/// spawned helper process.
pub fn attach_child(child: &Child) {
    #[cfg(windows)]
    {
        use std::sync::OnceLock;
        static JOB: OnceLock<Option<win::ChildJob>> = OnceLock::new();
        if let Some(job) = JOB.get_or_init(win::ChildJob::new) {
            let _ = job.assign(child);
        }
    }
    #[cfg(not(windows))]
    let _ = child;
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: u32 = 9;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;

    #[repr(C)]
    #[derive(Default)]
    struct JobObjectBasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    pub(super) struct JobObjectExtendedLimitInformation {
        basic_limit_information: JobObjectBasicLimitInformation,
        io_info: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    unsafe extern "system" {
        fn CreateJobObjectW(security_attributes: *const c_void, name: *const u16) -> *mut c_void;
        fn SetInformationJobObject(
            job: *mut c_void,
            info_class: u32,
            info: *const c_void,
            info_length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    pub(super) struct ChildJob {
        handle: *mut c_void,
    }

    // SAFETY: Windows Job Object handles are thread-safe kernel handles.
    unsafe impl Send for ChildJob {}
    unsafe impl Sync for ChildJob {}

    impl ChildJob {
        pub(super) fn new() -> Option<Self> {
            // SAFETY: null attributes and name create an unnamed job owned by this process
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return None;
            }
            let mut info = JobObjectExtendedLimitInformation::default();
            info.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            // SAFETY: valid job handle and matching extended limit info struct size
            let ok = unsafe {
                SetInformationJobObject(
                    handle,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                    (&info as *const JobObjectExtendedLimitInformation).cast::<c_void>(),
                    std::mem::size_of::<JobObjectExtendedLimitInformation>() as u32,
                )
            };
            if ok == 0 {
                // SAFETY: handle was returned by CreateJobObjectW above
                unsafe { CloseHandle(handle) };
                return None;
            }
            Some(Self { handle })
        }

        pub(super) fn assign(&self, child: &Child) -> bool {
            // SAFETY: valid job handle and child's live Win32 process handle
            unsafe { AssignProcessToJobObject(self.handle, child.as_raw_handle().cast::<c_void>()) != 0 }
        }
    }

    impl Drop for ChildJob {
        fn drop(&mut self) {
            // SAFETY: handle is owned by ChildJob and closed once on drop
            unsafe { CloseHandle(self.handle) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affinity_cores_is_at_least_one() {
        assert!(affinity_cores() >= 1);
    }

    #[cfg(windows)]
    #[test]
    fn job_object_kills_child_when_closed() {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        #[cfg(target_pointer_width = "64")]
        assert_eq!(std::mem::size_of::<win::JobObjectExtendedLimitInformation>(), 144);

        let job = win::ChildJob::new().expect("CreateJobObjectW");
        let node = std::env::var_os("RUR_NODE")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("node"));
        let mut child = Command::new(node)
            .args(["-e", "setInterval(() => {}, 60000);"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .spawn()
            .expect("spawn child");
        assert!(job.assign(&child), "AssignProcessToJobObject");
        assert!(matches!(child.try_wait(), Ok(None)), "child should still be running");
        let started = Instant::now();
        drop(job);
        let _status = child.wait().expect("wait on terminated child");
        // Windows terminates processes in a KILL_ON_JOB_CLOSE job immediately (exit code 0);
        // the child's setInterval would otherwise keep it alive indefinitely.
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "job close should terminate child promptly, took {:?}",
            started.elapsed()
        );
    }
}
