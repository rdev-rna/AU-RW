"""Preserve an ACU mapped image and create a PE view for static analysis only."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
from tools.native_probe import client


def create_view(mapped, image_base):
    data = bytearray(mapped)
    if len(data)<0x100 or data[:2]!=b"MZ":
        raise ValueError("mapped image lacks DOS header")
    pe = struct.unpack_from("<I",data,0x3C)[0]
    if pe+24>len(data) or data[pe:pe+4]!=b"PE\0\0":
        raise ValueError("invalid PE header")
    machine,count = struct.unpack_from("<HH",data,pe+4)
    optional_size = struct.unpack_from("<H",data,pe+20)[0]
    optional = pe+24
    if machine!=0x8664 or not 1<=count<=96 or optional_size<240 or optional+optional_size+count*40>len(data):
        raise ValueError("unsupported PE header")
    if struct.unpack_from("<H",data,optional)[0]!=0x20B:
        raise ValueError("expected PE32+")
    if struct.unpack_from("<I",data,optional+56)[0]!=len(data):
        raise ValueError("mapped image length differs from SizeOfImage")
    alignment = struct.unpack_from("<I",data,optional+36)[0]
    if alignment==0 or alignment&(alignment-1) or alignment>65536:
        raise ValueError("invalid file alignment")
    table = optional+optional_size
    sections = []
    for index in range(count):
        offset = table+index*40
        virtual_size,rva,raw_size,raw_pointer = struct.unpack_from("<4I",data,offset+8)
        if not 0<rva<len(data) or rva%alignment:
            raise ValueError("invalid section RVA")
        sections.append({"header_offset":offset,"name":bytes(data[offset:offset+8]).rstrip(b"\0").decode("ascii",errors="replace"),
                         "rva":rva,"virtual_size":virtual_size,"original_raw_size":raw_size,"original_raw_pointer":raw_pointer})
    ordered = sorted(sections,key=lambda row:row["rva"])
    for index,row in enumerate(ordered):
        end = ordered[index+1]["rva"] if index+1<len(ordered) else len(data)
        span = end-row["rva"]
        if span<=0 or span%alignment or row["virtual_size"]>span:
            raise ValueError("overlapping/invalid section virtual span")
        struct.pack_into("<II",data,row["header_offset"]+16,span,row["rva"])
        row["analysis_raw_pointer"] = row["rva"]
        row["analysis_raw_size"] = span
    original_base = struct.unpack_from("<Q",data,optional+24)[0]
    struct.pack_into("<Q",data,optional+24,image_base)
    struct.pack_into("<I",data,optional+64,0)  # Derived bytes invalidate checksum.
    struct.pack_into("<II",data,optional+112+4*8,0,0)  # Certificate is a disk-file range.
    return data,{"sections":sections,"original_preferred_base":hex(original_base),"analysis_base":hex(image_base),
                 "header_changes":["Section raw ranges map virtual spans directly.","ImageBase equals captured process base.","Checksum and certificate directory cleared."]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pid",type=int)
    parser.add_argument("output",type=Path)
    parser.add_argument("--expected-sha256",required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error("output already exists")
    status = client.checked(args.pid,"STATUS")
    if any(int(h["address"],16) for h in status["hooks"]):
        raise RuntimeError("clear hooks before creating the analysis view")
    modules = client.checked(args.pid,"MODULES")["modules"]
    main_module = next(row for row in modules if row["base"].lower()==status["image_base"].lower())
    disk = Path(main_module["path"])
    if client.file_hash(disk).lower()!=args.expected_sha256.lower():
        raise RuntimeError("target disk hash mismatch")
    args.output.mkdir(parents=True,exist_ok=False)
    capture = client.dump(args.pid,args.output/"capture",max_bytes=256*1024**2)
    source = args.output/"capture/main-image.mapped.bin"
    mapped = source.read_bytes()
    view,layout = create_view(mapped,int(status["image_base"],16))
    target = args.output/"ACU.runtime-analysis.pe"
    with target.open("xb") as stream:
        stream.write(view)
    report = {"pid":args.pid,"disk_executable_sha256":args.expected_sha256,"mapped_sha256":hashlib.sha256(mapped).hexdigest(),
              "analysis_view_sha256":hashlib.sha256(view).hexdigest(),"view_path":str(target.resolve()),"layout":layout,
              "capture_manifest":str((args.output/"capture/manifest.json").resolve()),"coherent_snapshot":False,
              "read_failures":len(capture["read_failures"]),
              "limitations":["Derived static-analysis view; do not execute it.","Runtime patch/import state and unreadable-page holes are retained.","Header remapping is not a reconstructed runnable executable."]}
    (args.output/"analysis-view.json").write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps({key:report[key] for key in ("view_path","mapped_sha256","analysis_view_sha256","read_failures")}))


if __name__=="__main__":
    main()
