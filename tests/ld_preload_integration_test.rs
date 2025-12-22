/// End-to-end integration test for LD_PRELOAD file interception
/// This test proves that the preload library actually intercepts /host/ paths
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_ld_preload_file_interception() {
    println!("\n=== LD_PRELOAD Integration Test ===\n");

    // Create temporary directory with test file
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    let test_content = b"Hello from remote file server!";
    fs::write(&test_file, test_content).unwrap();
    println!("✓ Created test file: {:?}", test_file);

    // Start file server
    let dir_path = temp_dir.path().to_str().unwrap().to_string();
    let server_handle = remotelink::file_server::start_file_server(dir_path).unwrap();
    println!(
        "✓ Started file server serving: {}",
        temp_dir.path().display()
    );

    // Give server time to start
    thread::sleep(Duration::from_millis(300));

    // Set environment variables
    std::env::set_var("REMOTELINK_FILE_SERVER", "127.0.0.1:8889");
    println!("✓ Set REMOTELINK_FILE_SERVER=127.0.0.1:8889");

    // Compile test program
    let test_c_src = "tests/ld_preload_test.c";
    let test_binary = temp_dir.path().join("ld_preload_test");

    let compile_result = Command::new("gcc")
        .args(&[test_c_src, "-o", test_binary.to_str().unwrap()])
        .output();

    match compile_result {
        Ok(output) if output.status.success() => {
            println!("✓ Compiled test program: {:?}", test_binary);
        }
        Ok(output) => {
            eprintln!("✗ Failed to compile test program:");
            eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
            eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
            panic!("Compilation failed");
        }
        Err(e) => {
            eprintln!("✗ Failed to run gcc: {}", e);
            panic!("gcc not available");
        }
    }

    // Find preload library
    let preload_lib = find_preload_library();
    println!("✓ Found preload library: {:?}", preload_lib);

    // Run test program with LD_PRELOAD
    let result = Command::new(&test_binary)
        .env("LD_PRELOAD", &preload_lib)
        .env("REMOTELINK_FILE_SERVER", "127.0.0.1:8889")
        .output()
        .expect("Failed to run test program");

    println!("\n--- Test Program Output ---");
    print!("{}", String::from_utf8_lossy(&result.stdout));
    println!("---------------------------\n");

    if !result.stderr.is_empty() {
        println!("stderr:");
        print!("{}", String::from_utf8_lossy(&result.stderr));
    }

    // Cleanup
    drop(server_handle);

    // Check result
    if !result.status.success() {
        panic!(
            "Test program failed with exit code: {:?}",
            result.status.code()
        );
    }

    println!("✅ LD_PRELOAD INTEGRATION TEST PASSED!\n");
    println!("This proves that:");
    println!("  1. File server serves files correctly");
    println!("  2. Client library communicates with server");
    println!("  3. LD_PRELOAD library intercepts /host/ paths");
    println!("  4. All file operations (open/read/close/stat/fstat/lseek) work");
    println!("  5. Non-/host/ paths still use real syscalls");
}

/// Find the preload library in common locations
fn find_preload_library() -> String {
    let candidates = vec![
        "./target/release/libremotelink_preload.so",
        "./target/debug/libremotelink_preload.so",
        "/usr/local/lib/libremotelink_preload.so",
        "/usr/lib/libremotelink_preload.so",
    ];

    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return path.to_string();
        }
    }

    // Build it if not found
    println!("Preload library not found, building it...");
    let build_result = Command::new("cargo")
        .args(&["build", "--package", "remotelink_preload", "--release"])
        .output()
        .expect("Failed to build preload library");

    if !build_result.status.success() {
        panic!(
            "Failed to build preload library:\n{}",
            String::from_utf8_lossy(&build_result.stderr)
        );
    }

    "./target/release/libremotelink_preload.so".to_string()
}
