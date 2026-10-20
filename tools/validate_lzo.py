"""Compare two small runtime archive vectors with an independent LZO DLL.

Reads original archive ranges in memory. Writes hashes/lengths only, no assets.
"""
import argparse
import ctypes
import hashlib
import json
from pathlib import Path
import struct
from acu import settings


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('lzo_dll', type=Path)
    args = p.parse_args()
    dll = ctypes.CDLL(str(args.lzo_dll.resolve()))
    decode = dll.lzo1x_decompress_safe
    decode.restype = ctypes.c_int
    decode.argtypes = [ctypes.c_void_p,ctypes.c_size_t,ctypes.c_void_p,
                       ctypes.POINTER(ctypes.c_size_t),ctypes.c_void_p]
    report = {'provider':str(args.lzo_dll.resolve()),
              'provider_sha256':hashlib.sha256(args.lzo_dll.read_bytes()).hexdigest(), 'samples':[]}
    game = settings()[1]['game_dir']
    for name,offset,size in [('DataPC.forge',672191,164),('DataPC_extra_patch_01.forge',10067,835)]:
        with (game/name).open('rb') as f:
            f.seek(offset); stored = f.read(size)
        cursor = 0
        for _ in range(2):
            if stored[cursor:cursor+8] != bytes.fromhex('33aafb5799fa0410'):
                raise ValueError('fixture magic mismatch')
            count = struct.unpack_from('<I',stored,cursor+15)[0]
            if count != 1: raise ValueError('only single-block oracle fixtures allowed')
            out_len,in_len = struct.unpack_from('<HH',stored,cursor+19)
            source = stored[cursor+27:cursor+27+in_len]
            if len(source) != in_len: raise ValueError('truncated source')
            dst = ctypes.create_string_buffer(out_len)
            result_len = ctypes.c_size_t(out_len)
            result = decode(source,len(source),dst,ctypes.byref(result_len),None)
            if result != 0 or result_len.value != out_len: raise ValueError(f'LZO oracle failure {result}')
            report['samples'].append(dict(archive=name,entry_offset=offset,frame_offset=cursor,
                stored_length=size,stored_sha256=hashlib.sha256(stored).hexdigest(),
                decoded_length=result_len.value,decoded_sha256=hashlib.sha256(dst.raw).hexdigest()))
            cursor += 27+in_len
        if cursor != size: raise ValueError('fixture trailing bytes')
    print(json.dumps(report,indent=2))


if __name__ == '__main__': main()
