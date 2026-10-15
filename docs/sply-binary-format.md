# SPLY binary format (v1.0)

Binary encoding of the **canonical Project JSON model** (`schemaVersion: 1`).
It is a lossless, structural encoding of exactly the JSON value tree that
`Project::to_json` produces: same field names, same defaults/omission rules,
same validation. It is *not* a second schema. Anything JSON-valid is
binary-encodable and vice versa; after decoding, the normal strict project
loading and `validate()` rules apply unchanged.

Goals: no text parsing, interned strings, raw audio bytes (no base64 text),
packed `f64` arrays (keyframes, points), bounds-checked decoding of hostile
input. Decoders exist in Rust (`splinely_document`) and JS
(`runtime/src/sply_decode.js`, no dependencies).

## Detection

A file/buffer is binary if it starts with the 8 magic bytes
`89 53 50 4C 59 0D 0A 1A` (`\x89SPLY\r\n\x1a`). Otherwise it is treated as
legacy JSON text. Extensions are only conventions (`.sply` is written as
binary by default; `.splinely.json`/`.json` remain text).

## Layout (all integers little-endian)

```
offset size field
0      8    magic  89 53 50 4C 59 0D 0A 1A
8      2    major  (= 1; decoders reject other majors)
10     2    minor  (= 0; decoders accept any minor <= their own, reject higher)
12     4    flags  (= 0; any set bit is rejected in v1)
16     4    string_count            (N)
20     4    strings_len             (bytes of string data)
24     4    body_len
28     4    crc32c of every byte from offset 32 to end of file (Castagnoli, init/xorout 0xFFFFFFFF)
32     4*(N+1)  u32 string offsets, relative to start of string data
              offsets[0] == 0, non-decreasing, offsets[N] == strings_len
...    strings_len  UTF-8 string data (each string valid UTF-8, no NUL requirement)
...    body_len     one value (see below), must consume body_len exactly
```

Total file size must equal `32 + 4*(N+1) + strings_len + body_len` exactly
(no trailing bytes). The whole file is limited to 64 MiB
(`MAX_PROJECT_FILE_BYTES`). All size arithmetic is checked (u64) and every
count is validated against the remaining bytes *before* any allocation.
The string table holds every string (object keys and string values), each
unique, in first-use order of a depth-first, object-member-order traversal.
Decoders only require uniqueness (reject duplicates) – not any ordering.

**Files are not required to be canonical.** The encoder's output is canonical
(first-use string order, tag 9/10 where applicable), but every decoder accepts
any structurally valid file: unused strings in the table, tag 9 arrays with 0 or
1 elements, an empty tag 10, tag 10 under keys other than `dataBase64`, and tag 7
arrays of floats all decode to the same tree. One project may therefore have
many byte-different files; do not use the file bytes as a content hash or
dedup key (hash the decoded canonical JSON instead).

## Value encoding (body)

One tag byte, then payload:

| tag | meaning | payload |
|----|---------|---------|
| 0 | null | – |
| 1 | false | – |
| 2 | true | – |
| 3 | unsigned int (JSON integer ≥ 0, ≤ 2^64-1) | LEB128 varint |
| 4 | negative int (JSON integer < 0, ≥ -2^63) | LEB128 varint of `(-value) - 1` |
| 5 | float (JSON number written with fraction/exponent, incl. `1.0`, `-0.0`) | 8 bytes IEEE-754 LE; must be finite |
| 6 | string | varint index into string table (< N) |
| 7 | array | varint count, then count values |
| 8 | object | varint count, then count × (varint key string index, value); keys unique |
| 9 | float array | varint count, then count × 8 bytes f64 LE (every element a finite JSON float, i.e. tag-5 semantics) |
| 10 | base64 blob | varint byte length, raw bytes; decodes to the **standard padded base64 string** (RFC 4648 §4) of those bytes |

Varints are minimal-length LEB128, at most 10 bytes; non-minimal encodings and
overflow are rejected. Max nesting depth: 128. The encoder emits tag 9 for
arrays of ≥ 2 elements that are all floats (tag 5 semantic), and tag 10 for
string values that are non-empty canonical padded standard base64 (and only
for the string field `dataBase64` of audio assets – other strings stay plain
strings, so the decoded tree is identical to the JSON tree).
Decoders must accept tag 9/10 anywhere a JSON array/string is valid and
produce the same tree as the plain encodings. Tag 9 counts as a container for the
depth limit (it is rejected at depth 129 like tags 7 and 8).

Integers vs floats are preserved, so `1` and `1.0` keep their JSON token kind
and the Rust strict loader (which rejects lossy int→float conversions) behaves
exactly as for JSON text.

## Integers beyond 2^53 in the browser

Tags 3/4 carry the full 64-bit range (`0 … 2^64-1`, `-2^63 … -1`), and the Rust
and Python decoders accept all of it. JavaScript has no 64-bit integers: the JS
decoder returns a `Number` and **rejects** (`sply: integer exceeds safe range`)
every integer with magnitude above 2^53 that is not exactly representable as a
double (2^60, 2^63 or -2^63 are accepted; 2^53 + 1 is not). It never rounds
silently. A valid `.sply` can therefore load with the CLI/native editor and fail
in the browser; keep such integers (ids, hashes) as strings in authored data.
"Decoders must produce the same tree" holds for every file the JS decoder accepts.

## Decoded result

The decoder yields the JSON value tree (Rust `serde_json::Value`-equivalent /
JS plain objects, arrays, numbers, strings). Rust then continues with the same
strict typed deserialisation and `validate()` as `Project::from_json`; JS
continues with its existing `normalize()`.

## Compatibility rules

* New optional fields stay optional / omitted when empty (JSON rules).
* A new **minor** may only add new tags ≥ 11 or new flags; decoders reject
  unknown tags/flags until updated. A new **major** is a breaking change.
* Legacy JSON text remains loadable by every host; CLI/convert can produce either.

## SVG embedding

`<metadata id="splinely-project" data-encoding="sply-base64">BASE64…</metadata>`
contains the standard-base64 of the whole binary file (including header and
CRC). Without `data-encoding` the content is legacy JSON text. The JS runtime
supports both. Bytes are not compressed in v1.

Embedded base64 must be canonical (padded, zero unused trailing bits; ASCII
whitespace is ignored) in every reader (Rust, JS, Python). The text, whitespace
included, may be at most `ceil(64 MiB / 3) * 4` = 89,478,488 characters; longer
text is rejected before it is copied, cleaned or decoded.

**Block lookup** is identical in the Rust reader (`Project::from_svg_metadata`),
the JS runtime (`readProject`): the block is an element in the SVG namespace
that is a **direct child of the root `svg`** and has `id="splinely-project"` or a
`data-splinely-project` attribute. There must be **exactly one** such element;
none is `missing-metadata`, several are `ambiguous-metadata` (never "first
wins"). Same-id elements in other namespaces, nested `svg`, `g`,
`foreignObject` and the like are ignored. The block text is all descendant text
of the element.

## Hostile-input requirements (both decoders)

Never panic/throw anything but a typed error; reject: wrong magic/major/flags,
size mismatch, CRC mismatch, offsets out of range/non-monotonic, invalid UTF-8,
duplicate strings or object keys, unknown tags, non-minimal varints,
non-finite floats, depth > 128 (tags 7, 8 and 9), counts larger than remaining
bytes, body bytes left over, invalid string index, negative integers below -2^63.

Resource rules, **identical in all three decoders** (Rust, JS, Python):

* No buffer is pre-sized from an untrusted count (header string count, container
  counts); reservations from header values are capped at a small constant and
  children are collected incrementally.
* **Claims:** the sum over all containers of `count × minimum item bytes` (tag 7:
  1, tag 8: 2, tag 9: 8) must not exceed the body length. Honest files always
  satisfy this (each claim covers a disjoint byte range of the body), while
  nested containers that each claim "almost everything that is left" are
  rejected at the second level (`count exceeds remaining bytes`).
* **Values:** at most 2^24 values (tags counted, every nested value included) per
  document (`too many values`); the encoder refuses larger trees.
* **String expansion:** the total byte length of every decoded string occurrence
  (each key and string value, counting a shared string once per reference; a tag-10
  blob counts `ceil(n/3) × 4`) must not exceed `min(16 × file_size + 64 KiB,
  256 MiB)` (`string expansion limit exceeded`).
* Base64 text limit and canonical form: see "SVG embedding".

Memory use is therefore bounded by file size × a small constant (a value costs
at most ~28 bytes of arena in Rust).
