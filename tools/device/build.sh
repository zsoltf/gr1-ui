#!/usr/bin/env bash
# Reproduce the private ARM package locally; never executes the ARM engine.
set -euo pipefail
cd "$(dirname "$0")/../.."
mkdir -p device-build/downloads
image=device-build/downloads/gr1_3part_3.0.0-b9_15jan25.img.zip
firmware=device-build/downloads/gr1_firmware_3.4.4_24117ba.tgz
if [[ ! -f "$image" ]]; then
  curl --fail --location --retry 2 -o "$image.partial" https://www.tastychips.nl/downloads/GR-1/SD/gr1_3part_3.0.0-b9_15jan25.img.zip
  mv "$image.partial" "$image"
fi
if [[ ! -f "$firmware" ]]; then
  curl --fail --location --retry 2 -o "$firmware.partial" https://www.tastychips.nl/downloads/GR-1/Firmware/gr1_firmware_3.4.4_24117ba.tgz
  mv "$firmware.partial" "$firmware"
fi
python3 -m venv device-build/tools-venv
device-build/tools-venv/bin/pip install -r tools/device/requirements.txt
device-build/tools-venv/bin/python tools/device/prepare.py --image "$image" --firmware "$firmware"
python3 tools/bootstrap.py --firmware "$firmware" --python device-build/tools-venv/bin/python
device-build/tools-venv/bin/python tools/device/generate.py "$firmware" --out device-build/generated
cmp ui/src/device/abi.rs device-build/generated/abi.rs
cmp ui/src/device/defaults.json device-build/generated/defaults.json
cargo metadata --locked --format-version 1 --manifest-path ui/Cargo.toml > device-build/cargo-metadata.json
sdl_source=$(python3 -c 'import json,pathlib; m=json.load(open("device-build/cargo-metadata.json")); print(next(str(pathlib.Path(p["manifest_path"]).parent/"SDL") for p in m["packages"] if p["name"]=="sdl2-sys"))')
docker build -t gr1-rust-build:1.90.0 -f tools/device/Dockerfile tools/device
# Keep build output writable by the host user that packages it.
docker run --rm --user "$(id -u):$(id -g)" -e CARGO_HOME=/work/device-build/docker-cargo -v "$PWD:/work" -v "$sdl_source:/sdl:ro" gr1-rust-build:1.90.0 sh tools/device/build-sdl.sh > device-build/sdl-build.log 2>&1
docker run --rm --user "$(id -u):$(id -g)" -e CARGO_HOME=/work/device-build/docker-cargo -v "$PWD:/work" gr1-rust-build:1.90.0 sh tools/device/build-app.sh > device-build/app-build.log 2>&1
device-build/tools-venv/bin/python tools/device/check-elf.py | tee device-build/elf-check.txt
device-build/tools-venv/bin/python tools/device/package.py --sdl-source "$sdl_source"
