use elaborate::std::env::set_current_dir_wc;

#[ctor::ctor(unsafe)]
fn initialize() {
    let _ = set_current_dir_wc("..");
}

#[test]
fn snapbox_real_github() {
    testing::snapbox::snapbox(true).unwrap();
}
