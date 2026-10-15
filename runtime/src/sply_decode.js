/* SPLY binary project decoder (docs/sply-binary-format.md, v1.0). Dependency-free, ES2018-safe, BigInt-free.
 * Browser <script>: defines host.SplyDecode = {decodeSply,isSply,decodeSplyBase64,crc32c}.
 * Node/CommonJS: module.exports = the same object.
 * Every failure throws Error with a message starting with "sply:". The decoder never loops unboundedly:
 * all counts are validated against remaining bytes before any allocation, the sum of all container claims is bounded by
 * the body length, the number of values by MAX_NODES, and the total decoded string bytes by 16 x file size + 64 KiB
 * (capped at 256 MiB) -- the same limits as the Rust and Python decoders. */
(function (host) {
  'use strict';
  const MAX_FILE_BYTES = 64 * 1024 * 1024;
  const MAX_DEPTH = 128; // top-level container = depth 1; the 129th nested container is rejected
  const MAX_NODES = 1 << 24; // total values in one document (Rust: SPLY_MAX_NODES)
  const EXPANSION_FACTOR = 16, EXPANSION_SLACK = 64 * 1024, EXPANSION_CAP = 256 * 1024 * 1024;
  const MAX_BASE64_TEXT = Math.ceil(MAX_FILE_BYTES / 3) * 4; // base64 of the file limit, whitespace included
  const HEADER = 32;
  const MAGIC = [0x89, 0x53, 0x50, 0x4c, 0x59, 0x0d, 0x0a, 0x1a];
  const fail = msg => { throw new Error('sply: ' + msg); };

  // ---- CRC32C (Castagnoli, reflected poly 0x82F63B78) ----
  let crcTable = null;
  function crc32c(bytes, start, end) {
    if (!crcTable) {
      crcTable = new Int32Array(256);
      for (let i = 0; i < 256; i++) {
        let c = i;
        for (let k = 0; k < 8; k++) c = c & 1 ? (c >>> 1) ^ 0x82f63b78 : c >>> 1;
        crcTable[i] = c;
      }
    }
    let crc = -1;
    const t = crcTable;
    for (let i = start === undefined ? 0 : start, e = end === undefined ? bytes.length : end; i < e; i++) crc = t[(crc ^ bytes[i]) & 255] ^ (crc >>> 8);
    return (crc ^ -1) >>> 0;
  }

  function toBytes(input) {
    if (input instanceof Uint8Array) return input;
    if (typeof ArrayBuffer !== 'undefined') {
      if (input instanceof ArrayBuffer) return new Uint8Array(input);
      if (ArrayBuffer.isView(input)) return new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
      if (Object.prototype.toString.call(input) === '[object ArrayBuffer]') return new Uint8Array(input);
    }
    return fail('input must be a Uint8Array or ArrayBuffer');
  }

  function isSply(input) {
    let b;
    try { b = toBytes(input); } catch (e) { return false; }
    if (b.length < 8) return false;
    for (let i = 0; i < 8; i++) if (b[i] !== MAGIC[i]) return false;
    return true;
  }

  // ---- strict UTF-8 ----
  let strictDecoder; // undefined = not probed, null = unavailable
  function utf8Decoder() {
    if (strictDecoder === undefined) {
      try { strictDecoder = typeof TextDecoder === 'function' ? new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }) : null; } catch (e) { strictDecoder = null; }
    }
    return strictDecoder;
  }
  // Fallback validating decoder: rejects overlongs, surrogates, > U+10FFFF, truncated/stray bytes.
  function utf8Manual(b, s, e) {
    const units = [];
    let out = '';
    for (let i = s; i < e;) {
      const c = b[i++];
      let cp;
      if (c < 0x80) cp = c;
      else {
        let n, min;
        if (c >= 0xc2 && c <= 0xdf) { n = 1; cp = c & 0x1f; min = 0x80; }
        else if (c >= 0xe0 && c <= 0xef) { n = 2; cp = c & 0x0f; min = 0x800; }
        else if (c >= 0xf0 && c <= 0xf4) { n = 3; cp = c & 0x07; min = 0x10000; }
        else return fail('invalid UTF-8 in string');
        if (i + n > e) return fail('invalid UTF-8 in string');
        for (let k = 0; k < n; k++) {
          const d = b[i++];
          if ((d & 0xc0) !== 0x80) return fail('invalid UTF-8 in string');
          cp = (cp << 6) | (d & 0x3f);
        }
        if (cp < min || cp > 0x10ffff || (cp >= 0xd800 && cp <= 0xdfff)) return fail('invalid UTF-8 in string');
      }
      if (cp >= 0x10000) { cp -= 0x10000; units.push(0xd800 + (cp >> 10), 0xdc00 + (cp & 0x3ff)); }
      else units.push(cp);
      if (units.length >= 4096) { out += String.fromCharCode.apply(null, units); units.length = 0; }
    }
    return out + String.fromCharCode.apply(null, units);
  }
  function decodeUtf8(b, s, e, forceManual) {
    let ascii = true;
    for (let i = s; i < e; i++) if (b[i] > 0x7f) { ascii = false; break; }
    if (ascii) {
      if (e - s <= 4096) return String.fromCharCode.apply(null, b.subarray(s, e));
      let out = '';
      for (let i = s; i < e; i += 4096) out += String.fromCharCode.apply(null, b.subarray(i, Math.min(e, i + 4096)));
      return out;
    }
    const d = forceManual ? null : utf8Decoder();
    if (d) {
      try { return d.decode(b.subarray(s, e)); } catch (err) { return fail('invalid UTF-8 in string'); }
    }
    return utf8Manual(b, s, e);
  }

  // ---- base64 (standard, padded) ----
  const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  function base64Encode(b, s, e) {
    const parts = [];
    let out = '';
    let i = s;
    for (; i + 2 < e; i += 3) {
      const n = (b[i] << 16) | (b[i + 1] << 8) | b[i + 2];
      out += B64[n >> 18] + B64[(n >> 12) & 63] + B64[(n >> 6) & 63] + B64[n & 63];
      if (out.length >= 8192) { parts.push(out); out = ''; }
    }
    if (e - i === 1) { const n = b[i] << 16; out += B64[n >> 18] + B64[(n >> 12) & 63] + '=='; }
    else if (e - i === 2) { const n = (b[i] << 16) | (b[i + 1] << 8); out += B64[n >> 18] + B64[(n >> 12) & 63] + B64[(n >> 6) & 63] + '='; }
    parts.push(out);
    return parts.join('');
  }

  // decodeSplyBase64(str): standard base64 (whitespace allowed, padding required) of a whole .sply file.
  function decodeSplyBase64(str) {
    if (typeof str !== 'string') return fail('base64 input must be a string');
    if (str.length > MAX_BASE64_TEXT) return fail('file exceeds 64 MiB limit'); // before any copy
    const clean = str.replace(/[ \t\r\n\f]+/g, '');
    if (clean.length % 4 !== 0 || !/^[A-Za-z0-9+/]*={0,2}$/.test(clean)) return fail('invalid base64');
    // Canonical encoding only (as in Rust): the unused trailing bits of a padded final group must be zero.
    if (clean.length > 0 && clean.endsWith('=')) {
      const two = clean.endsWith('==');
      const last = B64.indexOf(clean[clean.length - (two ? 3 : 2)]);
      if ((last & (two ? 0x0f : 0x03)) !== 0) return fail('invalid base64');
    }
    let bytes;
    if (typeof atob === 'function') {
      const bin = atob(clean);
      bytes = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    } else if (typeof Buffer !== 'undefined') {
      const buf = Buffer.from(clean, 'base64');
      bytes = new Uint8Array(buf.buffer, buf.byteOffset, buf.length);
    } else return fail('no base64 decoder available');
    return decodeSply(bytes);
  }

  // ---- main decoder ----
  // Integers: JS has no 64-bit ints. Values with |v| <= 2^53-1 are always exact. Larger values are accepted only
  // when exactly representable as a double (e.g. 2^60, 2^63, -2^63); otherwise 'sply: integer exceeds safe range'
  // is thrown rather than silently rounding. Tags 3/4 never produce -0.
  function decodeSply(input, opts) {
    const b = toBytes(input);
    const forceManualUtf8 = !!(opts && opts._manualUtf8);
    const len = b.length;
    if (len > MAX_FILE_BYTES) fail('file exceeds 64 MiB limit');
    if (len < HEADER) fail('truncated header');
    for (let i = 0; i < 8; i++) if (b[i] !== MAGIC[i]) fail('bad magic');
    const dv = new DataView(b.buffer, b.byteOffset, len);
    if (dv.getUint16(8, true) !== 1) fail('unsupported major version');
    if (dv.getUint16(10, true) > 0) fail('unsupported minor version');
    if (dv.getUint32(12, true) !== 0) fail('unsupported flags');
    const n = dv.getUint32(16, true);
    const stringsLen = dv.getUint32(20, true);
    const bodyLen = dv.getUint32(24, true);
    const crc = dv.getUint32(28, true);
    // u32 values: sums below stay far under 2^53, so they are exact doubles.
    const offsetsLen = 4 * (n + 1);
    const total = HEADER + offsetsLen + stringsLen + bodyLen;
    if (total !== len) fail('size mismatch');
    if (crc32c(b, HEADER, len) !== crc) fail('crc mismatch');

    const stringsStart = HEADER + offsetsLen;
    const bodyStart = stringsStart + stringsLen;
    // string table
    const strings = new Array(n);
    const stringLens = new Array(n); // UTF-8 byte length of each string (for the expansion budget)
    const seen = new Set();
    let prev = dv.getUint32(HEADER, true);
    if (prev !== 0) fail('invalid string offsets');
    for (let i = 0; i < n; i++) {
      const next = dv.getUint32(HEADER + 4 * (i + 1), true);
      if (next < prev || next > stringsLen) fail('invalid string offsets');
      const s = decodeUtf8(b, stringsStart + prev, stringsStart + next, forceManualUtf8);
      if (seen.has(s)) fail('duplicate string');
      seen.add(s);
      strings[i] = s;
      stringLens[i] = next - prev;
      prev = next;
    }
    if (prev !== stringsLen) fail('invalid string offsets');
    seen.clear();

    // body
    let pos = bodyStart;
    const end = len;
    let lo = 0, hi = 0; // result of readVar()
    let budget = Math.min(len * EXPANSION_FACTOR + EXPANSION_SLACK, EXPANSION_CAP); // decoded string bytes still allowed
    let claimed = 0; // sum of the minimum body bytes claimed by all container headers (<= body length)
    let nodes = 0; // values seen so far
    function spend(bytes) {
      budget -= bytes;
      if (budget < 0) fail('string expansion limit exceeded');
    }

    function readVar() {
      lo = 0; hi = 0;
      for (let i = 0; i < 10; i++) {
        if (pos >= end) fail('truncated body');
        const c = b[pos++];
        const v = c & 0x7f;
        const s = 7 * i;
        if (i === 9 && v > 1) fail('varint overflow');
        if (s < 32) {
          lo = (lo | (v << s)) >>> 0;
          if (s + 7 > 32) hi = (hi | (v >>> (32 - s))) >>> 0;
        } else hi = (hi | (v << (s - 32))) >>> 0;
        if (!(c & 0x80)) {
          if (i > 0 && v === 0) fail('non-minimal varint');
          return;
        }
      }
      fail('varint too long');
    }
    // varint that must fit in u32 and be <= remaining bytes (counts) or < limit
    function readCount(unitBytes, claim) {
      readVar();
      if (hi !== 0) fail('count exceeds remaining bytes');
      const need = lo * unitBytes;
      if (need > end - pos) fail('count exceeds remaining bytes');
      if (claim) {
        if (claimed + need > end - bodyStart) fail('count exceeds remaining bytes');
        claimed += need;
      }
      return lo;
    }
    function readIndex() {
      readVar();
      if (hi !== 0 || lo >= n) fail('invalid string index');
      return lo;
    }
    function checkedMagnitude(h, l) { // h:l as exact double, or throw
      if (h >= 0x200000) {
        const shift = (64 - Math.clz32(h)) - 53;
        if ((l & ((1 << shift) - 1)) !== 0) fail('integer exceeds safe range');
      }
      return h * 4294967296 + l;
    }
    function readFloat() {
      if (end - pos < 8) fail('truncated body');
      const v = dv.getFloat64(pos, true);
      pos += 8;
      if (v - v !== 0) fail('non-finite float'); // NaN or +-Infinity
      return v;
    }
    function setKey(o, k, v) {
      if (k === '__proto__') Object.defineProperty(o, k, { value: v, writable: true, enumerable: true, configurable: true });
      else o[k] = v;
    }

    function value(depth) {
      if (pos >= end) fail('truncated body');
      if (++nodes > MAX_NODES) fail('too many values');
      const tag = b[pos++];
      switch (tag) {
        case 0: return null;
        case 1: return false;
        case 2: return true;
        case 3: readVar(); return checkedMagnitude(hi, lo);
        case 4: {
          readVar();
          if (hi >= 0x80000000) fail('integer out of range');
          const l = (lo + 1) >>> 0;
          const h = l === 0 ? hi + 1 : hi; // magnitude = v + 1 <= 2^63
          return -checkedMagnitude(h, l);
        }
        case 5: return readFloat();
        case 6: { const i = readIndex(); spend(stringLens[i]); return strings[i]; }
        case 7: {
          if (depth >= MAX_DEPTH) fail('nesting too deep');
          const count = readCount(1, true);
          const arr = [];
          for (let i = 0; i < count; i++) arr.push(value(depth + 1));
          return arr;
        }
        case 8: {
          if (depth >= MAX_DEPTH) fail('nesting too deep');
          const count = readCount(2, true);
          const o = {};
          // strings are unique, so duplicate keys <=> duplicate indices; small objects use a linear scan
          const idx = count > 16 ? new Set() : null;
          const small = idx ? null : new Array(count);
          for (let i = 0; i < count; i++) {
            const ki = readIndex();
            spend(stringLens[ki]);
            if (idx) { if (idx.has(ki)) fail('duplicate object key'); idx.add(ki); }
            else { for (let j = 0; j < i; j++) if (small[j] === ki) fail('duplicate object key'); small[i] = ki; }
            setKey(o, strings[ki], value(depth + 1));
          }
          return o;
        }
        case 9: {
          if (depth >= MAX_DEPTH) fail('nesting too deep');
          const count = readCount(8, true);
          const arr = [];
          for (let i = 0; i < count; i++) arr.push(readFloat());
          return arr;
        }
        case 10: {
          const count = readCount(1, false);
          spend(Math.ceil(count / 3) * 4);
          const s = pos;
          pos += count;
          return base64Encode(b, s, pos);
        }
        default: return fail('unknown tag ' + tag);
      }
    }

    const result = value(0);
    if (pos !== end) fail('trailing bytes in body');
    return result;
  }

  const api = { decodeSply, isSply, decodeSplyBase64, crc32c };
  if (typeof module === 'object' && module && module.exports) module.exports = api;
  else if (host) host.SplyDecode = api;
})(typeof window !== 'undefined' ? window : globalThis);
