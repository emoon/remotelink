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

use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::os::raw::c_int;
use std::path::PathBuf;
use std::sync::Mutex;

// Global state
static FD_MAP: Mutex<Option<fd_map::FdMap>> = Mutex::new(None);
static CONNECTION: Mutex<Option<remotelink_client::FileServerClient>> = Mutex::new(None);

// Cache for shared libraries - these need to be real files for mmap() to work
static CACHED_FILES: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

// Directory state for opendir/readdir emulation
// Maps DIR* pointer value to (entries, current_index)
static DIR_STATE: Mutex<Option<HashMap<usize, DirState>>> = Mutex::new(None);

struct DirState {
    entries: Vec<remotelink_client::DirEntry>,
    index: usize,
    // Pre-allocated dirent for readdir to return
    dirent: libc::dirent,
}

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

    // Initialize directory state map
    *DIR_STATE.lock().unwrap() = Some(HashMap::new());

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

/// Check if a path should be handled remotely (explicit /host/ prefix)
fn is_remote_path(path: &str) -> bool {
    path.starts_with("/host/")
}

/// Check if we have an active remote connection
fn has_remote_connection() -> bool {
    CONNECTION.lock().unwrap().is_some()
}

/// Get the relative path by stripping /host/ prefix
pub(crate) fn get_relative_path(path: &str) -> &str {
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

/// Download a remote file's contents into memory
/// Returns the file data or an errno on error
pub(crate) fn download_remote_file(path: &str) -> Result<Vec<u8>, i32> {
    let conn_guard = CONNECTION.lock().unwrap();
    let conn = conn_guard.as_ref().ok_or(libc::ENOENT)?;

    let rel_path = get_relative_path(path);

    // Open the remote file
    let (handle, size) = conn.open(rel_path)?;

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
            Err(errno) => {
                let _ = conn.close(handle);
                return Err(errno);
            }
        }
    }

    // Close remote file
    let _ = conn.close(handle);

    Ok(data)
}

/// Cache a remote file locally and return the local path
/// This is used for shared libraries which need to be mmap'd by the dynamic linker
pub(crate) fn cache_remote_file(path: &str) -> Option<PathBuf> {
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

    // Download the file
    let data = download_remote_file(path).ok()?;

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
                    type OpenFn =
                        unsafe extern "C" fn(*const std::os::raw::c_char, c_int, c_int) -> c_int;
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
        Ok((size, mtime, is_dir)) => {
            unsafe {
                // Zero out the stat buffer
                std::ptr::write_bytes(statbuf, 0, 1);

                // Fill in the fields we have
                (*statbuf).st_size = size as i64;
                (*statbuf).st_mtime = mtime;
                // Set mode based on whether it's a directory or file
                (*statbuf).st_mode = if is_dir {
                    libc::S_IFDIR | 0o755 // Directory, readable and executable
                } else {
                    libc::S_IFREG | 0o644 // Regular file, readable
                };
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

/// Handle remote dlopen operation
/// Downloads the shared library to local cache and calls real dlopen on it
fn handle_remote_dlopen(path: &str, flags: c_int) -> *mut std::os::raw::c_void {
    // Cache the remote file locally
    if let Some(cache_path) = cache_remote_file(path) {
        if let Some(path_str) = cache_path.to_str() {
            if let Ok(c_path) = CString::new(path_str) {
                // Call real dlopen on the cached file
                type DlopenFn = unsafe extern "C" fn(
                    *const std::os::raw::c_char,
                    c_int,
                ) -> *mut std::os::raw::c_void;
                let real_dlopen: DlopenFn = unsafe { ffi::get_real_fn("dlopen") };
                return unsafe { real_dlopen(c_path.as_ptr(), flags) };
            }
        }
    }

    // Failed to cache, return null
    std::ptr::null_mut()
}

/// Handle remote opendir operation
pub(crate) fn handle_remote_opendir(path: &str) -> *mut libc::DIR {
    let conn_guard = CONNECTION.lock().unwrap();
    let conn = match conn_guard.as_ref() {
        Some(c) => c,
        None => {
            unsafe { *libc::__errno_location() = libc::ENOENT };
            return std::ptr::null_mut();
        }
    };

    let rel_path = get_relative_path(path);

    match conn.readdir(rel_path) {
        Ok(entries) => {
            drop(conn_guard);

            // Create a fake DIR pointer using Box
            let fake_dir = Box::new(0u8);
            let dir_ptr = Box::into_raw(fake_dir) as *mut libc::DIR;

            // Store the directory state
            let state = DirState {
                entries,
                index: 0,
                dirent: unsafe { std::mem::zeroed() },
            };

            if let Some(map) = DIR_STATE.lock().unwrap().as_mut() {
                map.insert(dir_ptr as usize, state);
            }

            dir_ptr
        }
        Err(errno) => {
            unsafe { *libc::__errno_location() = errno };
            std::ptr::null_mut()
        }
    }
}

/// Check if a DIR* is one we're tracking
pub(crate) fn is_virtual_dir(dir: *mut libc::DIR) -> bool {
    if let Some(map) = DIR_STATE.lock().unwrap().as_ref() {
        map.contains_key(&(dir as usize))
    } else {
        false
    }
}

/// Handle remote readdir operation
pub(crate) fn handle_remote_readdir(dir: *mut libc::DIR) -> *mut libc::dirent {
    let mut state_guard = DIR_STATE.lock().unwrap();
    let state_map = match state_guard.as_mut() {
        Some(m) => m,
        None => return std::ptr::null_mut(),
    };

    let state = match state_map.get_mut(&(dir as usize)) {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };

    if state.index >= state.entries.len() {
        return std::ptr::null_mut();
    }

    let entry = &state.entries[state.index];
    state.index += 1;

    state.dirent.d_ino = 1;
    state.dirent.d_off = state.index as i64;
    state.dirent.d_reclen = std::mem::size_of::<libc::dirent>() as u16;
    state.dirent.d_type = if entry.is_dir {
        libc::DT_DIR
    } else {
        libc::DT_REG
    };

    let name_bytes = entry.name.as_bytes();
    let max_len = state.dirent.d_name.len() - 1;
    let copy_len = std::cmp::min(name_bytes.len(), max_len);

    // Zero out the name buffer - cast to handle platform differences (i8 vs u8)
    unsafe {
        std::ptr::write_bytes(
            state.dirent.d_name.as_mut_ptr(),
            0,
            state.dirent.d_name.len(),
        );
        std::ptr::copy_nonoverlapping(
            name_bytes.as_ptr(),
            state.dirent.d_name.as_mut_ptr() as *mut u8,
            copy_len,
        );
    }

    &mut state.dirent as *mut libc::dirent
}

/// Handle remote closedir operation
pub(crate) fn handle_remote_closedir(dir: *mut libc::DIR) -> c_int {
    let mut state_guard = DIR_STATE.lock().unwrap();
    if let Some(state_map) = state_guard.as_mut() {
        if state_map.remove(&(dir as usize)).is_some() {
            let _ = unsafe { Box::from_raw(dir as *mut u8) };
            return 0;
        }
    }
    unsafe { *libc::__errno_location() = libc::EBADF };
    -1
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
