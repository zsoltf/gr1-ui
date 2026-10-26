# GR-1 UI 0.1.1

Clearer feedback from the physical LFO and grain-window controls.

- Pressing DEST immediately shows the newly selected target for either LFO,
  including when Amount is zero. The target remains visible below the LFO.
- Turning Amount shows the selected target and signed amount in the large
  amber readout. Other active modulation destinations remain active.
- LFO Frequency and window Sides, Tilt and Curve now show their large readouts.
- Changing either LFO's shape shows its name and a larger amber waveform icon
  for 1.5 seconds, including in menus.
- Removed the redundant "Dest" prefix from the small LFO target labels.

The updated controls were tested on a GR-1 running official firmware 3.4.4;
both LFO shape buttons were confirmed on the instrument. The sound engine,
saved sounds and firmware target are unchanged.

## 0.1.0

The first public release of the redesigned GR-1 interface, retaining the original
Tasty Chips sound engine and physical controls.

- Dedicated Sample, Patch and Performance pages with waveform and grain feedback.
- Responsive Sample, Patch and Performance loading menus.
- Light and Dark appearance; fresh settings start in Dark at 50% brightness.
- Existing appearance preferences survive the firmware 3.4.4 data-layout change.
- Browser preparation for installation through the normal USB firmware updater.

Install from the [website](https://zsoltf.github.io/gr1-ui/), following the
[short guide](README.md#install). You supply the official firmware file locally;
this project distributes no proprietary firmware or sound engine.

## Tested scope

USB installation and redesigned startup were observed on a Pi 3 Model B Plus
GR-1 running official firmware 3.4.4. Sound, screen and physical controls have
been checked in use. The control-feedback update passed desktop and ARM
builds, renderer and panel-event checks, and a native preview before the
instrument trial. Software tests substitute readback state for physical input.

USB recovery-file startup, a full power-off boot and exhaustive recording,
backup and save/recall workflows remain unverified on hardware. Testing covers
one instrument, not every GR-1 hardware revision. Back up your sounds first.
