use anyhow::{Context, Result, anyhow, bail};
use cargo_metadata::{Message, camino::Utf8PathBuf};
use elaborate::std::{env::var_wc, process::CommandContext};
use std::process::Command;

// This test runs the `ci` package's tests from the workspace root. An individual test can instead
// be run with, e.g., `cargo test -p ci supply_chain`, so long as the test hardcodes its paths.
//
// A test that relies on the current directory works only when run through this one, because Cargo
// sets a test executable's current directory to its package's root. Use `FILTER` to select such a
// test; a filter on the command line would apply to this test rather than to the tests it runs.
#[test]
fn ci() {
    let executable = test_executable().unwrap();
    let mut command = Command::new(executable);
    if let Ok(filter) = var_wc("FILTER") {
        command.arg(filter);
    }
    command.env_remove("CARGO_TERM_COLOR");
    let status = command.status_wc().unwrap();
    assert!(status.success());
}

fn test_executable() -> Result<Utf8PathBuf> {
    let mut command = Command::new("cargo");
    let output = command
        .args(["build", "-p", "ci", "--tests", "--message-format=json"])
        .env_remove("CARGO_TERM_COLOR")
        .output_wc()?;
    if !output.status.success() {
        bail!("command failed: {command:?}");
    }
    let messages = Message::parse_stream(output.stdout.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to parse metadata from: {command:?}"))?;
    let executables = messages
        .into_iter()
        .filter_map(|message| {
            if let Message::CompilerArtifact(artifact) = message
                && artifact.target.name == "ci"
                && artifact.target.is_lib()
                && artifact.profile.test
                && let Some(executable) = artifact.executable
            {
                Some(executable)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if executables.len() >= 2 {
        bail!("found multiple test executables: {executables:?}");
    }
    executables
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("found no test executables"))
}
