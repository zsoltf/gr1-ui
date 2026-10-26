# UI usage

Run `./ui/run.sh` from the source root. The desktop demo uses disposable state;
its file operations do not write instrument patch or performance files.
On macOS, `./ui/run.sh --bundle` prepares a local app linked to installed SDL2.

The waveform and grains stay visible on Main. Patch contains Play, Timing,
Shape, FX, Voices, Keys, Matrix, LFO 1 and LFO 2. Sample contains Record
and Sample. Perf contains Performances, Presets, Parts, Files and System.

The physical Select encoder moves focus; Select enters or grabs. Turning a
grabbed numeric field changes its value; Select keeps it and Escape restores it.
Escape returns to a page strip, then Main. Shift+Escape returns directly to Main.
Load and Save open the browser and Save sheet; confirmations begin on Cancel.
Shift+Patch, Shift+Sample and Shift+Perf open their roots. The first physical
encoder report establishes a reference after startup and does not move focus.

Pressing either LFO's DEST button immediately shows its selected target, even
at zero Amount. Amount edits that target; other nonzero destinations stay active.
DEST, Amount, LFO Frequency and window Sides/Tilt/Curve show their values in
the large amber readout for 1.5 seconds after the last change. LFO shape changes
show the waveform name and a larger shape icon for the same time, also in menus.
The selected destination remains visible below the LFO after the readout clears.

DSP metering is a thin bar along the top edge of the bottom status area when
System > DSP meter is On.

For desktop panel preview, use `./ui/run.sh --panel-preview`. F1, F2 and F3
open Patch, Sample and Perf, F4 returns to Main; arrow keys turn Select and
Enter presses it. Escape returns. F5/F6 stand in for Load/Save. Hold Shift
for alternate panel actions. Number keys 1-8 edit Position, Size, Spray, Density,
Scan, Pan spray, Loop start and Loop end on Main; Shift selects alternate roles. Space
toggles demo Play. The name editor accepts desktop keyboard text.

The default is Dark at 50% brightness; saved preferences take precedence.
System > Appearance selects Light or Dark. Brightness dims pixels, not the
hardware backlight. Desktop preferences persist separately from the demo.
`GR1_UI_SETTINGS` selects a preference file; `--theme light` or `--theme dark`
overrides startup appearance for one run. `--theme white` remains accepted for
compatibility. `GR1_UI_FONT` selects a regular
TrueType font; bundled DejaVu Bold and Condensed fall back to the regular face
if unavailable. Their full license is in [fonts/LICENSE-DejaVu.txt](fonts/LICENSE-DejaVu.txt).

F12 captures the framebuffer in the OS temporary directory. `--captures DIR`
writes demonstration frames. `--benchmark-render 600` measures desktop drawing
without engine calls or display presentation. Synthetic waveforms and motion
are demonstration only. Never run device mode on a Mac, generic Pi or emulator.
