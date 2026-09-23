#!/usr/bin/env python3
"""Sort the PT_LOAD program headers of an ELF64 LE file by p_vaddr, in place.

The ELF spec requires PT_LOAD entries to be in ascending p_vaddr order. Linux does
not enforce it but gVisor does, and rejects out-of-order binaries with ENOEXEC.
Only the order of the entries in the program header table changes: no segment is
moved, so the resulting memory image is identical.
"""

import struct
import sys

PT_LOAD = 1

with open(sys.argv[1], "r+b") as f:
    elf = f.read(64)
    assert elf[:4] == b"\x7fELF" and elf[4] == 2 and elf[5] == 1, "not an ELF64 LE file"

    (phoff,) = struct.unpack_from("<Q", elf, 0x20)
    phentsize, phnum = struct.unpack_from("<HH", elf, 0x36)

    f.seek(phoff)
    table = f.read(phentsize * phnum)
    phdrs = [table[i * phentsize : (i + 1) * phentsize] for i in range(phnum)]

    def is_load(ph):
        return struct.unpack_from("<I", ph, 0)[0] == PT_LOAD

    def vaddr(ph):
        return struct.unpack_from("<Q", ph, 0x10)[0]

    slots = [i for i, ph in enumerate(phdrs) if is_load(ph)]
    for slot, ph in zip(slots, sorted((phdrs[i] for i in slots), key=vaddr)):
        phdrs[slot] = ph

    f.seek(phoff)
    f.write(b"".join(phdrs))
