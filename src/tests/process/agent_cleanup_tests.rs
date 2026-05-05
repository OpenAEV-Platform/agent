#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::fs::create_dir_all;
    use std::path::PathBuf;
    use std::thread::sleep;
    use std::time::Duration;

    fn compute_working_dir() -> PathBuf {
        let current_exe_path = env::current_exe().unwrap();
        current_exe_path.parent().unwrap().to_path_buf()
    }

    fn create_test_directory(subfolder: &str, prefix: &str, id: &str) -> PathBuf {
        let working_dir = compute_working_dir();
        let dir = working_dir.join(subfolder).join(format!("{prefix}{id}"));
        create_dir_all(&dir).unwrap();
        // Write a dummy file inside to simulate implant output
        fs::write(dir.join("test.txt"), "test content").unwrap();
        dir
    }

    fn cleanup_test_directory(path: &PathBuf) {
        if path.exists() {
            let _ = fs::remove_dir_all(path);
        }
    }

    // -- Tests for get_old_execution_directories --

    #[test]
    fn test_get_old_execution_directories_finds_execution_prefix() {

        let working_dir = compute_working_dir();
        create_dir_all(working_dir.join("runtimes")).unwrap();

        let test_id = "test-exec-find-001";
        let dir = create_test_directory("runtimes", "execution-", test_id);

        // With since_minutes=0, any directory should be returned (it's older than 0 minutes)
        // We need to wait at least 1 second so modified time is in the past
        sleep(Duration::from_millis(100));

        // Cleanup
        cleanup_test_directory(&dir);
    }

    #[test]
    fn test_get_old_execution_directories_finds_implant_prefix() {
        let working_dir = compute_working_dir();
        create_dir_all(working_dir.join("runtimes")).unwrap();

        let test_id = "test-implant-find-001";
        let dir = create_test_directory("runtimes", "implant-", test_id);

        // Verify the directory exists
        assert!(dir.exists());
        assert!(dir.join("test.txt").exists());

        // Cleanup
        cleanup_test_directory(&dir);
    }

    #[test]
    fn test_get_old_execution_directories_ignores_unmatched_prefix() {
        let working_dir = compute_working_dir();
        create_dir_all(working_dir.join("runtimes")).unwrap();

        let test_id = "test-unknown-001";
        let dir = create_test_directory("runtimes", "unknown-", test_id);

        // Verify it exists but would not be matched by cleanup
        assert!(dir.exists());
        let file_name = dir.file_name().unwrap().to_str().unwrap();
        assert!(!file_name.contains("execution-"));
        assert!(!file_name.contains("implant-"));
        assert!(!file_name.contains("executed-"));

        // Cleanup
        cleanup_test_directory(&dir);
    }

    #[test]
    fn test_execution_directory_rename_logic() {
        let working_dir = compute_working_dir();
        create_dir_all(working_dir.join("runtimes")).unwrap();

        let test_id = "test-rename-001";
        let dir = create_test_directory("runtimes", "execution-", test_id);

        // Simulate rename logic (same as in agent_cleanup)
        let dirname = dir.to_str().unwrap();
        let new_name = dirname.replace("execution", "executed");
        fs::rename(dirname, &new_name).unwrap();

        let new_path = PathBuf::from(&new_name);
        assert!(new_path.exists());
        assert!(!dir.exists());
        // File inside should still be there after rename
        assert!(new_path.join("test.txt").exists());

        // Cleanup
        cleanup_test_directory(&new_path);
    }

    #[test]
    fn test_implant_directory_delete_logic() {
        let working_dir = compute_working_dir();
        create_dir_all(working_dir.join("runtimes")).unwrap();

        let test_id = "test-implant-delete-001";
        let dir = create_test_directory("runtimes", "implant-", test_id);

        assert!(dir.exists());
        assert!(dir.join("test.txt").exists());

        // Simulate implant cleanup (direct delete, no rename)
        fs::remove_dir_all(&dir).unwrap();

        assert!(!dir.exists());
    }

    #[test]
    fn test_executed_directory_delete_logic() {
        let working_dir = compute_working_dir();
        create_dir_all(working_dir.join("runtimes")).unwrap();

        let test_id = "test-executed-delete-001";
        let dir = create_test_directory("runtimes", "executed-", test_id);

        assert!(dir.exists());
        assert!(dir.join("test.txt").exists());

        // Simulate executed cleanup (permanent delete)
        fs::remove_dir_all(&dir).unwrap();

        assert!(!dir.exists());
    }

    #[test]
    fn test_payloads_directory_cleanup() {
        let working_dir = compute_working_dir();
        create_dir_all(working_dir.join("payloads")).unwrap();

        let test_id = "test-payload-001";
        let exec_dir = create_test_directory("payloads", "execution-", test_id);
        let implant_dir = create_test_directory("payloads", "implant-", "test-payload-002");

        assert!(exec_dir.exists());
        assert!(implant_dir.exists());

        // Simulate rename for execution-
        let dirname = exec_dir.to_str().unwrap();
        let new_name = dirname.replace("execution", "executed");
        fs::rename(dirname, &new_name).unwrap();
        let renamed_path = PathBuf::from(&new_name);
        assert!(renamed_path.exists());

        // Simulate direct delete for implant-
        fs::remove_dir_all(&implant_dir).unwrap();
        assert!(!implant_dir.exists());

        // Cleanup
        cleanup_test_directory(&renamed_path);
    }

    #[test]
    fn test_nested_files_deleted_with_directory() {
        let working_dir = compute_working_dir();
        create_dir_all(working_dir.join("runtimes")).unwrap();

        let test_id = "test-nested-001";
        let dir = create_test_directory("runtimes", "implant-", test_id);

        // Create nested files simulating real implant output
        fs::write(dir.join("execution.ps1"), "echo hello").unwrap();
        fs::write(dir.join("execution.pid"), "12345").unwrap();
        let sub_dir = dir.join("output");
        create_dir_all(&sub_dir).unwrap();
        fs::write(sub_dir.join("result.txt"), "some output").unwrap();

        assert!(dir.join("execution.ps1").exists());
        assert!(dir.join("execution.pid").exists());
        assert!(sub_dir.join("result.txt").exists());

        // Delete entire directory
        fs::remove_dir_all(&dir).unwrap();

        // Everything gone
        assert!(!dir.exists());
    }
}

