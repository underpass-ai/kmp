use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use kmp_mcp::lifecycle::{StoreRemovalGuard, StoreSessionLease, store_leases_dir};

const MODE_ENV: &str = "KMP_LEASE_TEST_MODE";
const DATA_HOME_ENV: &str = "KMP_LEASE_TEST_DATA_HOME";
const STORE_ENV: &str = "KMP_LEASE_TEST_STORE";
const READY_ENV: &str = "KMP_LEASE_TEST_READY";
const RELEASE_ENV: &str = "KMP_LEASE_TEST_RELEASE";

fn store_at(base: &Path) -> PathBuf {
    let store = base.join("project/.kernel");
    fs::create_dir_all(store.join("store")).expect("store directory");
    fs::write(store.join("FORMAT_VERSION"), "2").expect("format stamp");
    fs::write(store.join("store/kernel.sqlite3"), [0_u8; 8]).expect("sqlite placeholder");
    store
}

fn child(mode: &str, data_home: &Path, store: &Path, ready: &Path, release: &Path) -> Child {
    Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", mode, "--nocapture"])
        .env(MODE_ENV, mode)
        .env(DATA_HOME_ENV, data_home)
        .env(STORE_ENV, store)
        .env(READY_ENV, ready)
        .env(RELEASE_ENV, release)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("lease child starts")
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        path.exists(),
        "child did not signal readiness: {}",
        path.display()
    );
}

fn child_paths(base: &Path, label: &str) -> (PathBuf, PathBuf) {
    (
        base.join(format!("{label}.ready")),
        base.join(format!("{label}.release")),
    )
}

#[test]
fn lease_identity_and_directory_cardinality_are_bounded_per_store() {
    let base = tempfile::tempdir().expect("scratch");
    let data_home = base.path().join("data");
    let store = store_at(base.path());
    let alias = store.join("..").join(".kernel");

    let first = StoreSessionLease::acquire(&data_home, &store).expect("first lease");
    let lease_dir = store_leases_dir(&data_home);
    let first_entry = fs::read_dir(&lease_dir)
        .expect("lease directory")
        .next()
        .expect("one target lease")
        .expect("lease entry");
    #[cfg(unix)]
    let first_inode = inode(&first_entry.path());
    drop(first);

    let second = StoreSessionLease::acquire(&data_home, &alias).expect("canonical alias lease");
    let entries: Vec<_> = fs::read_dir(&lease_dir)
        .expect("lease directory")
        .map(|entry| entry.expect("lease entry").path())
        .collect();
    assert_eq!(entries.len(), 1, "aliases share one lease identity");
    #[cfg(unix)]
    assert_eq!(
        inode(&entries[0]),
        first_inode,
        "reopen preserves the lock inode"
    );
    drop(second);

    let other = base.path().join("other/.kernel");
    fs::create_dir_all(&other).expect("other store");
    let _other = StoreSessionLease::acquire(&data_home, &other).expect("other lease");
    let count = fs::read_dir(lease_dir).expect("lease directory").count();
    assert_eq!(
        count, 2,
        "one stable lease file per distinct canonical store"
    );
}

#[test]
fn live_old_and_current_owners_block_removal_but_crash_releases_the_lock() {
    let base = tempfile::tempdir().expect("scratch");
    let data_home = base.path().join("data");
    let store = store_at(base.path());

    let (ready, release) = child_paths(base.path(), "shared");
    let mut shared = child(
        "child_holds_shared_lease",
        &data_home,
        &store,
        &ready,
        &release,
    );
    wait_for(&ready);
    let local = StoreSessionLease::acquire(&data_home, &store).expect("shared owner coexists");
    let refusal = StoreRemovalGuard::acquire(&data_home, &store)
        .err()
        .expect("exclusive removal waits behind a live current owner");
    assert!(refusal.contains("active"), "{refusal}");
    drop(local);
    File::create(&release).expect("release shared child");
    assert!(shared.wait().expect("shared child waits").success());
    StoreRemovalGuard::acquire(&data_home, &store)
        .expect("store becomes removable after current owner exits");

    #[cfg(target_os = "linux")]
    {
        let (ready, release) = child_paths(base.path(), "old");
        let mut old = child(
            "child_holds_old_store_file",
            &data_home,
            &store,
            &ready,
            &release,
        );
        wait_for(&ready);
        let refusal = StoreRemovalGuard::acquire(&data_home, &store)
            .err()
            .expect("exclusive removal detects a pre-lease live owner");
        assert!(refusal.contains("active"), "{refusal}");
        File::create(&release).expect("release old child");
        assert!(old.wait().expect("old child waits").success());
        StoreRemovalGuard::acquire(&data_home, &store)
            .expect("store becomes removable after old owner exits");
    }

    let (ready, release) = child_paths(base.path(), "crash");
    let mut crashed = child(
        "child_terminates_without_drop",
        &data_home,
        &store,
        &ready,
        &release,
    );
    wait_for(&ready);
    assert!(crashed.wait().expect("crashed child waits").success());
    StoreRemovalGuard::acquire(&data_home, &store)
        .expect("an exited owner releases the OS lock without PID inspection");
}

#[cfg(unix)]
fn inode(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;

    fs::metadata(path).expect("lease metadata").ino()
}

#[test]
fn child_holds_shared_lease() {
    if std::env::var(MODE_ENV).ok().as_deref() != Some("child_holds_shared_lease") {
        return;
    }
    let data_home = PathBuf::from(std::env::var_os(DATA_HOME_ENV).expect("data home"));
    let store = PathBuf::from(std::env::var_os(STORE_ENV).expect("store"));
    let ready = PathBuf::from(std::env::var_os(READY_ENV).expect("ready"));
    let release = PathBuf::from(std::env::var_os(RELEASE_ENV).expect("release"));
    let _lease = StoreSessionLease::acquire(&data_home, &store).expect("child shared lease");
    File::create(ready).expect("ready");
    while !release.exists() {
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn child_holds_old_store_file() {
    if std::env::var(MODE_ENV).ok().as_deref() != Some("child_holds_old_store_file") {
        return;
    }
    let store = PathBuf::from(std::env::var_os(STORE_ENV).expect("store"));
    let ready = PathBuf::from(std::env::var_os(READY_ENV).expect("ready"));
    let release = PathBuf::from(std::env::var_os(RELEASE_ENV).expect("release"));
    let _store = OpenOptions::new()
        .read(true)
        .open(store.join("store/kernel.sqlite3"))
        .expect("old host store file");
    File::create(ready).expect("ready");
    while !release.exists() {
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn child_terminates_without_drop() {
    if std::env::var(MODE_ENV).ok().as_deref() != Some("child_terminates_without_drop") {
        return;
    }
    let data_home = PathBuf::from(std::env::var_os(DATA_HOME_ENV).expect("data home"));
    let store = PathBuf::from(std::env::var_os(STORE_ENV).expect("store"));
    let ready = PathBuf::from(std::env::var_os(READY_ENV).expect("ready"));
    let _lease = StoreSessionLease::acquire(&data_home, &store).expect("child crash lease");
    File::create(ready).expect("ready");
    // Simulate a process termination while the descriptor is still held. The
    // OS closes it without running Rust destructors, just as for a crash.
    std::process::exit(0);
}
