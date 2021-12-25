use std::fmt;

pub const LOG_TRACE: i32 = 0;
//pub const LOG_DEBUG: i32 = 1;
//pub const LOG_INFO: i32 = 2;
pub const LOG_WARN: i32 = 3;
pub const LOG_ERROR: i32 = 4;

static mut LOG_LEVEL: i32 = LOG_WARN;
#[rustfmt::skip]
static LOG_NAMES: &[&str] = &[
    "TRACE",
    "DEBUG",
    "INFO ",
    "WARN",
    "ERROR",
];

pub fn set_log_level(level: i32) {
    unsafe {
        LOG_LEVEL = level;
    }
}

pub fn do_log(log_level: i32, format: fmt::Arguments) {
    let t = format!("{}", format);

    unsafe {
        if log_level >= LOG_LEVEL {
            println!("[{}] {}", LOG_NAMES[log_level as usize], t);
        }
    }
}

#[macro_export]
macro_rules! trace {
    ($($arg:tt)+) => ({
        $crate::log::do_log($crate::log::LOG_TRACE, format_args!($($arg)*))
    });
}
