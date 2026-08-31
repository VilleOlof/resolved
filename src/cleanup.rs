use std::{
    fs::{DirEntry, OpenOptions, TryLockError},
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use resolved_shared::RESOLVED_ROOT;
use tokio::fs::{create_dir_all, remove_dir_all, remove_file};

use crate::{Error, script_handler::MODULE_NAME};

/// A global .lock file in the resolved dir so that only one handler can attempt a cleanup at a time.  
///
/// If another instance tries to cleanup and fails to get the lock it will just not attempt a cleanup and continue
const LOCK_FILE: &str = ".lock";

/// Configuration to use when checking if a cleanup should occur or if at all
#[derive(Debug, PartialEq, Clone)]
pub struct CleanupConfig {
    /// Amount of temporary directories needed to run the cleanup
    pub count: usize,
    /// Directores to remove must be at least this amount of time old
    pub age: Duration,
    /// This skips cleanup and never runs it
    pub skip: bool,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            count: 256,
            age: Duration::from_secs(12 * 60 * 60),
            skip: false,
        }
    }
}

/// Ensures the [`RESOLVED_ROOT`] directory exists for instances.  
///
/// Also spawns a background task that checks if a cleanup should start and if so begins to clean stale, old files
pub async fn check(config: CleanupConfig) -> Result<(), Error> {
    let base = RESOLVED_ROOT.clone();

    // we must run this so we for sure have our resolved root
    if !base.exists() {
        create_dir_all(&base).await?;
    }

    // its also fine if the panic crashes for some other reason here
    // like it will just cleanup some other time
    tokio::spawn(async move {
        let lock_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(base.join(LOCK_FILE))
            .expect("failed to open lock file");

        match lock_file.try_lock() {
            Ok(()) => (),
            Err(TryLockError::WouldBlock) => return,
            Err(e) => {
                eprintln!("failed to see lock: {e:?}");
                return;
            }
        }

        // maybe we'd wanna figure out if these fail more
        let dir_count = base.read_dir().map_or(0, std::iter::Iterator::count);
        // we must also have enough old directories
        // but we first do a simple check and then a more time taking check
        if dir_count <= config.count && !has_enough_old(&base, &config).unwrap_or(false) {
            return;
        }

        if let Err(e) = run_cleanup(base, &config).await {
            eprintln!("{e:?}");
        }

        lock_file.unlock().expect("failed to unlock lock file");
    });

    Ok(())
}

fn has_enough_old(base: &Path, config: &CleanupConfig) -> Result<bool, Error> {
    let now = SystemTime::now();
    let mut old_dirs = 0;
    for dir in base.read_dir()? {
        let dir = dir?;
        if is_old(&dir, now, config.age)? {
            old_dirs += 1;
        }

        // early return, weve gotten enough
        if old_dirs > config.count {
            return Ok(true);
        }
    }
    Ok(false)
}

fn is_old(dir: &DirEntry, now: SystemTime, age: Duration) -> Result<bool, Error> {
    let created = dir.metadata()?.created()?;
    if now.duration_since(created).is_ok_and(|d| d > age) {
        Ok(true)
    } else {
        Ok(false)
    }
}

async fn run_cleanup(base: PathBuf, config: &CleanupConfig) -> Result<(), Error> {
    let now = SystemTime::now();

    let mut cleaned = 0;
    for dir in base.read_dir()? {
        let dir = dir?;

        // this should only be the .lock file
        if dir.file_type()?.is_file() {
            continue;
        }

        if !is_old(&dir, now, config.age)? {
            continue;
        }

        // if we can remove the `vinci.dll` file we can delete the entire dir as that file is the only locking one
        let module_file = dir.path().join(format!("{MODULE_NAME}.dll"));
        match remove_file(&module_file).await {
            // if we didnt find it, we can go ahead and clean it up anyhow
            Err(e) if e.kind() == ErrorKind::NotFound => (),
            Err(_) => {
                // dll was locked, so we skip, currently in use probably
                continue;
            }
            Ok(()) => (),
        }

        remove_dir_all(dir.path()).await?; // spookys
        cleaned += 1;
    }

    #[cfg(feature = "tracing")]
    tracing::trace!(?cleaned, "cleaned old instance directories");
    let _ = cleaned;

    Ok(())
}
