use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_void};

/// Get the original libc function using dlsym
///
/// # Safety
/// The caller must ensure F matches the actual function signature
pub unsafe fn get_real_fn<F>(name: &str) -> F {
    let name_cstr = std::ffi::CString::new(name).unwrap();
    let ptr = libc::dlsym(libc::RTLD_NEXT, name_cstr.as_ptr());
    if ptr.is_null() {
        panic!("Failed to find original {}", name);
    }
    std::mem::transmute_copy(&ptr)
}

/// Convert a C path to a Rust string slice
/// Returns None if the path is not valid UTF-8
unsafe fn path_to_str(path: *const c_char) -> Option<&'static str> {
    CStr::from_ptr(path).to_str().ok()
}

/// Check if a file descriptor is virtual (managed by remotelink)
fn is_virtual_fd(fd: c_int) -> bool {
    let fd_map = crate::FD_MAP.lock().unwrap();
    fd_map.as_ref().map_or(false, |m| m.is_virtual_fd(fd))
}

/// Try local operation first, fall back to remote on ENOENT
///
/// This helper implements the common pattern:
/// 1. If path starts with /host/, use remote directly
/// 2. Otherwise try local first
/// 3. If local fails with ENOENT and we have a remote connection, try remote
unsafe fn with_remote_fallback<L, R>(path_str: &str, local_fn: L, remote_fn: R) -> c_int
where
    L: FnOnce() -> c_int,
    R: Fn() -> c_int,
{
    // If path starts with /host/, always use remote
    if crate::is_remote_path(path_str) {
        return remote_fn();
    }

    // Try local first
    let result = local_fn();

    // If local failed with ENOENT and we have a remote connection, try remote
    if result == -1 && *libc::__errno_location() == libc::ENOENT && crate::has_remote_connection() {
        let remote_result = remote_fn();
        if remote_result != -1 {
            return remote_result;
        }
        // Remote also failed, restore ENOENT
        *libc::__errno_location() = libc::ENOENT;
    }

    result
}

/// Wrapper for open()
/// Note: Mode parameter is optional in C, but we always accept it here
#[no_mangle]
pub unsafe extern "C" fn open(path: *const c_char, flags: c_int, mode: c_int) -> c_int {
    let Some(path_str) = path_to_str(path) else {
        type OpenFn = unsafe extern "C" fn(*const c_char, c_int, c_int) -> c_int;
        let real_open: OpenFn = get_real_fn("open");
        return real_open(path, flags, mode);
    };

    with_remote_fallback(
        path_str,
        || {
            type OpenFn = unsafe extern "C" fn(*const c_char, c_int, c_int) -> c_int;
            let real_open: OpenFn = get_real_fn("open");
            real_open(path, flags, mode)
        },
        || crate::handle_remote_open(path_str, flags, mode),
    )
}

/// Wrapper for open64() - same as open on 64-bit systems
#[no_mangle]
pub unsafe extern "C" fn open64(path: *const c_char, flags: c_int, mode: c_int) -> c_int {
    open(path, flags, mode)
}

/// Wrapper for openat() - used by modern glibc and the dynamic linker
#[no_mangle]
pub unsafe extern "C" fn openat(
    dirfd: c_int,
    path: *const c_char,
    flags: c_int,
    mode: c_int,
) -> c_int {
    let Some(path_str) = path_to_str(path) else {
        type OpenatFn = unsafe extern "C" fn(c_int, *const c_char, c_int, c_int) -> c_int;
        let real_openat: OpenatFn = get_real_fn("openat");
        return real_openat(dirfd, path, flags, mode);
    };

    with_remote_fallback(
        path_str,
        || {
            type OpenatFn = unsafe extern "C" fn(c_int, *const c_char, c_int, c_int) -> c_int;
            let real_openat: OpenatFn = get_real_fn("openat");
            real_openat(dirfd, path, flags, mode)
        },
        || crate::handle_remote_open(path_str, flags, mode),
    )
}

/// Wrapper for openat64() - same as openat on 64-bit systems
#[no_mangle]
pub unsafe extern "C" fn openat64(
    dirfd: c_int,
    path: *const c_char,
    flags: c_int,
    mode: c_int,
) -> c_int {
    openat(dirfd, path, flags, mode)
}

/// Wrapper for close()
#[no_mangle]
pub unsafe extern "C" fn close(fd: c_int) -> c_int {
    if is_virtual_fd(fd) {
        crate::handle_remote_close(fd)
    } else {
        type CloseFn = unsafe extern "C" fn(c_int) -> c_int;
        let real_close: CloseFn = get_real_fn("close");
        real_close(fd)
    }
}

/// Wrapper for read()
#[no_mangle]
pub unsafe extern "C" fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize {
    if is_virtual_fd(fd) {
        crate::handle_remote_read(fd, buf as *mut u8, count)
    } else {
        type ReadFn = unsafe extern "C" fn(c_int, *mut c_void, usize) -> isize;
        let real_read: ReadFn = get_real_fn("read");
        real_read(fd, buf, count)
    }
}

/// Wrapper for lseek()
#[no_mangle]
pub unsafe extern "C" fn lseek(fd: c_int, offset: libc::off_t, whence: c_int) -> libc::off_t {
    if is_virtual_fd(fd) {
        crate::handle_remote_lseek(fd, offset as i64, whence) as libc::off_t
    } else {
        type LseekFn = unsafe extern "C" fn(c_int, libc::off_t, c_int) -> libc::off_t;
        let real_lseek: LseekFn = get_real_fn("lseek");
        real_lseek(fd, offset, whence)
    }
}

/// Wrapper for lseek64()
#[no_mangle]
pub unsafe extern "C" fn lseek64(fd: c_int, offset: i64, whence: c_int) -> i64 {
    if is_virtual_fd(fd) {
        crate::handle_remote_lseek(fd, offset, whence)
    } else {
        type Lseek64Fn = unsafe extern "C" fn(c_int, i64, c_int) -> i64;
        let real_lseek64: Lseek64Fn = get_real_fn("lseek64");
        real_lseek64(fd, offset, whence)
    }
}

/// Wrapper for stat()
#[no_mangle]
pub unsafe extern "C" fn stat(path: *const c_char, statbuf: *mut libc::stat) -> c_int {
    let Some(path_str) = path_to_str(path) else {
        type StatFn = unsafe extern "C" fn(*const c_char, *mut libc::stat) -> c_int;
        let real_stat: StatFn = get_real_fn("stat");
        return real_stat(path, statbuf);
    };

    with_remote_fallback(
        path_str,
        || {
            type StatFn = unsafe extern "C" fn(*const c_char, *mut libc::stat) -> c_int;
            let real_stat: StatFn = get_real_fn("stat");
            real_stat(path, statbuf)
        },
        || crate::handle_remote_stat(path_str, statbuf),
    )
}

/// Wrapper for stat64()
#[no_mangle]
pub unsafe extern "C" fn stat64(path: *const c_char, statbuf: *mut libc::stat) -> c_int {
    stat(path, statbuf)
}

/// Wrapper for fstat()
#[no_mangle]
pub unsafe extern "C" fn fstat(fd: c_int, statbuf: *mut libc::stat) -> c_int {
    if is_virtual_fd(fd) {
        crate::handle_remote_fstat(fd, statbuf)
    } else {
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

/// Wrapper for access() - used by dynamic linker to check file existence
#[no_mangle]
pub unsafe extern "C" fn access(path: *const c_char, mode: c_int) -> c_int {
    let Some(path_str) = path_to_str(path) else {
        type AccessFn = unsafe extern "C" fn(*const c_char, c_int) -> c_int;
        let real_access: AccessFn = get_real_fn("access");
        return real_access(path, mode);
    };

    with_remote_fallback(
        path_str,
        || {
            type AccessFn = unsafe extern "C" fn(*const c_char, c_int) -> c_int;
            let real_access: AccessFn = get_real_fn("access");
            real_access(path, mode)
        },
        || crate::handle_remote_access(path_str, mode),
    )
}

/// Wrapper for faccessat() - used by some libc implementations
#[no_mangle]
pub unsafe extern "C" fn faccessat(
    dirfd: c_int,
    path: *const c_char,
    mode: c_int,
    flags: c_int,
) -> c_int {
    let Some(path_str) = path_to_str(path) else {
        type FaccessatFn = unsafe extern "C" fn(c_int, *const c_char, c_int, c_int) -> c_int;
        let real_faccessat: FaccessatFn = get_real_fn("faccessat");
        return real_faccessat(dirfd, path, mode, flags);
    };

    with_remote_fallback(
        path_str,
        || {
            type FaccessatFn = unsafe extern "C" fn(c_int, *const c_char, c_int, c_int) -> c_int;
            let real_faccessat: FaccessatFn = get_real_fn("faccessat");
            real_faccessat(dirfd, path, mode, flags)
        },
        || crate::handle_remote_access(path_str, mode),
    )
}

/// Wrapper for dlopen() - intercept loading of shared libraries from /host/
#[no_mangle]
pub unsafe extern "C" fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void {
    // Handle null path (returns handle to main program)
    if path.is_null() {
        type DlopenFn = unsafe extern "C" fn(*const c_char, c_int) -> *mut c_void;
        let real_dlopen: DlopenFn = get_real_fn("dlopen");
        return real_dlopen(path, flags);
    }

    let Some(path_str) = path_to_str(path) else {
        type DlopenFn = unsafe extern "C" fn(*const c_char, c_int) -> *mut c_void;
        let real_dlopen: DlopenFn = get_real_fn("dlopen");
        return real_dlopen(path, flags);
    };

    // If path starts with /host/, always use remote
    if crate::is_remote_path(path_str) {
        return crate::handle_remote_dlopen(path_str, flags);
    }

    // Try local first
    type DlopenFn = unsafe extern "C" fn(*const c_char, c_int) -> *mut c_void;
    let real_dlopen: DlopenFn = get_real_fn("dlopen");
    let result = real_dlopen(path, flags);

    // If local failed and we have a remote connection, try remote
    if result.is_null() && crate::has_remote_connection() {
        let remote_result = crate::handle_remote_dlopen(path_str, flags);
        if !remote_result.is_null() {
            return remote_result;
        }
    }

    result
}
