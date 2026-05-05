use crate::config::settings::CleanupSettings;
use crate::THREADS_CONTROL;
use log::{error, info, warn};
use std::fs::{self, DirEntry, File};
use std::io::{Error, ErrorKind, Write};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::thread::{self, sleep, JoinHandle};
use std::time::{Duration, SystemTime};
use std::env;

// Prefixes that identify active execution directories (both legacy and implant)
const EXECUTION_PREFIXES: [&str; 2] = ["execution-", "implant-"];
const EXECUTED_PREFIXES: [&str; 1] = ["executed-"];

fn executable_path() -> Result<std::path::PathBuf, Error> {
    let current_exe_path = env::current_exe()?;
    current_exe_path
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::NotFound,
                "Cannot resolve parent directory of current executable",
            )
        })
}

fn get_old_directories(
    subfolder: &str,
    prefixes: &[&str],
    since_minutes: u64,
) -> Result<Vec<DirEntry>, Error> {
    let now = SystemTime::now();
    let base = executable_path()?.join(subfolder);
    let entries = fs::read_dir(base)?;

    let mut old_dirs = Vec::new();
    for item in entries {
        let entry = match item {
            Ok(e) => e,
            Err(e) => {
                warn!("[cleanup thread] Failed reading directory entry in {subfolder}: {e}");
                continue;
            }
        };

        let file_name = entry.file_name();
        let Some(name_str) = file_name.to_str() else {
            continue;
        };

        if !prefixes.iter().any(|p| name_str.starts_with(p)) {
            continue;
        }

        let metadata = match fs::metadata(entry.path()) {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    "[cleanup thread] Failed reading metadata for {:?}: {e}",
                    entry.path()
                );
                continue;
            }
        };

        if !metadata.is_dir() {
            continue;
        }

        let modified = match metadata.modified() {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    "[cleanup thread] Failed reading modified time for {:?}: {e}",
                    entry.path()
                );
                continue;
            }
        };

        let age_minutes = match now.duration_since(modified) {
            Ok(d) => d.as_secs() / 60,
            Err(e) => {
                warn!(
                    "[cleanup thread] Clock skew for {:?}: {e}",
                    entry.path()
                );
                continue;
            }
        };

        if age_minutes > since_minutes {
            old_dirs.push(entry);
        }
    }

    Ok(old_dirs)
}

fn create_cleanup_scripts() -> Result<(), Error> {
    let base = executable_path()?;

    if cfg!(target_os = "windows") {
        let script_file_name = base.join("openaev_agent_kill.ps1");
        let mut file = File::create(script_file_name)?;
        // This script will take a specific path in parameter
        // Base on this path, all process matching except grep and current script are detected and then killed
        file.write_all(
            "param ([Parameter(Mandatory)]$location); \
             $pids = Get-Process | Where-Object { $_.Path -and $_.Path -imatch [regex]::Escape($location) } | Select-Object -ExpandProperty Id; \
             foreach ($process_pid in $pids) { Stop-Process -Id $process_pid -Force }"
                .as_bytes(),
        )?;
    }

    if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
        let script_file_name = base.join("openaev_agent_kill.sh");
        let mut file = File::create(script_file_name)?;
        // This script will take a specific path in parameter
        // Base on this path, all process matching except grep and current script are detected and then killed
        file.write_all(
            "for pid in $(ps axwww -o pid,command | grep \"$1\" | grep -v openaev_agent_kill.sh | grep -v grep | awk '{print $1}'); do kill -9 \"$pid\"; done"
                .as_bytes(),
        )?;
    }

    Ok(())
}

fn rename_to_executed(path: &Path, prefixes: &[&str]) -> Result<(), Error> {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Cannot convert directory name to UTF-8",
        ));
    };

    let mut new_name = None;
    for prefix in prefixes {
        if let Some(suffix) = name.strip_prefix(prefix) {
            new_name = Some(format!("executed-{suffix}"));
            break;
        }
    }

    let Some(target_name) = new_name else {
        return Ok(());
    };

    let new_path = path.with_file_name(target_name);
    fs::rename(path, new_path)
}

fn kill_processes_for_directory(dirname: &str) -> Result<(), Error> {
    let base = executable_path()?;

    if cfg!(target_os = "windows") {
        let script = base.join("openaev_agent_kill.ps1");
        Command::new("powershell")
            .args([
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                script.to_string_lossy().as_ref(),
                dirname,
            ])
            .output()?;
    }

    if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
        let script = base.join("openaev_agent_kill.sh");
        Command::new("bash")
            .args([script.to_string_lossy().as_ref(), dirname])
            .output()?;
    }

    Ok(())
}

pub fn clean(cleanup: CleanupSettings) -> Result<JoinHandle<()>, Error> {
    info!("Starting cleanup thread");
    let handle = thread::spawn(move || {
        // Create the expected script per operating system.
        if let Err(e) = create_cleanup_scripts() {
            error!("[cleanup thread] Failed to create cleanup scripts: {e}");
        }

        let executing_max_time = cleanup.executing_max_time_minutes;
        let directory_max_time = cleanup.directory_max_time_minutes;
        let cleanup_interval = cleanup.cleanup_interval_seconds.max(10);

        info!(
            "[cleanup thread] Config: executing_max_time={}min, directory_max_time={}min, interval={}s, prefixes={:?}",
            executing_max_time, directory_max_time, cleanup_interval, EXECUTION_PREFIXES
        );

        // While no stop signal received
        while THREADS_CONTROL.load(Ordering::Relaxed) {
            // region Handle killing old execution-/implant- directories in runtimes
            match get_old_directories("runtimes", &EXECUTION_PREFIXES, executing_max_time) {
                Ok(dirs) => {
                    for dir in dirs {
                        let dir_path = dir.path();
                        let dirname = dir_path.to_string_lossy().to_string();
                        info!("[cleanup thread] Killing process for directory {dirname}");
                        if let Err(e) = kill_processes_for_directory(&dirname) {
                            warn!(
                                "[cleanup thread] Failed to kill processes in {dirname}: {e}"
                            );
                        }
                        // After kill, rename from execution-/implant- to executed-
                        if let Err(e) = rename_to_executed(&dir_path, &EXECUTION_PREFIXES) {
                            warn!(
                                "[cleanup thread] Failed to rename runtime directory {dirname}: {e}"
                            );
                        }
                    }
                }
                Err(e) => warn!("[cleanup thread] Failed scanning runtimes for kill/rename: {e}"),
            }

            // Handle killing old execution-/implant- directories in payloads
            match get_old_directories("payloads", &EXECUTION_PREFIXES, executing_max_time) {
                Ok(dirs) => {
                    for dir in dirs {
                        let dir_path = dir.path();
                        let dirname = dir_path.to_string_lossy().to_string();
                        if let Err(e) = rename_to_executed(&dir_path, &EXECUTION_PREFIXES) {
                            warn!(
                                "[cleanup thread] Failed to rename payload directory {dirname}: {e}"
                            );
                        }
                    }
                }
                Err(e) => warn!("[cleanup thread] Failed scanning payloads for rename: {e}"),
            }
            // endregion

            // region Handle remove of old executed- directories
            match get_old_directories("runtimes", &EXECUTED_PREFIXES, directory_max_time) {
                Ok(dirs) => {
                    for dir in dirs {
                        let dir_path = dir.path();
                        let dirname = dir_path.to_string_lossy().to_string();
                        info!("[cleanup thread] Removing directory {dirname}");
                        if let Err(e) = fs::remove_dir_all(&dir_path) {
                            warn!(
                                "[cleanup thread] Failed removing runtime directory {dirname}: {e}"
                            );
                        }
                    }
                }
                Err(e) => warn!("[cleanup thread] Failed scanning runtimes for deletion: {e}"),
            }

            match get_old_directories("payloads", &EXECUTED_PREFIXES, directory_max_time) {
                Ok(dirs) => {
                    for dir in dirs {
                        let dir_path = dir.path();
                        let dirname = dir_path.to_string_lossy().to_string();
                        info!("[cleanup thread] Removing directory {dirname}");
                        if let Err(e) = fs::remove_dir_all(&dir_path) {
                            warn!(
                                "[cleanup thread] Failed removing payload directory {dirname}: {e}"
                            );
                        }
                    }
                }
                Err(e) => warn!("[cleanup thread] Failed scanning payloads for deletion: {e}"),
            }
            // endregion

            // Wait for the next cleanup cycle
            sleep(Duration::from_secs(cleanup_interval));
        }
    });
    Ok(handle)
}
