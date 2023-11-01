use std::sync::atomic::AtomicBool;

pub static __NEED_NEWLINE: AtomicBool = AtomicBool::new(false);

macro_rules! __print {
    ($fmt:expr) => {
        if crate::opts::get().verbose {
            $crate::verbose::__NEED_NEWLINE.store(true, std::sync::atomic::Ordering::SeqCst);
            eprint!($fmt);
            <_ as std::io::Write>::flush(&mut std::io::stderr()).unwrap();
        }
    };
    ($fmt:expr, $($arg:tt)*) => {
        if crate::opts::get().verbose {
            $crate::verbose::__NEED_NEWLINE.store(true, std::sync::atomic::Ordering::SeqCst);
            eprint!($fmt, $($arg)*);
            <_ as std::io::Write>::flush(&mut std::io::stderr()).unwrap();
        }
    };
}

macro_rules! __println {
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
            $crate::verbose::__println!();
        }
    };
}

macro_rules! wrap {
    ($f:expr, $fmt:expr, $($arg:tt)*) => {{
        $crate::verbose::__print!(concat!($fmt, "..."), $($arg)*);
        let result = $f();
        if result.is_ok() {
            $crate::verbose::__println!("ok");
        } else {
            $crate::verbose::__println!();
        }
        result
    }};
}

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
pub(crate) use {__print, __println, newline, update, wrap};
