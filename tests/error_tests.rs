mod common;

use anyhow::Result;
use std::thread;
use std::time::Duration;

#[test]
fn test_connection_timeout() -> Result<()> {
    // Try connecting to non-existent server
    let start = std::time::Instant::now();

    // Create a dummy executable to satisfy the client
    let source = r#"fn main() { println!("test"); }"#;
    let exe_path = common::compile_test_program(source, "timeout_test")?;

    let _result = std::process::Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--target")
        .arg("192.0.2.1:8888") // TEST-NET address, should timeout
        .arg("--connect-timeout-secs")
        .arg("2")
        .arg("--filename")
        .arg(&exe_path)
        .output();

    let elapsed = start.elapsed();

    // Should timeout in ~2 seconds, not hang forever
    assert!(elapsed < Duration::from_secs(5));

    common::cleanup_test_executable(&exe_path);

    Ok(())
}

#[test]
fn test_invalid_executable() -> Result<()> {
    // Try sending invalid data as executable
    let port = common::find_available_port();
    let server = common::start_test_server(port)?;
    thread::sleep(Duration::from_millis(500));

    // Create file with garbage data
    let temp_file = std::env::temp_dir().join("remotelink_test_invalid");
    std::fs::write(&temp_file, b"not an executable")?;

    let _output = std::process::Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--target")
        .arg(format!("127.0.0.1:{}", port))
        .arg("--filename")
        .arg(&temp_file)
        .output()?;

    // Should get error, not crash
    // The server should handle this gracefully

    std::fs::remove_file(&temp_file)?;
    common::stop_test_server(server)?;

    Ok(())
}

#[test]
fn test_nonexistent_file() -> Result<()> {
    let port = common::find_available_port();
    let server = common::start_test_server(port)?;
    thread::sleep(Duration::from_millis(500));

    // Try to send a file that doesn't exist
    let nonexistent = "/tmp/this_file_definitely_does_not_exist_12345";

    let output = std::process::Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--target")
        .arg(format!("127.0.0.1:{}", port))
        .arg("--filename")
        .arg(nonexistent)
        .output()?;

    // Should fail gracefully with an error message
    assert!(!output.status.success());

    common::stop_test_server(server)?;

    Ok(())
}

#[test]
fn test_server_disconnect_during_execution() -> Result<()> {
    let source = r#"
        fn main() {
            use std::thread;
            use std::time::Duration;

            println!("Starting...");
            thread::sleep(Duration::from_secs(5));
            println!("Finished!");
        }
    "#;
    let exe_path = common::compile_test_program(source, "disconnect_test")?;

    let port = common::find_available_port();
    let server = common::start_test_server(port)?;
    thread::sleep(Duration::from_millis(500));

    // Start client in background
    let mut client = std::process::Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--target")
        .arg(format!("127.0.0.1:{}", port))
        .arg("--filename")
        .arg(&exe_path)
        .spawn()?;

    // Let it start
    thread::sleep(Duration::from_secs(1));

    // Kill the server mid-execution
    common::stop_test_server(server)?;

    // Client should detect disconnect and exit
    // (may or may not be success depending on implementation)
    let _ = client.wait()?;

    common::cleanup_test_executable(&exe_path);
    common::cleanup_remotelink_temp_files();

    Ok(())
}

#[test]
fn test_invalid_port() -> Result<()> {
    let source = r#"fn main() { println!("test"); }"#;
    let exe_path = common::compile_test_program(source, "invalid_port")?;

    // Try connecting to port 0 (invalid)
    let output = std::process::Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--target")
        .arg("127.0.0.1:0")
        .arg("--filename")
        .arg(&exe_path)
        .output()?;

    // Should fail
    assert!(!output.status.success());

    common::cleanup_test_executable(&exe_path);

    Ok(())
}

#[test]
fn test_malformed_address() -> Result<()> {
    let source = r#"fn main() { println!("test"); }"#;
    let exe_path = common::compile_test_program(source, "malformed_addr")?;

    // Try connecting to malformed address
    let output = std::process::Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--target")
        .arg("not_an_address")
        .arg("--filename")
        .arg(&exe_path)
        .output()?;

    // Should fail gracefully
    assert!(!output.status.success());

    common::cleanup_test_executable(&exe_path);

    Ok(())
}

#[test]
fn test_empty_executable() -> Result<()> {
    let port = common::find_available_port();
    let server = common::start_test_server(port)?;
    thread::sleep(Duration::from_millis(500));

    // Create an empty file
    let temp_file = std::env::temp_dir().join("remotelink_test_empty");
    std::fs::write(&temp_file, b"")?;

    let _output = std::process::Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--target")
        .arg(format!("127.0.0.1:{}", port))
        .arg("--filename")
        .arg(&temp_file)
        .output()?;

    // Should handle gracefully (may succeed at transfer but fail at execution)

    std::fs::remove_file(&temp_file)?;
    common::stop_test_server(server)?;

    Ok(())
}
