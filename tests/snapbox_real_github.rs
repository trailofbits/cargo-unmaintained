#![cfg(feature = "test-ei")]

#[cfg_attr(target_os = "windows", ignore = "dependencies differ from Linux/macOS")]
#[test]
fn snapbox_real_github() {
    // smoelius: Running with `--all-targets` creates a lot of unfortunate snapbox churn. But for
    // the packages with which we test, use of the flag seems unavoidable, even when running only on
    // Linux and macOS.
    testing::snapbox::snapbox(true, true).unwrap();
}
