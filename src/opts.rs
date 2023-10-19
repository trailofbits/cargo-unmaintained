#![allow(clippy::unwrap_used)]

use once_cell::sync::OnceCell;

use super::Opts;

static OPTS: OnceCell<Opts> = OnceCell::new();

pub(crate) fn init(opts: Opts) {
    OPTS.set(opts).unwrap();
}

pub(crate) fn get() -> &'static Opts {
    OPTS.get().unwrap()
}
