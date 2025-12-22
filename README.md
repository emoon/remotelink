# Remotelink

Run executables on a remote system over the network with stdout/stderr piped back.

## Disclaimer

Running `remotelink --remote-runner` is **insecure** as it allows others to run arbitrary code on your machine. Only use on trusted networks.

## Quick Start

```bash
# On remote machine (e.g., Raspberry Pi)
remotelink --remote-runner

# On development machine
remotelink --target <remote-ip> --filename ./my_program
```

Press Ctrl-C to stop the remote executable.

## Watch Mode

Automatically restart when the executable is rebuilt:

```bash
remotelink --target <remote-ip> --filename ./my_program --watch
```

## Remote File Loading

Access files from the host machine transparently using the `/host/` prefix:

```bash
# Enable file server
remotelink --target <ip> --filename ./my_program --file-dir /path/to/data
```

```c
// In your program - reads from host machine
FILE* f = fopen("/host/config.json", "rb");
```

### Shared Library Loading

Remote shared libraries are also supported. Place libraries in `<file-dir>/libs/`:

```
my_project/
  libs/
    libcustom.so.1
  data/
    config.json
```

The runner automatically sets `LD_LIBRARY_PATH=/host/libs` so linked libraries are found.

For `dlopen()`, use the `/host/` prefix directly:

```c
void* handle = dlopen("/host/plugins/myplugin.so", RTLD_NOW);
```

### Limitations

- Read-only access only
- Files must use `/host/` prefix
- Requires `libremotelink_preload.so` on target (see Cross-Compilation)

## Command Line Options

| Option | Description |
|--------|-------------|
| `--remote-runner` | Run as the remote runner |
| `--target <IP>` | Connect to remote runner |
| `--filename <PATH>` | Executable to run |
| `--watch` | Auto-restart on file changes |
| `--file-dir <PATH>` | Enable file server from this directory |
| `--port <PORT>` | TCP port (default: 8888) |
| `--log-level <LEVEL>` | Logging: error, warn, info, debug, trace |

## Cross-Compilation (aarch64)

### Prerequisites

```bash
sudo apt install gcc-aarch64-linux-gnu
rustup target add aarch64-unknown-linux-gnu
```

### Build & Deploy

```bash
# Build for aarch64
./scripts/build-aarch64.sh

# Deploy to remote target
./scripts/deploy-aarch64.sh user@remote
```

This builds a static main binary and the required preload library, then copies both to the target.
