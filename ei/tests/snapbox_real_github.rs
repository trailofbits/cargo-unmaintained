use std::env::set_current_dir;

#[ctor::ctor(unsafe)]
fn initialize() {
    set_current_dir("..");
}

#[test]
fn snapbox_real_github() {
    testing::snapbox::snapbox(true).unwrap();
}
