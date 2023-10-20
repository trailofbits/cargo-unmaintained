macro_rules! __print {
    ($fmt:expr) => {
        if crate::opts::get().verbose {
            eprint!($fmt);
            <_ as std::io::Write>::flush(&mut std::io::stderr()).unwrap();
        }
    };
    ($fmt:expr, $($arg:tt)*) => {
        if crate::opts::get().verbose {
            eprint!($fmt, $($arg)*);
            <_ as std::io::Write>::flush(&mut std::io::stderr()).unwrap();
        }
    };
}

macro_rules! __println {
    () => {
        if crate::opts::get().verbose {
            eprintln!();
        }
    };
    ($fmt:expr) => {
        if crate::opts::get().verbose {
            eprintln!($fmt);
        }
    };
    ($fmt:expr, $($arg:tt)*) => {
        if crate::opts::get().verbose {
            eprintln!($fmt, $($arg)*);
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

macro_rules! newline {
    () => {
        $crate::verbose::__println!();
    };
}

// smoelius: "The trick": https://stackoverflow.com/a/31749071
pub(crate) use {__print, __println, newline, update, wrap};
