//! `cargo-unmaintained`
//!
//! A tool for finding unmaintained packages in Rust projects.
//!
//! `cargo-unmaintained` identifies packages that might be unmaintained by checking:
//! - Repository activity (last commit age)
//! - Repository status (e.g., archived, nonexistent)
//! - Dependency version compatibility (outdated version requirements)
//!
//! Detected issues are reported with contextual information and color-coded output.
//! The tool can be configured to check specific packages, adjust age thresholds,
//! and control output formatting.

fn main() -> anyhow::Result<()> {
    cargo_unmaintained::run()
}
