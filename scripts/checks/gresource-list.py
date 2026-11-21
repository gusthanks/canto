#!/usr/bin/env python3
"""Print every resource path in a .gresource file or in an ELF library.

A stand-in for `gresource list FILE` (GLib's development tools) that needs only
Python and PyGObject. For a shared library such as libgtk-4.so.1 it reads the
resource bundles the library embeds in its .gresource.* ELF sections.

Usage: gresource-list.py FILE
"""

import struct
import sys

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib  # noqa: E402


def elf_gresource_sections(data):
    """The contents of every .gresource.* section of an ELF file."""
    is64 = data[4] == 2
    order = "<" if data[5] == 1 else ">"
    if is64:
        (shoff,) = struct.unpack_from(order + "Q", data, 0x28)
        shentsize, shnum, shstrndx = struct.unpack_from(order + "HHH", data, 0x3A)
        header = order + "IIQQQQ"
    else:
        (shoff,) = struct.unpack_from(order + "I", data, 0x20)
        shentsize, shnum, shstrndx = struct.unpack_from(order + "HHH", data, 0x2E)
        header = order + "IIIIII"

    def section(index):
        name, _type, _flags, _addr, offset, size = struct.unpack_from(
            header, data, shoff + index * shentsize
        )
        return name, offset, size

    _, names_offset, _ = section(shstrndx)
    found = []
    for index in range(shnum):
        name, offset, size = section(index)
        start = names_offset + name
        if data[start : data.index(b"\0", start)].startswith(b".gresource."):
            found.append(data[offset : offset + size])
    return found


def walk(resource, path):
    for child in resource.enumerate_children(path, Gio.ResourceLookupFlags.NONE):
        if child.endswith("/"):
            yield from walk(resource, path + child)
        else:
            yield path + child


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: gresource-list.py FILE")
    with open(sys.argv[1], "rb") as f:
        data = f.read()
    bundles = elf_gresource_sections(data) if data[:4] == b"\x7fELF" else [data]
    for bundle in bundles:
        resource = Gio.Resource.new_from_data(GLib.Bytes.new(bundle))
        for path in walk(resource, "/"):
            print(path)


if __name__ == "__main__":
    main()
