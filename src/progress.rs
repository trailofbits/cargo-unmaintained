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
            self.finish();
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

    pub fn advance(&mut self, msg: &str) {
        self.draw(msg);
        assert!(self.i < self.n);
        self.i += 1;
    }

    #[cfg_attr(dylint_lib = "supplementary", allow(commented_out_code))]
    pub fn finish(&mut self) {
        // smoelius: Don't assert here. If --fail-fast was passed, `finish` may be called before all
        // packages have been scanned.
        // assert_eq!(self.i, self.n);
        self.draw("");
        self.newline();
        self.finished = true;
    }

    pub fn newline(&mut self) {
        if self.newline_needed {
            eprintln!();
        }
        self.newline_needed = false;
    }

    fn draw(&mut self, msg: &str) {
        let width_n = self.width_n;
        let percent = (self.i * 100).checked_div(self.n).unwrap_or(100);
        let formatted_msg = format!("{:>width_n$}/{} ({percent}%) {msg}", self.i, self.n);
        let width_to_overwrite = self.width_prev.saturating_sub(formatted_msg.len());
        eprint!("{formatted_msg}{:width_to_overwrite$}\r", "");
        self.width_prev = formatted_msg.len();
        self.newline_needed = true;
    }
}
