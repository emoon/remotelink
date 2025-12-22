use std::collections::HashMap;
use std::os::raw::c_int;

// Virtual FD offset - start virtual FDs at 10000 to avoid collision with real FDs
const VIRTUAL_FD_BASE: c_int = 10000;
const MAX_VIRTUAL_FDS: usize = 256;

/// Information about an open remote file
#[derive(Debug, Clone)]
struct FileInfo {
    handle: u32,
    offset: u64,
    size: u64,
}

/// Maps virtual file descriptors to remote file handles
pub struct FdMap {
    next_vfd: c_int,
    fd_to_info: HashMap<c_int, FileInfo>,
}

impl FdMap {
    pub fn new() -> Self {
        Self {
            next_vfd: VIRTUAL_FD_BASE,
            fd_to_info: HashMap::new(),
        }
    }

    /// Allocate a virtual FD for a remote file handle
    pub fn allocate(&mut self, handle: u32) -> Option<c_int> {
        if self.fd_to_info.len() >= MAX_VIRTUAL_FDS {
            return None;
        }

        let vfd = self.next_vfd;
        self.next_vfd = self.next_vfd.wrapping_add(1);
        if self.next_vfd < VIRTUAL_FD_BASE {
            self.next_vfd = VIRTUAL_FD_BASE;
        }

        self.fd_to_info.insert(
            vfd,
            FileInfo {
                handle,
                offset: 0,
                size: 0, // Will be set by caller if needed
            },
        );

        Some(vfd)
    }

    /// Release a virtual FD
    pub fn release(&mut self, vfd: c_int) {
        self.fd_to_info.remove(&vfd);
    }

    /// Get the remote file handle for a virtual FD
    pub fn get_handle(&self, vfd: c_int) -> Option<u32> {
        self.fd_to_info.get(&vfd).map(|info| info.handle)
    }

    /// Get the current offset for a virtual FD
    pub fn get_offset(&self, vfd: c_int) -> Option<u64> {
        self.fd_to_info.get(&vfd).map(|info| info.offset)
    }

    /// Get the file size for a virtual FD
    pub fn get_size(&self, vfd: c_int) -> Option<u64> {
        self.fd_to_info.get(&vfd).map(|info| info.size)
    }

    /// Update the offset for a virtual FD
    pub fn update_offset(&mut self, vfd: c_int, offset: u64) {
        if let Some(info) = self.fd_to_info.get_mut(&vfd) {
            info.offset = offset;
        }
    }

    /// Set the file size for a virtual FD
    pub fn set_size(&mut self, vfd: c_int, size: u64) {
        if let Some(info) = self.fd_to_info.get_mut(&vfd) {
            info.size = size;
        }
    }

    /// Check if a FD is a virtual FD (remote file)
    pub fn is_virtual_fd(&self, fd: c_int) -> bool {
        self.fd_to_info.contains_key(&fd)
    }

    /// Close all open virtual FDs
    pub fn cleanup(&mut self) {
        // In a real implementation, we would close all remote files here
        // For now, just clear the map
        self.fd_to_info.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocate_and_release() {
        let mut map = FdMap::new();

        let vfd = map.allocate(42).unwrap();
        assert!(vfd >= VIRTUAL_FD_BASE);
        assert_eq!(map.get_handle(vfd), Some(42));

        map.release(vfd);
        assert_eq!(map.get_handle(vfd), None);
    }

    #[test]
    fn test_offset_tracking() {
        let mut map = FdMap::new();

        let vfd = map.allocate(42).unwrap();
        assert_eq!(map.get_offset(vfd), Some(0));

        map.update_offset(vfd, 100);
        assert_eq!(map.get_offset(vfd), Some(100));
    }

    #[test]
    fn test_size_tracking() {
        let mut map = FdMap::new();

        let vfd = map.allocate(42).unwrap();
        assert_eq!(map.get_size(vfd), Some(0));

        map.set_size(vfd, 1024);
        assert_eq!(map.get_size(vfd), Some(1024));
    }

    #[test]
    fn test_is_virtual_fd() {
        let mut map = FdMap::new();

        let vfd = map.allocate(42).unwrap();
        assert!(map.is_virtual_fd(vfd));
        assert!(!map.is_virtual_fd(3)); // Regular FD
    }

    #[test]
    fn test_max_virtual_fds() {
        let mut map = FdMap::new();

        // Allocate MAX_VIRTUAL_FDS
        for i in 0..MAX_VIRTUAL_FDS {
            assert!(map.allocate(i as u32).is_some());
        }

        // Next allocation should fail
        assert!(map.allocate(999).is_none());
    }
}
