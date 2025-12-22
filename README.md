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

Access files from the host machine transparently:

```bash
# Enable file server
remotelink --target <ip> --filename ./my_program --file-dir /path/to/data
```

### How It Works

The preload library intercepts file operations (`open`, `fopen`, `stat`, `access`, `dlopen`, `opendir`) and uses a **local-first fallback** strategy:

1. **Normal paths** → Try local filesystem first. If file not found (ENOENT), try remote.
2. **`/host/` prefix** → Always fetch from remote (skips local).

```c
// Tries local first, falls back to remote if not found
FILE* f = fopen("data/config.json", "rb");

// Forces remote-only access
FILE* f = fopen("/host/data/config.json", "rb");
```

This allows the same binary to work both standalone (local files) and with remotelink (remote fallback).

### Shared Library Loading

Shared libraries (`.so` files) are automatically cached locally when loaded, as the dynamic linker requires `mmap()` access. The cache persists across runs and is validated via mtime/size comparison with the remote—stale files are re-downloaded automatically.

```c
// Tries local, falls back to remote (cached for mmap)
void* h = dlopen("libs/myplugin.so", RTLD_NOW);

// Forces remote
void* h = dlopen("/host/libs/myplugin.so", RTLD_NOW);
```

For implicitly linked libraries, the runner sets `LD_LIBRARY_PATH=.` so libraries in the file-dir root are found via fallback.

### Directory Listing

Directory operations are supported for remote directories:

```c
DIR* d = opendir("plugins");
while ((entry = readdir(d)) != NULL) {
    printf("%s\n", entry->d_name);
}
closedir(d);
```

### Limitations

- Read-only access
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
