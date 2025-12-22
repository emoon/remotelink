// LD_PRELOAD library for intercepting file operations and proxying them to remotelink file server
//
// This library intercepts libc functions for files with the /host/ prefix and forwards
// the operations to the file server over TCP.
//
// Shared libraries (.so files) are automatically cached locally to enable mmap() by the
// dynamic linker.

#![allow(non_camel_case_types)]

mod fd_map;
mod ffi;

use std::collections::HashSet;
use std::ffi::CString;
use std::os::raw::c_int;
use std::path::PathBuf;
use std::sync::Mutex;

// Global state
static FD_MAP: Mutex<Option<fd_map::FdMap>> = Mutex::new(None);
static CONNECTION: Mutex<Option<remotelink_client::FileServerClient>> = Mutex::new(None);

// Cache for shared libraries - these need to be real files for mmap() to work
static CACHED_FILES: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

/// Get the cache directory path, including process ID to avoid conflicts
fn get_cache_dir() -> PathBuf {
    PathBuf::from(format!("/tmp/remotelink-cache-{}", std::process::id()))
}

/// Initialize the library - called when library is loaded
fn initialize() {
    // Initialize FD map
    *FD_MAP.lock().unwrap() = Some(fd_map::FdMap::new());

    // Initialize cached files set
    *CACHED_FILES.lock().unwrap() = Some(HashSet::new());

    // Initialize connection if REMOTELINK_FILE_SERVER is set
    if let Ok(addr) = std::env::var("REMOTELINK_FILE_SERVER") {
        match remotelink_client::FileServerClient::new(&addr) {
            Ok(conn) => {
                *CONNECTION.lock().unwrap() = Some(conn);
            }
            Err(e) => {
                eprintln!(
                    "remotelink_preload: Failed to connect to file server {}: {}",
                    addr, e
                );
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

    // Clean up cached shared library files
    if let Some(cached) = CACHED_FILES.lock().unwrap().as_ref() {
        for path in cached.iter() {
            let _ = std::fs::remove_file(path);
        }
    }

    // Try to remove the cache directory (will only succeed if empty)
    let cache_dir = get_cache_dir();
    let _ = std::fs::remove_dir_all(&cache_dir);
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

/// Check if a path refers to a shared library
/// Matches: .so, .so.1, .so.1.2, .so.1.2.3, etc.
fn is_shared_library(path: &str) -> bool {
    // Check for exact .so extension
    if path.ends_with(".so") {
        return true;
    }

    // Check for versioned .so files like libfoo.so.1 or libfoo.so.1.2.3
    if let Some(idx) = path.rfind(".so.") {
        // Verify everything after .so. is digits and dots (version number)
        let suffix = &path[idx + 4..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_shared_library() {
        // Positive cases
        assert!(is_shared_library("/host/libs/libfoo.so"));
        assert!(is_shared_library("/host/libs/libfoo.so.1"));
        assert!(is_shared_library("/host/libs/libfoo.so.1.2"));
        assert!(is_shared_library("/host/libs/libfoo.so.1.2.3"));
        assert!(is_shared_library("libbar.so"));
        assert!(is_shared_library("libc.so.6"));

        // Negative cases
        assert!(!is_shared_library("/host/data/file.txt"));
        assert!(!is_shared_library("/host/data/file.json"));
        assert!(!is_shared_library("/host/data/file.so.txt")); // Not a version number
        assert!(!is_shared_library("/host/data/file.so.abc")); // Not digits
        assert!(!is_shared_library("/host/data/myfile"));
    }
}

/// Cache a remote file locally and return the local path
/// This is used for shared libraries which need to be mmap'd by the dynamic linker
fn cache_remote_file(path: &str) -> Option<PathBuf> {
    let conn_guard = CONNECTION.lock().unwrap();
    let conn = conn_guard.as_ref()?;

    let rel_path = get_relative_path(path);
    let cache_dir = get_cache_dir();
    let cache_path = cache_dir.join(rel_path);

    // Check if already cached
    {
        let cached = CACHED_FILES.lock().unwrap();
        if let Some(set) = cached.as_ref() {
            if set.contains(&cache_path) && cache_path.exists() {
                return Some(cache_path);
            }
        }
    }

    // Create parent directories
    if let Some(parent) = cache_path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return None;
        }
    }

    // Open the remote file
    let (handle, size) = match conn.open(rel_path) {
        Ok(result) => result,
        Err(_) => return None,
    };

    // Download the entire file
    let mut data = Vec::with_capacity(size as usize);
    let mut offset = 0u64;
    let chunk_size = 4 * 1024 * 1024u32; // 4MB chunks

    while offset < size {
        let to_read = std::cmp::min(chunk_size, (size - offset) as u32);
        match conn.read(handle, offset, to_read) {
            Ok(chunk) => {
                if chunk.is_empty() {
                    break;
                }
                offset += chunk.len() as u64;
                data.extend(chunk);
            }
            Err(_) => {
                let _ = conn.close(handle);
                return None;
            }
        }
    }

    // Close remote file
    let _ = conn.close(handle);

    // Write to cache
    if std::fs::write(&cache_path, &data).is_err() {
        return None;
    }

    // Track cached file for cleanup
    if let Some(set) = CACHED_FILES.lock().unwrap().as_mut() {
        set.insert(cache_path.clone());
    }

    Some(cache_path)
}

/// Handle remote open operation
fn handle_remote_open(path: &str, flags: c_int, mode: c_int) -> c_int {
    // For shared libraries, cache locally and return a real FD
    // This allows the dynamic linker to mmap() the file
    if is_shared_library(path) {
        if let Some(cache_path) = cache_remote_file(path) {
            if let Some(path_str) = cache_path.to_str() {
                if let Ok(c_path) = CString::new(path_str) {
                    // Call real open on the cached file
                    type OpenFn = unsafe extern "C" fn(*const i8, c_int, c_int) -> c_int;
                    let real_open: OpenFn = unsafe { ffi::get_real_fn("open") };
                    return unsafe { real_open(c_path.as_ptr(), flags, mode) };
                }
            }
        }
        // Failed to cache, return error
        unsafe { *libc::__errno_location() = libc::ENOENT };
        return -1;
    }

    // For non-shared-library files, use virtual FD approach
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
        Some(handle) => match conn.close(handle) {
            Ok(()) => {
                map.release(fd);
                0
            }
            Err(errno) => {
                unsafe { *libc::__errno_location() = errno };
                -1
            }
        },
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

/// Handle remote access operation
/// Used by the dynamic linker to check if files exist before opening them
fn handle_remote_access(path: &str, _mode: c_int) -> c_int {
    let conn_guard = CONNECTION.lock().unwrap();
    let conn = match conn_guard.as_ref() {
        Some(c) => c,
        None => {
            unsafe { *libc::__errno_location() = libc::ENOENT };
            return -1;
        }
    };

    let rel_path = get_relative_path(path);

    // Use stat to check if file exists
    match conn.stat(rel_path) {
        Ok(_) => 0, // File exists and is accessible
        Err(errno) => {
            unsafe { *libc::__errno_location() = errno };
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
