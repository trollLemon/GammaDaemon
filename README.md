# GammaDaemon
Auto adjusts screen gamma based on notebook battery life for Linux systems.

## Purpose
This project is one of many software that adjusts screen brightness depending on battery life. I wanted to try writing my own for my i3 setup on my ThinkPad.

## Requirements
This software uses the xrandr command to change screen brightness, so the XOrg Display server is required.
Currently, this software doesn't work with Wayland.

## Compatibility
Theoretically, this should work with any laptop with XOrg installed. 

### Devices Tested
- ThinkPad E15 Gen3 on ArchLinux: working



## Installation

First, compile the project:
```
cargo build
```

## Contributing
Any contributions are welcome. Just make a pull request with the changes you want to add. If you tested this software on a device not listed in the Compatibility section, add the device to the *Devices Tested* list.


## Todos
- Smooth transition between gamma settings
- Wayland Support
- Systemd service file
- Configuration file 
