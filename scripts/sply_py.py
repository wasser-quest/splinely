"""Dependency-free SPLY (docs/sply-binary-format.md, v1.0) reader for repo scripts.

    decode_sply(bytes) -> JSON value tree      (raises SplyError, a ValueError)
    project_from_metadata(element) -> dict     (<metadata id="splinely-project">: sply-base64 or legacy JSON)

Limits match the Rust and JS decoders (docs/sply-binary-format.md, "Hostile-input requirements"): the sum of all
container claims <= body length, <= MAX_NODES values, total decoded string bytes <= min(16 x file size + 64 KiB, 256 MiB).
    load_project_file(path) -> dict            (binary .sply or JSON text, sniffed by magic)
"""
import base64
import json
import re
import struct

MAGIC = b"\x89SPLY\r\n\x1a"
HEADER = 32
MAX_DEPTH = 128
MAX_FILE = 64 * 1024 * 1024
MAX_NODES = 1 << 24
EXPANSION_FACTOR = 16
EXPANSION_SLACK = 64 * 1024
EXPANSION_CAP = 256 * 1024 * 1024
MAX_BASE64_TEXT = -(-MAX_FILE // 3) * 4  # base64 of the file limit, whitespace included
_B64_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"


class SplyError(ValueError):
    pass


def _fail(msg):
    raise SplyError("sply: " + msg)


_TABLE = []
for _i in range(256):
    _c = _i
    for _ in range(8):
        _c = (_c >> 1) ^ 0x82F63B78 if _c & 1 else _c >> 1
    _TABLE.append(_c)


def crc32c(data):
    crc = 0xFFFFFFFF
    t = _TABLE
    for b in data:
        crc = t[(crc ^ b) & 255] ^ (crc >> 8)
    return crc ^ 0xFFFFFFFF


def is_sply(data):
    return bytes(data[:8]) == MAGIC


def decode_sply(data):
    data = bytes(data)
    n_total = len(data)
    if n_total > MAX_FILE:
        _fail("file exceeds 64 MiB limit")
    if n_total < HEADER:
        _fail("truncated header")
    if data[:8] != MAGIC:
        _fail("bad magic")
    major, minor, flags, n, strings_len, body_len, crc = struct.unpack_from("<HHIIIII", data, 8)
    if major != 1:
        _fail("unsupported major version")
    if minor > 0:
        _fail("unsupported minor version")
    if flags != 0:
        _fail("unsupported flags")
    offsets_len = 4 * (n + 1)
    if HEADER + offsets_len + strings_len + body_len != n_total:
        _fail("size mismatch")
    if crc32c(memoryview(data)[HEADER:]) != crc:
        _fail("crc mismatch")
    offsets = struct.unpack_from("<%dI" % (n + 1), data, HEADER)
    s0 = HEADER + offsets_len
    if offsets[0] != 0 or offsets[-1] != strings_len or any(a > b for a, b in zip(offsets, offsets[1:])):
        _fail("invalid string offsets")
    strings = []
    for i in range(n):
        try:
            strings.append(data[s0 + offsets[i]: s0 + offsets[i + 1]].decode("utf-8"))
        except UnicodeDecodeError:
            _fail("invalid utf-8")
    if len(set(strings)) != n:
        _fail("duplicate string")
    string_lens = [offsets[i + 1] - offsets[i] for i in range(n)]  # UTF-8 byte lengths
    pos = s0 + strings_len
    end = n_total
    body_start = pos
    budget = min(n_total * EXPANSION_FACTOR + EXPANSION_SLACK, EXPANSION_CAP)  # decoded string bytes still allowed
    claimed = 0  # sum of the minimum body bytes claimed by all container headers (<= body length)
    nodes = 0

    def spend(nbytes):
        nonlocal budget
        budget -= nbytes
        if budget < 0:
            _fail("string expansion limit exceeded")

    def varint():
        nonlocal pos
        result = 0
        for i in range(10):
            if pos >= end:
                _fail("truncated body")
            c = data[pos]
            pos += 1
            result |= (c & 0x7F) << (7 * i)
            if not c & 0x80:
                if i > 0 and (c & 0x7F) == 0:
                    _fail("non-minimal varint")
                if result >= 1 << 64:
                    _fail("varint overflow")
                return result
        _fail("varint too long")

    def count(unit, claim=True):
        nonlocal claimed
        c = varint()
        need = c * unit
        if need > end - pos:
            _fail("count exceeds remaining bytes")
        if claim:
            if claimed + need > end - body_start:
                _fail("count exceeds remaining bytes")
            claimed += need
        return c

    def index():
        i = varint()
        if i >= n:
            _fail("invalid string index")
        spend(string_lens[i])
        return i

    def flt():
        nonlocal pos
        if end - pos < 8:
            _fail("truncated body")
        (v,) = struct.unpack_from("<d", data, pos)
        pos += 8
        if v != v or v in (float("inf"), float("-inf")):
            _fail("non-finite float")
        return v

    def value(depth):
        nonlocal pos, nodes
        if pos >= end:
            _fail("truncated body")
        nodes += 1
        if nodes > MAX_NODES:
            _fail("too many values")
        tag = data[pos]
        pos += 1
        if tag == 0:
            return None
        if tag == 1:
            return False
        if tag == 2:
            return True
        if tag == 3:
            return varint()
        if tag == 4:
            v = varint()
            if v > (1 << 63) - 1:
                _fail("negative integer out of range")
            return -v - 1
        if tag == 5:
            return flt()
        if tag == 6:
            return strings[index()]
        if tag == 7:
            if depth >= MAX_DEPTH:
                _fail("nesting too deep")
            return [value(depth + 1) for _ in range(count(1))]
        if tag == 8:
            if depth >= MAX_DEPTH:
                _fail("nesting too deep")
            out = {}
            for _ in range(count(2)):
                k = strings[index()]
                if k in out:
                    _fail("duplicate object key")
                out[k] = value(depth + 1)
            return out
        if tag == 9:
            if depth >= MAX_DEPTH:
                _fail("nesting too deep")
            return [flt() for _ in range(count(8))]
        if tag == 10:
            c = count(1, claim=False)
            spend(-(-c // 3) * 4)
            raw = data[pos: pos + c]
            pos += c
            return base64.b64encode(raw).decode("ascii")
        _fail("unknown tag %d" % tag)

    result = value(0)
    if pos != end:
        _fail("trailing bytes in body")
    return result


def decode_sply_base64(text):
    """Standard padded base64 of a whole .sply file (whitespace allowed); canonical encodings only."""
    if len(text) > MAX_BASE64_TEXT:  # before any copy
        _fail("file exceeds 64 MiB limit")
    clean = "".join(text.split())
    if len(clean) % 4 != 0 or not re.fullmatch(r"[A-Za-z0-9+/]*={0,2}", clean):
        _fail("invalid base64")
    # The unused trailing bits of a padded final group must be zero (as in Rust).
    if clean.endswith("="):
        two = clean.endswith("==")
        if _B64_ALPHABET.index(clean[-3 if two else -2]) & (0x0F if two else 0x03):
            _fail("invalid base64")
    try:
        raw = base64.b64decode(clean, validate=True)
    except ValueError:
        _fail("invalid base64")
    return decode_sply(raw)


def project_from_metadata(element):
    """Project dict from a <metadata id="splinely-project"> ElementTree element (or anything with .get/.text)."""
    encoding = element.get("data-encoding")
    text = "".join(element.itertext())  # all descendant text, like Rust and JS
    if encoding == "sply-base64":
        return decode_sply_base64(text)
    if encoding in (None, "", "json"):
        return json.loads(text)
    raise SplyError("unsupported splinely-project data-encoding: %s" % encoding)


def load_project_file(path):
    with open(path, "rb") as f:
        raw = f.read()
    return decode_sply(raw) if is_sply(raw) else json.loads(raw.decode("utf-8"))
