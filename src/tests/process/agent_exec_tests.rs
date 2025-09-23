#[cfg(test)]
mod tests {
    use crate::process::agent_exec::command_execution;
    use std::env;
    use std::fs;
    use std::panic;
    use std::path::PathBuf;
    use std::fs::create_dir_all;

    const TEST_AGENT_ID: &str = &"62e1e7a6-79af-47ae-ac4a-8324c2b82197";
    const CLEANUP_ENABLED: bool = true;

    fn compute_working_dir() -> PathBuf {
        let current_exe_path = env::current_exe().unwrap();
        current_exe_path.parent().unwrap().to_path_buf()
    }

    fn cleanup_after_tests() {
        if CLEANUP_ENABLED {
            let working_dir = compute_working_dir();
            let path = working_dir.join("runtimes").join(format!("execution-{}", TEST_AGENT_ID));
            let _ = fs::remove_dir_all(path);
        }
    }

    #[test]
    fn test_simple_execution_no_panic() {
        let working_dir = compute_working_dir();
        create_dir_all(working_dir.join("runtimes")).expect("Failed to create runtimes directory");
        let result = panic::catch_unwind(|| {
            return command_execution(TEST_AGENT_ID, &"echo 'Hello World'");
        });
        cleanup_after_tests();
        assert!(result.unwrap().is_ok());
    }
}
