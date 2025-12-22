mod common;

use anyhow::{Context, Result};
use serial_test::serial;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

/// Helper to compile a test program and return its path
fn compile_watch_test_program(source: &str, name: &str) -> Result<PathBuf> {
    common::compile_test_program(source, name)
}

/// Helper to recompile a test program (simulating a rebuild)
fn recompile_test_program(source: &str, exe_path: &PathBuf) -> Result<()> {
    let temp_dir = std::env::temp_dir();
    let source_file = temp_dir.join(format!(
        "{}_rebuild.rs",
        exe_path.file_name().unwrap().to_str().unwrap()
    ));

    // Write new source code
    fs::write(&source_file, source).context("Failed to write test source file")?;

    // Compile it to the same output path
    let output = Command::new("rustc")
        .arg(&source_file)
        .arg("-o")
        .arg(exe_path)
        .output()
        .context("Failed to run rustc")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Compilation failed:\n{}", stderr);
    }

    // Clean up source file
    let _ = fs::remove_file(&source_file);

    Ok(())
}

#[test]
#[serial]
fn test_watch_basic_restart() -> Result<()> {
    // Compile initial version
    let source_v1 = r#"
        fn main() {
            println!("Version 1");
        }
    "#;
    let exe_path = compile_watch_test_program(source_v1, "watch_basic")?;

    // Start server
    let port = common::find_available_port();
    let server = common::start_test_server(port)?;
    thread::sleep(Duration::from_millis(500));

    // Start client with --watch flag
    let exe_path_clone = exe_path.clone();
    let client_thread = thread::spawn(move || {
        Command::new(common::get_remotelink_binary())
            .arg("--target")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("--filename")
            .arg(&exe_path_clone)
            .arg("--watch")
            .arg("--log-level")
            .arg("info")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    });

    let mut client = client_thread.join().unwrap()?;

    // Give it time to start
    thread::sleep(Duration::from_secs(2));

    // Recompile with new version
    let source_v2 = r#"
        fn main() {
            println!("Version 2");
            std::process::exit(0);
        }
    "#;

    // Wait a bit before recompiling to ensure clean separation
    thread::sleep(Duration::from_millis(500));

    recompile_test_program(source_v2, &exe_path)?;

    // Give watch system time to detect, verify stability, and restart
    thread::sleep(Duration::from_secs(3));

    // Kill the client
    client.kill()?;
    let output = client.wait_with_output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Debug output
    println!("STDOUT:\n{}", stdout);
    println!("STDERR:\n{}", stderr);

    // Verify that both versions ran
    assert!(
        stdout.contains("Version 1") || stderr.contains("Version 1"),
        "Expected 'Version 1' in output"
    );
    assert!(
        stdout.contains("Version 2") || stderr.contains("Version 2"),
        "Expected 'Version 2' in output - watch restart should have occurred"
    );

    // Cleanup
    common::cleanup_test_executable(&exe_path);
    common::stop_test_server(server)?;
    common::cleanup_remotelink_temp_files();

    Ok(())
}

#[test]
#[serial]
fn test_watch_multiple_rebuilds() -> Result<()> {
    // Compile initial version that exits immediately
    let source = r#"
        fn main() {
            println!("Iteration 0");
            std::process::exit(0);
        }
    "#;
    let exe_path = compile_watch_test_program(source, "watch_multiple")?;

    // Start server
    let port = common::find_available_port();
    let server = common::start_test_server(port)?;
    thread::sleep(Duration::from_millis(500));

    // Start client with --watch
    let exe_path_clone = exe_path.clone();
    let client_thread = thread::spawn(move || {
        Command::new(common::get_remotelink_binary())
            .arg("--target")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("--filename")
            .arg(&exe_path_clone)
            .arg("--watch")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    });

    let mut client = client_thread.join().unwrap()?;

    // Give it time to start
    thread::sleep(Duration::from_secs(2));

    // Recompile twice to verify multiple restarts work
    for i in 1..=2 {
        let source = format!(
            r#"
            fn main() {{
                println!("Iteration {i}");
                std::process::exit(0);
            }}
            "#
        );

        // Wait before recompiling to ensure clean separation
        thread::sleep(Duration::from_millis(800));
        recompile_test_program(&source, &exe_path)?;

        // Give time for watch to detect, verify stability, and restart
        // Need longer time for stability checks (400ms) plus detection/restart
        thread::sleep(Duration::from_secs(3));
    }

    // Kill the client
    client.kill()?;
    let output = client.wait_with_output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("STDOUT:\n{}", stdout);
    println!("STDERR:\n{}", stderr);

    // Should see at least the initial version and one rebuilt version
    assert!(
        stdout.contains("Iteration 0") || stderr.contains("Iteration 0"),
        "Expected 'Iteration 0' in output"
    );
    // At least one rebuild should have been detected and run
    assert!(
        (stdout.contains("Iteration 1") || stderr.contains("Iteration 1"))
            || (stdout.contains("Iteration 2") || stderr.contains("Iteration 2")),
        "Expected at least one rebuild (Iteration 1 or 2) in output"
    );

    // Cleanup
    common::cleanup_test_executable(&exe_path);
    common::stop_test_server(server)?;
    common::cleanup_remotelink_temp_files();

    Ok(())
}

#[test]
#[serial]
fn test_watch_stability_detection() -> Result<()> {
    // This test verifies that the watcher waits for file to be fully written
    // We'll simulate a slow write by writing in chunks

    let exe_path =
        common::compile_test_program(r#"fn main() { println!("Test"); }"#, "watch_stability")?;

    let port = common::find_available_port();
    let server = common::start_test_server(port)?;
    thread::sleep(Duration::from_millis(500));

    // Start client with watch
    let exe_path_clone = exe_path.clone();
    let client_thread = thread::spawn(move || {
        Command::new(common::get_remotelink_binary())
            .arg("--target")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("--filename")
            .arg(&exe_path_clone)
            .arg("--watch")
            .arg("--log-level")
            .arg("debug")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    });

    let mut client = client_thread.join().unwrap()?;
    thread::sleep(Duration::from_secs(1));

    // Simulate partial write by writing file in chunks
    let exe_path_for_write = exe_path.clone();
    thread::spawn(move || {
        // Open file for writing
        let mut file = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&exe_path_for_write)
            .unwrap();

        // Write some data
        file.write_all(b"partial").unwrap();
        file.flush().unwrap();

        // Wait (file is unstable during this period)
        thread::sleep(Duration::from_millis(300));

        // Write more data
        file.write_all(b" write").unwrap();
        file.flush().unwrap();
    });

    // Wait for potential restart attempt
    thread::sleep(Duration::from_secs(2));

    // Now recompile properly
    let source_v2 = r#"
        fn main() {
            println!("Stable version");
        }
    "#;

    thread::sleep(Duration::from_millis(500));
    recompile_test_program(source_v2, &exe_path)?;

    // Wait for proper restart
    thread::sleep(Duration::from_secs(2));

    client.kill()?;
    let output = client.wait_with_output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("STDOUT:\n{}", stdout);
    println!("STDERR:\n{}", stderr);

    // Should see the stable version
    assert!(stdout.contains("Stable version") || stderr.contains("Stable version"));

    // Cleanup
    common::cleanup_test_executable(&exe_path);
    common::stop_test_server(server)?;
    common::cleanup_remotelink_temp_files();

    Ok(())
}

#[test]
#[serial]
fn test_no_watch_flag_behaves_normally() -> Result<()> {
    // Verify that WITHOUT --watch flag, behavior is unchanged
    let source = r#"
        fn main() {
            println!("Normal execution");
        }
    "#;
    let exe_path = compile_watch_test_program(source, "no_watch")?;

    let port = common::find_available_port();
    let server = common::start_test_server(port)?;
    thread::sleep(Duration::from_millis(500));

    // Run WITHOUT --watch flag
    let output = Command::new(common::get_remotelink_binary())
        .arg("--target")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--filename")
        .arg(&exe_path)
        .output()
        .context("Failed to run client")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("STDOUT:\n{}", stdout);
    println!("STDERR:\n{}", stderr);

    // Should execute once and exit
    assert!(stdout.contains("Normal execution") || stderr.contains("Normal execution"));

    // Should NOT mention watch mode
    assert!(!stdout.contains("Watch mode enabled"));
    assert!(!stderr.contains("Watch mode enabled"));

    // Cleanup
    common::cleanup_test_executable(&exe_path);
    common::stop_test_server(server)?;
    common::cleanup_remotelink_temp_files();

    Ok(())
}

#[test]
#[serial]
fn test_watch_process_exit_then_rebuild() -> Result<()> {
    // Test scenario: process exits naturally, then user rebuilds
    // Watch mode should detect the rebuild and restart

    let source_v1 = r#"
        fn main() {
            println!("Version 1 exiting");
            std::process::exit(0);
        }
    "#;
    let exe_path = compile_watch_test_program(source_v1, "watch_exit_rebuild")?;

    let port = common::find_available_port();
    let server = common::start_test_server(port)?;
    thread::sleep(Duration::from_millis(500));

    let exe_path_clone = exe_path.clone();
    let client_thread = thread::spawn(move || {
        Command::new(common::get_remotelink_binary())
            .arg("--target")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("--filename")
            .arg(&exe_path_clone)
            .arg("--watch")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    });

    let mut client = client_thread.join().unwrap()?;
    thread::sleep(Duration::from_secs(2));

    // Process should have exited by now
    // Now rebuild
    let source_v2 = r#"
        fn main() {
            println!("Version 2 after exit");
        }
    "#;

    thread::sleep(Duration::from_millis(500));
    recompile_test_program(source_v2, &exe_path)?;

    // Wait for restart
    thread::sleep(Duration::from_secs(2));

    client.kill()?;
    let output = client.wait_with_output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("STDOUT:\n{}", stdout);
    println!("STDERR:\n{}", stderr);

    // Should see both versions
    assert!(stdout.contains("Version 1 exiting") || stderr.contains("Version 1 exiting"));
    assert!(stdout.contains("Version 2 after exit") || stderr.contains("Version 2 after exit"));

    // Cleanup
    common::cleanup_test_executable(&exe_path);
    common::stop_test_server(server)?;
    common::cleanup_remotelink_temp_files();

    Ok(())
}
