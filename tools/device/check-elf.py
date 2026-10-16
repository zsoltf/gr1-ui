#!/usr/bin/env python3
"""Check the ARM package against the stock sysroot without executing any ELF."""
import argparse
from collections import deque
from pathlib import Path
import re
from elftools.elf.elffile import ELFFile


def metadata(path):
    with path.open('rb') as f:
        e=ELFFile(f)
        dynamic=e.get_section_by_name('.dynamic')
        needed=[t.needed for t in dynamic.iter_tags() if t.entry.d_tag=='DT_NEEDED'] if dynamic else []
        version_ids={}
        verneed=e.get_section_by_name('.gnu.version_r')
        if verneed:
            for _, aux in verneed.iter_versions():
                for item in aux: version_ids[item['vna_other']]=item.name
        verdef=e.get_section_by_name('.gnu.version_d')
        if verdef:
            for version, aux in verdef.iter_versions():
                names=[item.name for item in aux]
                version_ids[version['vd_ndx']]=names[0]
        sym=e.get_section_by_name('.dynsym')
        versions=e.get_section_by_name('.gnu.version')
        imports=[];exports=set()
        for i,s in enumerate(sym.iter_symbols() if sym else []):
            v=versions.get_symbol(i)['ndx'] if versions else None
            v=version_ids.get(v & 0x7fff) if isinstance(v,int) else None
            if s['st_shndx']=='SHN_UNDEF':
                if s.name and s['st_info']['bind']!='STB_WEAK': imports.append((s.name,v))
            else:
                exports.add((s.name,v));exports.add((s.name,None))
        interp=e.get_section_by_name('.interp')
        return {'path':str(path),'machine':e['e_machine'],'class':e.elfclass,'flags':e['e_flags'],
                'needed':needed,'imports':imports,'exports':exports,
                'interpreter':interp.data().rstrip(b'\0').decode() if interp else None}


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root',type=Path,default=Path('device-build'))
    p.add_argument('--sysroot',type=Path,help='Read-only copy of the connected device libraries')
    a=p.parse_args()
    sysroot=a.sysroot or a.root/'sysroot'
    roots=[a.root/'package/lib',sysroot/'lib/arm-linux-gnueabihf',sysroot/'usr/lib/arm-linux-gnueabihf',sysroot/'opt/vc/lib',sysroot/'lib']
    queue=deque([a.root/'package/gr1-ui',a.root/'package/lib/libgr.so.1.0.1'])
    seen={};errors=[]
    # SDL's RPi display and evdev discovery load these at runtime, so they do
    # not appear in its DT_NEEDED table. Include their complete dependency trees.
    for name in ('libbrcmEGL.so', 'libbrcmGLESv2.so', 'libudev.so.1'):
        found=next((r/name for r in roots if (r/name).is_file()),None)
        if found is None:errors.append('Missing SDL runtime library '+name)
        else:queue.append(found)
    while queue:
        path=queue.popleft().resolve()
        if path in seen:continue
        m=metadata(path);seen[path]=m
        if m['machine']!='EM_ARM' or m['class']!=32 or not (m['flags']&0x400):errors.append(f'Not ARM32 hard float: {path}')
        if m['interpreter'] and not (sysroot/m['interpreter'].lstrip('/')).exists():errors.append('Missing ELF interpreter '+m['interpreter'])
        for name in m['needed']:
            found=next((r/name for r in roots if (r/name).is_file()),None)
            if found is None:errors.append(f'{path.name}: missing {name}')
            else:queue.append(found)
    exports=set().union(*(m['exports'] for m in seen.values()))
    for m in seen.values():
        for symbol in m['imports']:
            if symbol not in exports:errors.append(f'{Path(m["path"]).name}: unresolved {symbol[0]}@{symbol[1]}')
    source=Path('ui/src/device/native.rs').read_text()
    names=set(re.findall(r'^\s+(\w+)\([^\n;]*;',source,re.M))
    api=source.split('api! {',1)[1].split('\n}',1)[0]
    names=set(re.findall(r'\b(\w+)\(',re.sub(r'//[^\n]*','',api)))
    names.update(re.findall(r'b"(\w+)\\0"',source))
    engine=next(m for m in seen.values() if Path(m['path']).name=='libgr.so.1.0.1')
    for name in names:
        if (name,None) not in engine['exports']:errors.append('Missing engine binding '+name)
    if errors:
        print('\n'.join(errors));raise SystemExit(1)
    print(f'ARM hard-float package: {len(seen)} ELF objects, all strong imports/version requirements resolved; {len(names)} native bindings exported.')
    for m in seen.values():
        versions=sorted({v for _,v in m['imports'] if v and re.fullmatch(r'GLIBC_[0-9.]+',v)},key=lambda s:tuple(map(int,s[6:].split('.'))))
        print(Path(m['path']).name, 'needs', ','.join(m['needed']), 'max glibc',versions[-1] if versions else 'none')

if __name__=='__main__':main()
