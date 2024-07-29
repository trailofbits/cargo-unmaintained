use crate::flush::Flush;
use anyhow::{Context, Result};

pub struct Progress {
    n: usize,
    i: usize,
    width_n: usize,
    width_prev: usize,
    newline_needed: bool,
    finished: bool,
}

impl Drop for Progress {
    fn drop(&mut self) {
        if !self.finished {
            self.finish().unwrap_or_default();
        }
    }
}

impl Progress {
    pub fn new(n: usize) -> Self {
        Self {
            n,
            i: 0,
            width_n: n.to_string().len(),
            width_prev: 0,
            newline_needed: false,
            finished: false,
        }
    }

    pub fn advance(&mut self, msg: &str) -> Result<()> {
        self.draw(msg)?;
        assert!(self.i < self.n);
        self.i += 1;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<()> {
        assert!(self.i == self.n);
        self.draw("")?;
        self.newline();
        self.finished = true;
        Ok(())
    }

    pub fn newline(&mut self) {
        if self.newline_needed {
            eprintln!();
        }
        self.newline_needed = false;
    }

    fn draw(&mut self, msg: &str) -> Result<()> {
        let width_n = self.width_n;
        let percent = if self.n == 0 {
            100
        } else {
            self.i * 100 / self.n
        };
        let formatted_msg = format!("{:>width_n$}/{} ({percent}%) {msg}", self.i, self.n,);
        let width_to_overwrite = self.width_prev.saturating_sub(formatted_msg.len());
        eprint!("{formatted_msg}{:width_to_overwrite$}\r", "");
        <_ as Flush>::flush(&mut std::io::stderr()).with_context(|| "failed to flush stderr")?;
        self.width_prev = formatted_msg.len();
        self.newline_needed = true;
        Ok(())
    }
}
