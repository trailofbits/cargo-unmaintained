use std::sync::atomic::AtomicBool;

pub static __NEED_NEWLINE: AtomicBool = AtomicBool::new(false);

macro_rules! __eprint {
    ($fmt:expr) => {
        if crate::opts::get().verbose {
            $crate::verbose::__NEED_NEWLINE.store(true, std::sync::atomic::Ordering::SeqCst);
            eprint!($fmt);
            <_ as $crate::flush::Flush>::flush(&mut std::io::stderr()).unwrap();
        }
    };
    ($fmt:expr, $($arg:tt)*) => {
        if crate::opts::get().verbose {
            $crate::verbose::__NEED_NEWLINE.store(true, std::sync::atomic::Ordering::SeqCst);
            eprint!($fmt, $($arg)*);
            <_ as $crate::flush::Flush>::flush(&mut std::io::stderr()).unwrap();
        }
    };
}

macro_rules! __eprintln {
    () => {
        if crate::opts::get().verbose {
            eprintln!();
            $crate::verbose::__NEED_NEWLINE.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    };
    ($fmt:expr) => {
        if crate::opts::get().verbose {
            eprintln!($fmt);
            $crate::verbose::__NEED_NEWLINE.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    };
    ($fmt:expr, $($arg:tt)*) => {
        if crate::opts::get().verbose {
            eprintln!($fmt, $($arg)*);
            $crate::verbose::__NEED_NEWLINE.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    };
}

macro_rules! newline {
    () => {
        if $crate::verbose::__NEED_NEWLINE.load(std::sync::atomic::Ordering::SeqCst) {
            $crate::verbose::__eprintln!();
        }
    };
}

macro_rules! wrap {
    ($f:expr, $fmt:expr, $($arg:tt)*) => {{
        $crate::verbose::__eprint!(concat!($fmt, "..."), $($arg)*);
        #[allow(clippy::redundant_closure_call)]
        let result = $f();
        if result.is_ok() {
            $crate::verbose::__eprintln!("ok");
        } else {
            $crate::verbose::__eprintln!();
        }
        result
    }};
}

#[allow(unused_macros)]
macro_rules! update {
    ($fmt:expr) => {
        if crate::opts::get().verbose {
            $crate::verbose::__print!(concat!($fmt, "..."));
        }
    };
    ($fmt:expr, $($arg:tt)*) => {
        if crate::opts::get().verbose {
            $crate::verbose::__print!(concat!($fmt, "..."), $($arg)*);
        }
    };
}

// smoelius: "The trick": https://stackoverflow.com/a/31749071
pub(crate) use {__eprint, __eprintln, newline, wrap};
