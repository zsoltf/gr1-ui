#!/usr/bin/env python3
"""Inventory the pinned GR-1 GUI bytecode without importing or executing it.

Requires xdis==6.3.0. Input is the manufacturer's unmodified 3.4.4 archive.
Output contains interface metadata, not firmware, decompiled bodies, or keys.
"""

import argparse
import hashlib
import json
from pathlib import Path
import tarfile
import tempfile

from xdis.bytecode import Bytecode
from xdis.load import load_module
from xdis.op_imports import get_opcode_module


FIRMWARE_SHA256 = "559e1e64bca5b16e17567231aeeb82eae4640adab9c06d910da15275294f4c5b"
MEMBER = "grui/grmain_nologging.cpython-34.pyo"


def expression(instructions, end):
    """Read a bounded expression backwards; never evaluate firmware code."""
    if end < 0:
        raise ValueError("expression underflow")
    instruction = instructions[end]
    op, value = instruction.opname, instruction.argval
    if op == "LOAD_CONST" and isinstance(value, (str, int, float, type(None))):
        return value, end - 1
    if op in {"LOAD_NAME", "LOAD_GLOBAL"}:
        return {"name": value}, end - 1
    if op in {"BUILD_LIST", "BUILD_TUPLE"}:
        values = []
        cursor = end - 1
        for _ in range(value):
            item, cursor = expression(instructions, cursor)
            values.append(item)
        return list(reversed(values)), cursor
    if op in {"BINARY_MULTIPLY", "BINARY_ADD"}:
        right, cursor = expression(instructions, end - 1)
        left, cursor = expression(instructions, cursor)
        return {"op": "*" if op == "BINARY_MULTIPLY" else "+", "left": left, "right": right}, cursor
    if op == "CALL_FUNCTION" and value < 256:
        args = []
        cursor = end - 1
        for _ in range(value):
            item, cursor = expression(instructions, cursor)
            args.append(item)
        function, cursor = expression(instructions, cursor)
        return {"call": function, "args": list(reversed(args))}, cursor
    raise ValueError(f"unsupported expression opcode {op} at {instruction.offset}")


def display(value):
    if isinstance(value, dict):
        if "name" in value:
            return value["name"]
        if "call" in value:
            return f"{display(value['call'])}({', '.join(map(display, value['args']))})"
        return f"({display(value['left'])} {value['op']} {display(value['right'])})"
    if isinstance(value, list):
        return "[" + ", ".join(map(display, value)) + "]"
    return repr(value)


def numeric(value, constants):
    if isinstance(value, (int, float)):
        return value
    if isinstance(value, dict):
        if "name" in value:
            return constants[value["name"]]
        if "op" in value:
            left = numeric(value["left"], constants)
            right = numeric(value["right"], constants)
            return left * right if value["op"] == "*" else left + right
    raise ValueError("not a numeric constant")


def inspect(archive_path):
    digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
    if digest != FIRMWARE_SHA256:
        raise ValueError(f"wrong firmware SHA256: {digest}; expected {FIRMWARE_SHA256}")
    with tarfile.open(archive_path) as archive:
        member = archive.getmember(MEMBER)
        if not member.isfile():
            raise ValueError("GUI member is not a regular file")
        bytecode = archive.extractfile(member).read()
    with tempfile.TemporaryDirectory(prefix="gr1-bytecode-") as directory:
        path = Path(directory) / "grmain.pyo"
        path.write_bytes(bytecode)
        version, _, _, root, *_ = load_module(str(path))
    opcode = get_opcode_module(version, "CPython")
    result = {
        "firmware_sha256": digest,
        "member": MEMBER,
        "bytecode_sha256": hashlib.sha256(bytecode).hexdigest(),
        "python_version": list(version),
        "scope": "GUI references to lib and its module aliases; declarations are not verified native prototypes.",
        "constants": {},
        "structures": {},
        "native_symbols": {},
        "native_aliases": {},
        "unresolved_lib_accesses": [],
        "functions": {},
    }
    root_instructions = list(Bytecode(root, opcode))
    for index, instruction in enumerate(root_instructions):
        if (instruction.opname in {"STORE_NAME", "STORE_GLOBAL"} and index >= 2
                and root_instructions[index - 1].opname == "LOAD_ATTR"
                and root_instructions[index - 2].argval == "lib"):
            result["native_aliases"][instruction.argval] = root_instructions[index - 1].argval

    def walk(code, name):
        instructions = list(Bytecode(code, opcode))
        line = code.co_firstlineno
        native = set()
        for index, instruction in enumerate(instructions):
            line = instruction.starts_line or line
            location = {"function": name, "offset": instruction.offset, "source_line_hint": line}
            if instruction.opname in {"LOAD_NAME", "LOAD_GLOBAL"} and instruction.argval == "lib":
                following = instructions[index + 1]
                if following.opname == "LOAD_ATTR":
                    symbol = following.argval
                    native.add(symbol)
                    entry = result["native_symbols"].setdefault(symbol, {"references": [], "declarations": {}})
                    entry["references"].append(location)
                else:
                    result["unresolved_lib_accesses"].append({**location, "next_opcode": following.opname})
            if instruction.opname == "STORE_ATTR" and instruction.argval in {"argtypes", "restype"}:
                symbol = None
                if index >= 3 and instructions[index - 1].opname == "LOAD_ATTR" and instructions[index - 2].argval == "lib":
                    symbol = instructions[index - 1].argval
                    value, _ = expression(instructions, index - 3)
                elif instructions[index - 1].opname in {"LOAD_NAME", "LOAD_GLOBAL"}:
                    symbol = result["native_aliases"].get(instructions[index - 1].argval)
                    if symbol:
                        value, _ = expression(instructions, index - 2)
                if symbol:
                    entry = result["native_symbols"].setdefault(symbol, {"references": [], "declarations": {}})
                    entry["declarations"][instruction.argval] = {"expression": display(value), **location}
            if instruction.opname in {"LOAD_NAME", "LOAD_GLOBAL"} and instruction.argval in result["native_aliases"]:
                symbol = result["native_aliases"][instruction.argval]
                native.add(symbol)
                entry = result["native_symbols"].setdefault(symbol, {"references": [], "declarations": {}})
                entry["references"].append({**location, "via_alias": instruction.argval})
            if instruction.opname == "STORE_NAME" and instruction.argval == "_fields_":
                fields, _ = expression(instructions, index - 1)
                result["structures"][name] = {
                    "location": location,
                    "fields": [{"name": field, "type": display(kind)} for field, kind in fields],
                }
            if name == "<module>" and instruction.opname == "STORE_NAME" and instruction.argval.isupper():
                try:
                    value, _ = expression(instructions, index - 1)
                    result["constants"][instruction.argval] = numeric(value, result["constants"])
                except (ValueError, KeyError):
                    pass
        if name != "<module>":
            result["functions"][name] = {
                "source_line_hint": code.co_firstlineno,
                "native_symbols": sorted(native),
                "names": list(code.co_names),
            }
        for constant in code.co_consts:
            if hasattr(constant, "co_code"):
                child = constant.co_name if name == "<module>" else name + "." + constant.co_name
                walk(constant, child)

    walk(root, "<module>")
    return result


def render(result, directory):
    directory.mkdir(parents=True, exist_ok=True)
    header = (
        "Generated by `tools/inspect_firmware.py` from the pinned 3.4.4 archive.\n"
        "See [the interface map](gui-engine-interface.md) for interpretation and limits.\n"
        "Locators are function names and bytecode offsets in `" + MEMBER + "`.\n"
        "Source-line metadata is only a hint; offsets identify the inspected instructions.\n\n"
    )
    lines = ["# Native API references\n\n", header,
             "This is the GUI-used surface, not a native export listing or a C header.\n"
             "`Unspecified` means the GUI does not declare that part of the signature.\n"
             "It does not mean no arguments, void return, or a verified integer type.\n"
             "ctypes defaults must not be promoted into native prototypes.\n"
             "Pointer aliases: `c_float_p = POINTER(c_float)`, "
             "`c_bool_p = POINTER(c_bool)`.\n"
             "The sample-load declaration/call mismatch is explained in the interface map.\n\n",
             "| Symbol | Declared arguments | Declared return | Reference locators |\n",
             "|---|---|---|---|\n"]
    for name, entry in sorted(result["native_symbols"].items()):
        declarations = entry["declarations"]
        arguments = declarations.get("argtypes", {}).get("expression", "Unspecified")
        returns = declarations.get("restype", {}).get("expression", "Unspecified")
        # One locator per referencing function, including the declaration owner.
        sites = {}
        for reference in entry["references"]:
            sites.setdefault(reference["function"], reference["offset"])
        locations = "; ".join(f"`{function}@{offset}`" for function, offset in sorted(sites.items()))
        lines.append(f"| `{name}` | `{arguments}` | `{returns}` | {locations} |\n")
    lines.extend(["\n## Aliases\n\n", "| GUI name | Native symbol |\n", "|---|---|\n"])
    for alias, symbol in sorted(result["native_aliases"].items()):
        lines.append(f"| `{alias}` | `{symbol}` |\n")
    (directory / "native-api.md").write_text("".join(lines), encoding="ascii")

    lines = ["# State layout and numeric constants\n\n", header,
             "Field order and ctypes declarations are recovered exactly for this bytecode.\n"
             "Native size, alignment, lifetime, and concurrent access still need validation.\n"
             "Do not derive target offsets using the development Mac's ABI.\n\n"]
    for name, structure in sorted(result["structures"].items()):
        location = structure["location"]
        lines.extend([f"## {name}\n\n", f"Declaration: `{name}@{location['offset']}`.\n\n",
                      "| Zero-based field index | Name | ctypes declaration |\n", "|---|---|---|\n"])
        for index, field in enumerate(structure["fields"]):
            lines.append(f"| {index} | `{field['name']}` | `{field['type']}` |\n")
        lines.append("\n")
    lines.extend(["## Numeric constants\n\n",
                  "Includes event IDs, screen IDs, dimensions, and enum values used by the GUI.\n"
                  "Only statically resolved numeric module assignments are listed.\n\n",
                  "| Name | Value |\n", "|---|---|\n"])
    for name, value in sorted(result["constants"].items()):
        lines.append(f"| `{name}` | `{value}` |\n")
    (directory / "interface-layout.md").write_text("".join(lines), encoding="ascii")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("firmware", type=Path)
    parser.add_argument("--output", type=Path, help="Optional detailed JSON inventory")
    parser.add_argument("--markdown-dir", type=Path, help="Write the two generated reference documents")
    arguments = parser.parse_args()
    if not arguments.output and not arguments.markdown_dir:
        parser.error("provide --output or --markdown-dir")
    result = inspect(arguments.firmware)
    if arguments.output:
        arguments.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="ascii")
    if arguments.markdown_dir:
        render(result, arguments.markdown_dir)
    print(f"{len(result['native_symbols'])} native symbols, {len(result['structures'])} structures, "
          f"{len(result['constants'])} numeric constants, "
          f"{len(result['unresolved_lib_accesses'])} unresolved direct lib accesses")


if __name__ == "__main__":
    main()
