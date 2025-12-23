use std::fs;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

use remotelink_client::FileServerClient;

// Each test uses a different port to avoid conflicts
const PORT_TEST_1: u16 = 8890;
const PORT_TEST_2: u16 = 8891;
const PORT_TEST_3: u16 = 8892;
const PORT_TEST_4: u16 = 8893;
const PORT_TEST_5: u16 = 8894;

#[test]
fn test_file_server_basic_open_read_close() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    let test_content = b"Hello from host!";
    fs::write(&test_file, test_content).unwrap();

    let dir_path = temp_dir.path().to_str().unwrap().to_string();
    let server_handle =
        remotelink::file_server::start_file_server_on_port(dir_path, PORT_TEST_1).unwrap();
    thread::sleep(Duration::from_millis(300));

    let client = FileServerClient::new(&format!("127.0.0.1:{}", PORT_TEST_1)).unwrap();

    // Test: Open file
    let (handle, size) = client.open("test.txt").unwrap();
    assert_eq!(size, test_content.len() as u64);

    // Test: Read file
    let data = client.read(handle, 0, size as u32).unwrap();
    assert_eq!(data, test_content);

    // Test: Close file
    client.close(handle).unwrap();

    drop(client);
    drop(server_handle);
}

#[test]
fn test_file_server_stat() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("stat_test.txt");
    fs::write(&test_file, b"Test data for stat").unwrap();

    let dir_path = temp_dir.path().to_str().unwrap().to_string();
    let server_handle =
        remotelink::file_server::start_file_server_on_port(dir_path, PORT_TEST_2).unwrap();
    thread::sleep(Duration::from_millis(300));

    let client = FileServerClient::new(&format!("127.0.0.1:{}", PORT_TEST_2)).unwrap();

    // Test: Stat file
    let (size, _mtime, is_dir) = client.stat("stat_test.txt").unwrap();
    assert_eq!(size, 18);
    assert!(!is_dir);

    drop(client);
    drop(server_handle);
}

#[test]
fn test_file_server_path_traversal_blocked() {
    let temp_dir = TempDir::new().unwrap();

    let dir_path = temp_dir.path().to_str().unwrap().to_string();
    let server_handle =
        remotelink::file_server::start_file_server_on_port(dir_path, PORT_TEST_3).unwrap();
    thread::sleep(Duration::from_millis(300));

    let client = FileServerClient::new(&format!("127.0.0.1:{}", PORT_TEST_3)).unwrap();

    // Test: Path traversal should be rejected
    let result = client.open("../../../etc/passwd");
    assert!(result.is_err());

    let result = client.open("subdir/../../etc/passwd");
    assert!(result.is_err());

    drop(client);
    drop(server_handle);
}

#[test]
fn test_file_server_missing_file() {
    let temp_dir = TempDir::new().unwrap();

    let dir_path = temp_dir.path().to_str().unwrap().to_string();
    let server_handle =
        remotelink::file_server::start_file_server_on_port(dir_path, PORT_TEST_4).unwrap();
    thread::sleep(Duration::from_millis(300));

    let client = FileServerClient::new(&format!("127.0.0.1:{}", PORT_TEST_4)).unwrap();

    // Test: Opening non-existent file should fail
    let result = client.open("does_not_exist.txt");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), libc::ENOENT);

    drop(client);
    drop(server_handle);
}

#[test]
fn test_file_server_partial_read() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("partial.txt");
    let test_content = b"0123456789ABCDEF";
    fs::write(&test_file, test_content).unwrap();

    let dir_path = temp_dir.path().to_str().unwrap().to_string();
    let server_handle =
        remotelink::file_server::start_file_server_on_port(dir_path, PORT_TEST_5).unwrap();
    thread::sleep(Duration::from_millis(300));

    let client = FileServerClient::new(&format!("127.0.0.1:{}", PORT_TEST_5)).unwrap();

    let (handle, _size) = client.open("partial.txt").unwrap();

    // Test: Read first 5 bytes
    let data = client.read(handle, 0, 5).unwrap();
    assert_eq!(data, b"01234");

    // Test: Read middle 5 bytes
    let data = client.read(handle, 5, 5).unwrap();
    assert_eq!(data, b"56789");

    // Test: Read last 6 bytes
    let data = client.read(handle, 10, 6).unwrap();
    assert_eq!(data, b"ABCDEF");

    client.close(handle).unwrap();

    drop(client);
    drop(server_handle);
}
