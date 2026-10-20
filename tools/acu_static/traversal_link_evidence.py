"""Bounded evidence for ACU-RE-0031 against the identified saved runtime image."""
import argparse
import hashlib
import json
import struct
from pathlib import Path
import capstone

BASE = 0x140000000
SHA = "9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520"
RANGES = {
    "scan_store": (0x1A7000, 1093), "point_getter": (0x1A7C80, 156),
    "target_builder": (0x1A94EC0, 4558), "event_binding": (0x187200, 713),
    "event_refresh": (0x187B50, 551), "event_timing": (0x1886E0, 634),
    "correction": (0x188F00, 4145), "translation_excerpt": (0x18979E, 0x189B81-0x18979E),
    "controller_walk": (0x15E370,187), "target_count": (0x1A6390,7),
}
CORPUS = ["1401A7000", "1401A7A70", "1401A7B50", "1401A7C80", "141A519E0", "141A94EC0", "140159070", "1401536A0", "14015F590", "140187200", "140187B50", "1401886E0", "140188F00", "14015E370"]

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("image",type=Path)
    p.add_argument("--output",type=Path,required=True)
    p.add_argument("--corpus",type=Path,default=Path("ACU - Decompiled"))
    args=p.parse_args()
    data=args.image.read_bytes()
    assert hashlib.sha256(data).hexdigest()==SHA,"unrecognized ACU mapped image"
    args.output.mkdir(parents=True,exist_ok=True)
    md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_64)
    result={"image":str(args.image.resolve()),"sha256":SHA,"base":hex(BASE),"capstone":capstone.__version__,"ranges":{},"corpus":{},"constructor_name_candidates":{}}
    for label,(rva,length) in RANGES.items():
        code=data[rva:rva+length]
        listing="\n".join(f"{i.address:#x} {i.mnemonic} {i.op_str}" for i in md.disasm(code,BASE+rva))
        (args.output/f"{label}.asm.txt").write_text(listing+"\n",encoding="utf-8")
        result["ranges"][label]={"va":hex(BASE+rva),"rva":hex(rva),"length":length,"sha256":hashlib.sha256(code).hexdigest()}
    # The scanner function occupies byte slot 3920 in this constructor's table.
    table=0x3A65DA0
    slots=[464,528,560,592,816,2448,3920]
    result["analysis_dispatch"]={"table_va":hex(BASE+table),"slots":{str(i):hex(struct.unpack_from("<Q",data,table+i)[0]) for i in slots}}
    for name in CORPUS:
        path=args.corpus/f"{name}.c"
        raw=path.read_bytes()
        result["corpus"][name]={"path":str(path),"sha256":hashlib.sha256(raw).hexdigest(),"lines":len(raw.splitlines())}
    for target in [0x1401B06E0,0x1401A1670,0x1401536A0]:
        cursor=0; found=[]
        while (cursor:=data.find(struct.pack("<Q",target),cursor))>=0:
            candidate=struct.unpack_from("<Q",data,max(0,cursor-56))[0]
            if BASE+0x2E00000<candidate<BASE+0x3600000:
                raw=data[candidate-BASE:data.find(b"\0",candidate-BASE)]
                found.append({"pointer_va":hex(BASE+cursor),"name_candidate":raw[:100].decode("ascii",errors="replace")})
            cursor+=1
        result["constructor_name_candidates"][hex(target)]=found
    (args.output/"manifest.json").write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8")
    print(json.dumps({"ranges":len(RANGES),"corpus":len(CORPUS),"candidates":result["constructor_name_candidates"]}))

if __name__=="__main__": main()
