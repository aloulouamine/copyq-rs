"""Écrit un PNG de test (dégradé) sans dépendance : python3 ci/make_png.py out.png [l] [h]"""
import struct, sys, zlib

path = sys.argv[1]
w = int(sys.argv[2]) if len(sys.argv) > 2 else 640
h = int(sys.argv[3]) if len(sys.argv) > 3 else 400
rows = []
for y in range(h):
    row = bytearray([0])
    for x in range(w):
        row += bytes((90 + 120 * x // w, 70 + 100 * y // h, 220 - 80 * x // w))
    rows.append(bytes(row))


def chunk(tag, data):
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)


png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(b"".join(rows), 9)) + chunk(b"IEND", b"")
open(path, "wb").write(png)
