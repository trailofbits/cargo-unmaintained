#![cfg(feature = "test-ei")]

// This test has no need of the `__mock_github` feature. The test binary does not link this
// package's library. Rather, it runs the `cargo-unmaintained-with-mock-github` binary as a
// subprocess. That binary is built separately, in the `mock_github` nested workspace, which enables
// `__mock_github` on its dependency on this package's library.

// smoelius: Even though this test uses the `cargo-unmaintained-with-mock-github` binary, it is
// still "externally influenced". For example, a tested package's dependency could go from
// unmaintained to maintained because its repository was updated. Such a change necessitated the
// following commit:
// https://github.com/trailofbits/cargo-unmaintained/commit/ca3242dec8e6fd2e8e3e7cc6021c32c02f34ec59

#[cfg_attr(target_os = "windows", ignore = "dependencies differ from Linux/macOS")]
#[test]
fn snapbox_mock_github() {
    testing::snapbox::snapbox(false, false).unwrap();
}
