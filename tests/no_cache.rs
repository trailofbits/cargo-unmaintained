#![cfg(all(feature = "on-disk-cache", not(windows)))]

use assert_cmd::cargo::CommandCargoExt;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

/// The package to use for testing
const TEST_PACKAGE: &str = "anyhow";

/// The cache version (v2 currently, but could change in the future)
const CACHE_VERSION: &str = "v2";

/// Test that --no-cache option works properly
///
/// The test verifies that running with --no-cache produces the same output
/// as running after purging the cache completely.
#[allow(clippy::disallowed_methods)]
#[test]
fn test_no_cache() {
    // Create a temporary directory for XDG_CACHE_HOME
    let cache_dir = tempdir().unwrap();

    // Define paths for verification
    let cache_root_path = cache_dir.path().join("cargo-unmaintained");
    let cache_version_path = cache_root_path.join(CACHE_VERSION);
    let entries_path = cache_version_path.join("entries");
    let package_entry_path = entries_path.join(TEST_PACKAGE);

    // Helper function to run cargo-unmaintained with specified arguments
    let run_command = |args: &[&str]| {
        let mut cmd = Command::cargo_bin("cargo-unmaintained").unwrap();
        cmd.arg("unmaintained");
        cmd.args(args);
        // Use our temporary directory as cache location
        cmd.env("XDG_CACHE_HOME", cache_dir.path());
        // Use our test package
        cmd.arg(format!("--package={TEST_PACKAGE}"));
        // Use JSON output for consistent comparison
        cmd.arg("--json");
        // Execute and ignore output (we'll check the cache directly)
        cmd.output().unwrap();
    };

    // Helper function to check if the package entry exists in the cache
    let entry_exists = || Path::new(&package_entry_path).exists();

    // Initial check - cache entry should not exist yet
    assert!(!entry_exists(), "Cache entry should not exist initially");

    // Step 1: Populate the cache with a normal run
    run_command(&[]);

    // Verify cache was populated
    assert!(entry_exists(), "Cache entry should exist after initial run");

    // Step 2: Run with --no-cache
    run_command(&["--no-cache"]);

    // Verify cache entry still exists (--no-cache shouldn't affect it)
    assert!(
        entry_exists(),
        "Cache entry should still exist after --no-cache run"
    );

    // Step 3: Purge the cache
    run_command(&["--purge"]);

    // Verify cache was purged
    assert!(!entry_exists(), "Cache entry should not exist after purge");
    assert!(
        !cache_root_path.exists(),
        "Cache directory should not exist after purge"
    );

    // Step 4: Run normally after purge
    run_command(&[]);

    // Verify cache was recreated
    assert!(entry_exists(), "Cache entry should exist after final run");
}
