#[cfg_attr(target_os = "windows", ignore = "dependencies differ from Linux/macOS")]
#[test]
fn snapbox() {
    testing::snapbox::snapbox(false, false).unwrap();
}
