pub trait Flush {
    fn flush(&mut self) -> std::io::Result<()>;
}

impl<T: std::io::Write> Flush for T {
    fn flush(&mut self) -> std::io::Result<()> {
        // smoelius: Do not call `std::io::Write::flush` when `CI` is set, e.g., when running on
        // GitHub.
        #[allow(clippy::disallowed_methods)]
        option_env!("CI").map_or_else(|| <_ as std::io::Write>::flush(self), |_| Ok(()))
    }
}
