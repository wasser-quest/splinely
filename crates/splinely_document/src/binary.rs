//! Binary `.sply` encoding of the canonical project JSON value tree.
//! Spec: `docs/sply-binary-format.md`. The decoder treats input as hostile:
//! it never panics, never pre-sizes a buffer from an untrusted count, bounds
//! the total number of claimed container items by the body length and the
//! number of values by `SPLY_MAX_NODES`, and bounds the total size of
//! materialised strings.
use crate::project_file::MAX_PROJECT_FILE_BYTES;
use crate::tree::{Entry, Node, Tree, ROOT};
use crate::{json_loading, DocumentError, Project};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

type Result<T> = std::result::Result<T, DocumentError>;

/// `\x89SPLY\r\n\x1a`
pub const SPLY_MAGIC: [u8; 8] = [0x89, b'S', b'P', b'L', b'Y', b'\r', b'\n', 0x1a];
pub const SPLY_MAJOR: u16 = 1;
pub const SPLY_MINOR: u16 = 0;
const HEADER_LEN: usize = 32;
/// Maximum container nesting depth (the root container has depth 1).
pub const SPLY_MAX_DEPTH: usize = 128;
/// Decoded strings (keys, values, base64 text) may total at most this many
/// bytes per input byte (plus a small constant), capped absolutely. Real
/// projects expand by well under 8x; this stops "string table reference
/// amplification" attacks.
const EXPANSION_FACTOR: u64 = 16;
const EXPANSION_SLACK: u64 = 64 * 1024;
const EXPANSION_CAP: u64 = 256 * 1024 * 1024;
/// Maximum number of values (nodes) in one document. Every value takes at
/// least one body byte, but a node costs ~28 bytes of memory, so a 64 MiB body
/// of `null`s would otherwise need ~1.9 GB.
pub const SPLY_MAX_NODES: u64 = 1 << 24;
/// Longest accepted `sply-base64` text (whitespace included): base64 of the
/// 64 MiB file limit. Checked before the text is cleaned or decoded.
pub(crate) const MAX_BASE64_TEXT: usize = (MAX_PROJECT_FILE_BYTES as usize).div_ceil(3) * 4;
/// Most values a document may have when it needs the reference loading
/// pipeline (a `Value` tree); real assets have under 100000 values in total.
pub const FALLBACK_MAX_VALUES: usize = 1 << 20;
/// Upper bound for table-size reservations made from untrusted header counts.
const MAX_PREALLOC: usize = 1 << 16;

const TAG_NULL: u8 = 0;
const TAG_FALSE: u8 = 1;
const TAG_TRUE: u8 = 2;
const TAG_UINT: u8 = 3;
const TAG_NEGINT: u8 = 4;
const TAG_FLOAT: u8 = 5;
const TAG_STRING: u8 = 6;
const TAG_ARRAY: u8 = 7;
const TAG_OBJECT: u8 = 8;
const TAG_FLOATS: u8 = 9;
const TAG_BASE64: u8 = 10;

fn fail(message: impl std::fmt::Display) -> DocumentError {
    DocumentError(format!("sply: {message}"))
}

// ---------------------------------------------------------------- CRC32C

const fn crc_tables() -> [[u32; 256]; 8] {
    let mut tables = [[0u32; 256]; 8];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0x82F6_3B78
            } else {
                crc >> 1
            };
            bit += 1;
        }
        tables[0][i] = crc;
        i += 1;
    }
    let mut t = 1;
    while t < 8 {
        let mut i = 0;
        while i < 256 {
            let prev = tables[t - 1][i];
            tables[t][i] = (prev >> 8) ^ tables[0][(prev & 0xff) as usize];
            i += 1;
        }
        t += 1;
    }
    tables
}
static CRC_TABLES: [[u32; 256]; 8] = crc_tables();

/// CRC-32C (Castagnoli), init and xor-out `0xFFFFFFFF`.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn crc32c(data: &[u8]) -> u32 {
    let t = &CRC_TABLES;
    let mut crc = !0u32;
    let mut chunks = data.chunks_exact(8);
    for c in &mut chunks {
        let lo = u32::from_le_bytes([c[0], c[1], c[2], c[3]]) ^ crc;
        let hi = u32::from_le_bytes([c[4], c[5], c[6], c[7]]);
        crc = t[7][(lo & 0xff) as usize]
            ^ t[6][((lo >> 8) & 0xff) as usize]
            ^ t[5][((lo >> 16) & 0xff) as usize]
            ^ t[4][(lo >> 24) as usize]
            ^ t[3][(hi & 0xff) as usize]
            ^ t[2][((hi >> 8) & 0xff) as usize]
            ^ t[1][((hi >> 16) & 0xff) as usize]
            ^ t[0][(hi >> 24) as usize];
    }
    for &b in chunks.remainder() {
        crc = (crc >> 8) ^ t[0][((crc ^ b as u32) & 0xff) as usize];
    }
    !crc
}

// ---------------------------------------------------------------- base64

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Canonical padded standard base64 → bytes (`None` if not canonical or empty).
#[allow(clippy::chunks_exact_to_as_chunks)]
pub(crate) fn base64_decode_canonical(text: &str) -> Option<Vec<u8>> {
    let b = text.as_bytes();
    if b.is_empty() || !b.len().is_multiple_of(4) {
        return None;
    }
    let digit = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a') as u32 + 26),
            b'0'..=b'9' => Some((c - b'0') as u32 + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(b.len() / 4 * 3);
    let last = b.len() / 4 - 1;
    for (i, quad) in b.chunks_exact(4).enumerate() {
        let pad = if i == last {
            quad.iter().rev().take_while(|&&c| c == b'=').count()
        } else {
            0
        };
        if pad > 2 {
            return None;
        }
        let mut n = 0u32;
        for (j, &c) in quad.iter().enumerate() {
            let d = if j >= 4 - pad { 0 } else { digit(c)? };
            n = n << 6 | d;
        }
        match pad {
            0 => out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8]),
            1 => {
                if n & 0xff != 0 {
                    return None;
                }
                out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8]);
            }
            _ => {
                if n & 0xffff != 0 {
                    return None;
                }
                out.push((n >> 16) as u8);
            }
        }
    }
    Some(out)
}

// ---------------------------------------------------------------- encoder

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

struct Encoder<'a> {
    ids: HashMap<&'a str, u32>,
    strings: Vec<&'a str>,
    strings_len: u64,
    nodes: u64,
    body: Vec<u8>,
}

impl<'a> Encoder<'a> {
    fn intern(&mut self, s: &'a str) -> Result<u64> {
        if let Some(&id) = self.ids.get(s) {
            return Ok(id as u64);
        }
        let id = u32::try_from(self.strings.len()).map_err(|_| fail("too many strings"))?;
        self.strings_len += s.len() as u64;
        if self.strings_len > MAX_PROJECT_FILE_BYTES {
            return Err(fail("project exceeds 64 MiB limit"));
        }
        self.ids.insert(s, id);
        self.strings.push(s);
        Ok(id as u64)
    }

    fn value(&mut self, value: &'a Value, key: Option<&str>, depth: usize) -> Result<()> {
        self.nodes += 1;
        if self.nodes > SPLY_MAX_NODES {
            return Err(fail("too many values"));
        }
        match value {
            Value::Null => self.body.push(TAG_NULL),
            Value::Bool(false) => self.body.push(TAG_FALSE),
            Value::Bool(true) => self.body.push(TAG_TRUE),
            Value::Number(n) => {
                if let Some(u) = n.as_u64() {
                    self.body.push(TAG_UINT);
                    put_varint(&mut self.body, u);
                } else if let Some(i) = n.as_i64() {
                    self.body.push(TAG_NEGINT);
                    put_varint(&mut self.body, !i as u64);
                } else {
                    let f = n
                        .as_f64()
                        .filter(|f| f.is_finite())
                        .ok_or_else(|| fail("non-finite float"))?;
                    self.body.push(TAG_FLOAT);
                    self.body.extend_from_slice(&f.to_le_bytes());
                }
            }
            Value::String(s) => {
                if key == Some("dataBase64") {
                    if let Some(bytes) = base64_decode_canonical(s) {
                        self.body.push(TAG_BASE64);
                        put_varint(&mut self.body, bytes.len() as u64);
                        self.body.extend_from_slice(&bytes);
                        return Ok(());
                    }
                }
                let id = self.intern(s)?;
                self.body.push(TAG_STRING);
                put_varint(&mut self.body, id);
            }
            Value::Array(items) => {
                if depth + 1 > SPLY_MAX_DEPTH {
                    return Err(fail("nesting too deep"));
                }
                if items.len() >= 2
                    && items
                        .iter()
                        .all(|v| matches!(v, Value::Number(n) if n.is_f64()))
                {
                    self.body.push(TAG_FLOATS);
                    put_varint(&mut self.body, items.len() as u64);
                    for item in items {
                        let f = item.as_f64().unwrap_or(0.0);
                        self.body.extend_from_slice(&f.to_le_bytes());
                    }
                } else {
                    self.body.push(TAG_ARRAY);
                    put_varint(&mut self.body, items.len() as u64);
                    for item in items {
                        self.value(item, None, depth + 1)?;
                    }
                }
            }
            Value::Object(map) => {
                if depth + 1 > SPLY_MAX_DEPTH {
                    return Err(fail("nesting too deep"));
                }
                self.body.push(TAG_OBJECT);
                put_varint(&mut self.body, map.len() as u64);
                for (k, v) in map {
                    let id = self.intern(k)?;
                    put_varint(&mut self.body, id);
                    self.value(v, Some(k), depth + 1)?;
                }
            }
        }
        if self.body.len() as u64 > MAX_PROJECT_FILE_BYTES {
            return Err(fail("project exceeds 64 MiB limit"));
        }
        Ok(())
    }
}

/// Encode a JSON value tree as a complete binary `.sply` file.
pub fn encode_value(value: &Value) -> Result<Vec<u8>> {
    let mut encoder = Encoder {
        ids: HashMap::new(),
        strings: Vec::new(),
        strings_len: 0,
        nodes: 0,
        body: Vec::new(),
    };
    encoder.value(value, None, 0)?;
    let n = encoder.strings.len();
    let total =
        HEADER_LEN as u64 + 4 * (n as u64 + 1) + encoder.strings_len + encoder.body.len() as u64;
    if total > MAX_PROJECT_FILE_BYTES {
        return Err(fail("project exceeds 64 MiB limit"));
    }
    let mut out = Vec::with_capacity(total as usize);
    out.extend_from_slice(&SPLY_MAGIC);
    out.extend_from_slice(&SPLY_MAJOR.to_le_bytes());
    out.extend_from_slice(&SPLY_MINOR.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(n as u32).to_le_bytes());
    out.extend_from_slice(&(encoder.strings_len as u32).to_le_bytes());
    out.extend_from_slice(&(encoder.body.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]); // crc placeholder
    let mut offset = 0u32;
    out.extend_from_slice(&offset.to_le_bytes());
    for s in &encoder.strings {
        offset += s.len() as u32;
        out.extend_from_slice(&offset.to_le_bytes());
    }
    for s in &encoder.strings {
        out.extend_from_slice(s.as_bytes());
    }
    out.extend_from_slice(&encoder.body);
    let crc = crc32c(&out[HEADER_LEN..]);
    out[28..32].copy_from_slice(&crc.to_le_bytes());
    Ok(out)
}

// ---------------------------------------------------------------- decoder

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    strings: Vec<&'a str>,
    budget: u64,
    tree: Tree<'a>,
    max_nodes: u64,
    /// Sum of the minimum body bytes claimed by every container header so far.
    /// A valid body never claims more than its own length (claims are disjoint
    /// byte ranges), so this bounds the total slots of all containers.
    claimed: u64,
    /// Children of the containers being parsed (stack discipline); copied into
    /// the arena only once a container is complete.
    scratch_kids: Vec<u32>,
    scratch_entries: Vec<Entry<'a>>,
    scratch_peak: usize,
}

/// Work counters of one decode, for regression tests of the resource bounds.
#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct DecodeStats {
    pub nodes: u64,
    pub claimed: u64,
    pub scratch_peak: usize,
    pub body_len: u64,
}

impl<'a> Parser<'a> {
    fn remaining(&self) -> u64 {
        (self.bytes.len() - self.pos) as u64
    }
    fn byte(&mut self) -> Result<u8> {
        let b = *self
            .bytes
            .get(self.pos)
            .ok_or_else(|| fail("unexpected end of body"))?;
        self.pos += 1;
        Ok(b)
    }
    fn take(&mut self, n: u64) -> Result<&'a [u8]> {
        if n > self.remaining() {
            return Err(fail("unexpected end of body"));
        }
        let slice = &self.bytes[self.pos..self.pos + n as usize];
        self.pos += n as usize;
        Ok(slice)
    }
    fn varint(&mut self) -> Result<u64> {
        let mut value = 0u64;
        for i in 0..10 {
            let b = self.byte()?;
            if i == 9 && b > 1 {
                return Err(fail("varint overflow"));
            }
            value |= ((b & 0x7f) as u64) << (7 * i);
            if b & 0x80 == 0 {
                if i > 0 && b == 0 {
                    return Err(fail("non-minimal varint"));
                }
                return Ok(value);
            }
        }
        Err(fail("varint overflow"))
    }
    fn spend(&mut self, n: usize) -> Result<()> {
        self.budget = self
            .budget
            .checked_sub(n as u64)
            .ok_or_else(|| fail("string expansion limit exceeded"))?;
        Ok(())
    }
    fn string(&mut self) -> Result<(u32, &'a str)> {
        let idx = self.varint()?;
        let (i, s) = usize::try_from(idx)
            .ok()
            .and_then(|i| self.strings.get(i).map(|s| (i, *s)))
            .ok_or_else(|| fail("invalid string index"))?;
        self.spend(s.len())?;
        Ok((i as u32, s))
    }
    /// Read an item count. `claim` containers must account for their items in
    /// the parser-wide `claimed` budget; nothing is allocated from the count.
    fn count(&mut self, min_item_bytes: u64, claim: bool) -> Result<usize> {
        let count = self.varint()?;
        match count.checked_mul(min_item_bytes) {
            Some(need) if need <= self.remaining() => {
                if claim {
                    let total = self.claimed + need;
                    if total > self.bytes.len() as u64 {
                        return Err(fail("count exceeds remaining bytes"));
                    }
                    self.claimed = total;
                }
                Ok(count as usize)
            }
            _ => Err(fail("count exceeds remaining bytes")),
        }
    }
    fn note_scratch(&mut self) {
        self.scratch_peak = self
            .scratch_peak
            .max(self.scratch_kids.len() + self.scratch_entries.len());
    }
    fn float(&mut self) -> Result<f64> {
        let raw = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(raw);
        let f = f64::from_le_bytes(a);
        if f.is_finite() {
            Ok(f)
        } else {
            Err(fail("non-finite float"))
        }
    }
    /// Parse one value into the arena and return its node id (root is 0).
    fn value(&mut self, depth: usize) -> Result<u32> {
        if self.tree.nodes.len() as u64 >= self.max_nodes {
            return Err(fail("too many values"));
        }
        let node = match self.byte()? {
            TAG_NULL => Node::Null,
            TAG_FALSE => Node::Bool(false),
            TAG_TRUE => Node::Bool(true),
            TAG_UINT => Node::U64(self.varint()?),
            TAG_NEGINT => {
                let v = self.varint()?;
                if v > i64::MAX as u64 {
                    return Err(fail("negative integer out of range"));
                }
                Node::I64(!(v as i64))
            }
            TAG_FLOAT => Node::F64(self.float()?),
            TAG_STRING => Node::Str(self.string()?.1),
            TAG_ARRAY => {
                if depth + 1 > SPLY_MAX_DEPTH {
                    return Err(fail("nesting too deep"));
                }
                let count = self.count(1, true)?;
                let id = self.tree.push(Node::Null);
                let base = self.scratch_kids.len();
                for _ in 0..count {
                    let child = self.value(depth + 1)?;
                    self.scratch_kids.push(child);
                    self.note_scratch();
                }
                let start = self.tree.kids.len();
                self.tree.kids.extend(self.scratch_kids.drain(base..));
                self.tree.nodes[id as usize] = Node::Array {
                    start: start as u32,
                    len: count as u32,
                };
                return Ok(id);
            }
            TAG_OBJECT => {
                if depth + 1 > SPLY_MAX_DEPTH {
                    return Err(fail("nesting too deep"));
                }
                let count = self.count(2, true)?;
                let id = self.tree.push(Node::Null);
                let base = self.scratch_entries.len();
                // Keys are unique string-table indices, so a duplicate key is a
                // repeated index. Small objects scan, large ones hash.
                let mut large: Option<HashSet<u32>> = (count > 16).then(HashSet::new);
                for i in 0..count {
                    let (idx, key) = self.string()?;
                    let node = self.value(depth + 1)?;
                    let duplicate = match &mut large {
                        Some(seen) => !seen.insert(idx),
                        None => self.scratch_entries[base..base + i]
                            .iter()
                            .any(|e| e.idx == idx),
                    };
                    if duplicate {
                        return Err(fail("duplicate object key"));
                    }
                    self.scratch_entries.push(Entry { key, idx, node });
                    self.note_scratch();
                }
                let start = self.tree.entries.len();
                self.tree.entries.extend(self.scratch_entries.drain(base..));
                self.tree.nodes[id as usize] = Node::Object {
                    start: start as u32,
                    len: count as u32,
                };
                return Ok(id);
            }
            TAG_FLOATS => {
                if depth + 1 > SPLY_MAX_DEPTH {
                    return Err(fail("nesting too deep"));
                }
                let count = self.count(8, true)?;
                let raw = self.take(count as u64 * 8)?;
                if raw
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .any(|c| !f64::from_le_bytes(*c).is_finite())
                {
                    return Err(fail("non-finite float"));
                }
                Node::Floats(raw)
            }
            TAG_BASE64 => {
                let len = self.varint()?;
                let raw = self.take(len)?;
                self.spend(raw.len().div_ceil(3) * 4)?;
                self.tree.owned.push(base64_encode(raw));
                Node::Owned((self.tree.owned.len() - 1) as u32)
            }
            _ => return Err(fail("unknown tag")),
        };
        Ok(self.tree.push(node))
    }
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    let mut a = [0u8; 4];
    a.copy_from_slice(&bytes[at..at + 4]);
    u32::from_le_bytes(a)
}

/// Validate the container and parse the body into an arena. Hostile-safe.
fn decode_to_tree(bytes: &[u8]) -> Result<Tree<'_>> {
    decode_to_tree_with(bytes, SPLY_MAX_NODES, &mut DecodeStats::default())
}

fn decode_to_tree_with<'a>(
    bytes: &'a [u8],
    max_nodes: u64,
    stats: &mut DecodeStats,
) -> Result<Tree<'a>> {
    if !bytes.starts_with(&SPLY_MAGIC) {
        return Err(fail("bad magic"));
    }
    if bytes.len() < HEADER_LEN {
        return Err(fail("truncated header"));
    }
    if bytes.len() as u64 > MAX_PROJECT_FILE_BYTES {
        return Err(fail("file exceeds 64 MiB limit"));
    }
    let major = u16::from_le_bytes([bytes[8], bytes[9]]);
    let minor = u16::from_le_bytes([bytes[10], bytes[11]]);
    if major != SPLY_MAJOR {
        return Err(fail(format!("unsupported major version {major}")));
    }
    if minor > SPLY_MINOR {
        return Err(fail(format!("unsupported minor version {minor}")));
    }
    if read_u32(bytes, 12) != 0 {
        return Err(fail("unsupported flags"));
    }
    let count = read_u32(bytes, 16) as u64;
    let strings_len = read_u32(bytes, 20) as u64;
    let body_len = read_u32(bytes, 24) as u64;
    let crc = read_u32(bytes, 28);
    let expected = HEADER_LEN as u64 + 4 * (count + 1) + strings_len + body_len;
    if expected != bytes.len() as u64 {
        return Err(fail("size mismatch"));
    }
    if crc32c(&bytes[HEADER_LEN..]) != crc {
        return Err(fail("crc mismatch"));
    }
    let offsets_at = HEADER_LEN;
    let data_at = offsets_at + 4 * (count as usize + 1);
    let body_at = data_at + strings_len as usize;
    let data = &bytes[data_at..body_at];
    let offset = |i: usize| read_u32(bytes, offsets_at + 4 * i) as u64;
    if offset(0) != 0 || offset(count as usize) != strings_len {
        return Err(fail("invalid string offsets"));
    }
    // Never reserve from the untrusted header count beyond a small constant.
    let reserve = (count as usize).min(MAX_PREALLOC);
    let mut strings = Vec::with_capacity(reserve);
    let mut seen: HashSet<&str> = HashSet::with_capacity(reserve);
    for i in 0..count as usize {
        let (start, end) = (offset(i), offset(i + 1));
        if start > end || end > strings_len {
            return Err(fail("invalid string offsets"));
        }
        let s = std::str::from_utf8(&data[start as usize..end as usize])
            .map_err(|_| fail("invalid utf-8 in string table"))?;
        if !seen.insert(s) {
            return Err(fail("duplicate string in table"));
        }
        strings.push(s);
    }
    drop(seen);
    let budget = (bytes.len() as u64 * EXPANSION_FACTOR + EXPANSION_SLACK).min(EXPANSION_CAP);
    let mut parser = Parser {
        bytes: &bytes[body_at..],
        pos: 0,
        strings,
        budget,
        tree: Tree::default(),
        max_nodes,
        claimed: 0,
        scratch_kids: Vec::new(),
        scratch_entries: Vec::new(),
        scratch_peak: 0,
    };
    let parsed = parser.value(0);
    *stats = DecodeStats {
        nodes: parser.tree.nodes.len() as u64,
        claimed: parser.claimed,
        scratch_peak: parser.scratch_peak,
        body_len,
    };
    parsed?;
    if parser.pos != parser.bytes.len() {
        return Err(fail("trailing bytes in body"));
    }
    Ok(parser.tree)
}

/// Decode a binary `.sply` file into its JSON value tree. Hostile-safe.
pub fn decode_to_value(bytes: &[u8]) -> Result<Value> {
    Ok(decode_to_tree(bytes)?.to_value(ROOT))
}

// ---------------------------------------------------------------- Project

impl Project {
    /// True if `bytes` starts with the binary `.sply` magic.
    pub fn is_binary(bytes: &[u8]) -> bool {
        bytes.starts_with(&SPLY_MAGIC)
    }

    /// Validate and encode as binary `.sply`.
    pub fn to_binary(&self) -> Result<Vec<u8>> {
        self.validate()?;
        encode_value(&serde_json::to_value(self)?)
    }

    /// Decode binary `.sply` with the same strictness as `from_json`
    /// (unknown fields, lossy numbers and `validate()` apply unchanged).
    ///
    /// Typed deserialization runs straight from the parsed arena and the
    /// preservation check compares against it without building `Value`s. When
    /// the fast path cannot prove success (including every error and every
    /// document `normalize_project_value` would rewrite) the reference
    /// pipeline reruns on an equivalent `Value`, so outcomes are identical.
    pub fn from_binary(bytes: &[u8]) -> Result<Self> {
        Ok(Self::from_binary_exact(bytes)?.0)
    }

    /// `from_binary`, plus whether `bytes` hold exactly the canonical members of
    /// the project (nothing normalized, defaulted or lost), so exports may embed
    /// `bytes` as they are.
    pub(crate) fn from_binary_exact(bytes: &[u8]) -> Result<(Self, bool)> {
        let tree = decode_to_tree(bytes)?;
        if let Some((project, complete)) = crate::fast_load(&tree) {
            project.validate()?;
            return Ok((project, complete));
        }
        // The reference pipeline needs a `Value` per arena node (several times the
        // arena); real documents that take it are tiny, hostile ones are not.
        if tree.nodes.len() > FALLBACK_MAX_VALUES {
            return Err(fail(format!(
                "document cannot be loaded strictly: more than {FALLBACK_MAX_VALUES} values need the slow path (unknown fields, wrong types or legacy data)"
            )));
        }
        let mut input = tree.to_value(ROOT);
        drop(tree);
        crate::normalize_project_value(&mut input);
        let project = Self::deserialize(&input)?;
        json_loading::check_preserved(&input, &serde_json::to_value(&project)?, "")?;
        project.validate()?;
        Ok((project, false))
    }

    /// Binary if the magic matches, otherwise UTF-8 JSON text. Input over
    /// `MAX_PROJECT_FILE_BYTES` is rejected before parsing.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() as u64 > MAX_PROJECT_FILE_BYTES {
            return Err(DocumentError("project file exceeds 64 MiB limit".into()));
        }
        if Self::is_binary(bytes) {
            return Self::from_binary(bytes);
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|_| DocumentError("stream did not contain valid UTF-8".into()))?;
        Self::from_json(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32c_check_value() {
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
        assert_eq!(crc32c(b""), 0);
        let long: Vec<u8> = (0..1000u32).map(|i| (i * 7) as u8).collect();
        let mut slow = !0u32;
        for &b in &long {
            slow ^= b as u32;
            for _ in 0..8 {
                slow = if slow & 1 != 0 {
                    (slow >> 1) ^ 0x82F6_3B78
                } else {
                    slow >> 1
                };
            }
        }
        assert_eq!(crc32c(&long), !slow);
    }

    #[test]
    fn base64_matches_rfc4648() {
        for (raw, text) in [
            (&b"f"[..], "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
        ] {
            assert_eq!(base64_encode(raw), text);
            assert_eq!(base64_decode_canonical(text).unwrap(), raw);
        }
        for bad in ["", "Zg", "Zh==", "Zm9=", "Zg=a", "=Zg=", "Z===", "Zg==Zg=="] {
            assert!(base64_decode_canonical(bad).is_none(), "{bad}");
        }
    }

    fn put(out: &mut Vec<u8>, v: u64) {
        put_varint(out, v);
    }

    /// Assemble a complete, CRC-valid file around hand-written strings and body.
    fn build_file(strings: &[&str], body: &[u8]) -> Vec<u8> {
        let strings_len: usize = strings.iter().map(|s| s.len()).sum();
        let mut out = Vec::new();
        out.extend_from_slice(&SPLY_MAGIC);
        out.extend_from_slice(&SPLY_MAJOR.to_le_bytes());
        out.extend_from_slice(&SPLY_MINOR.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(strings.len() as u32).to_le_bytes());
        out.extend_from_slice(&(strings_len as u32).to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        let mut offset = 0u32;
        out.extend_from_slice(&offset.to_le_bytes());
        for s in strings {
            offset += s.len() as u32;
            out.extend_from_slice(&offset.to_le_bytes());
        }
        for s in strings {
            out.extend_from_slice(s.as_bytes());
        }
        out.extend_from_slice(body);
        let crc = crc32c(&out[HEADER_LEN..]);
        out[28..32].copy_from_slice(&crc.to_le_bytes());
        out
    }

    /// ~1 MB body of nested containers whose headers each claim (almost) all of
    /// the remaining bytes. Before the fix every level pre-sized its slots from
    /// that count (~1.5 GiB of RSS at this size, linear in file size).
    fn nested_claims(tag: u8, per_item: u64, total: usize) -> Vec<u8> {
        let claim = total as u64 / per_item - 64;
        let mut body = Vec::new();
        for _ in 0..SPLY_MAX_DEPTH - 1 {
            body.push(tag);
            put(&mut body, claim);
            if tag == TAG_OBJECT {
                put(&mut body, 0); // key: string index 0
            }
        }
        body.resize(total, TAG_NULL);
        body
    }

    #[test]
    fn nested_container_claims_stay_bounded_and_are_rejected() {
        for (tag, per_item) in [(TAG_ARRAY, 1), (TAG_OBJECT, 2)] {
            let bytes = build_file(&["k"], &nested_claims(tag, per_item, 1 << 20));
            let mut stats = DecodeStats::default();
            let err = decode_to_tree_with(&bytes, SPLY_MAX_NODES, &mut stats)
                .err()
                .expect("hostile nesting must be rejected");
            assert!(err.0.contains("count exceeds remaining bytes"), "{err}");
            // Work done is tiny and independent of the claimed counts.
            assert!(stats.claimed <= stats.body_len, "{stats:?}");
            assert!(stats.nodes <= 4, "{stats:?}");
            assert!(stats.scratch_peak <= 4, "{stats:?}");
        }
        assert!(
            decode_to_value(&build_file(&["k"], &nested_claims(TAG_OBJECT, 2, 1 << 20))).is_err()
        );
    }

    #[test]
    fn honest_documents_never_exceed_their_claim_budget() {
        // Nested arrays/objects/floats: the claim sum must stay within the body.
        let value = serde_json::json!({
            "a": [[1, 2], {"b": [1.5, 2.5, 3.5], "c": null}, true, false, null],
            "d": {"e": {"f": [[], {}, [[]]]}},
        });
        let bytes = encode_value(&value).unwrap();
        let mut stats = DecodeStats::default();
        let tree = decode_to_tree_with(&bytes, SPLY_MAX_NODES, &mut stats).unwrap();
        assert_eq!(tree.to_value(ROOT), value);
        assert!(stats.claimed <= stats.body_len, "{stats:?}");
        assert_eq!(stats.nodes as usize, tree.nodes.len());
        // A body of only nulls is the densest legal array: claim == body - header.
        let mut body = vec![TAG_ARRAY];
        put(&mut body, 5000);
        body.resize(body.len() + 5000, TAG_NULL);
        let dense = build_file(&[], &body);
        let mut stats = DecodeStats::default();
        decode_to_tree_with(&dense, SPLY_MAX_NODES, &mut stats).unwrap();
        assert_eq!((stats.nodes, stats.claimed), (5001, 5000));
    }

    #[test]
    fn total_nodes_are_capped() {
        let mut body = vec![TAG_ARRAY];
        put(&mut body, 5000);
        body.resize(body.len() + 5000, TAG_NULL);
        let bytes = build_file(&[], &body);
        let mut stats = DecodeStats::default();
        let err = decode_to_tree_with(&bytes, 1000, &mut stats).err().unwrap();
        assert!(err.0.contains("too many values"), "{err}");
        assert_eq!(stats.nodes, 1000);
        assert!(decode_to_tree_with(&bytes, 5001, &mut stats).is_ok());
    }

    #[test]
    fn string_table_reservation_is_capped() {
        // A header claiming millions of strings but too few bytes is rejected
        // by the size equation before anything is reserved from the count.
        let mut bytes = build_file(&[], &[TAG_NULL]);
        bytes[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_to_value(&bytes).is_err());
    }
}
