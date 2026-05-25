use elaborate::std::env::set_current_dir_wc;

#[test]
fn snapbox_real_github() {
    // smoelius: Since there are no other tests in this test executable, changing the current
    // directory is safe.
    set_current_dir_wc("..").unwrap();
    testing::snapbox::snapbox(true).unwrap();
}
