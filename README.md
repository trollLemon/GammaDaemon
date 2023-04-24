# GammaDaemon
Auto adjusts screen gamma based on notebook battery life for Linux systems.

## Compatibility
GammaDaemon *should* work with any Linux laptop.

### Devices Tested
- ThinkPad E15 Gen3 on ArchLinux: 
    - AC detection: Working
    - Screen brightness change: Working



## Installation

### Cargo
Run either of the following:
```
cargo install gamma_daemon
```

```
cargo install --path ./
```
If you do the above method, it has to be in the root folder.

### Without Cargo (Custom install location)

First, run
```
cargo build --release
```

Then copy the binary to where you want to put it; For example,
in the /usr/bin/:

```
sudo cp target/release/gamma_daemon /usr/bin/
```

## Configuration
GammaDaemon will look in $USER/.config/GammaDaemon/conf.toml for gamma configurations. If it cannot find this file, it will use a default configuration.
Here is an example config:
```
full = 255
charging = 255
discharging = 155
unknown = 100
ac_in = 255
```
## Contributing
Any contributions and testing are welcome. Just make a pull request with the changes you want to add. 

If you tested this software on a device not listed in the Compatibility section, add the device to the *Devices Tested* list.
