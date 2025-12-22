/// Integration test for local-first fallback behavior
/// Tests that files try local first, then fall back to remote on ENOENT
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_local_first_fallback() {
    println!("\n=== Local-First Fallback Test ===\n");

    // Create two directories: one for local files, one for remote (file server)
    let local_dir = TempDir::new().unwrap();
    let remote_dir = TempDir::new().unwrap();

    // Create data subdirectory in both (test uses relative paths like "data/file.txt")
    fs::create_dir(local_dir.path().join("data")).unwrap();
    fs::create_dir(remote_dir.path().join("data")).unwrap();

    // Create local-only file (exists locally, not on remote)
    let local_only = local_dir.path().join("data/local_only.txt");
    fs::write(&local_only, b"LOCAL content").unwrap();
    println!("Created local-only file: {:?}", local_only);

    // Create remote-only file (exists on remote, not locally)
    let remote_only = remote_dir.path().join("data/remote_only.txt");
    fs::write(&remote_only, b"REMOTE content").unwrap();
    println!("Created remote-only file: {:?}", remote_only);

    // Start file server serving remote_dir
    let dir_path = remote_dir.path().to_str().unwrap().to_string();
    let server_handle = remotelink::file_server::start_file_server(dir_path).unwrap();
    println!(
        "Started file server serving: {}",
        remote_dir.path().display()
    );

    // Give server time to start
    thread::sleep(Duration::from_millis(300));

    // Compile test program
    let test_binary = local_dir.path().join("fallback_test");
    let compile_result = Command::new("gcc")
        .args(&["tests/fallback_test.c", "-o", test_binary.to_str().unwrap()])
        .output();

    match compile_result {
        Ok(output) if output.status.success() => {
            println!("Compiled test program: {:?}", test_binary);
        }
        Ok(output) => {
            eprintln!("Failed to compile test program:");
            eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
            panic!("Compilation failed");
        }
        Err(e) => {
            eprintln!("Failed to run gcc: {}", e);
            panic!("gcc not available");
        }
    }

    // Find preload library
    let preload_lib = find_preload_library();
    println!("Found preload library: {:?}", preload_lib);

    // Run test program with LD_PRELOAD, working directory set to local_dir
    // The test program uses relative paths from its working directory
    let result = Command::new(&test_binary)
        .current_dir(local_dir.path())
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

    println!("LOCAL-FIRST FALLBACK TEST PASSED!\n");
    println!("This proves that:");
    println!("  1. Files existing locally are read from local filesystem");
    println!("  2. Files not found locally fall back to remote");
    println!("  3. Files not found anywhere return ENOENT");
    println!("  4. stat() and access() also use fallback");
}

/// Find the preload library and return absolute path
fn find_preload_library() -> String {
    let candidates = vec![
        "./target/release/libremotelink_preload.so",
        "./target/debug/libremotelink_preload.so",
    ];

    for path in &candidates {
        let p = std::path::Path::new(path);
        if p.exists() {
            return std::fs::canonicalize(p)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
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

    std::fs::canonicalize("./target/release/libremotelink_preload.so")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string()
}
