#!/usr/bin/env python3
"""Tests for scripts/sply_py.py against the shared SPLY fixtures. Run: python3 scripts/test_sply_py.py"""
import base64
import json
import pathlib
import struct
import sys
import unittest
import xml.etree.ElementTree as ET

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import sply_py  # noqa: E402

FIXTURES = pathlib.Path(__file__).resolve().parent.parent / "crates/splinely_document/tests/fixtures/sply"
MANIFEST = json.loads((FIXTURES / "manifest.json").read_text())


def build(body, strings=()):
    data = b"".join(s.encode() for s in strings)
    out = bytearray(sply_py.MAGIC)
    out += struct.pack("<HHIIII", 1, 0, 0, len(strings), len(data), len(body))
    out += b"\0\0\0\0"
    off = 0
    out += struct.pack("<I", 0)
    for s in strings:
        off += len(s.encode())
        out += struct.pack("<I", off)
    out += data + body
    out[28:32] = struct.pack("<I", sply_py.crc32c(bytes(out[32:])))
    return bytes(out)


class SharedFixtures(unittest.TestCase):
    def test_valid_fixtures_decode_to_the_declared_tree(self):
        for case in MANIFEST["valid"]:
            data = (FIXTURES / case["file"]).read_bytes()
            expected = json.loads(case["expectedText"])
            got = sply_py.decode_sply(data)
            # json.dumps keeps int/float token kinds and -0.0, so text equality is a strict tree comparison
            # (keys sorted: the manifest text is serialised in sorted-key order).
            self.assertEqual(json.dumps(got, sort_keys=True), json.dumps(expected, sort_keys=True), case["file"])

    def test_malformed_fixtures_are_rejected(self):
        for case in MANIFEST["malformed"]:
            data = (FIXTURES / case["file"]).read_bytes()
            with self.assertRaises(sply_py.SplyError, msg=case["file"]) as ctx:
                sply_py.decode_sply(data)
            self.assertTrue(str(ctx.exception).startswith("sply: "), case["file"])


class Parity(unittest.TestCase):
    def test_tag9_depth_limit(self):
        floats = bytes([9, 2]) + bytes(16)
        self.assertEqual(len(sply_py.decode_sply(build(bytes([7, 1]) * 127 + floats))), 1)
        with self.assertRaisesRegex(sply_py.SplyError, "nesting too deep"):
            sply_py.decode_sply(build(bytes([7, 1]) * 128 + floats))

    def test_negative_integer_range(self):
        self.assertEqual(sply_py.decode_sply(build(bytes([4]) + b"\xff" * 8 + b"\x7f")), -(1 << 63))
        with self.assertRaisesRegex(sply_py.SplyError, "negative integer out of range"):
            sply_py.decode_sply(build(bytes([4]) + b"\x80" * 9 + b"\x01"))  # 2^63 -> -2^63-1

    def test_container_claims_are_bounded_by_the_body(self):
        body = bytearray([7, 30, 7, 30, 7, 30])
        body.extend(bytes(34))
        with self.assertRaisesRegex(sply_py.SplyError, "count exceeds remaining bytes"):
            sply_py.decode_sply(build(bytes(body)))
        self.assertEqual(len(sply_py.decode_sply(build(bytes([7, 0xB8, 0x17]) + bytes(3000)))), 3000)

    def test_string_expansion_limit(self):
        big = "x" * 100_000
        body = bytes([7, 0xB8, 0x17]) + bytes([6, 0]) * 3000
        with self.assertRaisesRegex(sply_py.SplyError, "string expansion limit exceeded"):
            sply_py.decode_sply(build(body, [big]))

    def test_base64_must_be_canonical_and_bounded(self):
        for name in sorted(p.name for p in (FIXTURES / "valid").glob("*.sply")):
            raw = (FIXTURES / "valid" / name).read_bytes()
            if len(raw) % 3 == 0:
                continue
            try:
                sply_py.decode_sply(raw)
            except sply_py.SplyError:
                continue
            text = base64.b64encode(raw).decode()
            sply_py.decode_sply_base64(text)
            at = len(text) - (3 if text.endswith("==") else 2)
            alphabet = sply_py._B64_ALPHABET
            forged = text[:at] + alphabet[alphabet.index(text[at]) | 1] + text[at + 1:]
            self.assertEqual(base64.b64decode(forged), raw)  # lenient decoders accept it
            with self.assertRaisesRegex(sply_py.SplyError, "invalid base64"):
                sply_py.decode_sply_base64(forged)
            break
        else:
            self.fail("no padded fixture")
        with self.assertRaisesRegex(sply_py.SplyError, "64 MiB"):
            sply_py.decode_sply_base64("A" * (sply_py.MAX_BASE64_TEXT + 4))

    def test_metadata_text_includes_descendant_text(self):
        raw = (FIXTURES / "valid" / "scalars.sply").read_bytes()
        text = base64.b64encode(raw).decode()
        half = len(text) // 2
        el = ET.fromstring(
            '<metadata data-encoding="sply-base64">%s<b>%s</b></metadata>' % (text[:half], text[half:])
        )
        self.assertEqual(sply_py.project_from_metadata(el), [None, False, True])
        el = ET.fromstring('<metadata>{"a":<b>1</b>}</metadata>')
        self.assertEqual(sply_py.project_from_metadata(el), {"a": 1})


if __name__ == "__main__":
    unittest.main()
