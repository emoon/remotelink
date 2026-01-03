/// Comprehensive end-to-end test for the file server
/// This test starts one server and performs all operations to avoid port conflicts
use std::fs;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

use remotelink_client::FileServerClient;

#[test]
fn test_file_server_comprehensive() {
    // Create temporary directory with multiple test files
    let temp_dir = TempDir::new().unwrap();

    // Test file 1: Basic read
    let test_file_1 = temp_dir.path().join("test1.txt");
    let content_1 = b"Hello from host!";
    fs::write(&test_file_1, content_1).unwrap();

    // Test file 2: Partial reads
    let test_file_2 = temp_dir.path().join("test2.txt");
    let content_2 = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    fs::write(&test_file_2, content_2).unwrap();

    // Test file 3: Stat test
    let test_file_3 = temp_dir.path().join("test3.txt");
    let content_3 = b"Stat test data";
    fs::write(&test_file_3, content_3).unwrap();

    // Start file server
    let dir_path = temp_dir.path().to_str().unwrap().to_string();
    let server_handle = remotelink::file_server::start_file_server(vec![dir_path]).unwrap();

    // Give server time to start
    thread::sleep(Duration::from_millis(300));

    // Connect client
    let client = FileServerClient::new("127.0.0.1:8889").unwrap();

    println!("✓ Server started and client connected");

    // TEST 1: Basic open, read, close
    {
        let (handle, size) = client.open("test1.txt").unwrap();
        assert_eq!(size, content_1.len() as u64);
        println!("✓ Opened test1.txt, size={}", size);

        let data = client.read(handle, 0, size as u32).unwrap();
        assert_eq!(data, content_1);
        println!("✓ Read test1.txt, data matches");

        client.close(handle).unwrap();
        println!("✓ Closed test1.txt");
    }

    // TEST 2: Stat
    {
        let (size, _mtime, is_dir) = client.stat("test3.txt").unwrap();
        assert_eq!(size, content_3.len() as u64);
        assert!(!is_dir);
        println!("✓ Stat test3.txt, size={}", size);
    }

    // TEST 3: Partial reads
    {
        let (handle, size) = client.open("test2.txt").unwrap();
        assert_eq!(size, content_2.len() as u64);
        println!("✓ Opened test2.txt for partial reads");

        // Read first 10 bytes
        let data = client.read(handle, 0, 10).unwrap();
        assert_eq!(data, b"0123456789");
        println!(
            "✓ Read bytes 0-9: {:?}",
            std::str::from_utf8(&data).unwrap()
        );

        // Read middle 10 bytes
        let data = client.read(handle, 10, 10).unwrap();
        assert_eq!(data, b"ABCDEFGHIJ");
        println!(
            "✓ Read bytes 10-19: {:?}",
            std::str::from_utf8(&data).unwrap()
        );

        // Read last part
        let data = client.read(handle, 20, 16).unwrap();
        assert_eq!(data, b"KLMNOPQRSTUVWXYZ");
        println!(
            "✓ Read bytes 20-35: {:?}",
            std::str::from_utf8(&data).unwrap()
        );

        client.close(handle).unwrap();
        println!("✓ Closed test2.txt");
    }

    // TEST 4: Path traversal protection
    {
        let result = client.open("../../../etc/passwd");
        assert!(result.is_err());
        println!("✓ Path traversal blocked: ../../../etc/passwd");

        let result = client.open("subdir/../../etc/passwd");
        assert!(result.is_err());
        println!("✓ Path traversal blocked: subdir/../../etc/passwd");
    }

    // TEST 5: Missing file
    {
        let result = client.open("does_not_exist.txt");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), libc::ENOENT);
        println!("✓ Missing file returns ENOENT");
    }

    // TEST 6: Multiple files open simultaneously
    {
        let (handle1, _) = client.open("test1.txt").unwrap();
        let (handle2, _) = client.open("test2.txt").unwrap();
        let (handle3, _) = client.open("test3.txt").unwrap();
        println!(
            "✓ Multiple files opened: handles {}, {}, {}",
            handle1, handle2, handle3
        );

        client.close(handle1).unwrap();
        client.close(handle2).unwrap();
        client.close(handle3).unwrap();
        println!("✓ All handles closed");
    }

    // Cleanup
    drop(client);
    drop(server_handle);

    println!("\n✅ ALL TESTS PASSED!");
}
