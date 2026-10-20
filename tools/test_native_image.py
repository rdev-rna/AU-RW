import struct
import unittest

from native_image import Image, inventory


def fixture():
    data=bytearray(0x1000)
    data[:2]=b'MZ'
    struct.pack_into('<I',data,0x3c,0x80)
    data[0x80:0x84]=b'PE\0\0'
    struct.pack_into('<HHIIIHH',data,0x84,0x8664,3,0,0,0,0xf0,0)
    opt=0x98
    struct.pack_into('<H',data,opt,0x20b)
    struct.pack_into('<Q',data,opt+24,0x140000000)
    struct.pack_into('<II',data,opt+56,0x4000,0x200)
    struct.pack_into('<I',data,opt+108,16)
    struct.pack_into('<II',data,opt+112+3*8,0x3000,12)
    for i,(name,rva,raw,size,flags) in enumerate([
        (b'.text',0x1000,0x200,0x200,0x60000020),
        (b'.rdata',0x2000,0x400,0x400,0x40000040),
        (b'.pdata',0x3000,0x800,0x100,0x40000040)]):
        struct.pack_into('<8sIIIIIIHHI',data,opt+0xf0+40*i,name,size,rva,size,raw,0,0,0,0,flags)
    data[0x200:0x210]=b'\x90'*15+b'\xc3'
    data[0x430:0x43b]=b'.?AVTest@@\0'
    struct.pack_into('<IIIIII',data,0x480,1,0,0,0x2020,0,0x2080)
    struct.pack_into('<QQ',data,0x4c0,0x140002080,0x140001000)
    struct.pack_into('<III',data,0x800,0x1000,0x1010,0x2100)
    return bytes(data)


class ImageTests(unittest.TestCase):
    def test_file_and_mapped_inventory_with_independent_expected_addresses(self):
        image=Image(fixture())
        summary,functions,types=inventory(image)
        self.assertEqual(summary['unwind_ranges'],1)
        self.assertEqual(functions[0]['rva'],'00001000')
        self.assertEqual(functions[0]['expected_bytes'],'90'*15+'c3')
        self.assertEqual(types[0]['type_name'],'.?AVTest@@')
        self.assertEqual(types[0]['vtable_rva'],'000020c8')
        self.assertEqual(types[0]['methods'][0]['rva'],'00001000')
        mapped=bytearray(0x4000)
        mapped[:0x200]=image.data[:0x200]
        for s in image.sections:
            mapped[s['rva']:s['rva']+s['raw_size']]=image.read(s['rva'],s['raw_size'])
        loaded=Image(bytes(mapped),mapped=True)
        self.assertEqual(inventory(loaded)[1:],(functions,types))
        # A corrupted self-RVA must not become a class/hook association.
        struct.pack_into('<I',mapped,0x2094,0x2084)
        self.assertEqual(inventory(Image(bytes(mapped),True))[2],[])
        with self.assertRaises(ValueError):image.read(0x1100,0x200)
        with self.assertRaises(ValueError):image.read(0xffffffff,16)
        with self.assertRaises(ValueError):Image(b'not a PE')


if __name__=='__main__':unittest.main()
