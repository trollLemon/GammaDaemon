# GammaDaemon
Auto adjusts screen gamma based on notebook battery life for Linux systems.

## Compatibility
GammaDaemon *should* work with any Linux laptop.

### Devices Tested
- ThinkPad E15 Gen3 with ArchLinux: 
    - AC detection: Working
    - Screen brightness change: Working



## Installation

### Cargo
Run either of the following:
```bash
cargo install gamma_daemon
```

```bash
cargo install --path ./
```
If you do the above method, it has to be in the root folder.

### Without Cargo (Custom install location)

First, run
```bash
cargo build --release
```

Then copy the binary to where you want to put it; For example,
in the /usr/bin/:

```bash
$ cp target/release/gamma_daemon /usr/bin/
```

## Udev Rules
To run GammaDaemon without running as root, create a udev rule that will allow users in a certain group to read and write 
to */sys/class/backlight/(backlight)/brightness*. For example, udev rules for users in the video group listed in the documentation for bulbb:
```bash
ACTION=="add", SUBSYSTEM=="backlight", RUN+="/bin/chgrp video /sys/class/backlight/%k/brightness"
ACTION=="add", SUBSYSTEM=="backlight", RUN+="/bin/chmod g+w /sys/class/backlight/%k/brightness"
```
For more information, see the [bulbb documentation](https://docs.rs/bulbb/latest/bulbb/monitor/struct.MonitorDevice.html#method.set_brightness).

## Configuration
GammaDaemon will look in $USER/.config/GammaDaemon/conf.toml for gamma configurations. If GammaDaemon cannot find this file, it will use a default configuration.
Here is an example config:
```toml
full = 240
low = 100
low_perc = 25 # out of 100
charging = 255
discharging = 134
unknown = 255
ac_in = 255
```
## Contributing
Any contributions and testing are welcome. Just make a pull request with the changes you want to add. 

If you tested this software on a device not listed in the Compatibility section, add the device to the *Devices Tested* list.
