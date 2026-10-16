#!/usr/bin/env python3
"""Read the pinned native payload as data; never load or execute firmware.

Requires xdis==6.3.0, pycryptodome and pyelftools. The CLI emits interface
metadata only; loader constants stay in memory. The local device packager
may use decode_native to prepare a private installation.
"""

import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import tarfile
import tempfile

from Crypto.Cipher import AES
from elftools.elf.elffile import ELFFile
from xdis.bytecode import Bytecode
from xdis.load import load_module
from xdis.op_imports import get_opcode_module

from inspect_firmware import FIRMWARE_SHA256, MEMBER, inspect


NATIVE_SHA256 = "3dbdc64487b33a28eaf55f12fe66d4184d64c85c6d88c1b18d80a93ed42bc682"


def decode_native(archive_path):
    data = archive_path.read_bytes()
    if hashlib.sha256(data).hexdigest() != FIRMWARE_SHA256:
        raise ValueError("archive does not match the pinned firmware SHA-256")
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        for name in (MEMBER, "grui/graphics"):
            if not archive.getmember(name).isfile():
                raise ValueError(f"not a regular archive member: {name}")
        gui = archive.extractfile(MEMBER).read()
        payload = archive.extractfile("grui/graphics").read()
    with tempfile.TemporaryDirectory(prefix="gr1-native-bytecode-") as directory:
        path = Path(directory) / "gui.pyo"
        path.write_bytes(gui)
        version, _, _, root, *_ = load_module(str(path))
    instructions = {
        item.offset: item
        for item in Bytecode(root, get_opcode_module(version, "CPython"))
    }
    # Reproduce only the pinned loader's data transform, not its code execution.
    def literal(offset):
        item = instructions[offset]
        if item.opname != "LOAD_CONST":
            raise ValueError("unexpected pinned loader instruction")
        return item.argval

    def binary(value):
        return value.encode("latin1") if isinstance(value, str) else value

    decoded = bytearray(AES.new(
        binary(literal(6748)), AES.MODE_CFB,
        iv=binary(literal(6754)), segment_size=8,
    ).decrypt(payload))
    decoded[:16] = bytes(literal(offset) for offset in range(6863, 6911, 3))
    if hashlib.sha256(decoded).hexdigest() != NATIVE_SHA256:
        raise ValueError("decoded payload does not match the pinned native SHA-256")
    return bytes(decoded)


def native_inventory(archive_path):
    decoded = decode_native(archive_path)
    elf = ELFFile(io.BytesIO(decoded))
    table = elf.get_section_by_name(".dynsym")
    symbols = list(table.iter_symbols())
    defined = {s.name: s for s in symbols if s["st_shndx"] != "SHN_UNDEF"}
    gui_symbols = inspect(archive_path)["native_symbols"]
    rodata = elf.get_section_by_name(".rodata")
    signatures = []
    for match in re.finditer(rb"[\x20-\x7e]{8,}", rodata.data()):
        value = match.group().decode("ascii")
        if re.fullmatch(r"(?:void|bool|int|float|double|unsigned\w*) [\w:]+\([^)]*\)", value):
            signatures.append({"address": hex(rodata["sh_addr"] + match.start()), "text": value})
    attributes = []
    for subsection in elf.get_section_by_name(".ARM.attributes").iter_subsections():
        for scope in subsection.iter_subsubsections():
            attributes.extend(str(attr) for attr in scope.iter_attributes())
    requirements = {}
    for version, auxiliary in elf.get_section_by_name(".gnu.version_r").iter_versions():
        requirements[version.name] = [item.name for item in auxiliary]
    return {
        "firmware_sha256": FIRMWARE_SHA256,
        "native_sha256": NATIVE_SHA256,
        "native_bytes": len(decoded),
        "scope": "Static metadata, not a complete source header or a thread-safety guarantee.",
        "machine": elf["e_machine"],
        "elf_class": elf.elfclass,
        "little_endian": elf.little_endian,
        "flags": hex(elf["e_flags"]),
        "arm_attributes": attributes,
        "needed": [t.needed for t in elf.get_section_by_name(".dynamic").iter_tags()
                   if t.entry.d_tag == "DT_NEEDED"],
        "symbol_versions": requirements,
        "has_debug_info_section": elf.get_section_by_name(".debug_info") is not None,
        "has_symtab": elf.get_section_by_name(".symtab") is not None,
        "dynamic_symbol_count": len(symbols),
        "imports": sorted(s.name for s in symbols if s.name and s["st_shndx"] == "SHN_UNDEF"),
        "gui_exports": {
            name: {"address": hex(defined[name]["st_value"]),
                   "size": defined[name]["st_size"], "type": defined[name]["st_info"]["type"]}
            for name in sorted(gui_symbols) if name in defined
        },
        "missing_gui_exports": sorted(set(gui_symbols) - set(defined)),
        "embedded_signature_strings": signatures,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = native_inventory(args.archive)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(f"{len(result['gui_exports'])} GUI references exported; "
          f"missing: {', '.join(result['missing_gui_exports'])}")


if __name__ == "__main__":
    main()
