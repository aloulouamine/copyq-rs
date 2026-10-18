"""Liste les polices d'un fichier .ttc (index → nom complet) : python3 ci/ttc_faces.py f.ttc…"""
import struct, sys

def names(data, off):
    num = struct.unpack(">H", data[off + 4:off + 6])[0]
    for i in range(num):
        tag, _, toff, _ = struct.unpack(">4sIII", data[off + 12 + 16 * i:off + 28 + 16 * i])
        if tag != b"name":
            continue
        _, count, strings = struct.unpack(">HHH", data[toff:toff + 6])
        found = {}
        for j in range(count):
            pid, eid, lid, nid, length, soff = struct.unpack(">HHHHHH", data[toff + 6 + 12 * j:toff + 18 + 12 * j])
            if nid == 4 and pid == 3 and lid == 0x409:
                raw = data[toff + strings + soff:toff + strings + soff + length]
                found["full"] = raw.decode("utf-16-be", "replace")
        return found.get("full", "?")

for path in sys.argv[1:]:
    data = open(path, "rb").read()
    if data[:4] != b"ttcf":
        print(path, "→ police simple :", names(data, 0))
        continue
    n = struct.unpack(">I", data[8:12])[0]
    offs = struct.unpack(">%dI" % n, data[12:12 + 4 * n])
    print(path)
    for i, off in enumerate(offs):
        print(f"  {i}: {names(data, off)}")
