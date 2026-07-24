use crate::config::settings::CleanupSettings;
use crate::THREADS_CONTROL;
use log::{error, info, warn};
use std::fs::{DirEntry, File};
use std::io::{Error, Write};
use std::panic;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::thread::{sleep, JoinHandle};
use std::time::{Duration, SystemTime};
use std::{env, fs, thread};

// Prefix for active execution directories (renamed to executed- after kill)
const EXECUTION_PREFIX: &str = "execution-";
// Prefix for directories pending permanent deletion
const EXECUTED_PREFIX: &str = "executed-";

// Lists directories under `subfolder` whose name contains `prefix` (a prefix marker,
// EXECUTION_PREFIX or EXECUTED_PREFIX) and whose last-modified time is older than
// `since_minutes`.
//
// NB: entries are collected best-effort. A single unreadable/racy entry (e.g. removed
// concurrently by another process, or a clock skew making `duration_since` fail) must
// never abort the whole scan: it is logged and skipped instead, so the cleanup thread
// keeps making progress on every other directory.
pub(crate) fn get_old_execution_directories(
    subfolder: &str,
    prefix: &str,
    since_minutes: u64,
) -> Result<Vec<DirEntry>, Error> {
    let now = SystemTime::now();
    let current_exe_path = env::current_exe()?;
    let executable_path = current_exe_path.parent().ok_or_else(|| {
        Error::new(
            std::io::ErrorKind::NotFound,
            "Cannot resolve parent directory of current executable",
        )
    })?;
    let entries = fs::read_dir(executable_path.join(subfolder))?;

    let mut matches = Vec::new();
    for entry in entries {
        let file_entry = match entry {
            Ok(e) => e,
            Err(err) => {
                // A racy directory listing (entry removed/renamed concurrently) must not
                // abort the whole scan: skip it, it will be picked up on a later cycle
                // if it still needs cleaning up.
                warn!("[cleanup thread] Skipping unreadable directory entry in {subfolder}: {err}");
                continue;
            }
        };

        let metadata = match fs::metadata(file_entry.path()) {
            Ok(m) => m,
            Err(err) => {
                warn!(
                    "[cleanup thread] Skipping entry with unreadable metadata {:?}: {err}",
                    file_entry.path()
                );
                continue;
            }
        };

        let file_name_str = file_entry.file_name().to_string_lossy().into_owned();
        if !metadata.is_dir() || !file_name_str.contains(prefix) {
            continue;
        }

        let file_modified = match metadata.modified() {
            Ok(m) => m,
            Err(err) => {
                warn!(
                    "[cleanup thread] Skipping entry with unreadable modified time {:?}: {err}",
                    file_entry.path()
                );
                continue;
            }
        };

        // duration_since fails if file_modified is in the future relative to `now`
        // (clock skew, NTP resync, VM snapshot/resume). Treat as "not old enough yet"
        // rather than crashing the whole cleanup cycle.
        let old_minutes = match now.duration_since(file_modified) {
            Ok(d) => d.as_secs() / 60,
            Err(_) => {
                warn!(
                    "[cleanup thread] Entry {:?} has a modified time in the future, skipping this cycle",
                    file_entry.path()
                );
                continue;
            }
        };

        if old_minutes > since_minutes {
            matches.push(file_entry);
        }
    }
    Ok(matches)
}

// Creates the per-OS kill helper script used by kill_processes_for_directory.
// Failing to (re)create it is logged but must not panic: the current cycle simply
// won't be able to kill stale processes, but renaming/deleting old directories
// (the actual source of disk growth) can still proceed.
fn create_cleanup_scripts() {
    let current_exe_path = match env::current_exe() {
        Ok(p) => p,
        Err(err) => {
            error!("[cleanup thread] Cannot resolve current executable path: {err}");
            return;
        }
    };
    let executable_path = match current_exe_path.parent() {
        Some(p) => p,
        None => {
            error!("[cleanup thread] Cannot resolve parent directory of current executable");
            return;
        }
    };

    if cfg!(target_os = "windows") {
        let script_file_name = executable_path.join("openaev_agent_kill.ps1");
        if let Err(err) = write_script_file(
            &script_file_name,
            "param ([Parameter(Mandatory)]$location); echo $location; $pids = Get-process | where {$_.Path -imatch [regex]::Escape($location)} | Select-Object -ExpandProperty Id; foreach ($process_pid in $pids) { Stop-Process -ID $process_pid -Force };",
        ) {
            error!("[cleanup thread] Failed to write kill script {:?}: {err}", script_file_name);
        }
    }
    if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
        let script_file_name = executable_path.join("openaev_agent_kill.sh");
        if let Err(err) = write_script_file(
            &script_file_name,
            "for pid in $(ps axwww -o pid,command | grep $1 | grep -v openaev_agent_kill.sh | grep -v grep | awk '{print $1}'); do kill -9 $pid; done",
        ) {
            error!("[cleanup thread] Failed to write kill script {:?}: {err}", script_file_name);
        }
    }
}

fn write_script_file(path: &std::path::Path, content: &str) -> Result<(), Error> {
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())
}

// Kills processes still running under `dirname`. Spawning the helper script can fail
// (execution policy, AV/EDR blocking the spawn) - this is logged and treated as
// non-fatal so the caller can still proceed to rename/delete the directory.
fn kill_processes_for_directory(dirname: &str) {
    let escaped_dirname = format!("\"{dirname}\"");
    let result = if cfg!(target_os = "windows") {
        Command::new("powershell")
            .args([
                "-ExecutionPolicy",
                "Bypass",
                "openaev_agent_kill.ps1",
                escaped_dirname.as_str(),
            ])
            .output()
    } else {
        Command::new("bash")
            .args(["openaev_agent_kill.sh", dirname])
            .output()
    };

    if let Err(err) = result {
        error!("[cleanup thread] Failed to spawn kill script for {dirname}: {err}");
    }
}

// Runs a single cleanup cycle: kill+rename stale execution- directories, then remove
// stale executed- directories, for both runtimes and payloads.
// Wrapped by the caller in catch_unwind, so this must not be relied upon as the
// only safety net - but it is also written defensively (no bare unwrap()) so that
// legitimate, expected error cases never reach a panic in the first place.
fn run_cleanup_cycle(executing_max_time: u64, directory_max_time: u64) {
    // region Handle killing old execution- directories
    for (subfolder, should_kill) in [("runtimes", true), ("payloads", false)] {
        let directories =
            match get_old_execution_directories(subfolder, EXECUTION_PREFIX, executing_max_time) {
                Ok(dirs) => dirs,
                Err(err) => {
                    error!(
                        "[cleanup thread] Failed to scan {subfolder} for stale execution directories: {err}"
                    );
                    continue;
                }
            };
        for dir in directories {
            let dirname = dir.path().to_string_lossy().into_owned();
            if should_kill {
                info!("[cleanup thread] Killing process for runtime directory {dirname}");
                kill_processes_for_directory(&dirname);
            }
            info!("[cleanup thread] Renaming {subfolder} directory {dirname}");
            if let Err(err) = fs::rename(&dirname, dirname.replace("execution", "executed")) {
                error!("[cleanup thread] Failed to rename {subfolder} directory {dirname}: {err}");
            }
        }
    }
    // endregion

    // region Handle remove of old executed- directories
    for subfolder in ["runtimes", "payloads"] {
        let directories =
            match get_old_execution_directories(subfolder, EXECUTED_PREFIX, directory_max_time) {
                Ok(dirs) => dirs,
                Err(err) => {
                    error!(
                        "[cleanup thread] Failed to scan {subfolder} for stale executed directories: {err}"
                    );
                    continue;
                }
            };
        for dir in directories {
            let dir_path = dir.path();
            let dirname = dir_path.to_string_lossy().into_owned();
            info!("[cleanup thread] Removing {subfolder} directory {dirname}");
            if let Err(err) = fs::remove_dir_all(&dir_path) {
                error!("[cleanup thread] Failed to remove {subfolder} directory {dirname}: {err}");
            }
        }
    }
    // endregion
}

pub fn clean(cleanup: CleanupSettings) -> Result<JoinHandle<()>, Error> {
    info!("Starting cleanup thread");
    let handle = thread::spawn(move || {
        let executing_max_time = cleanup.executing_max_time_minutes;
        let directory_max_time = cleanup.directory_max_time_minutes;
        let cleanup_interval = cleanup.cleanup_interval_seconds;

        // Create kill scripts once at startup (content is static)
        create_cleanup_scripts();

        // While no stop signal received
        while THREADS_CONTROL.load(Ordering::Relaxed) {
            // Safety net: run_cleanup_cycle is written to avoid panicking, but this
            // catch_unwind ensures that ANY unforeseen panic (including from future
            // changes/dependencies) only skips this single cycle instead of killing
            // the cleanup thread for the remaining lifetime of the process - which is
            // what caused unbounded disk growth in production (thread killed once,
            // never respawned, even across reboots since the same failure recurs).
            if let Err(panic_payload) = panic::catch_unwind(|| {
                run_cleanup_cycle(executing_max_time, directory_max_time);
            }) {
                let cause = panic_payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic_payload.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "<cause unknown>".to_string());
                error!(
                    "[cleanup thread] Cleanup cycle panicked, will retry next cycle: {cause}"
                );
            }
            // Wait for the next cleanup
            sleep(Duration::from_secs(cleanup_interval));
        }
    });
    Ok(handle)
}
