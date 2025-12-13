#[test]
fn clippy() {
    for disallow_elaborate_methods in [false, true] {
        let mut command = assert_cmd::Command::new("cargo");
        command.args([
            "+nightly",
            "clippy",
            "--all-features",
            "--all-targets",
            "--",
            "--deny=warnings",
        ]);
        if disallow_elaborate_methods {
            command.env("CLIPPY_CONF_DIR", "assets/elaborate");
        }
        command.assert().success();
    }
}
