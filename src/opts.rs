#![allow(clippy::unwrap_used)]

use std::sync::OnceLock;

use super::Opts;

static OPTS: OnceLock<Opts> = OnceLock::new();

pub(crate) fn init(opts: Opts) {
    OPTS.set(opts).unwrap();
}

pub(crate) fn get() -> &'static Opts {
    OPTS.get().unwrap()
}
