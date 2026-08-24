#![cfg(all(feature = "on-disk-cache", not(windows)))]

#[test]
fn test_purge() {
    use assert_cmd::cargo::cargo_bin;
    use elaborate::std::{
        fs::{create_dir_all_wc, write_wc},
        path::PathContext,
        process::CommandContext,
    };
    use std::process::Command;
    use tempfile::tempdir;

    // The cache version (v2 currently, but could change in the future)
    const CACHE_VERSION: &str = "v2";

    // Create a mock cache directory
    let dir = tempdir().unwrap();
    let cache_path = dir.path().join("cargo-unmaintained").join(CACHE_VERSION);
    create_dir_all_wc(&cache_path).unwrap();

    // Create a dummy file inside
    let test_file = cache_path.join("test.txt");
    write_wc(&test_file, "test").unwrap();

    // Verify the file exists
    assert!(test_file.try_exists_wc().unwrap());

    // Run the purge command
    #[cfg_attr(dylint_lib = "general", allow(unnecessary_conversion_for_trait))]
    let mut cmd = Command::new(cargo_bin!("cargo-unmaintained"));

    // Set environment variable for XDG_CACHE_HOME to our temp directory
    cmd.env("XDG_CACHE_HOME", dir.path());

    // Run the unmaintained command with --purge
    cmd.arg("unmaintained").arg("--purge");

    // Execute and assert success
    let output = cmd.output_wc().unwrap();
    assert!(
        output.status.success(),
        "Command failed with: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify the directory was removed
    assert!(
        !cache_path.try_exists_wc().unwrap(),
        "Cache directory still exists"
    );
}
