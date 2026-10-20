"""Inventory x64 PE unwind ranges and MSVC RTTI candidates, on disk or mapped.

Use --mapped only with a captured loaded image laid out by RVA. These records
are hook discovery candidates, never proof of function semantics or execution.
No dependency installation, executable changes or process access is performed.
"""
import argparse
import bisect
import collections
import hashlib
import json
from pathlib import Path
import re
import struct


class Image:
    def __init__(self, data, mapped=False):
        self.data, self.mapped = data, mapped
        if len(data) > 1024**3 or data[:2] != b"MZ":
            raise ValueError("not a bounded PE image")
        pe = struct.unpack_from("<I", data, 0x3c)[0]
        if data[pe:pe+4] != b"PE\0\0":
            raise ValueError("PE signature missing")
        machine, count, _, _, _, size, _ = struct.unpack_from("<HHIIIHH", data, pe+4)
        opt = pe+24
        if machine != 0x8664 or struct.unpack_from("<H", data, opt)[0] != 0x20b or count > 96:
            raise ValueError("requires bounded x64 PE32+ section table")
        self.base = struct.unpack_from("<Q", data, opt+24)[0]
        self.preferred_base = self.base
        self.size = struct.unpack_from("<I", data, opt+56)[0]
        self.headers = struct.unpack_from("<I", data, opt+60)[0]
        self.directories = [struct.unpack_from("<II", data, opt+112+i*8) for i in range(min(16,struct.unpack_from("<I",data,opt+108)[0]))]
        self.sections=[]
        for i in range(count):
            row=struct.unpack_from("<8sIIIIIIHHI",data,opt+size+i*40)
            self.sections.append(dict(name=row[0].rstrip(b"\0").decode("ascii",errors="replace"),
                                      virtual_size=row[1],rva=row[2],raw_size=row[3],raw_offset=row[4],flags=row[9]))
    def read(self, rva, size):
        if rva < 0 or size < 0 or rva+size > self.size:
            raise ValueError("RVA outside image")
        if self.mapped or rva+size <= self.headers:
            start=rva
        else:
            section=next((s for s in self.sections if s['rva']<=rva and rva+size<=s['rva']+s['raw_size']),None)
            if section is None:
                raise ValueError("RVA has no file-backed bytes")
            start=section['raw_offset']+rva-section['rva']
        if start+size>len(self.data):
            raise ValueError("truncated image bytes")
        return self.data[start:start+size]
    def executable(self,rva):
        return any(s['flags']&0x20000000 and s['rva']<=rva<s['rva']+s['virtual_size'] for s in self.sections)


def inventory(image):
    functions=[]
    unavailable=[]
    if len(image.directories)>3:
        rva,size=image.directories[3]
        if size%12 or size>32*1024*1024:
            raise ValueError("invalid exception directory size")
        try:
            table=image.read(rva,size)
        except ValueError as error:
            table=b""
            unavailable.append(str(error))
        for start,end,unwind in struct.iter_unpack("<III",table):
            if not start or end<=start or not image.executable(start) or not image.executable(end-1):
                continue
            try:
                code=image.read(start,min(16,end-start))
            except ValueError:
                code=b""
            functions.append(dict(rva=f"{start:08x}",end_rva_exclusive=f"{end:08x}",unwind_rva=f"{unwind:08x}",
                                  expected_bytes=code.hex(),evidence="PE exception-directory range; may be a function fragment",
                                  semantics="unknown",available=bool(code and any(code))))
    functions.sort(key=lambda r:int(r['rva'],16))
    # Type names alone are hints. Require self-validating x64 COL structures
    # and executable vtable members before associating a method with a type.
    sections=[]
    descriptors={}
    for s in image.sections:
        if s['name'] not in ('.rdata','_RDATA','.data'):
            continue
        try:
            data=image.read(s['rva'],s['virtual_size'] if image.mapped else s['raw_size'])
        except ValueError:
            continue
        sections.append((s['rva'],data))
        for m in re.finditer(rb'\.\?A[UV][ -~]{1,1024}?@@\x00',data):
            descriptor=s['rva']+m.start()-16
            if descriptor>=s['rva']:
                descriptors[descriptor]=m.group()[:-1].decode('ascii')
    locators={}
    for base,data in sections:
        for offset in range(12,len(data)-12,4):
            type_rva=struct.unpack_from('<I',data,offset)[0]
            if type_rva not in descriptors:
                continue
            col=struct.unpack_from('<IIIIII',data,offset-12)
            if col[0]==1 and col[5]==base+offset-12:
                locators[image.base+col[5]]=(col[5],type_rva,col[1])
    classes=[]
    for base,data in sections:
        for offset in range(0,len(data)-16,8):
            col=locators.get(struct.unpack_from('<Q',data,offset)[0])
            if col is None:
                continue
            methods=[]
            for slot in range(2048):
                at=offset+8+slot*8
                if at+8>len(data):break
                va=struct.unpack_from('<Q',data,at)[0]
                rva=va-image.base
                if not image.executable(rva):break
                try:code=image.read(rva,16).hex()
                except ValueError:code=None
                methods.append(dict(slot=slot,rva=f'{rva:08x}',expected_bytes=code,semantics='unknown'))
            if methods:
                classes.append(dict(type_name=descriptors[col[1]],type_descriptor_rva=f'{col[1]:08x}',
                                    locator_rva=f'{col[0]:08x}',object_offset=col[2],vtable_rva=f'{base+offset+8:08x}',methods=methods,
                                    evidence='MSVC x64 COL self-RVA + type descriptor + executable pointer sequence; candidate ABI interpretation'))
    summary=dict(layout='mapped' if image.mapped else 'file',
                 preferred_image_base=f'{image.preferred_base:016x}',
                 runtime_base=f'{image.base:016x}' if image.mapped else None,
                 image_size=image.size,sections=image.sections,unwind_ranges=len(functions),
                 available_range_bytes=sum(r['available'] for r in functions),type_names=len(descriptors),vtable_candidates=len(classes),
                 unavailable=unavailable,limitations=[
                     'Packed file images may expose none of the runtime code. Capture after unpacking for useful live targets.',
                     'Mapped image headers contain preferred base. Relocated RTTI pointers need the actual module base via --live-base.',
                     'Unwind ranges include fragments and omit leaf/JIT/unregistered functions; vtable sequence boundaries can be ambiguous.',
                     'Candidates do not establish Arno ownership, subsystem semantics, calling convention or observed execution.'])
    return summary,functions,classes


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('image',type=Path)
    parser.add_argument('output',type=Path)
    parser.add_argument('--mapped',action='store_true')
    parser.add_argument('--live-base',type=lambda x:int(x,0))
    args=parser.parse_args()
    data=args.image.read_bytes()
    image=Image(data,args.mapped)
    if args.live_base is not None:
        if not args.mapped:parser.error('--live-base requires --mapped')
        image.base=args.live_base
    summary,functions,classes=inventory(image)
    summary.update(source_path=str(args.image.resolve()),source_sha256=hashlib.sha256(data).hexdigest())
    args.output.mkdir(parents=True,exist_ok=True)
    for name,rows in [('functions.jsonl',functions),('types.jsonl',classes)]:
        with (args.output/name).open('w',encoding='utf-8') as stream:
            for row in rows:stream.write(json.dumps(row)+'\n')
    (args.output/'summary.json').write_text(json.dumps(summary,indent=2),encoding='utf-8')
    print(json.dumps({k:summary[k] for k in ('source_sha256','layout','unwind_ranges','available_range_bytes','type_names','vtable_candidates')}))


if __name__=='__main__':
    main()
