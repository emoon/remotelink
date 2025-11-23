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

## Command Line Options

Use `remotelink --help` to see all available options:

- `--remote-runner` - Run as remote runner (the machine that executes code)
- `--target <IP>` - Connect to remote runner at this IP address
- `--filename <PATH>` - Executable file to run remotely
- `--watch` - Enable automatic restart on file changes
- `--port <PORT>` - TCP port to use (default: 8888)
- `--log-level <LEVEL>` - Set logging verbosity (error, warn, info, debug, trace)

