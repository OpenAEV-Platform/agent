#[cfg(test)]
mod tests {
    use crate::process::agent_cleanup::get_old_execution_directories;
    use std::env;
    use std::fs;
    use std::fs::create_dir_all;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    fn compute_working_dir() -> PathBuf {
        let current_exe_path = env::current_exe().unwrap();
        current_exe_path.parent().unwrap().to_path_buf()
    }

    fn create_test_directory(subfolder: &str, prefix: &str, id: &str) -> PathBuf {
        let working_dir = compute_working_dir();
        let dir = working_dir.join(subfolder).join(format!("{prefix}{id}"));
        create_dir_all(&dir).unwrap();
        // Write a dummy file inside to simulate execution output
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

        // Set mtime to 2 minutes ago so the directory qualifies as "older than 1 minute"
        let past = SystemTime::now() - Duration::from_secs(120);
        filetime::set_file_mtime(&dir, filetime::FileTime::from_system_time(past)).unwrap();

        // Call the actual function: directory with mtime 2 min ago should be returned
        // when since_minutes=1
        let results = get_old_execution_directories("runtimes", "execution-", 1).unwrap();
        let found = results
            .iter()
            .any(|e| e.file_name().to_string_lossy().contains(test_id));
        assert!(found, "expected to find the test directory in results");

        // Cleanup
        cleanup_test_directory(&dir);
    }

    #[test]
    fn test_get_old_execution_directories_ignores_unmatched_prefix() {
        let working_dir = compute_working_dir();
        create_dir_all(working_dir.join("runtimes")).unwrap();

        let test_id = "test-unknown-001";
        let dir = create_test_directory("runtimes", "unknown-", test_id);

        // Set mtime to 2 minutes ago so it would qualify by age
        let past = SystemTime::now() - Duration::from_secs(120);
        filetime::set_file_mtime(&dir, filetime::FileTime::from_system_time(past)).unwrap();

        // Call the actual function: a directory with "unknown-" prefix must not appear
        // in results when scanning for "execution-"
        let results = get_old_execution_directories("runtimes", "execution-", 1).unwrap();
        let found = results
            .iter()
            .any(|e| e.file_name().to_string_lossy().contains(test_id));
        assert!(!found, "directory with unknown- prefix must not be returned");

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

        assert!(exec_dir.exists());

        // Simulate rename for execution-
        let dirname = exec_dir.to_str().unwrap();
        let new_name = dirname.replace("execution", "executed");
        fs::rename(dirname, &new_name).unwrap();
        let renamed_path = PathBuf::from(&new_name);
        assert!(renamed_path.exists());

        // Cleanup
        cleanup_test_directory(&renamed_path);
    }

    // -- Regression tests: resilience to the failure modes that used to panic the
    // -- cleanup thread forever (see agent_cleanup::get_old_execution_directories) --

    #[test]
    fn test_future_mtime_does_not_panic_and_is_treated_as_not_old_enough() {
        // Simulates a clock skew / NTP resync / VM snapshot-resume scenario where a
        // directory's modified time is ahead of "now". Before the fix, computing
        // now.duration_since(file_modified) would return an Err and the bare
        // .unwrap() on it would panic and kill the cleanup thread forever.
        let working_dir = compute_working_dir();
        create_dir_all(working_dir.join("runtimes")).unwrap();

        let test_id = "test-future-mtime-001";
        let dir = create_test_directory("runtimes", "execution-", test_id);

        // Set the directory's modified time to the future to trigger the
        // duration_since failure path inside get_old_execution_directories.
        let future_time = SystemTime::now() + Duration::from_secs(3600);
        filetime::set_file_mtime(
            &dir,
            filetime::FileTime::from_system_time(future_time),
        )
        .unwrap();

        // Must not panic, and must not return the future-mtime directory
        // (it is skipped as "not old enough yet").
        let results = get_old_execution_directories("runtimes", "execution-", 0).unwrap();
        let found = results
            .iter()
            .any(|e| e.file_name().to_string_lossy().contains(test_id));
        assert!(
            !found,
            "directory with future mtime must be skipped, not returned"
        );

        cleanup_test_directory(&dir);
    }

    #[test]
    fn test_concurrently_removed_entry_does_not_abort_the_whole_scan() {
        // Simulates a directory entry that disappears between fs::read_dir() listing
        // it and the subsequent fs::metadata() call (removed by a concurrent cleanup
        // cycle, or by the implant itself). Before the fix, fs::metadata(..).unwrap()
        // on a missing path would panic and kill the cleanup thread forever.
        //
        // We cannot perfectly simulate a racy removal between read_dir iteration and
        // metadata(), but we can verify that a scan with only valid entries succeeds
        // (the defensive error handling does not break normal operation), and that the
        // unit assertions on fs::metadata still hold.
        let working_dir = compute_working_dir();
        create_dir_all(working_dir.join("runtimes")).unwrap();

        let test_id = "test-vanishing-001";
        let dir = create_test_directory("runtimes", "execution-", test_id);

        // Remove it "concurrently" (simulated).
        fs::remove_dir_all(&dir).unwrap();

        // The function must not panic even if entries vanish during the scan.
        // Here the entry is already gone before read_dir, so it simply won't appear.
        let results = get_old_execution_directories("runtimes", "execution-", 0).unwrap();
        let found = results
            .iter()
            .any(|e| e.file_name().to_string_lossy().contains(test_id));
        assert!(!found, "removed directory must not appear in results");
    }

    #[test]
    fn test_one_bad_entry_does_not_prevent_cleanup_of_valid_entries() {
        // Best-effort semantics: among several execution- directories, one that is
        // problematic must not prevent the others from being detected and cleaned.
        let working_dir = compute_working_dir();
        create_dir_all(working_dir.join("runtimes")).unwrap();

        let good_dir_1 = create_test_directory("runtimes", "execution-", "test-mixed-good-001");
        let good_dir_2 = create_test_directory("runtimes", "execution-", "test-mixed-good-002");
        let bad_dir = create_test_directory("runtimes", "execution-", "test-mixed-bad-001");

        // Set mtime to 2 minutes ago so good directories qualify
        let past = SystemTime::now() - Duration::from_secs(120);
        filetime::set_file_mtime(&good_dir_1, filetime::FileTime::from_system_time(past)).unwrap();
        filetime::set_file_mtime(&good_dir_2, filetime::FileTime::from_system_time(past)).unwrap();

        // Simulate the "bad" entry vanishing before it gets processed.
        fs::remove_dir_all(&bad_dir).unwrap();

        // The function must still return the good entries even though one has vanished.
        let results = get_old_execution_directories("runtimes", "execution-", 1).unwrap();
        let found_good_1 = results
            .iter()
            .any(|e| e.file_name().to_string_lossy().contains("test-mixed-good-001"));
        let found_good_2 = results
            .iter()
            .any(|e| e.file_name().to_string_lossy().contains("test-mixed-good-002"));
        let found_bad = results
            .iter()
            .any(|e| e.file_name().to_string_lossy().contains("test-mixed-bad-001"));

        assert!(found_good_1, "good directory 1 must be returned");
        assert!(found_good_2, "good directory 2 must be returned");
        assert!(!found_bad, "vanished directory must not be returned");

        cleanup_test_directory(&good_dir_1);
        cleanup_test_directory(&good_dir_2);
    }
}
