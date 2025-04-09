use anyhow::{ Context, Result };
use assert_cmd::prelude::*;
use std::path::Path;
use std::process::Command;

fn run_test_on_fixture(fixture_path: &str) -> Result<String> {
    let output = Command::cargo_bin("cargo-unmaintained")
        .with_context(|| "failed to get cargo-unmaintained binary")?
        .args(["unmaintained"])
        .current_dir(fixture_path)
        .output()
        .with_context(|| format!("failed to run command in {fixture_path}"))?;

    String::from_utf8(output.stderr).with_context(|| "failed to parse command output as UTF-8")
}

#[test]
fn renamed_package_not_flagged() {
    // Skip test if the fixtures directory doesn't exist
    if !Path::new("fixtures").try_exists().expect("failed to check if fixtures directory exists") {
        eprintln!("Skipping test: fixtures directory not found");
        return;
    }

    // Make sure our fixture directories exist
    let fixture_path = "fixtures/icu-rename/after";
    if !Path::new(fixture_path).try_exists().expect("failed to check if fixture path exists") {
        eprintln!("Skipping test: {fixture_path} not found");
        return;
    }

    // Run the test against the renamed package
    let output = run_test_on_fixture(fixture_path).expect("failed to run test on fixture");

    // Should not contain "not in" error indicating package not found in repository
    assert!(
        !output.contains("not in"),
        "Renamed package was incorrectly flagged as not in repository"
    );

    // Should show "No unmaintained packages found"
    assert!(
        output.contains("No unmaintained packages found"),
        "Expected 'No unmaintained packages found'"
    );
}
