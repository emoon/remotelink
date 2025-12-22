mod common;

use anyhow::Result;
use std::thread;
use std::time::Duration;

#[test]
#[ignore] // Run with: cargo test --ignored
fn test_rapid_connections() -> Result<()> {
    let port = common::find_available_port();
    let server = common::start_test_server(port)?;

    // Compile simple test program
    let source = r#"fn main() { println!("OK"); }"#;
    let exe_path = common::compile_test_program(source, "rapid")?;

    // Run 100 times
    for i in 0..100 {
        if i % 10 == 0 {
            println!("Iteration {}/100", i);
        }

        let _ = std::process::Command::new("cargo")
            .arg("run")
            .arg("--")
            .arg("--target")
            .arg(format!("127.0.0.1:{}", port))
            .arg("--filename")
            .arg(&exe_path)
            .output();
    }

    // Verify no temp files left behind
    thread::sleep(Duration::from_secs(2));
    let temp_count = count_remotelink_temp_files();
    assert_eq!(temp_count, 0, "Temp files leaked: {}", temp_count);

    common::cleanup_test_executable(&exe_path);
    common::stop_test_server(server)?;

    Ok(())
}

#[test]
#[ignore]
fn test_high_volume_output() -> Result<()> {
    let source = r#"
        fn main() {
            for i in 0..100000 {
                println!("Line {}", i);
            }
        }
    "#;
    let exe_path = common::compile_test_program(source, "high_volume")?;

    let port = common::find_available_port();
    let server = common::start_test_server(port)?;
    thread::sleep(Duration::from_millis(500));

    let start = std::time::Instant::now();

    let output = std::process::Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--target")
        .arg(format!("127.0.0.1:{}", port))
        .arg("--filename")
        .arg(&exe_path)
        .output()?;

    let elapsed = start.elapsed();
    println!("High volume test took: {:?}", elapsed);

    // Verify all lines received
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line_count = stdout.lines().count();
    assert!(
        line_count >= 99000,
        "Not all lines received: {}",
        line_count
    );

    common::cleanup_test_executable(&exe_path);
    common::stop_test_server(server)?;
    common::cleanup_remotelink_temp_files();

    Ok(())
}

#[test]
#[ignore]
fn test_large_executable() -> Result<()> {
    // Create a large executable by embedding data
    let source = r#"
        fn main() {
            // Embed some large static data
            const DATA: &[u8] = &[0u8; 10_000_000]; // 10MB of data
            println!("Data size: {}", DATA.len());
        }
    "#;
    let exe_path = common::compile_test_program(source, "large_exe")?;

    let port = common::find_available_port();
    let server = common::start_test_server(port)?;
    thread::sleep(Duration::from_millis(500));

    let start = std::time::Instant::now();

    let output = std::process::Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--target")
        .arg(format!("127.0.0.1:{}", port))
        .arg("--filename")
        .arg(&exe_path)
        .output()?;

    let elapsed = start.elapsed();
    println!("Large executable transfer took: {:?}", elapsed);

    // Verify it ran successfully
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Data size"));

    common::cleanup_test_executable(&exe_path);
    common::stop_test_server(server)?;
    common::cleanup_remotelink_temp_files();

    Ok(())
}

#[test]
#[ignore]
fn test_long_running_process() -> Result<()> {
    let source = r#"
        fn main() {
            use std::thread;
            use std::time::Duration;

            for i in 0..10 {
                println!("Progress: {}%", i * 10);
                thread::sleep(Duration::from_secs(1));
            }
            println!("Complete!");
        }
    "#;
    let exe_path = common::compile_test_program(source, "long_running")?;

    let port = common::find_available_port();
    let server = common::start_test_server(port)?;
    thread::sleep(Duration::from_millis(500));

    let start = std::time::Instant::now();

    let output = std::process::Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--target")
        .arg(format!("127.0.0.1:{}", port))
        .arg("--filename")
        .arg(&exe_path)
        .output()?;

    let elapsed = start.elapsed();
    println!("Long running test took: {:?}", elapsed);

    // Should have taken around 10 seconds
    assert!(elapsed.as_secs() >= 9 && elapsed.as_secs() <= 15);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Complete!"));

    common::cleanup_test_executable(&exe_path);
    common::stop_test_server(server)?;
    common::cleanup_remotelink_temp_files();

    Ok(())
}

#[test]
#[ignore]
fn test_concurrent_connections() -> Result<()> {
    let port = common::find_available_port();
    let server = common::start_test_server(port)?;

    let source = r#"fn main() { println!("OK"); }"#;
    let exe_path = common::compile_test_program(source, "concurrent")?;

    // Spawn 5 concurrent clients
    let mut handles = vec![];
    for i in 0..5 {
        let exe_path_clone = exe_path.clone();
        let port_clone = port;

        let handle = thread::spawn(move || {
            for j in 0..10 {
                println!("Thread {} iteration {}", i, j);
                let _ = std::process::Command::new("cargo")
                    .arg("run")
                    .arg("--")
                    .arg("--target")
                    .arg(format!("127.0.0.1:{}", port_clone))
                    .arg("--filename")
                    .arg(&exe_path_clone)
                    .output();
            }
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Verify cleanup
    thread::sleep(Duration::from_secs(2));
    let temp_count = count_remotelink_temp_files();
    assert_eq!(temp_count, 0, "Temp files leaked: {}", temp_count);

    common::cleanup_test_executable(&exe_path);
    common::stop_test_server(server)?;

    Ok(())
}

fn count_remotelink_temp_files() -> usize {
    let temp_dir = std::env::temp_dir();
    std::fs::read_dir(&temp_dir)
        .ok()
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .map(|n| n.starts_with("remotelink-"))
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
}
