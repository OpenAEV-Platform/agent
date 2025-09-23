use log::info;
use std::env;
use std::fs::{create_dir, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::common::error_model::Error;

fn compute_working_dir(asset_agent_id: &str) -> PathBuf {
    let current_exe_patch = env::current_exe().unwrap();
    let executable_path = current_exe_patch.parent().unwrap();
    executable_path.join("runtimes").join(format!("execution-{asset_agent_id}"))
}

fn command_with_context(asset_agent_id: &str, command: &str) -> String {
    let working_dir = compute_working_dir(asset_agent_id);
    let command_server_location = command.replace("#{location}", working_dir.to_str().unwrap());
    if cfg!(target_os = "windows") {
        return format!(
            "Set-Location -Path \"{}\"; {}",
            working_dir.to_str().unwrap(),
            command_server_location
        );
    }
    if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
        return format!(
            "cd \"{}\"; {}",
            working_dir.to_str().unwrap(),
            command_server_location
        );
    }
    String::from(command)
}

#[cfg(target_os = "windows")]
pub fn command_execution(asset_agent_id: &str, raw_command: &str) -> Result<(), Error> {
    use std::os::windows::process::CommandExt;

    let command = command_with_context(asset_agent_id, raw_command);
    let working_dir = compute_working_dir(asset_agent_id);
    info!(identifier:? = asset_agent_id, command:? = command; "Invoking execution");
    // Write the script in specific directory
    create_dir(working_dir.clone())?;
    let script_file_name = working_dir.join("execution.ps1");
    {
        let mut file = File::create(script_file_name.clone())?;
        file.write_all(command.as_bytes())?;
    }
    // Prepare and execute the command
    let win_path = format!("\"{}\"", script_file_name.to_str().unwrap());
    let command_args = &[
        "/d",
        "/c",
        "powershell.exe",
        "-ExecutionPolicy",
        "Bypass",
        "-WindowStyle",
        "Hidden",
        "-NonInteractive",
        "-NoProfile",
        "-File",
    ];
    let child_execution = Command::new("cmd.exe")
        .args(command_args)
        .raw_arg(win_path.as_str())
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .spawn()?;
    // Save execution pid
    let pid_file_name = working_dir.join("execution.pid");
    {
        let mut file = File::create(pid_file_name.clone())?;
        file.write_all(child_execution.id().to_string().as_bytes())?;
    }
    info!(identifier:? = asset_agent_id; "Invoking result");
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn command_execution(asset_agent_id: &str, raw_command: &str) -> Result<(), Error> {
    let command = command_with_context(asset_agent_id, raw_command);
    let working_dir = compute_working_dir(asset_agent_id);
    info!(identifier = asset_agent_id, command = &command.as_str(); "Invoking execution");
    // Write the script in specific directory
    create_dir(working_dir.clone())?;
    let script_file_name = working_dir.join("execution.sh");
    {
        let mut file = File::create(script_file_name.clone())?;
        file.write_all(command.as_bytes())?;
    }
    // Prepare and execute the command
    let command_args = &[script_file_name.to_str().unwrap(), "&"];
    let mut child_execution = Command::new("bash")
        .args(command_args)
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .spawn()?;
    // Save execution pid
    let pid_file_name = working_dir.join("execution.pid");
    {
        let mut file = File::create(pid_file_name.clone())?;
        file.write_all(child_execution.id().to_string().as_bytes())?;
    }
    info!(identifier = asset_agent_id; "Revoking execution");

    // start a fire-and-forget thread to wait for process termination
    // so as to reap it and avoid zombies
    // we purposefully ignore status codes since the implant reports
    // it directly to the openbas C2 server
    // note that hanging threads may prevent process shutdown that must then be killed
    std::thread::spawn(move || {
        child_execution.wait().expect("Process not running.");
    });
    Ok(())
}
