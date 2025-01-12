#![cfg_attr(dylint_lib = "general", allow(crate_wide_allow))]
#![allow(dead_code)]

use anyhow::{Context, Result};
use std::{
    env::var,
    io::Read,
    process::{Command, ExitStatus, Stdio},
};

#[derive(Clone, Copy)]
pub enum Tee {
    Stdout,
    Stderr,
}

pub struct Output {
    pub status: ExitStatus,
    pub captured: Vec<u8>,
}

const BUF_SIZE: usize = 1024;

pub fn tee(mut command: Command, which: Tee) -> Result<Output> {
    match which {
        Tee::Stdout => {
            command.stdout(Stdio::piped());
        }
        Tee::Stderr => {
            command.stderr(Stdio::piped());
        }
    }

    let mut child = command
        .spawn()
        .with_context(|| format!("command failed: {command:?}"))?;

    let stream: &mut dyn Read = match which {
        Tee::Stdout => child.stdout.as_mut().unwrap(),
        Tee::Stderr => child.stderr.as_mut().unwrap(),
    };

    let mut captured = Vec::new();

    loop {
        let mut buf = [0u8; BUF_SIZE];
        let size = stream.read(&mut buf).with_context(|| "`read` failed")?;
        if size == 0 {
            break;
        }
        let s = std::str::from_utf8(&buf)?;
        print!("{s}");
        captured.extend_from_slice(&buf[..size]);
    }

    let status = child.wait().with_context(|| "`wait` failed")?;

    Ok(Output { status, captured })
}

#[must_use]
pub fn enabled(key: &str) -> bool {
    var(key).is_ok_and(|value| value != "0")
}

#[must_use]
pub fn split_at_cut_line(s: &str) -> Option<(&str, &str)> {
    const CUT_LINE: &str = "\n---\n";
    // smoelius: Preserve initial newline.
    s.find(CUT_LINE)
        .map(|i| (&s[..=i], &s[i + CUT_LINE.len()..]))
}
