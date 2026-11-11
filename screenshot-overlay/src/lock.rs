//! Single-instance lock: one overlay at a time, arbitrated by the kernel.
//!
//! Two overlays mean two fullscreen layer surfaces stacked on each other — the
//! top one takes every click and absorbs every pointer event, and the bottom one
//! is dead weight until the first is dismissed. The launcher cannot prevent that
//! on its own: it only sees the launches it made, and its view of the process
//! table is a cached, substring-matched guess. A file lock in the user's runtime
//! directory is the authority instead, and it has the property that makes it safe
//! to rely on: **the kernel drops the lock when the holder dies**, including on
//! `SIGKILL`, so there is no stale state to clean up and no way to wedge the
//! overlay permanently.
//!
//! `std::fs::File::try_lock` does the locking. On Linux that is `flock(2)`, which
//! is the semantics this needs: the lock belongs to the *open file description*,
//! so two opens contend even inside one process (which is what makes the tests
//! below possible), and it is released when the description goes away. The
//! alternatives are worse fits: a **PID file** outlives a crash and needs a
//! staleness heuristic (plus PID-reuse bugs), and **`fcntl` record locks** are
//! dropped by *any* close of the file in the process — a trap for the next
//! refactor.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions, TryLockError};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// The file name, in whichever directory `lock_path` settles on.
const NAME: &str = "screenshot-overlay.lock";

/// Held for as long as the overlay runs.
///
/// The lock lives on this descriptor, so it must stay alive for the whole run —
/// dropping it releases the lock while the overlay is still on screen. `main`
/// keeps it in a local for exactly that reason.
#[derive(Debug)]
pub struct Lock {
    _file: File,
}

/// Whether this process may run the overlay.
#[derive(Debug)]
pub enum Availability {
    /// Nobody else held it, and it is ours now.
    Held(Lock),
    /// Another overlay is already running. Not an error — it is the answer.
    Busy,
}

/// Take the single-instance lock.
///
/// Being told the overlay is already up is `Ok(Availability::Busy)`. Failing to
/// *reach* the lock file is an `Err`, and the caller has to decide what that
/// means: the lock is the only thing standing between the user and two stacked
/// overlays, so a lock file we cannot reach is worth refusing over rather than
/// pressing on without the guarantee.
pub fn acquire() -> Result<Availability, String> {
    acquire_at(&lock_path()?)
}

/// Where the lock lives, from the environment as it stands.
fn lock_path() -> Result<PathBuf, String> {
    lock_path_from(
        std::env::var_os("XDG_RUNTIME_DIR").as_deref(),
        std::env::var_os("XDG_CACHE_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

/// The same, from explicit values, so a test can cover every branch without
/// mutating the process environment (which is racy across threads).
///
/// Every candidate is already private to the user, so the file name itself needs
/// no uid — and adding one would not help: the file is `0600`, so in a *shared*
/// directory the second user could not even open it, and would be refused
/// outright rather than merely losing the race.
fn lock_path_from(
    runtime: Option<&OsStr>,
    cache: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Result<PathBuf, String> {
    // First choice: private, on tmpfs, cleared at logout. A Wayland session has
    // it by construction — it is where the compositor's socket lives.
    if let Some(dir) = existing_dir(runtime) {
        return Ok(dir.join(NAME));
    }

    // Then the XDG cache directory, which is the user's own. Ordinary for an
    // XDG-aware tool to create, and it has to be created here: unlike the runtime
    // directory, nothing guarantees it exists yet.
    let cache_dir = match cache.filter(|dir| !dir.is_empty()) {
        Some(dir) => Some(PathBuf::from(dir)),
        None => home
            .filter(|dir| !dir.is_empty())
            .map(|dir| PathBuf::from(dir).join(".cache")),
    };
    if let Some(dir) = cache_dir {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        return Ok(dir.join(NAME));
    }

    // Neither set. There is no per-user directory to put it in, and a shared one
    // is worse than not locking at all, so say so rather than degrade silently.
    Err("no XDG_RUNTIME_DIR, XDG_CACHE_HOME or HOME for the lock file".to_string())
}

/// A path that is present and is a directory, or `None`.
fn existing_dir(value: Option<&OsStr>) -> Option<PathBuf> {
    let path = PathBuf::from(value?);
    path.is_dir().then_some(path)
}

/// `acquire` at an explicit path, so a test can use its own and not fight with a
/// real overlay — or with a concurrently running test.
fn acquire_at(path: &Path) -> Result<Availability, String> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        // Owner-only. The directories above are already private, but the mode
        // costs nothing and keeps a future fallback from leaking the lock: a lock
        // another user can open is one they can hold against us.
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;

    match file.try_lock() {
        Ok(()) => Ok(Availability::Held(Lock { _file: file })),
        Err(TryLockError::WouldBlock) => Ok(Availability::Busy),
        Err(TryLockError::Error(e)) => Err(format!("cannot lock {}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ssr-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    /// Two opens of the same file contend, even inside one process — the lock
    /// belongs to the *open file description*, not the process — which is what
    /// makes this testable without spawning a second binary.
    #[test]
    fn a_second_holder_is_refused_and_drop_releases() {
        let path = scratch("basic.lock");

        let first = acquire_at(&path).unwrap();
        assert!(
            matches!(first, Availability::Held(_)),
            "the first acquire must win"
        );

        let second = acquire_at(&path).unwrap();
        assert!(
            matches!(second, Availability::Busy),
            "the second must be told the overlay is already up"
        );

        // The kernel releases the lock when the holder is dropped, which is the
        // same mechanism that makes a crash safe.
        drop(first);
        let third = acquire_at(&path).unwrap();
        assert!(
            matches!(third, Availability::Held(_)),
            "dropping the holder must release the lock"
        );
    }

    /// Separate lock files do not contend: the lock is per overlay, not per run.
    #[test]
    fn different_files_do_not_contend() {
        let a = acquire_at(&scratch("a.lock")).unwrap();
        let b = acquire_at(&scratch("b.lock")).unwrap();
        assert!(matches!(a, Availability::Held(_)));
        assert!(matches!(b, Availability::Held(_)));
    }

    /// An unwritable path is an `Err`, not a silent `Busy` — the caller must be
    /// able to tell "someone else is running this" from "I cannot lock at all".
    #[test]
    fn an_unreachable_path_is_an_error() {
        let missing = scratch("no-such-dir").join("deep").join("x.lock");
        let err = acquire_at(&missing).expect_err("a missing directory cannot be locked");
        assert!(err.contains("cannot open"), "{err}");
    }

    /// The runtime directory wins when it is usable, and is skipped when it is
    /// not — a stale value pointing at a path that no longer exists must not stop
    /// the lock from being taken somewhere real.
    #[test]
    fn the_runtime_directory_wins_when_it_exists() {
        // `scratch` only creates the parent, and the whole point of this branch is
        // that the value has to be a directory that is actually there.
        let dir = scratch("runtime");
        std::fs::create_dir_all(&dir).unwrap();

        let from_runtime = lock_path_from(Some(dir.as_os_str()), None, None).unwrap();
        assert_eq!(from_runtime, dir.join(NAME));

        let missing = scratch("gone");
        let fell_through =
            lock_path_from(Some(missing.as_os_str()), Some(dir.as_os_str()), None).unwrap();
        assert_eq!(
            fell_through,
            dir.join(NAME),
            "a missing runtime dir is skipped"
        );
    }

    /// With no runtime directory the cache directory is used, and created if it
    /// is not there yet — nothing guarantees it exists, unlike the runtime one.
    #[test]
    fn the_cache_directory_is_created_on_demand() {
        let cache = scratch("cache-not-yet");
        let _ = std::fs::remove_dir_all(&cache);

        let path = lock_path_from(None, Some(cache.as_os_str()), None).unwrap();
        assert_eq!(path, cache.join(NAME));
        assert!(cache.is_dir(), "the cache directory must have been created");

        // `HOME` is the last resort, and the same rules apply to `~/.cache`.
        let home = scratch("home");
        let from_home = lock_path_from(None, None, Some(home.as_os_str())).unwrap();
        assert_eq!(from_home, home.join(".cache").join(NAME));
    }

    /// No per-user directory at all is an error rather than a shared lock: the
    /// file is `0600`, so a shared path would not merely be contended — the other
    /// user could not open it, and would be refused outright.
    #[test]
    fn no_per_user_directory_is_an_error() {
        let err = lock_path_from(None, None, None).expect_err("there is nowhere to lock");
        assert!(err.contains("HOME"), "{err}");
    }
}
