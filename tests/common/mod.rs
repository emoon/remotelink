use anyhow::{Context, Result};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

/// Get the path to the pre-built remotelink binary
pub fn get_remotelink_binary() -> PathBuf {
    // Test executables are in target/debug/deps/, binary is in target/debug/
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            // Go from target/debug/deps/test_name-hash to target/debug
            p.parent()?.parent().map(|p| p.join("remotelink"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from("target/debug/remotelink"))
}

/// Compile a Rust test program and return path to executable
pub fn compile_test_program(source: &str, name: &str) -> Result<PathBuf> {
    let temp_dir = std::env::temp_dir();
    let source_file = temp_dir.join(format!("remotelink_test_{}.rs", name));
    let exe_file = temp_dir.join(format!("remotelink_test_{}", name));

    // Write source code
    std::fs::write(&source_file, source).context("Failed to write test source file")?;

    // Compile it
    let output = Command::new("rustc")
        .arg(&source_file)
        .arg("-o")
        .arg(&exe_file)
        .output()
        .context("Failed to run rustc")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Compilation failed:\n{}", stderr);
    }

    // Clean up source file
    let _ = std::fs::remove_file(&source_file);

    // Ensure the binary is fully written and accessible
    // This is necessary because filesystem operations may not be immediately visible
    for _ in 0..50 {
        if let Ok(metadata) = std::fs::metadata(&exe_file) {
            if metadata.len() > 0 {
                // File exists and has content, give it one more moment to stabilize
                thread::sleep(Duration::from_millis(10));
                return Ok(exe_file);
            }
        }
        thread::sleep(Duration::from_millis(10));
    }

    Ok(exe_file)
}

/// Start a test server on a random available port
pub fn start_test_server(port: u16) -> Result<Child> {
    let mut server = Command::new(get_remotelink_binary())
        .arg("--remote-runner")
        .arg("--port")
        .arg(port.to_string())
        .arg("--bind-address")
        .arg("127.0.0.1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to start test server")?;

    // Wait for server to be ready
    for attempt in 0..100 {
        thread::sleep(Duration::from_millis(100));
        if TcpStream::connect(format!("127.0.0.1:{}", port)).is_ok() {
            // Server is accepting connections, give it a bit more time to fully initialize
            thread::sleep(Duration::from_millis(500));
            return Ok(server);
        }
        if attempt == 50 {
            // Check if server process is still alive
            if let Ok(Some(_)) = server.try_wait() {
                anyhow::bail!("Server process exited prematurely");
            }
        }
    }

    anyhow::bail!("Server failed to start within 10 seconds");
}

/// Stop a test server
pub fn stop_test_server(mut server: Child) -> Result<()> {
    server.kill().context("Failed to kill server")?;
    server.wait().context("Failed to wait for server")?;
    Ok(())
}

/// Find an available port for testing
pub fn find_available_port() -> u16 {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Clean up test executables
pub fn cleanup_test_executable(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
}

/// Clean up remotelink temp files
pub fn cleanup_remotelink_temp_files() {
    let temp_dir = std::env::temp_dir();
    if let Ok(entries) = std::fs::read_dir(&temp_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with("remotelink-") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

/// Run the client to execute a program on the test server
#[allow(dead_code)]
pub fn run_client(port: u16, exe_path: &std::path::Path) -> Result<std::process::Output> {
    use std::process::Command;

    Command::new(get_remotelink_binary())
        .arg("--target")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--filename")
        .arg(exe_path)
        .output()
        .context("Failed to run client")
}
