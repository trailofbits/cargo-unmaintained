use std::env::set_current_dir;

#[ctor::ctor]
fn initialize() {
    set_current_dir("..");
}

#[test]
fn snapbox_real_github() {
    testing::snapbox::snapbox(true).unwrap();
}
