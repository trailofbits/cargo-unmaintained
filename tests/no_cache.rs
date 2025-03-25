#[cfg(all(feature = "on-disk-cache", not(windows)))]
mod tests {
    use assert_cmd::cargo::CommandCargoExt;
    use std::process::Command;
    use tempfile::tempdir;

    /// Test that --no-cache option works properly
    ///
    /// The test verifies that running with --no-cache produces the same output
    /// as running after purging the cache completely.
    #[test]
    fn test_no_cache() {
        // Create a temporary directory for XDG_CACHE_HOME
        let cache_dir = tempdir().unwrap();

        // Helper function to run the cargo-unmaintained command
        // with specified arguments, using our temporary cache directory
        let run_command = |args: &[&str]| -> String {
            let mut cmd = Command::cargo_bin("cargo-unmaintained").unwrap();
            cmd.arg("unmaintained");
            cmd.args(args);
            // Use our temporary directory as cache location
            cmd.env("XDG_CACHE_HOME", cache_dir.path());
            // Use a simple test package
            cmd.arg("--package=anyhow");
            // Use JSON output for consistent comparison
            cmd.arg("--json");
            // Add --no-exit-code to avoid non-zero exit status
            cmd.arg("--no-exit-code");
            // Execute and capture output
            let output = cmd.output().unwrap();
            String::from_utf8_lossy(&output.stdout).to_string()
        };

        // Step 1: Populate the cache with a normal run
        let _ = run_command(&[]);

        // Step 2: Run with --no-cache
        let no_cache_output = run_command(&["--no-cache"]);
        // Step 3: Purge the cache
        let _ = run_command(&["--purge"]);
        // Step 4: Run normally after purge
        let after_purge_output = run_command(&[]);
        // Verify that the outputs match
        assert_eq!(
            no_cache_output, after_purge_output,
            "--no-cache output should match output after purge"
        );
    }
}
