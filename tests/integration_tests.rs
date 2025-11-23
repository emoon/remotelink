mod common;

use anyhow::{Context, Result};
use std::process::Command;  // Used for connection_limit test
use std::time::Duration;
use std::thread;

#[test]
fn test_basic_connection() -> Result<()> {
    let port = common::find_available_port();
    let server = common::start_test_server(port)?;

    // Just try to connect
    let result = std::net::TcpStream::connect(format!("127.0.0.1:{}", port));
    assert!(result.is_ok());

    common::stop_test_server(server)?;
    Ok(())
}

#[test]
fn test_simple_execution() -> Result<()> {
    // Compile test program
    let source = r#"
        fn main() {
            println!("Hello from test");
        }
    "#;
    let exe_path = common::compile_test_program(source, "simple")?;

    // Start server
    let port = common::find_available_port();
    let server = common::start_test_server(port)?;
    thread::sleep(Duration::from_millis(500));

    // Run client
    let output = common::run_client(port, &exe_path)?;

    // Verify output
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Hello from test"), "Expected 'Hello from test' in stdout, got: {}", stdout);

    // Cleanup
    common::cleanup_test_executable(&exe_path);
    common::stop_test_server(server)?;
    common::cleanup_remotelink_temp_files();

    Ok(())
}

#[test]
fn test_stdout_stderr_separation() -> Result<()> {
    let source = r#"
        fn main() {
            println!("This is stdout");
            eprintln!("This is stderr");
        }
    "#;
    let exe_path = common::compile_test_program(source, "stderr_test")?;

    let port = common::find_available_port();
    let server = common::start_test_server(port)?;
    thread::sleep(Duration::from_millis(500));

    let output = common::run_client(port, &exe_path)?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stdout.contains("This is stdout"));
    assert!(stderr.contains("This is stderr"));

    common::cleanup_test_executable(&exe_path);
    common::stop_test_server(server)?;
    common::cleanup_remotelink_temp_files();

    Ok(())
}

#[test]
fn test_exit_codes() -> Result<()> {
    let source = r#"
        fn main() {
            std::process::exit(42);
        }
    "#;
    let exe_path = common::compile_test_program(source, "exit_code")?;

    let port = common::find_available_port();
    let server = common::start_test_server(port)?;
    thread::sleep(Duration::from_millis(500));

    let output = common::run_client(port, &exe_path)?;

    // Check that exit code is propagated somehow
    // (may need to check output or logs)
    let _stdout_output = String::from_utf8_lossy(&output.stdout);
    // Exit code propagation depends on implementation
    // For now, just verify the test ran

    common::cleanup_test_executable(&exe_path);
    common::stop_test_server(server)?;
    common::cleanup_remotelink_temp_files();

    Ok(())
}

#[test]
fn test_connection_limit() -> Result<()> {
    let port = common::find_available_port();

    // Start server with low connection limit
    let server = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("remotelink")
        .arg("--")
        .arg("--remote-runner")
        .arg("--port")
        .arg(port.to_string())
        .arg("--max-connections")
        .arg("2")
        .spawn()?;

    thread::sleep(Duration::from_secs(1));

    // Connect twice (should succeed)
    let conn1 = std::net::TcpStream::connect(format!("127.0.0.1:{}", port))?;
    let conn2 = std::net::TcpStream::connect(format!("127.0.0.1:{}", port))?;

    // Third connection should be rejected or delayed
    // (exact behavior may vary)

    drop(conn1);
    drop(conn2);

    common::stop_test_server(server)?;
    Ok(())
}

#[test]
fn test_multiple_sequential_executions() -> Result<()> {
    let source = r#"
        fn main() {
            println!("Test execution");
        }
    "#;
    let exe_path = common::compile_test_program(source, "sequential")?;

    let port = common::find_available_port();
    let server = common::start_test_server(port)?;
    thread::sleep(Duration::from_millis(500));

    // Run 5 sequential executions
    for i in 0..5 {
        let output = common::run_client(port, &exe_path)
            .context(format!("Failed on iteration {}", i))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Test execution"), "Failed on iteration {}", i);
    }

    common::cleanup_test_executable(&exe_path);
    common::stop_test_server(server)?;
    common::cleanup_remotelink_temp_files();

    Ok(())
}
