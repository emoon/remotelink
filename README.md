# Remotelink

Remotelink is a tool that allows you to run executables on another system over network and get the text written out piped back directly back.

## Disclaimer

Running `remotlink --host` on a machine is **very insecure** as it allows others to run any code on your target. Only use this tool if you know what you are doing.
This tool is currently also very much WIP.

## How to use

Remotelink is a tool that allows you to run executables on another system over network and get the text written out piped back directly back. Lets show an example

1. A machine (like a Raspberry Pi) starts remotelink with `remotelink --host`
2. Another machine (like a regular PC) produces a executable compatible and run it with `remotelink --target <ip of raspberry pi> /path/to/executable`
3. The executable will be sent to the Raspberry Pi and start running, if any output (over stdout) is printed it will be sent back to the PC.
4. By pressing ctrl-c on the PC side the executable will be stopped on the Raspberry Pi side. Now the process can repeat

