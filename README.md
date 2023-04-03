# GammaDaemon
Auto adjusts screen gamma based on notebook battery life for Linux systems.

## Purpose
This project is one of many software that adjusts screen brightness depending on battery life. I wanted to try writing my own for my i3 setup on my ThinkPad.

## Compatibility
This should work with any Linux laptop.

### Devices Tested
- ThinkPad E15 Gen3 on ArchLinux: 2/2 working
    - AC detection: Working
    - Screen brightness change: working



## Installation

First, compile the project:
```
cargo build
```

## Contributing
Any contributions are welcome. Just make a pull request with the changes you want to add. If you tested this software on a device not listed in the Compatibility section, add the device to the *Devices Tested* list. 

## Todos
- Smooth transition between gamma settings
- Systemd service file
- Configuration file 
