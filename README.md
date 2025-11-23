# Remotelink

Remotelink is a tool that allows you to run executables on another system over network and get the text written out piped back directly back.

## Disclaimer

Running `remotlink --host` on a machine is **very insecure** as it allows others to run any code on your target. Only use this tool if you know what you are doing.
This tool is currently also very much WIP.

## How to use

Remotelink is a tool that allows you to run executables on another system over network and get the text written out piped back directly back. Lets show an example

1. A machine (like a Raspberry Pi) starts remotelink with `remotelink --remote-runner`
2. Another machine (like a regular PC) produces a executable compatible and run it with `remotelink --target <ip of raspberry pi> --filename /path/to/executable`
3. The executable will be sent to the Raspberry Pi and start running, if any output (over stdout/stderr) is printed it will be sent back to the PC.
4. By pressing ctrl-c on the PC side the executable will be stopped on the Raspberry Pi side. Now the process can repeat

## Watch Mode (Automatic Restart)

For rapid development iteration, you can enable watch mode to automatically restart the remote executable when you rebuild:

```bash
# On the remote runner (e.g., Raspberry Pi)
remotelink --remote-runner

# On your development machine
remotelink --target <ip of raspberry pi> --filename /path/to/executable --watch
```

Now whenever you rebuild your executable, remotelink will:
1. Detect the file has changed
2. Verify the file is fully written (stability checks)
3. Automatically stop the running process
4. Send the new version and restart it

This enables seamless compile/test cycles without manual intervention. See [WATCH_MODE.md](WATCH_MODE.md) for complete documentation.

## Remote File Loading

Remotelink supports transparent remote file loading, allowing remote executables to access files from the host machine over the network without manual copying.

### How It Works

When you enable the file server with `--file-dir`, your remote executable can access files from the host by prefixing paths with `/host/`. For example:

```c
// This reads from <file-dir>/data/config.json on the host machine
FILE *f = fopen("/host/data/config.json", "r");
```

The feature uses LD_PRELOAD to intercept libc file operations (open, read, close, stat, etc.) and transparently proxy them over the network.

### Usage

**On the host machine (where files are located):**

```bash
# Start remotelink with file server enabled
remotelink --target <ip> --filename ./my_program --file-dir /path/to/test/data
```

This starts a file server on port 8889 serving files from `/path/to/test/data`.

**On the remote runner machine:**

```bash
# Normal remote runner - no changes needed
remotelink --remote-runner
```

**Build and deploy the LD_PRELOAD library:**

```bash
# Build the preload library
cargo build --release -p remotelink_preload

# Copy to remote machine
scp target/release/libremotelink_preload.so user@remote:/usr/local/lib/

# Or install locally for testing
sudo cp target/release/libremotelink_preload.so /usr/local/lib/
```

The runner automatically sets `REMOTELINK_FILE_SERVER` and `LD_PRELOAD` environment variables for spawned executables.

### Example Test Program

```c
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>

int main() {
    // Access files from host machine
    int fd = open("/host/test_file.txt", O_RDONLY);
    if (fd < 0) {
        perror("open");
        return 1;
    }

    char buffer[1024];
    ssize_t n = read(fd, buffer, sizeof(buffer));
    write(1, buffer, n);
    close(fd);

    return 0;
}
```

### Supported Operations

- `open()` / `open64()` - Open files for reading
- `read()` - Read file contents
- `close()` - Close files
- `stat()` / `stat64()` - Get file metadata
- `fstat()` / `fstat64()` - Get file metadata by descriptor
- `lseek()` / `lseek64()` - Seek within files

### Limitations

- Read-only access (write operations not supported)
- Maximum read size: 4MB per operation
- Maximum open files: 256 per process
- Operation timeout: 30 seconds
- Only files with `/host/` prefix are intercepted

### Security Considerations

- The file server only serves files within the specified `--file-dir` directory
- Path traversal attempts (e.g., `../`) are rejected
- No directory listing or file discovery (client must know exact paths)
- Connection is not encrypted (use VPN/SSH tunnel for untrusted networks)
- Use firewall rules to restrict access to port 8889

### Testing

A test program is included to verify functionality:

```bash
# Compile test program
gcc -o test_data/test_reader test_data/test_reader.c

# Run with file server enabled
remotelink --target 127.0.0.1 --filename test_data/test_reader --file-dir test_data
```

## Command Line Options

Use `remotelink --help` to see all available options:

- `--remote-runner` - Run as remote runner (the machine that executes code)
- `--target <IP>` - Connect to remote runner at this IP address
- `--filename <PATH>` - Executable file to run remotely
- `--watch` - Enable automatic restart on file changes
- `--file-dir <PATH>` - Enable file server and serve files from this directory
- `--port <PORT>` - TCP port to use (default: 8888)
- `--log-level <LEVEL>` - Set logging verbosity (error, warn, info, debug, trace)

