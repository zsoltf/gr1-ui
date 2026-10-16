# Third-party notices

The source license covers authored source only and grants no rights to vendor
firmware, synthesis engine, factory content or system image. Those inputs are
obtained privately from the manufacturer and are absent from this export.

Bundled DejaVu Sans Bold and Condensed 2.37 retain the complete Bitstream Vera
and Arev notices in [ui/fonts/LICENSE-DejaVu.txt](ui/fonts/LICENSE-DejaVu.txt).
The source also includes photos of the redesigned interface.

Rust crate sources are not bundled. UI downloads include full dependency license
texts in `DEPENDENCY-LICENSES.txt`, alongside the font and SDL licenses.
Exact resolved versions are in
[ui/Cargo.lock](ui/Cargo.lock). Retrieve their complete license inventory with:

```sh
cargo metadata --locked --format-version 1 --manifest-path ui/Cargo.toml
```

The inventory includes dependencies beyond MIT/Apache-2.0: `sdl2` and
`sdl2-sys` declare MIT and Zlib licensing; `libloading` uses ISC; `foldhash`
and `zlib-rs` use Zlib; `unicode-ident` declares
`(MIT OR Apache-2.0) AND Unicode-3.0`. Respect each crate's own notices when
redistributing builds. SDL2 itself uses the Zlib license; private device
packaging includes its license and the image's font copyright.

Python build/inspection dependencies are pinned in
[tools/device/requirements.txt](tools/device/requirements.txt); their packages
and system build tools are installed separately and are not bundled here.
