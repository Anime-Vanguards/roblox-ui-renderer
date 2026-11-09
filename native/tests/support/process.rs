//! Copying an executable and launching a process, in tests that run on several threads.
//!
//! Use `copy_executable` instead of `std::fs::copy` for a binary a test will run, and
//! `output_locked` / `spawn_locked` instead of `Command::output` / `Command::spawn`. Both
//! sides take `LAUNCH_LOCK`.
use std::{
    io,
    path::Path,
    process::{Child, Command, Output},
    sync::RwLock,
};

/// Keeps a copy of an executable from overlapping any process launch in this test process.
///
/// On Linux a thread that forks while another thread is copying a binary inherits the copy's
/// write descriptor until it execs. Executing the fresh copy then fails with ETXTBSY ("Text file
/// busy"), even after the copying thread has closed its descriptor. So a copy holds the lock
/// exclusively, and every launch holds it shared, including a launch of an unrelated executable:
/// its fork could carry the descriptor of any copy that is in progress.
struct LaunchLock(RwLock<()>);

impl LaunchLock {
    const fn new() -> Self {
        Self(RwLock::new(()))
    }

    /// Runs `copy` with no other copy and no launch running. The lock is released when `copy`
    /// returns, whether it succeeded or not.
    fn exclusive<T>(&self, copy: impl FnOnce() -> T) -> T {
        let _guard = self.0.write().unwrap();
        copy()
    }

    /// Runs `launch` beside other launches, but never during a copy. The lock is released when
    /// `launch` returns, whether it succeeded or not.
    fn shared<T>(&self, launch: impl FnOnce() -> T) -> T {
        let _guard = self.0.read().unwrap();
        launch()
    }
}

static LAUNCH_LOCK: LaunchLock = LaunchLock::new();

/// Copies `source` to `destination` under the exclusive lock (see `LaunchLock`). The destination
/// is closed again before the lock is released.
pub fn copy_executable(source: &Path, destination: &Path) -> io::Result<u64> {
    LAUNCH_LOCK.exclusive(|| std::fs::copy(source, destination))
}

/// `Command::spawn` and `Command::output` under the shared lock (see `LaunchLock`).
pub trait LockedLaunch {
    fn spawn_locked(&mut self) -> io::Result<Child>;
    fn output_locked(&mut self) -> io::Result<Output>;
}

impl LockedLaunch for Command {
    fn spawn_locked(&mut self) -> io::Result<Child> {
        LAUNCH_LOCK.shared(|| self.spawn())
    }

    /// Keeps the stdio defaults of `Command::output`. The lock is held until the child exits, so
    /// a copy waits for the commands that are running; commands do not wait for each other.
    fn output_locked(&mut self) -> io::Result<Output> {
        LAUNCH_LOCK.shared(|| self.output())
    }
}

#[test]
fn a_copy_excludes_other_copies_and_launches() {
    let lock = LaunchLock::new();
    lock.exclusive(|| {
        assert!(lock.0.try_write().is_err());
        assert!(lock.0.try_read().is_err());
    });
    assert!(lock.0.try_write().is_ok());
}

#[test]
fn launches_share_the_lock_and_release_it_after_an_error() {
    let lock = LaunchLock::new();
    let failed_launch: io::Result<()> = lock.shared(|| {
        assert!(lock.0.try_write().is_err());
        assert!(lock.0.try_read().is_ok());
        Err(io::Error::other("test launch failed"))
    });
    assert!(failed_launch.is_err());
    assert!(lock.0.try_write().is_ok());

    let failed_copy: io::Result<()> = lock.exclusive(|| Err(io::Error::other("test copy failed")));
    assert!(failed_copy.is_err());
    assert!(lock.0.try_read().is_ok());
}
