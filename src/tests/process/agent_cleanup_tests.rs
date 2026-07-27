#[cfg(test)]
mod tests {
    use crate::process::agent_cleanup::{get_old_execution_directories, run_cleanup_cycle};
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

    // -- Tests for run_cleanup_cycle --

    #[test]
    fn test_run_cleanup_cycle_renames_stale_execution_to_executed() {
        let working_dir = compute_working_dir();
        create_dir_all(working_dir.join("runtimes")).unwrap();
        create_dir_all(working_dir.join("payloads")).unwrap();

        let test_id = "test-cycle-rename-001";
        let dir = create_test_directory("runtimes", "execution-", test_id);
        let expected_renamed = dir.with_file_name(format!("executed-{test_id}"));

        // Set mtime to 2 minutes ago so it qualifies as stale
        let past = SystemTime::now() - Duration::from_secs(120);
        filetime::set_file_mtime(&dir, filetime::FileTime::from_system_time(past)).unwrap();

        // Phase 1 renames execution- to executed- (executing_max_time=1 minute).
        // Phase 2 threshold set high so it won't delete yet.
        run_cleanup_cycle(1, 9999);

        assert!(!dir.exists(), "original execution- directory must be gone");
        assert!(
            expected_renamed.exists(),
            "executed- directory must exist after rename"
        );
        assert!(
            expected_renamed.join("test.txt").exists(),
            "file inside must survive rename"
        );

        cleanup_test_directory(&expected_renamed);
    }

    #[test]
    fn test_run_cleanup_cycle_deletes_stale_executed_directories() {
        let working_dir = compute_working_dir();
        create_dir_all(working_dir.join("runtimes")).unwrap();
        create_dir_all(working_dir.join("payloads")).unwrap();

        let test_id = "test-cycle-delete-001";
        let dir = create_test_directory("runtimes", "executed-", test_id);

        assert!(dir.exists());
        assert!(dir.join("test.txt").exists());

        // Set mtime to 2 minutes ago so it qualifies for deletion
        let past = SystemTime::now() - Duration::from_secs(120);
        filetime::set_file_mtime(&dir, filetime::FileTime::from_system_time(past)).unwrap();

        // Phase 1 threshold set high so no rename happens.
        // Phase 2 deletes executed- directories older than 1 minute.
        run_cleanup_cycle(9999, 1);

        assert!(
            !dir.exists(),
            "executed- directory must be permanently deleted"
        );
    }

    #[test]
    fn test_run_cleanup_cycle_handles_payloads_subfolder() {
        let working_dir = compute_working_dir();
        create_dir_all(working_dir.join("runtimes")).unwrap();
        create_dir_all(working_dir.join("payloads")).unwrap();

        let test_id = "test-cycle-payload-001";
        let exec_dir = create_test_directory("payloads", "execution-", test_id);
        let expected_renamed = exec_dir.with_file_name(format!("executed-{test_id}"));

        // Set mtime to 2 minutes ago
        let past = SystemTime::now() - Duration::from_secs(120);
        filetime::set_file_mtime(&exec_dir, filetime::FileTime::from_system_time(past)).unwrap();

        // Phase 1 renames, phase 2 threshold high so no delete
        run_cleanup_cycle(1, 9999);

        assert!(
            !exec_dir.exists(),
            "original execution- payload must be gone"
        );
        assert!(
            expected_renamed.exists(),
            "executed- payload must exist after rename"
        );

        cleanup_test_directory(&expected_renamed);
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
        filetime::set_file_mtime(&dir, filetime::FileTime::from_system_time(future_time)).unwrap();

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
        let found_good_1 = results.iter().any(|e| {
            e.file_name()
                .to_string_lossy()
                .contains("test-mixed-good-001")
        });
        let found_good_2 = results.iter().any(|e| {
            e.file_name()
                .to_string_lossy()
                .contains("test-mixed-good-002")
        });
        let found_bad = results.iter().any(|e| {
            e.file_name()
                .to_string_lossy()
                .contains("test-mixed-bad-001")
        });

        assert!(found_good_1, "good directory 1 must be returned");
        assert!(found_good_2, "good directory 2 must be returned");
        assert!(!found_bad, "vanished directory must not be returned");

        cleanup_test_directory(&good_dir_1);
        cleanup_test_directory(&good_dir_2);
    }
}
