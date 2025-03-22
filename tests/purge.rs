#[cfg(all(feature = "on-disk-cache", not(windows)))]
mod tests {
    #[test]
    fn test_purge() {
        use assert_cmd::cargo::CommandCargoExt;
        use std::fs::{create_dir_all, write};
        use std::process::Command;
        use tempfile::tempdir;

        // Create a mock cache directory
        let dir = tempdir().unwrap();
        let cache_path = dir.path().join("cargo-unmaintained/v2");
        create_dir_all(&cache_path).unwrap();

        // Create a dummy file inside
        let test_file = cache_path.join("test.txt");
        write(&test_file, "test").unwrap();

        // Verify the file exists
        assert!(test_file.exists());

        // Run the purge command
        let mut cmd = Command::cargo_bin("cargo-unmaintained").unwrap();

        // Set environment variable for XDG_CACHE_HOME to our temp directory
        cmd.env("XDG_CACHE_HOME", dir.path());

        // Run the unmaintained command with --purge
        cmd.arg("unmaintained").arg("--purge");

        // Execute and assert success
        let output = cmd.output().unwrap();
        assert!(
            output.status.success(),
            "Command failed with: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Verify the directory was removed
        assert!(!cache_path.exists(), "Cache directory still exists");
    }
}
