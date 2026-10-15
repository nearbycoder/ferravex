#!/usr/bin/env python3
"""Replace local gallery previews using successful original-host smoke captures.

Only Python stdlib zlib is used to compress the original PNG stream for storage.
This tool never uploads or changes the running public site.
"""
import argparse
import json
from pathlib import Path
import struct
import zlib
ROOT=Path(__file__).resolve().parent.parent

def compact_png(data):
    if data[:8] != b'\x89PNG\r\n\x1a\n':raise ValueError('not PNG')
    chunks=[];compressed=bytearray();offset=8
    while offset<len(data):
        length=struct.unpack_from('>I',data,offset)[0]
        kind=data[offset+4:offset+8];payload=data[offset+8:offset+8+length]
        checksum=struct.unpack_from('>I',data,offset+8+length)[0]
        if zlib.crc32(kind+payload)!=checksum:raise ValueError('bad PNG checksum')
        if kind==b'IDAT':compressed.extend(payload)
        else:chunks.append((kind,payload))
        offset+=12+length
    raw=zlib.decompress(compressed)
    chunks.insert(1,(b'IDAT',zlib.compress(raw,9)))
    return data[:8]+b''.join(struct.pack('>I',len(payload))+kind+payload+struct.pack('>I',zlib.crc32(kind+payload)) for kind,payload in chunks)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('reports',type=Path,nargs='+')
    args=parser.parse_args()
    catalog={g['id']:g for g in json.loads((ROOT/'examples/catalog.json').read_text())}
    for report in args.reports:
        for result in json.loads(report.read_text())['results']:
            if not result['passed'] or result['game'] not in catalog:raise ValueError('unknown or failed capture')
            source=Path(result['screenshot']);source=source if source.is_absolute() else ROOT/source
            image=catalog[result['game']]['image'];out=ROOT/'docs/screenshots'/image
            data=compact_png(source.read_bytes());out.write_bytes(data)
            print(f'{result["game"]}: {len(data):,} bytes -> {out.relative_to(ROOT)}')
if __name__=='__main__':main()
