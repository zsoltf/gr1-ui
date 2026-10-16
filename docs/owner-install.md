# Earlier network installation and recovery

This is the earlier stock 2.8.3 network procedure. The new owner path is
[USB preparation in the browser](../web/index.html); the commands below do not
apply to the USB package or to stock 3.4.4.

The download is built by GitHub Actions. Local preparation needs Python 3.11+
and network access, not Rust or Docker. It adds the pinned 3.4.4 engine privately;
it does not upgrade the stock firmware. Installation has been tried on one GR-1
with Raspberry Pi 3 Model B Plus Rev 1.3, stock 2.8.3 and original startup.
Matching guards are a prerequisite, not proof another unit was tested.
Commands run from the extracted `gr1-ui-download` directory on your computer.

### 1. Save, back up and connect

Save any current sound edits in the stock UI. Copy your performances, samples
and other user files from internal storage to your computer before proceeding.
The [manufacturer's manual](https://www.tastychips.nl/downloads/GR-1/UserManual/User-manual-GR-1-v3.4.c.pdf)
explains LAN/SFTP access in its Network file sharing chapter. Keep your backup
off the instrument; the installer's later on-device backup is not a full SD-card
image or a substitute for this copy.

Connect the GR-1 and computer to the same local network. In a terminal on your
computer, set the target and check login:

```sh
export GR1_TARGET=sysop@gr-1.local
ssh "$GR1_TARGET"
```

Use your GR-1 account password. If you changed the account or the host name,
change `GR1_TARGET`; if `.local` does not resolve, use the instrument's IP address.
The manual documents factory credentials. Check the host identity before
accepting SSH's first-connection prompt. Type `exit` to return to your computer.

### 2. Check compatibility before installing anything

Run the read-only inventory on the instrument, saving its report locally.
Run each command separately and stop on any error:

```sh
ssh "$GR1_TARGET" 'sh -s' < inspect-device.sh > gr1-inventory.txt
ssh "$GR1_TARGET" 'sha256sum /home/sysop/GR-1/grui/grmain.exe /etc/systemd/system/rungr.service && systemctl show rungr.service -p DropInPaths'
ssh "$GR1_TARGET" 'ls -d /media/internal/performance?? /media/internal/factory && test -f /media/internal/performance01/performance.grm && test ! -d /media/internal/user/perf'
```

Open `gr1-inventory.txt`. The current installer requires:

| Check | Required setup |
|---|---|
| Hardware | Raspberry Pi 3 Model B Plus Rev 1.3, `armv7l`, 32-bit ARM Linux |
| Audio/panel | GR-1 audio card and `/dev/ttyAMA0` present |
| Stock program SHA-256 | `4b6ac24bff75f6b3e9d44a72965f1726e7936225d824572dba4f04a7ff5bf299` |
| Stock service SHA-256 | `96350a1360b3df90d8c19318b1798bda954d675405ec347a673774f9712ee12f` |
| Startup overrides | Empty `DropInPaths=` |
| Data | `performance01` through `performance50` and `factory` directly under `/media/internal`, with `performance01/performance.grm`, without `/media/internal/user/perf` |

Stop here if a check differs or cannot be read. Do not change hashes or remove
checks to force installation. A stock firmware upgrade can change these values;
upgrading to 3.4.4 is not a way to satisfy the current installer.
Check the inventory's free-space report: the trial makes another copy of the
50 performance folders and Factory, and installation creates a compressed
recovery backup. Leave additional space for future saves.

### 3. Prepare the downloaded UI privately

Download `gr1-ui-armhf-download` from a successful Actions run, unzip the
GitHub artifact, then extract `gr1-ui-download.tar.gz` and enter that folder.
Run:

```sh
python3 tools/prepare-download.py --bundle ../gr1-ui-download.tar.gz
```

The helper checks the download entries/checksums, obtains hash-pinned official
firmware, installs its Python decoding dependencies automatically and decodes
the engine as data. It does not execute it or contact the instrument. You can
supply an already obtained archive with `--firmware /path/to/archive.tgz`.
The result is `device-build/gr1-ui-3.4.4-armhf.tar.gz`; keep it private. Wait for
successful exit. It refuses to overwrite an existing package. The download
checksums establish consistency, not trust in an unknown workflow or fork;
choose a run from the source revision you intend to use.

The website now prepares a USB update archive. For this earlier network
procedure, use the terminal preparation command above, which still produces
the separate package expected by these commands.

### 4. Copy and verify the package

Run these commands in the same computer terminal. They read the full source
revision from the package manifest and refuse an existing destination:

```sh
GR1_REVISION=$(python3 -c 'import json,re; v=json.load(open("device-build/package/manifest.json"))["source_base"]; assert re.fullmatch("[0-9a-f]{40}", v); print(v)') &&
GR1_PACKAGE="/media/internal/gr1-ui-installed/$GR1_REVISION" &&
scp device-build/gr1-ui-3.4.4-armhf.tar.gz "$GR1_TARGET:gr1-ui-upload-$GR1_REVISION.tar.gz" &&
ssh -t "$GR1_TARGET" "sudo sh -c 'test ! -e $GR1_PACKAGE && mkdir -p $GR1_PACKAGE && tar -xzf /home/sysop/gr1-ui-upload-$GR1_REVISION.tar.gz -C $GR1_PACKAGE --strip-components=1 && cd $GR1_PACKAGE && sha256sum -c SHA256SUMS'" &&
printf 'Keep this package path for recovery: %s\n' "$GR1_PACKAGE"
```

The upload path above assumes the `sysop` account. Adapt `/home/sysop/` if your
login account differs. Every checksum must say `OK`; stop on any error.
The directory already existing is an error, not an upgrade procedure.
This step only copies files; stock continues running and startup is unchanged.
`sudo` asks for the GR-1 account password because the scripts control the
instrument's service, audio setup and startup files.

### Troubleshooting before the trial

- SSH login fails: check `GR1_TARGET`, the network connection and your account
  password. A hostname problem can be resolved by using the instrument's IP.
- Python dependency setup fails: confirm Python 3.11+ and network access; do not
  proceed until preparation succeeds.
- Download/hash check fails: stop; do not disable hash verification. Use the
  pinned official inputs described in the build guide.
- A checksum, compatibility guard or existing-directory check fails: do not
  continue to trial/install or overwrite an installed package.

### 5. Try the UI with a separate copy of your sounds

Keep a second SSH terminal open for ending the trial. From the computer:

```sh
ssh -t "$GR1_TARGET" "sudo '$GR1_PACKAGE/temporary-trial.sh' '/media/internal/gr1-ui-repair-data/$GR1_REVISION' --copy-stock"
```

Stock stops temporarily and the new UI uses an isolated copy of the stock data.
Try playing, turning the physical controls, loading and saving a patch, and the
Patch, Sample and Perf pages. Trial saves stay in that copy; installation later
uses your original internal data. No startup override is written during the trial.
If the trial data path already exists, omit `--copy-stock` to resume it.

To end the trial, use the second SSH terminal on the GR-1:

```sh
pgrep -x gr1-ui
```

Note its single PID, then substitute that number for `1234` below:

```sh
sudo readlink /proc/1234/exe
```

The result must be the printed package path followed by `/gr1-ui`. If there is
no PID, more than one PID, or a different path, stop and inspect the processes.
Once the exact process is identified:

```sh
sudo kill -TERM 1234
```

Wait for the first terminal to report stock readiness and `Trial ended`, then
check the original screen and sound physically. Do not rely on closing SSH or
Ctrl-C through sudo to stop the engine; that signal path has failed in a trial.
If stock does not return, do not proceed to permanent installation.

### 6. Make it start at boot

Proceed only after the trial and stock return worked on your instrument.
This command is your choice to install this exact package as the default:

```sh
ssh -t "$GR1_TARGET" "sudo '$GR1_PACKAGE/install-boot.sh'"
```

The installer preserves the stock application and service, creates a timestamped
backup in `/media/internal/gr1-ui-recovery/` and writes one managed systemd
override. It starts the new UI using your original `/media/internal` data and
checks the running executable and audio. Read its output and check screen,
sound and controls before rebooting. Then:

```sh
ssh -t "$GR1_TARGET" 'sudo reboot'
```

SSH disconnecting during reboot is expected. Reconnect after startup and verify
the new screen, sound and physical controls again. Copy the printed recovery
backup off the instrument using SFTP; keep the package path from step 4.
Reboot startup was checked on the development unit; power-off cold boot and
broader recording/Files/full save-recall remain unverified.

### Return to the original UI

From the same computer terminal, while SSH is available:

```sh
ssh -t "$GR1_TARGET" "sudo '$GR1_PACKAGE/recover-stock.sh'"
```

After reopening a terminal, set `GR1_TARGET` again and set `GR1_PACKAGE` to the
exact path you saved in step 4. Recovery stops the replacement, removes only
its marked startup override and starts stock. It keeps the package and user
data. Verify stock screen and sound, then reboot to check stock startup.
This is service recovery, not a full-card restore. If SSH is unavailable, use
your off-device backup and the manufacturer's recovery instructions; do not
flash a fresh image over your only copy of user data.


For the optional source build and static checks, see [device build](device-build.md).
