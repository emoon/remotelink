/// Integration test for remote shared library loading via LD_PRELOAD
/// Tests that .so files served from /host/ can be loaded with dlopen()
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_remote_shared_library_loading() {
    println!("\n=== Remote Shared Library Loading Test ===\n");

    // Create temporary directory structure
    let temp_dir = TempDir::new().unwrap();
    let libs_dir = temp_dir.path().join("libs");
    fs::create_dir(&libs_dir).unwrap();
    println!("✓ Created temp directory: {:?}", temp_dir.path());

    // Compile the shared library
    let shared_lib = libs_dir.join("libshared_test.so");
    let compile_lib = Command::new("gcc")
        .args(&[
            "-shared",
            "-fPIC",
            "-o",
            shared_lib.to_str().unwrap(),
            "tests/shared_lib_test.c",
        ])
        .output();

    match compile_lib {
        Ok(output) if output.status.success() => {
            println!("✓ Compiled shared library: {:?}", shared_lib);
        }
        Ok(output) => {
            eprintln!("✗ Failed to compile shared library:");
            eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
            panic!("Shared library compilation failed");
        }
        Err(e) => {
            eprintln!("✗ Failed to run gcc: {}", e);
            panic!("gcc not available");
        }
    }

    // Compile the test program
    let test_binary = temp_dir.path().join("dlopen_test");
    let compile_test = Command::new("gcc")
        .args(&[
            "-o",
            test_binary.to_str().unwrap(),
            "tests/dlopen_test.c",
            "-ldl",
        ])
        .output();

    match compile_test {
        Ok(output) if output.status.success() => {
            println!("✓ Compiled test program: {:?}", test_binary);
        }
        Ok(output) => {
            eprintln!("✗ Failed to compile test program:");
            eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
            panic!("Test program compilation failed");
        }
        Err(e) => {
            eprintln!("✗ Failed to run gcc: {}", e);
            panic!("gcc not available");
        }
    }

    // Start file server
    let dir_path = temp_dir.path().to_str().unwrap().to_string();
    let server_handle = remotelink::file_server::start_file_server(vec![dir_path]).unwrap();
    println!(
        "✓ Started file server serving: {}",
        temp_dir.path().display()
    );

    // Give server time to start
    thread::sleep(Duration::from_millis(300));

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

    println!("✅ REMOTE SHARED LIBRARY LOADING TEST PASSED!\n");
    println!("This proves that:");
    println!("  1. Shared libraries can be served from the file server");
    println!("  2. dlopen() works with /host/ paths");
    println!("  3. The .so is cached locally for mmap()");
    println!("  4. dlsym() can find symbols in the loaded library");
    println!("  5. Functions in the remote library work correctly");
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
