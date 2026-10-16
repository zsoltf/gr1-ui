# Private device build and recovery

For installation, use the [USB update instructions](../README.md#install).
This guide covers source builds and the earlier network helpers.

The source targets ARM32 Linux on the existing GR-1 OS, using the original
3.4.4 native engine. One Raspberry Pi 3 Model B Plus Rev 1.3 GR-1 was used for
development trials. Reboot startup, stock recovery and the final screen, sound
and controls were accepted on that unit. Power-off cold boot, broader sound
preservation and complete recording, Files and save/recall workflows remain
unobserved. Those results do not establish compatibility with other units.
GitHub Actions distributes a UI-only download. The website combines it with
your official firmware file locally in the browser.

Never execute the vendor engine on macOS, generic Linux, a generic Pi or an
emulator. Its audio failure path can reboot the host. The build and inspection
tools read vendor inputs as data; they do not load the engine.

## Build locally

These optional developer commands require the source tree, rather than the
UI-only download. Install Python 3.11+, Rust 1.90.0, curl and Docker.
From the source root:

```sh
python3 tools/bootstrap.py
./tools/device/build.sh
```

The build obtains pinned official firmware and a system image, prepares an
isolated sysroot, generates metadata, cross-builds SDL2/Rust and checks ELF
dependencies and native bindings statically. The original files and generated
tables stay in ignored local directories. All output under `device-build/`,
especially `package/` and the tarball, is private: it includes proprietary
engine and image-derived libraries. Do not redistribute it with this source.
Loader material is derived in memory from the user's firmware archive.

## Earlier network helpers

These helpers target the earlier stock 2.8.3 installation. They are not the
USB update procedure for firmware 3.4.4. See [network recovery](owner-install.md).

### Inventory and temporary trial

Before running, inventory the exact unit, original startup and data; back up the
card and user files. Preserve the original launcher and service. The helpers
retain an exact stock service hash and hardware/stock executable guards; they
do not automatically support other units. A guard mismatch requires independent
inspection and an explicit local adaptation, not bypassing checks blindly.

Verify package `SHA256SUMS`, stop the identified stock host and verify all engine
children have exited before any new host starts. Never run competing engines.
On the inventoried unit only, stage a private package in
`/media/internal/gr1-ui-repair/<source revision>/` and run as root:

```sh
./temporary-trial.sh /media/internal/gr1-ui-repair-data/trial --copy-stock
```

The trial stops stock, copies stock data to a new isolated root, performs the
preflight and restores stock on exit. Omit `--copy-stock` to resume saved trial
data. Copying needs sufficient disk space. Stock and converted data layouts are
detected in place; host settings stay separate from performances.

To stop, independently identify the exact package engine PID and check its
`/proc/<PID>/exe`, then send TERM to that engine. Wrapper EXIT cleanup waits for
the child and restores stock. Independently check the stock process, ALSA
RUNNING, screen and sound. Engine TERM/recovery was observed in trials; wrapper
signals through sudo/SSH remain unverified and an earlier wrapper TERM left its
child running. Recovery observes stock readiness for up to 30 seconds.

### Guarded boot preparation

Verify the target unit, package and recovery procedure before installation.
Stage under
`/media/internal/gr1-ui-installed/<full source commit>/`, verify hashes, then
use `install-boot.sh` after a successful trial. It preserves original startup,
takes a recovery archive and writes one marked systemd override. `ExecStartPre`
runs `prepare-host.sh`: optional ALSA preallocation 4096, four performance CPU
governors and UART low latency. `run-device.sh` sets the original locale.
Partition expansion, reboot and the original GUI launch are excluded.

`recover-stock.sh` removes only the recognized override and starts stock after
identity checks. Data and package remain. A shell failure can restore stock;
an engine-triggered reboot cannot execute cleanup. Independently observe cold
boot, screen, controls, audio and stock recovery on the target before accepting
installation. Neither install nor recovery script performs a reboot.
