// This test has no need of the `__mock_github` feature. The test binary does not link this
// package's library. Rather, it runs the `cargo-unmaintained-with-mock-github` binary as a
// subprocess. That binary is built separately, in the `mock_github` nested workspace, which enables
// `__mock_github` on its dependency on this package's library.

#[cfg_attr(target_os = "windows", ignore = "dependencies differ from Linux/macOS")]
#[test]
fn snapbox() {
    testing::snapbox::snapbox(false, false).unwrap();
}
