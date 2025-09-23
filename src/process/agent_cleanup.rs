use crate::THREADS_CONTROL;
use log::info;
use std::fs::{DirEntry, File};
use std::io::{Error, Write};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::thread::{sleep, JoinHandle};
use std::time::{Duration, SystemTime};
use std::{env, fs, thread};

// The executing max time will prevent started process to remains active.
// After X minutes define in this constant, all process under 'execution-' sub dirs will be killed
static EXECUTING_MAX_TIME: u64 = 20; // 20 minutes
                                     // The storing directory max time will prevent too much disk space usage.
                                     // After X minutes define in this constant, all dir matching 'execution-' will be removed
static DIRECTORY_MAX_TIME: u64 = 2880; // 2 days

fn get_old_execution_directories(subfolder: &str, path: &str, since_minutes: u64) -> Result<Vec<DirEntry>, Error> {
    let now = SystemTime::now();
    let current_exe_patch = env::current_exe().unwrap();
    let executable_path = current_exe_patch.parent().unwrap();
    let entries = fs::read_dir(executable_path.join(subfolder)).unwrap();
    entries
        .into_iter()
        .filter(|entry| {
            let file_entry = entry.as_ref().unwrap();
            let file_name = file_entry.file_name();
            let metadata = fs::metadata(file_entry.path()).unwrap();
            let file_name_str = file_name.to_str().unwrap();
            if metadata.is_dir() && String::from(file_name_str).contains(path) {
                let file_modified = metadata.modified().unwrap();
                let old_minutes = now.duration_since(file_modified).unwrap().as_secs() / 60;
                return old_minutes > since_minutes;
            }
            false
        })
        .collect()
}

fn create_cleanup_scripts() {
    let current_exe_patch = env::current_exe().unwrap();
    let executable_path = current_exe_patch.parent().unwrap();
    if cfg!(target_os = "windows") {
        let script_file_name = executable_path.join("openbas_agent_kill.ps1");
        let mut file = File::create(script_file_name.clone()).unwrap();
        // This script will take a specific path in parameter
        // Base on this path, all process matching except grep and current script are detected and then killed
        file.write_all("param ([Parameter(Mandatory)]$location); echo $location; $pids = Get-process | where {$_.Path -imatch [regex]::Escape($location)} | Select-Object -ExpandProperty Id; foreach ($process_pid in $pids) { Stop-Process -ID $process_pid -Force };".as_bytes()).unwrap();
    }
    if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
        let script_file_name = executable_path.join("openbas_agent_kill.sh");
        let mut file = File::create(script_file_name.clone()).unwrap();
        // This script will take a specific path in parameter
        // Base on this path, all process matching except grep and current script are detected and then killed
        file.write_all("for pid in $(ps axwww -o pid,command | grep $1 | grep -v openbas_agent_kill.sh | grep -v grep | awk '{print $1}'); do kill -9 $pid; done".as_bytes()).unwrap();
    }
}

pub fn clean() -> Result<JoinHandle<()>, Error> {
    info!("Starting cleanup thread");
    let handle = thread::spawn(move || {
        // Create the expected script per operating system.
        create_cleanup_scripts();
        // While no stop signal received
        while THREADS_CONTROL.load(Ordering::Relaxed) {
            let kill_runtimes_directories =
                get_old_execution_directories("runtimes", "execution-", EXECUTING_MAX_TIME).unwrap();
            // region Handle killing old execution- directories
            for dir in kill_runtimes_directories {
                let dir_path = dir.path();
                let dirname = dir_path.to_str().unwrap();
                info!("[cleanup thread] Killing process for directory {dirname}");
                let escaped_dirname = format!("\"{dirname}\"");
                if cfg!(target_os = "windows") {
                    Command::new("powershell")
                        .args([
                            "-ExecutionPolicy",
                            "Bypass",
                            "openbas_agent_kill.ps1",
                            escaped_dirname.as_str(),
                        ])
                        .output()
                        .unwrap();
                }
                if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
                    Command::new("bash")
                        .args(["openbas_agent_kill.sh", dirname])
                        .output()
                        .unwrap();
                }
                // After kill, rename from execution to executed
                fs::rename(dirname, dirname.replace("execution", "executed")).unwrap();
            }
            let rename_payloads_directories = get_old_execution_directories("payloads", "execution-", EXECUTING_MAX_TIME).unwrap();
            for dir in rename_payloads_directories {
                let dir_path = dir.path();
                let dirname = dir_path.to_str().unwrap();
                fs::rename(dirname, dirname.replace("execution", "executed")).unwrap();
            }
            // endregion
            // region Handle remove of old executed- directories
            let remove_runtimes_directories =
                get_old_execution_directories("runtimes", "executed-", DIRECTORY_MAX_TIME).unwrap();
            for dir in remove_runtimes_directories {
                let dir_path = dir.path();
                let dirname = dir_path.to_str().unwrap();
                info!("[cleanup thread] Removing directory {dirname}");
                fs::remove_dir_all(dir_path).unwrap()
            }
            let remove_payloads_directories =
                get_old_execution_directories("payloads", "executed-", DIRECTORY_MAX_TIME).unwrap();
            for dir in remove_payloads_directories {
                let dir_path = dir.path();
                let dirname = dir_path.to_str().unwrap();
                info!("[cleanup thread] Removing directory {dirname}");
                fs::remove_dir_all(dir_path).unwrap()
            }
            // endregion
            // Wait for the next cleanup (3 minutes)
            sleep(Duration::from_secs(3 * 60));
        }
    });
    Ok(handle)
}
