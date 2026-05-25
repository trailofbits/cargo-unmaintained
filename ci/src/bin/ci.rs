use anyhow::{Result, anyhow, bail};
use cargo_metadata::{Message, camino::Utf8PathBuf};
use elaborate::std::process::CommandContext;
use std::process::Command;

fn main() {
    let executable = test_executable().unwrap();
    let status = Command::new(executable).status_wc().unwrap();
    assert!(status.success());
}

fn test_executable() -> Result<Utf8PathBuf> {
    let mut command = Command::new("cargo");
    let output = command
        .args(["build", "--workspace", "--tests", "--message-format=json"])
        .output_wc()?;
    if !output.status.success() {
        bail!("command failed: {command:?}");
    }
    let messages =
        Message::parse_stream(output.stdout.as_slice()).collect::<Result<Vec<_>, _>>()?;
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
