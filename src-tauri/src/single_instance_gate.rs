//! Cross-process single-instance gate that runs before ANY startup side effect.
//!
//! The Tauri single-instance plugin only evaluates inside `Builder::build`,
//! which is after the bundled engine, the embedded Tailscale daemon and the
//! database are already started. A second launch (for example the OS opening
//! an `adc://` delivery link while the app is running) therefore spawned its
//! own `transmission-daemon` on the same config directory, took the peer
//! port, and was then killed by the plugin, orphaning that daemon. The
//! orphan kept holding the port while the real instance's daemon could not
//! bind it.
//!
//! The gate takes an exclusive advisory lock on a file in the data directory
//! for the life of the primary process. A secondary process writes its
//! arguments to a hand-off file the primary polls for, then exits without
//! having started anything.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static PRIMARY_LOCK: OnceLock<File> = OnceLock::new();

/// Hand-offs older than this are dropped: the user asked for them a while
/// ago and a surprise open of a stale link would be worse than losing it.
const HANDOFF_MAX_AGE: Duration = Duration::from_secs(120);
/// How long a secondary waits for a primary that is shutting down to release
/// the lock before it takes over as the primary itself.
const WAIT_FOR_PRIMARY: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const CONSUMER_INTERVAL: Duration = Duration::from_millis(400);

#[derive(Debug, PartialEq, Eq)]
pub enum Gate {
    /// This process owns the instance lock and must start normally.
    Primary,
    /// The arguments were handed to a running instance; exit immediately.
    HandedOff,
}

fn gate_dir(internal_dir: &Path) -> PathBuf {
    internal_dir.join("instance")
}

fn handoff_dir(internal_dir: &Path) -> PathBuf {
    gate_dir(internal_dir).join("handoff")
}

/// Takes the exclusive lock, or reports that another process holds it.
/// `Ok(None)` means the lock is held elsewhere; filesystems that cannot lock
/// at all fall back to `Ok(Some(..))` so startup is never blocked by the gate.
fn try_take_lock(lock_path: &Path) -> std::io::Result<Option<File>> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(lock_path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => {
            tracing::warn!("single-instance lock unavailable ({e}); continuing without the gate");
            Ok(Some(file))
        }
    }
}

fn encode_args(args: &[String]) -> Vec<u8> {
    args.join("\0").into_bytes()
}

fn decode_args(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .split('\0')
        .map(str::to_string)
        .collect()
}

fn write_handoff(dir: &Path, args: &[String]) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let final_path = dir.join(format!("{stamp}-{}.args", std::process::id()));
    let tmp_path = final_path.with_extension("tmp");
    let mut f = File::create(&tmp_path)?;
    f.write_all(&encode_args(args))?;
    f.sync_all()?;
    drop(f);
    fs::rename(&tmp_path, &final_path)?;
    Ok(final_path)
}

/// Decides whether this process is the primary instance. Must be the first
/// thing `run()` does. Failures to prepare the gate never block startup.
pub fn acquire(internal_dir: &Path) -> Gate {
    let dir = gate_dir(internal_dir);
    if fs::create_dir_all(&dir).is_err() {
        return Gate::Primary;
    }
    let lock_path = dir.join("primary.lock");
    match try_take_lock(&lock_path) {
        Ok(Some(file)) => {
            let _ = PRIMARY_LOCK.set(file);
            return Gate::Primary;
        }
        Ok(None) => {}
        Err(_) => return Gate::Primary,
    }

    let args: Vec<String> = std::env::args().collect();
    let handoff = match write_handoff(&handoff_dir(internal_dir), &args) {
        Ok(p) => p,
        Err(_) => return Gate::Primary,
    };

    let deadline = std::time::Instant::now() + WAIT_FOR_PRIMARY;
    while std::time::Instant::now() < deadline {
        if !handoff.exists() {
            return Gate::HandedOff;
        }
        if let Ok(Some(file)) = try_take_lock(&lock_path) {
            // The previous primary exited before reading the hand-off; this
            // process starts normally and receives the arguments directly.
            let _ = fs::remove_file(&handoff);
            let _ = PRIMARY_LOCK.set(file);
            return Gate::Primary;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    Gate::HandedOff
}

/// Drains hand-off files left by secondary launches and returns their argv.
pub fn drain_handoffs(internal_dir: &Path) -> Vec<Vec<String>> {
    let dir = handoff_dir(internal_dir);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "args"))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for path in files {
        let fresh = fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_none_or(|age| age <= HANDOFF_MAX_AGE);
        let bytes = fs::read(&path);
        let _ = fs::remove_file(&path);
        if let (true, Ok(bytes)) = (fresh, bytes) {
            out.push(decode_args(&bytes));
        }
    }
    out
}

/// Polls for hand-offs for the life of the primary process.
pub fn spawn_consumer<F>(internal_dir: PathBuf, handler: F)
where
    F: Fn(Vec<String>) + Send + 'static,
{
    let _ = std::thread::Builder::new()
        .name("instance-handoff".into())
        .spawn(move || loop {
            for argv in drain_handoffs(&internal_dir) {
                handler(argv);
            }
            std::thread::sleep(CONSUMER_INTERVAL);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nebulae-gate-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn second_lock_attempt_is_refused_until_the_first_is_dropped() {
        let dir = scratch("lock");
        let path = dir.join("primary.lock");
        let first = try_take_lock(&path).unwrap().expect("first takes the lock");
        assert!(try_take_lock(&path).unwrap().is_none());
        drop(first);
        assert!(try_take_lock(&path).unwrap().is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn handoff_round_trips_arguments_including_spaces_and_urls() {
        let dir = scratch("roundtrip");
        let args = vec![
            "nebulae.exe".to_string(),
            format!("adc://delivery/{}", "ab".repeat(32)),
            "C:\\Users\\me\\My Torrents\\a b.torrent".to_string(),
        ];
        write_handoff(&handoff_dir(&dir), &args).unwrap();
        let drained = drain_handoffs(&dir);
        assert_eq!(drained, vec![args]);
        assert!(drain_handoffs(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_handoffs_are_discarded() {
        let dir = scratch("stale");
        let hd = handoff_dir(&dir);
        fs::create_dir_all(&hd).unwrap();
        let path = hd.join("1-1.args");
        fs::write(&path, b"nebulae\0magnet:?xt=urn:btih:aa").unwrap();
        let old = SystemTime::now() - HANDOFF_MAX_AGE - Duration::from_secs(5);
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        assert!(drain_handoffs(&dir).is_empty());
        assert!(!path.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
