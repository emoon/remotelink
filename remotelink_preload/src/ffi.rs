use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_void};

/// Get the original libc function using dlsym
unsafe fn get_real_fn<F>(name: &str) -> F {
    let name_cstr = std::ffi::CString::new(name).unwrap();
    let ptr = libc::dlsym(libc::RTLD_NEXT, name_cstr.as_ptr());
    if ptr.is_null() {
        panic!("Failed to find original {}", name);
    }
    std::mem::transmute_copy(&ptr)
}

/// Wrapper for open()
/// Note: Mode parameter is optional in C, but we always accept it here
#[no_mangle]
pub unsafe extern "C" fn open(path: *const c_char, flags: c_int, mode: c_int) -> c_int {
    // Convert path to Rust string
    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => {
            // Invalid UTF-8, pass to real open
            type OpenFn = unsafe extern "C" fn(*const c_char, c_int, c_int) -> c_int;
            let real_open: OpenFn = get_real_fn("open");
            return real_open(path, flags, mode);
        }
    };

    // Check if this is a remote path
    if crate::is_remote_path(path_str) {
        crate::handle_remote_open(path_str, flags, mode)
    } else {
        // Call real open
        type OpenFn = unsafe extern "C" fn(*const c_char, c_int, c_int) -> c_int;
        let real_open: OpenFn = get_real_fn("open");
        real_open(path, flags, mode)
    }
}

/// Wrapper for open64() - same as open on 64-bit systems
#[no_mangle]
pub unsafe extern "C" fn open64(path: *const c_char, flags: c_int, mode: c_int) -> c_int {
    open(path, flags, mode)
}

/// Wrapper for close()
#[no_mangle]
pub unsafe extern "C" fn close(fd: c_int) -> c_int {
    // Check if this is a virtual FD
    let is_virtual = {
        let fd_map = crate::FD_MAP.lock().unwrap();
        fd_map.as_ref().map_or(false, |m| m.is_virtual_fd(fd))
    };

    if is_virtual {
        crate::handle_remote_close(fd)
    } else {
        // Call real close
        type CloseFn = unsafe extern "C" fn(c_int) -> c_int;
        let real_close: CloseFn = get_real_fn("close");
        real_close(fd)
    }
}

/// Wrapper for read()
#[no_mangle]
pub unsafe extern "C" fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize {
    // Check if this is a virtual FD
    let is_virtual = {
        let fd_map = crate::FD_MAP.lock().unwrap();
        fd_map.as_ref().map_or(false, |m| m.is_virtual_fd(fd))
    };

    if is_virtual {
        crate::handle_remote_read(fd, buf as *mut u8, count)
    } else {
        // Call real read
        type ReadFn = unsafe extern "C" fn(c_int, *mut c_void, usize) -> isize;
        let real_read: ReadFn = get_real_fn("read");
        real_read(fd, buf, count)
    }
}

/// Wrapper for lseek()
#[no_mangle]
pub unsafe extern "C" fn lseek(fd: c_int, offset: libc::off_t, whence: c_int) -> libc::off_t {
    // Check if this is a virtual FD
    let is_virtual = {
        let fd_map = crate::FD_MAP.lock().unwrap();
        fd_map.as_ref().map_or(false, |m| m.is_virtual_fd(fd))
    };

    if is_virtual {
        crate::handle_remote_lseek(fd, offset as i64, whence) as libc::off_t
    } else {
        // Call real lseek
        type LseekFn = unsafe extern "C" fn(c_int, libc::off_t, c_int) -> libc::off_t;
        let real_lseek: LseekFn = get_real_fn("lseek");
        real_lseek(fd, offset, whence)
    }
}

/// Wrapper for lseek64()
#[no_mangle]
pub unsafe extern "C" fn lseek64(fd: c_int, offset: i64, whence: c_int) -> i64 {
    // Check if this is a virtual FD
    let is_virtual = {
        let fd_map = crate::FD_MAP.lock().unwrap();
        fd_map.as_ref().map_or(false, |m| m.is_virtual_fd(fd))
    };

    if is_virtual {
        crate::handle_remote_lseek(fd, offset, whence)
    } else {
        // Call real lseek64
        type Lseek64Fn = unsafe extern "C" fn(c_int, i64, c_int) -> i64;
        let real_lseek64: Lseek64Fn = get_real_fn("lseek64");
        real_lseek64(fd, offset, whence)
    }
}

/// Wrapper for stat()
#[no_mangle]
pub unsafe extern "C" fn stat(path: *const c_char, statbuf: *mut libc::stat) -> c_int {
    // Convert path to Rust string
    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => {
            // Invalid UTF-8, pass to real stat
            type StatFn = unsafe extern "C" fn(*const c_char, *mut libc::stat) -> c_int;
            let real_stat: StatFn = get_real_fn("stat");
            return real_stat(path, statbuf);
        }
    };

    // Check if this is a remote path
    if crate::is_remote_path(path_str) {
        crate::handle_remote_stat(path_str, statbuf)
    } else {
        // Call real stat
        type StatFn = unsafe extern "C" fn(*const c_char, *mut libc::stat) -> c_int;
        let real_stat: StatFn = get_real_fn("stat");
        real_stat(path, statbuf)
    }
}

/// Wrapper for stat64()
#[no_mangle]
pub unsafe extern "C" fn stat64(path: *const c_char, statbuf: *mut libc::stat) -> c_int {
    stat(path, statbuf)
}

/// Wrapper for fstat()
#[no_mangle]
pub unsafe extern "C" fn fstat(fd: c_int, statbuf: *mut libc::stat) -> c_int {
    // Check if this is a virtual FD
    let is_virtual = {
        let fd_map = crate::FD_MAP.lock().unwrap();
        fd_map.as_ref().map_or(false, |m| m.is_virtual_fd(fd))
    };

    if is_virtual {
        crate::handle_remote_fstat(fd, statbuf)
    } else {
        // Call real fstat
        type FstatFn = unsafe extern "C" fn(c_int, *mut libc::stat) -> c_int;
        let real_fstat: FstatFn = get_real_fn("fstat");
        real_fstat(fd, statbuf)
    }
}

/// Wrapper for fstat64()
#[no_mangle]
pub unsafe extern "C" fn fstat64(fd: c_int, statbuf: *mut libc::stat) -> c_int {
    fstat(fd, statbuf)
}
