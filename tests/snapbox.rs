#[cfg_attr(target_os = "windows", ignore)]
#[test]
fn snapbox() {
    testing::snapbox::snapbox(false, false).unwrap();
}
