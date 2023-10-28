use snapbox::{assert_matches_path, cmd::Command};

#[test]
fn rustsec_advisory_comparison() {
    let assert = Command::new("cargo")
        .arg("run")
        .current_dir("rustsec_advisory_comparison")
        .assert();

    let stdout_actual = std::str::from_utf8(&assert.get_output().stdout).unwrap();

    assert_matches_path("tests/rustsec_advisory_comparison.stdout", stdout_actual);
}
