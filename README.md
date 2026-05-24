# GammaDaemon
Auto adjusts screen backlight brightness based on notebook battery life for Linux systems.

GammaDaemon runs as a background service that watches your battery state (charging,
discharging, full, low, etc.) and sets the screen backlight to a brightness level you
configure for each state. A companion CLI (`gd`) lets you query and control the running
daemon over a local Unix socket.

## Compatibility
GammaDaemon *should* work with any Linux laptop with a `/sys/class/backlight` device
and a battery exposed through UPower/sysfs.

### Devices Tested
- ThinkPad E15 Gen3 with ArchLinux:
    - AC detection: Working
    - Screen brightness change: Working


## Backlight Permissions
GammaDaemon writes to `/sys/class/backlight/<device>/brightness`. To run the daemon
without root, create a udev rule that grants a group write access to the brightness
file. For example, for users in the `video` group:

```bash
ACTION=="add", SUBSYSTEM=="backlight", RUN+="/bin/chgrp video /sys/class/backlight/%k/brightness"
ACTION=="add", SUBSYSTEM=="backlight", RUN+="/bin/chmod g+w /sys/class/backlight/%k/brightness"
```

Otherwise, run the daemon as root.

## Running the Daemon

```bash
gamma-daemon [CONFIG_PATH]
```

If no config path is given, the daemon reads `/etc/gamma_daemon/config.toml`. If that
file cannot be read or parsed, the daemon falls back to a built-in default configuration.

On startup the daemon:
- Creates the runtime directory `/var/run/gamma_daemon` (mode `0700`).
- Binds a Unix socket at `/var/run/gamma_daemon/gamma_daemon.sock` (mode `0600`) for the
  CLI to connect to.
- Polls the battery on the configured interval and adjusts the backlight when the
  battery state changes.

## Running as a systemd service

A unit file is provided at `dist/gamma-daemon.service`. It expects the daemon
binary at `/usr/bin/gamma-daemon` — install it there using the "Without Cargo"
steps above (edit the unit's `ExecStart=` if you put it elsewhere).

Install and enable the service:

```bash
sudo cp dist/gamma-daemon.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now gamma-daemon.service
```

Check its status and follow its logs:

```bash
systemctl status gamma-daemon
journalctl -u gamma-daemon -f
```

The unit runs the daemon as `root` so it can create its runtime directory under
`/run` and write to the backlight device. Because the control socket is created
with `0600` permissions, the `gd` CLI must run as the same user — i.e. `sudo gd`.

To run without root instead, set up the udev rule from
[Backlight Permissions](#backlight-permissions) and change the unit to a
dedicated user with `User=`/`Group=` (the group must have backlight write
access). systemd's `RuntimeDirectory=` will create `/run/gamma_daemon` owned by
that user.

## CLI Usage
The `gd` CLI communicates with a running daemon over its Unix socket.

```bash
gd set <VALUE>   # Set the backlight to a gamma fraction in [0.0, 1.0]
gd enable        # Enable automatic battery-based adjustment
gd disable       # Disable automatic adjustment (leaves brightness as-is)
gd status        # Show whether the daemon is enabled and its current gamma state
```

`gd set` takes a fraction of the device's maximum brightness — `0.0` is minimum,
`1.0` is maximum.

## Configuration

GammaDaemon is configured with a TOML file. All `gamma_*` values are fractions of the
display's maximum brightness, in the range `[0.0, 1.0]`. `gamma_low_percentage` is a
battery state-of-charge fraction, also in `[0.0, 1.0]`.

```toml
[backlight_config]
poll_interval = 5            # how often to check battery status, in seconds
gamma_full = 1.0             # brightness when the battery is full
gamma_charging = 0.8         # brightness when charging but not full
gamma_discharging = 0.6      # brightness when discharging (above the low threshold)
gamma_plugged = 0.9          # brightness when plugged in but not charging
gamma_unknown = 0.7          # brightness when the battery state is unknown
gamma_low = 0.4              # brightness when discharging and below the low threshold
gamma_low_percentage = 0.2   # state-of-charge fraction below which gamma_low applies
```

The values above are also the built-in defaults used when no config file is found.

## Contributing
Any contributions and testing are welcome. Just make a pull request with the changes you want to add.

If you tested this software on a device not listed in the Compatibility section, add the device to the *Devices Tested* list.
</content>
</invoke>
