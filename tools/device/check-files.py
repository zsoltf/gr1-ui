#!/usr/bin/env python3
"""Independently decode Rust-written GRP/GRM using the stock Python file readers.

Uses stdlib ConfigParser and literal_eval, not the Rust parser and not firmware
code. Compare original fixture values, matrix labels and multi configurations.
"""
import argparse
import ast
import configparser
import math
from pathlib import Path


def read(path, section):
    parser = configparser.ConfigParser()
    with path.open() as source:
        parser.read_file(source)
    return dict(parser[section])


def numeric(value):
    if value.lower() in ('true', 'yes', 'on'):
        return 1.0
    if value.lower() in ('false', 'no', 'off'):
        return 0.0
    return float(value)


def equal(left, right):
    try:
        a, b = numeric(left), numeric(right)
    except ValueError:
        return left == right
    return math.isclose(a, b, rel_tol=2e-6, abs_tol=1e-7)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('original', type=Path)
    p.add_argument('exported', type=Path)
    args = p.parse_args()
    counts = {'grp': 0, 'grm': 0}
    for output in args.exported.rglob('*'):
        kind = output.suffix[1:]
        if kind not in counts or not output.is_file():
            continue
        source = args.original / output.relative_to(args.exported)
        section = 'gr-1 patch' if kind == 'grp' else 'performance'
        before, after = read(source, section), read(output, section)
        for key, value in before.items():
            if key in ('version', 'matrix_rows', 'matrix_cols', 'mod_matrix', 'aux_buf', 'western_scale', 'multi_presets') or key.endswith('dest'):
                continue
            assert key in after and equal(value, after[key]), (source, key, value, after.get(key))
        if kind == 'grp':
            assert after['version'] == '3.4.4'
            rows, cols = ([x.strip() for x in after[k].split(',')] for k in ('matrix_rows', 'matrix_cols'))
            values = list(map(float, after['mod_matrix'].split()))
            assert len(rows) == 9 and len(cols) == 11 and len(values) == 99
            if 'matrix_rows' in before and 'matrix_cols' in before:
                old_rows, old_cols = ([x.strip() for x in before[k].split(',')] for k in ('matrix_rows', 'matrix_cols'))
                old = list(map(float, before['mod_matrix'].split()))
                for r, name in enumerate(old_rows):
                    for c, dest in enumerate(old_cols):
                        if name in rows and dest in cols:
                            assert math.isclose(old[r * len(old_cols) + c], values[rows.index(name) * 11 + cols.index(dest)], rel_tol=2e-6, abs_tol=1e-7), source
                for key in ('lfo1dest', 'lfo2dest', 'cv1dest', 'cv2dest'):
                    if key in before:
                        assert old_cols[int(before[key])] == cols[int(after[key])], (source, key)
            assert len(after['western_scale'].split()) == 12
            assert len(after['aux_buf'].split()) == int(after['aux_len'])
            for key in ('western_scale', 'aux_buf'):
                if key in before:
                    limit = 12 if key == 'western_scale' else int(after['aux_len'])
                    a, b = before[key].split()[:limit], after[key].split()
                    assert len(a) == len(b) and all(equal(x, y) for x, y in zip(a, b)), (source, key)
        else:
            multi = ast.literal_eval(after['multi_presets'])
            assert len(multi) == 4
            for m in multi:
                assert isinstance(m['name'], str) and 0 <= m['stencilnr_in_focus'] < 4
                assert len(m['patchnums']) == len(m['partenables']) == len(m['part_volumes']) == 4
            if 'multi_presets' in before:
                old = ast.literal_eval(before['multi_presets'])
                for a, b in zip(old, multi):
                    for k in ('name', 'patchnums', 'partenables', 'stencilnr_in_focus'):
                        assert a[k] == b[k], (source, k)
                    assert all(math.isclose(x, y, rel_tol=2e-6, abs_tol=1e-7) for x, y in zip(a['part_volumes'], b['part_volumes'])), source
            else:
                for key in ('patchnums', 'partenables'):
                    if key in before:
                        assert list(map(numeric, before[key].split())) == multi[0][key], (source, key)
        counts[kind] += 1
    assert counts['grp'] and counts['grm'], 'No patch/performance fixtures checked'
    print(f'Independent Python readers verified {counts["grp"]} patches and {counts["grm"]} performances; no engine executed.')


if __name__ == '__main__':
    main()
