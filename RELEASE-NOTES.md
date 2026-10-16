# GR-1 UI 0.1.0

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
been checked in use. The appearance fix passed desktop and ARM builds, the
122-test Rust suite and a native preview; that rebuilt binary has not been
installed on the instrument.

USB recovery-file startup, a full power-off boot and exhaustive recording,
backup and save/recall workflows remain unverified on hardware. Testing covers
one instrument, not every GR-1 hardware revision. Back up your sounds first.
