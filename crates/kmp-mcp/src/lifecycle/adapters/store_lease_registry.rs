//! Every store lease this process holds, one open lease file per store.
//!
//! The lock is a POSIX record lock (`F_SETLK`), owned by the process rather
//! than by a file description. `flock` belonged to the description, and a
//! child shares every description between `fork` and `exec`, so a host that
//! spawned anything while holding a claim lent the claim to that child for a
//! moment. A record lock is never inherited, so no spawn can lend it.
//!
//! Record locks carry two rules this registry exists to keep. The kernel sees
//! one owner per process, so two claims in this process never conflict there;
//! the registry keeps the in-process meaning instead (shared claims coexist,
//! an exclusive one stands alone). And closing any descriptor of the file
//! drops every lock the process has on it, so the file is opened once per
//! store and closed only after the last claim is released.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use super::lease_claim::LeaseClaim;
use super::lease_claim_error::LeaseClaimError;
use super::store_lease_files::open_store_lease;

/// Per lease path: the open file, how many shared claims use it, and whether
/// an exclusive claim holds it.
type Held = HashMap<PathBuf, (File, usize, bool)>;

fn held() -> MutexGuard<'static, Held> {
    static HELD: OnceLock<Mutex<Held>> = OnceLock::new();
    HELD.get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Claim the store's lease, shared or exclusive, without waiting.
pub(crate) fn claim(
    data_home: &Path,
    store: &Path,
    exclusive: bool,
) -> Result<LeaseClaim, LeaseClaimError> {
    let mut held = held();
    let path = super::store_lease_files::store_lease_path(data_home, store);
    if let Some((_, shared, held_exclusively)) = held.get_mut(&path) {
        if exclusive || *held_exclusively {
            return Err(LeaseClaimError::Busy);
        }
        *shared += 1;
        return Ok(LeaseClaim { path });
    }
    let (file, path) = open_store_lease(data_home, store).map_err(LeaseClaimError::Io)?;
    lock(&file, exclusive).map_err(|error| match error {
        LockFailure::Busy => LeaseClaimError::Busy,
        LockFailure::Io(error) => LeaseClaimError::Io(format!(
            "could not claim store-use lock `{}`: {error}",
            path.display()
        )),
    })?;
    held.insert(path.clone(), (file, usize::from(!exclusive), exclusive));
    Ok(LeaseClaim { path })
}

/// Drop one claim; the last one unlocks and closes the file.
pub(super) fn release(path: &PathBuf) {
    let mut held = held();
    let last = match held.get_mut(path) {
        Some((_, shared, exclusive)) if !*exclusive && *shared > 1 => {
            *shared -= 1;
            false
        }
        Some(_) => true,
        None => false,
    };
    if last && let Some((file, _, _)) = held.remove(path) {
        unlock(&file);
    }
}

enum LockFailure {
    Busy,
    Io(std::io::Error),
}

#[cfg(unix)]
fn lock(file: &File, exclusive: bool) -> Result<(), LockFailure> {
    use rustix::fs::{FlockOperation, fcntl_lock};
    let operation = if exclusive {
        FlockOperation::NonBlockingLockExclusive
    } else {
        FlockOperation::NonBlockingLockShared
    };
    fcntl_lock(file, operation).map_err(|errno| {
        if errno == rustix::io::Errno::AGAIN || errno == rustix::io::Errno::ACCESS {
            LockFailure::Busy
        } else {
            LockFailure::Io(errno.into())
        }
    })
}

#[cfg(unix)]
fn unlock(file: &File) {
    let _ = rustix::fs::fcntl_lock(file, rustix::fs::FlockOperation::NonBlockingUnlock);
}

/// Windows has no `fork`; its handle locks are never lent.
#[cfg(not(unix))]
fn lock(file: &File, exclusive: bool) -> Result<(), LockFailure> {
    use std::fs::TryLockError;
    let attempt = if exclusive {
        file.try_lock()
    } else {
        file.try_lock_shared()
    };
    attempt.map_err(|error| match error {
        TryLockError::WouldBlock => LockFailure::Busy,
        TryLockError::Error(error) => LockFailure::Io(error),
    })
}

#[cfg(not(unix))]
fn unlock(file: &File) {
    let _ = file.unlock();
}
