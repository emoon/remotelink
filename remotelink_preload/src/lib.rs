// LD_PRELOAD library for intercepting file operations and proxying them to remotelink file server
//
// This library intercepts libc functions for files with the /host/ prefix and forwards
// the operations to the file server over TCP.

#![allow(non_camel_case_types)]

mod fd_map;
mod ffi;

use std::sync::Mutex;
use std::os::raw::c_int;

// Global state
static FD_MAP: Mutex<Option<fd_map::FdMap>> = Mutex::new(None);
static CONNECTION: Mutex<Option<remotelink_client::FileServerClient>> = Mutex::new(None);

/// Initialize the library - called when library is loaded
fn initialize() {
    // Initialize FD map
    *FD_MAP.lock().unwrap() = Some(fd_map::FdMap::new());

    // Initialize connection if REMOTELINK_FILE_SERVER is set
    if let Ok(addr) = std::env::var("REMOTELINK_FILE_SERVER") {
        match remotelink_client::FileServerClient::new(&addr) {
            Ok(conn) => {
                *CONNECTION.lock().unwrap() = Some(conn);
            }
            Err(e) => {
                eprintln!("remotelink_preload: Failed to connect to file server {}: {}", addr, e);
            }
        }
    }
}

/// Cleanup when library is unloaded
fn cleanup() {
    // Close all open file descriptors
    if let Some(fd_map) = FD_MAP.lock().unwrap().as_mut() {
        fd_map.cleanup();
    }

    // Close connection
    *CONNECTION.lock().unwrap() = None;
}

/// Check if a path should be handled remotely
fn is_remote_path(path: &str) -> bool {
    path.starts_with("/host/")
}

/// Get the relative path by stripping /host/ prefix
fn get_relative_path(path: &str) -> &str {
    if path.starts_with("/host/") {
        &path[6..]
    } else {
        path
    }
}

/// Handle remote open operation
fn handle_remote_open(path: &str, _flags: c_int, _mode: c_int) -> c_int {
    let conn_guard = CONNECTION.lock().unwrap();
    let conn = match conn_guard.as_ref() {
        Some(c) => c,
        None => {
            unsafe { *libc::__errno_location() = libc::ENOENT };
            return -1;
        }
    };

    let rel_path = get_relative_path(path);

    match conn.open(rel_path) {
        Ok((handle, size)) => {
            // Map remote handle to virtual FD
            let mut fd_map = FD_MAP.lock().unwrap();
            if let Some(map) = fd_map.as_mut() {
                match map.allocate(handle) {
                    Some(vfd) => {
                        // Store file size for later use
                        map.set_size(vfd, size);
                        vfd
                    }
                    None => {
                        // Failed to allocate virtual FD, close the remote file
                        let _ = conn.close(handle);
                        unsafe { *libc::__errno_location() = libc::EMFILE };
                        -1
                    }
                }
            } else {
                unsafe { *libc::__errno_location() = libc::ENOENT };
                -1
            }
        }
        Err(errno) => {
            unsafe { *libc::__errno_location() = errno };
            -1
        }
    }
}

/// Handle remote close operation
fn handle_remote_close(fd: c_int) -> c_int {
    let conn_guard = CONNECTION.lock().unwrap();
    let conn = match conn_guard.as_ref() {
        Some(c) => c,
        None => {
            unsafe { *libc::__errno_location() = libc::EBADF };
            return -1;
        }
    };

    let mut fd_map = FD_MAP.lock().unwrap();
    let map = match fd_map.as_mut() {
        Some(m) => m,
        None => {
            unsafe { *libc::__errno_location() = libc::EBADF };
            return -1;
        }
    };

    match map.get_handle(fd) {
        Some(handle) => {
            match conn.close(handle) {
                Ok(()) => {
                    map.release(fd);
                    0
                }
                Err(errno) => {
                    unsafe { *libc::__errno_location() = errno };
                    -1
                }
            }
        }
        None => {
            unsafe { *libc::__errno_location() = libc::EBADF };
            -1
        }
    }
}

/// Handle remote read operation
fn handle_remote_read(fd: c_int, buf: *mut u8, count: usize) -> isize {
    let conn_guard = CONNECTION.lock().unwrap();
    let conn = match conn_guard.as_ref() {
        Some(c) => c,
        None => {
            unsafe { *libc::__errno_location() = libc::EBADF };
            return -1;
        }
    };

    let mut fd_map = FD_MAP.lock().unwrap();
    let map = match fd_map.as_mut() {
        Some(m) => m,
        None => {
            unsafe { *libc::__errno_location() = libc::EBADF };
            return -1;
        }
    };

    match map.get_handle(fd) {
        Some(handle) => {
            let offset = map.get_offset(fd).unwrap_or(0);
            let size = std::cmp::min(count, 4 * 1024 * 1024) as u32; // Cap at 4MB

            match conn.read(handle, offset, size) {
                Ok(data) => {
                    let bytes_read = data.len();
                    if bytes_read > 0 {
                        unsafe {
                            std::ptr::copy_nonoverlapping(data.as_ptr(), buf, bytes_read);
                        }
                        map.update_offset(fd, offset + bytes_read as u64);
                    }
                    bytes_read as isize
                }
                Err(errno) => {
                    unsafe { *libc::__errno_location() = errno };
                    -1
                }
            }
        }
        None => {
            unsafe { *libc::__errno_location() = libc::EBADF };
            -1
        }
    }
}

/// Handle remote lseek operation
fn handle_remote_lseek(fd: c_int, offset: i64, whence: c_int) -> i64 {
    let mut fd_map = FD_MAP.lock().unwrap();
    let map = match fd_map.as_mut() {
        Some(m) => m,
        None => {
            unsafe { *libc::__errno_location() = libc::EBADF };
            return -1;
        }
    };

    match map.get_handle(fd) {
        Some(_handle) => {
            let current_offset = map.get_offset(fd).unwrap_or(0);
            let size = map.get_size(fd).unwrap_or(0);

            let new_offset = match whence {
                libc::SEEK_SET => {
                    if offset < 0 {
                        unsafe { *libc::__errno_location() = libc::EINVAL };
                        return -1;
                    }
                    offset as u64
                }
                libc::SEEK_CUR => {
                    let result = (current_offset as i64).checked_add(offset);
                    match result {
                        Some(r) if r >= 0 => r as u64,
                        _ => {
                            unsafe { *libc::__errno_location() = libc::EINVAL };
                            return -1;
                        }
                    }
                }
                libc::SEEK_END => {
                    let result = (size as i64).checked_add(offset);
                    match result {
                        Some(r) if r >= 0 => r as u64,
                        _ => {
                            unsafe { *libc::__errno_location() = libc::EINVAL };
                            return -1;
                        }
                    }
                }
                _ => {
                    unsafe { *libc::__errno_location() = libc::EINVAL };
                    return -1;
                }
            };

            map.update_offset(fd, new_offset);
            new_offset as i64
        }
        None => {
            unsafe { *libc::__errno_location() = libc::EBADF };
            -1
        }
    }
}

/// Handle remote stat operation
fn handle_remote_stat(path: &str, statbuf: *mut libc::stat) -> c_int {
    let conn_guard = CONNECTION.lock().unwrap();
    let conn = match conn_guard.as_ref() {
        Some(c) => c,
        None => {
            unsafe { *libc::__errno_location() = libc::ENOENT };
            return -1;
        }
    };

    let rel_path = get_relative_path(path);

    match conn.stat(rel_path) {
        Ok((size, mtime)) => {
            unsafe {
                // Zero out the stat buffer
                std::ptr::write_bytes(statbuf, 0, 1);

                // Fill in the fields we have
                (*statbuf).st_size = size as i64;
                (*statbuf).st_mtime = mtime;
                (*statbuf).st_mode = libc::S_IFREG | 0o644; // Regular file, readable
            }
            0
        }
        Err(errno) => {
            unsafe { *libc::__errno_location() = errno };
            -1
        }
    }
}

/// Handle remote fstat operation
fn handle_remote_fstat(fd: c_int, statbuf: *mut libc::stat) -> c_int {
    let fd_map = FD_MAP.lock().unwrap();
    let map = match fd_map.as_ref() {
        Some(m) => m,
        None => {
            unsafe { *libc::__errno_location() = libc::EBADF };
            return -1;
        }
    };

    match (map.get_handle(fd), map.get_size(fd)) {
        (Some(_handle), Some(size)) => {
            unsafe {
                // Zero out the stat buffer
                std::ptr::write_bytes(statbuf, 0, 1);

                // Fill in the fields we have
                (*statbuf).st_size = size as i64;
                (*statbuf).st_mode = libc::S_IFREG | 0o644; // Regular file, readable
            }
            0
        }
        _ => {
            unsafe { *libc::__errno_location() = libc::EBADF };
            -1
        }
    }
}

// Constructor - called when library is loaded
#[cfg(target_os = "linux")]
#[link_section = ".init_array"]
#[used]
static INIT: extern "C" fn() = init;

#[cfg(target_os = "linux")]
extern "C" fn init() {
    initialize();
}

// Destructor - called when library is unloaded
#[cfg(target_os = "linux")]
#[link_section = ".fini_array"]
#[used]
static FINI: extern "C" fn() = fini;

#[cfg(target_os = "linux")]
extern "C" fn fini() {
    cleanup();
}
