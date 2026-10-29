/* Splinely runtime v1. Dependency-free; embed this file unchanged in an SVG script. */
(function (host) {
  'use strict';
  // <sply-decode>
  // GENERATED from runtime/src/sply_decode.js by scripts/sync-runtime-decoder.cjs. Do not edit; run the script.
  const SplyDecode = (function () {
    const module = { exports: {} }; // captures the decoder export; keeps it off the global object
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
    // Native decoder where available: 'strict' accepts exactly what the checks below accept (ASCII whitespace
    // skipped, standard alphabet, full padding, zero trailing bits).
    if (typeof Uint8Array.fromBase64 === 'function') {
      let bytes;
      try { bytes = Uint8Array.fromBase64(str, { lastChunkHandling: 'strict' }); } catch (_) { return fail('invalid base64'); }
      return decodeSply(bytes);
    }
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
    return module.exports;
  })();
  // </sply-decode>
  const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
  const finite = (v, label) => { if (typeof v !== 'number' || !Number.isFinite(v)) throw new Error(label + ' must be finite'); return v; };
  const weightValue = v => { finite(v,'Clip weight'); if(v<0 || v>1)throw new Error('Clip weight must be in [0,1]'); return v; };
  const fadeDuration = v => { finite(v,'Fade duration'); if(v<0)throw new Error('Fade duration must be nonnegative'); return v; };
  const list = v => Array.isArray(v) ? v : Object.entries(v || {}).map(([id, item]) => ({ ...item, id: item.id || id }));
  const unwrap = v => { if (!v) return v; const t = v.type; return t === 'number' || t === 'color' || t === 'gradient' || t === 'path' ? v.value : v; };
  const own = (o, k) => Object.prototype.hasOwnProperty.call(o, k);
  // Accessibility data (opt-in, browser-only semantics). Limits mirror crates/splinely_document/src/accessibility.rs.
  const A11Y_ROLES = ['button','link','img','group','heading'], A11Y_KEYS = ['label','description','role','hidden'];
  const A11Y_INTERACTIVE = ['click','dblclick','pointerenter','pointerleave','pointerdown','pointermove','pointerup','pointercancel','wheel','keydown','keyup','focus','blur','dragstart','drag','dragend'];
  const a11yText = (v, max, what) => {
    if (typeof v !== 'string' || !v || v.trim() !== v || [...v].length > max || /[\u0000-\u001f\u007f-\u009f\ufeff\ufffe\uffff]/.test(v)) throw new Error(what + ' must be 1..' + max + ' characters without control characters or surrounding spaces');
    return v;
  };
  // IDs of nodes targeted by a non-timer transition event or interaction binding named in `names`.
  function interactiveIds(machines, interactions, names = A11Y_INTERACTIVE) {
    const ids = new Set();
    for (const m of machines) for (const t of [...(m.transitions || []), ...m.states.flatMap(s => s.transitions || [])]) {
      const e = t.event;
      if (t.after == null && e && typeof e === 'object' && typeof e.target === 'string' && names.includes(e.name)) ids.add(e.target);
    }
    for (const b of interactions || []) if (typeof b.target === 'string' && names.includes(b.event)) ids.add(b.target);
    return ids;
  }
  function equal(a,b) {
    if(a===b)return true;if(!a || !b || typeof a!=='object' || typeof b!=='object' || Array.isArray(a)!==Array.isArray(b))return false;
    const keys=Object.keys(a);return keys.length===Object.keys(b).length && keys.every(k=>own(b,k) && equal(a[k],b[k]));
  }
  function color(v) {
    if (v && typeof v === 'object' && !Array.isArray(v) && ['r','g','b'].every(k => Number.isFinite(v[k])) && (v.a==null || Number.isFinite(v.a))) return [v.r, v.g, v.b, v.a == null ? 1 : v.a];
    if (typeof v !== 'string' || !/^#([a-f\d]{3,4}|[a-f\d]{6}|[a-f\d]{8})$/i.test(v)) return null;
    let hex = v.slice(1); if (hex.length < 5) hex = [...hex].map(c => c + c).join('');
    return [0,2,4,6].map((i) => i < hex.length ? parseInt(hex.slice(i,i+2),16) / 255 : 1);
  }
  // Structured values as shape + numbers: gradients (angle, stops) and path
  // data (letters, numbers) interpolate and blend number by number.
  const pathToken = /[a-zA-Z]|[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?/g;
  const pathNumber = v => String(Math.round(v*1000)/1000+0);
  function vectorOf(v) {
    v = unwrap(v);
    if (typeof v === 'number') return {kind:'n', shape:'n', nums:[v]};
    const c = color(v); if (c) return {kind:'c', shape:'c', nums:c};
    if (v && typeof v === 'object' && Array.isArray(v.stops)) {
      const nums=[Number(v.angle)||0];
      for (const s of v.stops) nums.push(Number(s.offset)||0, ...(color(s.color) || [0,0,0,1]));
      return {kind:'g', shape:'g'+(v.radial?'r':'l')+v.stops.length, nums, radial:!!v.radial};
    }
    if (typeof v === 'string' && /^\s*[Mm]/.test(v)) {
      const tokens = v.match(pathToken) || [];
      return {kind:'p', shape:tokens.map(t=>/^[a-zA-Z]$/.test(t)?t:'#').join(''), nums:tokens.filter(t=>!/^[a-zA-Z]$/.test(t)).map(Number), template:tokens};
    }
    return null;
  }
  const smoothstep=t=>{t=Math.max(0,Math.min(1,t));return t*t*(3-2*t);};
  // Crossfade mix: numbers, colours, gradients and matching paths blend; anything else switches halfway.
  function mixValues(a,b,t) {
    const va=vectorOf(a),vb=vectorOf(b);
    if(va && vb && va.shape===vb.shape)return fromVector(va,va.nums.map((x,i)=>x+(vb.nums[i]-x)*t));
    return t<0.5?a:b;
  }
  function fromVector(vec, n) {
    if (vec.kind === 'n') return n[0];
    if (vec.kind === 'c') return {r:n[0], g:n[1], b:n[2], a:n[3]};
    if (vec.kind === 'g') {
      const stops=[]; for (let i=1;i<n.length;i+=5) stops.push({offset:clamp(n[i],0,1), color:{r:n[i+1], g:n[i+2], b:n[i+3], a:n[i+4]}});
      return {radial:vec.radial, angle:n[0], stops};
    }
    let k=0; return vec.template.map(t => /^[a-zA-Z]$/.test(t) ? t : pathNumber(n[k++])).join(' ');
  }
  function interpolate(a, b, t) {
    a = unwrap(a); b = unwrap(b);
    if (typeof a === 'number' && typeof b === 'number') return t <= 0 ? a : t >= 1 ? b : a*(1-t)+b*t;
    const ca = color(a), cb = color(b);
    if (ca && cb) {
      const c = ca.map((v,i) => t <= 0 ? v : t >= 1 ? cb[i] : v*(1-t)+cb[i]*t);
      return {r:c[0],g:c[1],b:c[2],a:c[3]};
    }
    if (Array.isArray(a) && Array.isArray(b) && a.length === b.length && [...a,...b].every(Number.isFinite)) return a.map((v,i) => t <= 0 ? v : t >= 1 ? b[i] : v*(1-t)+b[i]*t);
    const va = vectorOf(a), vb = vectorOf(b);
    if (va && vb && va.shape === vb.shape) return fromVector(va, va.nums.map((v,i) => t <= 0 ? v : t >= 1 ? vb.nums[i] : v*(1-t)+vb.nums[i]*t));
    return t < 1 ? a : b;
  }
  function easing(spec, t) {
    const type = Array.isArray(spec) ? 'cubicBezier' : typeof spec === 'string' ? spec : (spec || {}).type || 'linear';
    if (type === 'step' || type === 'discrete') return t < 1 ? 0 : 1;
    if (type === 'linear') return t;
    const points = Array.isArray(spec) ? spec : type === 'cubicBezier' ? [spec.x1,spec.y1,spec.x2,spec.y2] : null;
    if (!points || points.length !== 4 || !points.every(Number.isFinite) || points[0] < 0 || points[0] > 1 || points[2] < 0 || points[2] > 1) throw new Error('Unsupported easing');
    if (t <= 0 || t >= 1) return t;
    const curve = (s,a,b) => 3*(1-s)*(1-s)*s*a + 3*(1-s)*s*s*b + s*s*s;
    let lo=0, hi=1;
    for (let i=0;i<60;i++) { const mid=(lo+hi)/2; if (curve(mid,points[0],points[2]) < t) lo=mid; else hi=mid; }
    return curve((lo+hi)/2,points[1],points[3]);
  }
  function evaluateTrack(track, time) {
    const frames = track.keyframes || [];
    if (!frames.length) return undefined;
    if (time < frames[0].time) return unwrap(frames[0].value);
    // Last duplicate at an exact timestamp wins; normalization preserves stored order.
    let i = 0; while (i+1 < frames.length && frames[i+1].time <= time) i++;
    if (i+1 === frames.length) return unwrap(frames[i].value);
    const a=frames[i], b=frames[i+1];
    return interpolate(a.value,b.value,easing(a.easing,(time-a.time)/(b.time-a.time)));
  }
  // Load-time track tables: typed times, unwrapped values and per-segment easing/interpolation
  // kinds, so the hot loop is a binary search plus arithmetic. Arithmetic mirrors easing()/interpolate().
  const bezier = (s,a,b) => 3*(1-s)*(1-s)*s*a + 3*(1-s)*s*s*b + s*s*s;
  function compileTrack(track) {
    try {
      const frames=track.keyframes || [], n=frames.length;
      if(!n) return null;
      const ct={times:new Float64Array(n),nums:new Float64Array(n),vals:new Array(n),raw:new Array(n),col:new Array(n),ease:new Uint8Array(n),pts:new Array(n),seg:new Uint8Array(n)};
      for(let i=0;i<n;i++) {
        const f=frames[i], v=unwrap(f.value);
        ct.times[i]=f.time;ct.raw[i]=f.value;ct.vals[i]=v;ct.nums[i]=typeof v==='number'?v:0;ct.col[i]=typeof v==='number'?null:color(v);
        const spec=f.easing, type=Array.isArray(spec)?'cubicBezier':typeof spec==='string'?spec:(spec||{}).type||'linear';
        if(type==='step' || type==='discrete') ct.ease[i]=1;
        else if(type==='linear') ct.ease[i]=0;
        else {
          const points=Array.isArray(spec)?spec:type==='cubicBezier'?[spec.x1,spec.y1,spec.x2,spec.y2]:null;
          if(!points || points.length!==4 || !points.every(Number.isFinite) || points[0]<0 || points[0]>1 || points[2]<0 || points[2]>1) return null;
          ct.ease[i]=2;ct.pts[i]=points.slice();
        }
      }
      for(let i=0;i+1<n;i++) ct.seg[i]=typeof ct.vals[i]==='number' && typeof ct.vals[i+1]==='number'?0:ct.col[i] && ct.col[i+1]?1:2;
      return ct;
    } catch(_) { return null; }
  }
  const mixc = (v,w,t) => t <= 0 ? v : t >= 1 ? w : v*(1-t)+w*t;
  function evalCompiled(ct, time) {
    const times=ct.times, n=times.length;
    if (time < times[0]) return ct.vals[0];
    // Last duplicate at an exact timestamp wins: last index with times[i] <= time.
    let i = 0;
    if (n > 8) { let lo=0, hi=n-1; while (lo < hi) { const mid=(lo+hi+1)>>1; if (times[mid] <= time) lo=mid; else hi=mid-1; } i=lo; }
    else while (i+1 < n && times[i+1] <= time) i++;
    if (i+1 === n) return ct.vals[i];
    const t0=times[i];
    let t=(time-t0)/(times[i+1]-t0);
    const kind=ct.ease[i];
    if (kind === 1) t = t < 1 ? 0 : 1;
    else if (kind === 2 && t > 0 && t < 1) {
      const p=ct.pts[i]; let lo=0, hi=1;
      for (let k=0;k<60;k++) { const mid=(lo+hi)/2; if (bezier(mid,p[0],p[2]) < t) lo=mid; else hi=mid; }
      t = bezier((lo+hi)/2,p[1],p[3]);
    }
    const seg=ct.seg[i];
    if (seg === 0) { const a=ct.nums[i], b=ct.nums[i+1]; return t <= 0 ? a : t >= 1 ? b : a*(1-t)+b*t; }
    if (seg === 1) { const a=ct.col[i], b=ct.col[i+1]; return {r:mixc(a[0],b[0],t),g:mixc(a[1],b[1],t),b:mixc(a[2],b[2],t),a:mixc(a[3],b[3],t)}; }
    return interpolate(ct.raw[i],ct.raw[i+1],t);
  }
  const boneProperties = new Set(['x','y','rotation']);
  function bonePatch(props) {
    const result={};
    for(const [key,value] of Object.entries(props)) {
      if(!boneProperties.has(key)) throw new Error('Unsupported bone property: '+key);
      if(value && typeof value==='object' && (value.type!=='number' || !own(value,'value'))) throw new Error('Bone '+key+' must be numeric');
      result[key]=finite(unwrap(value),'Bone '+key);
    }
    return result;
  }
  function solveBones(bones) {
    const byId=new Map(bones.map(b=>[b.id,b])), poses=new Map(), visiting=new Set();
    function solve(id,depth) {
      if(depth>128) throw new Error('Bone depth limit exceeded (128)');
      if(visiting.has(id)) throw new Error('Bone cycle: '+id);
      const b=byId.get(id); if(!b) throw new Error('Missing bone parent: '+id);
      visiting.add(id);
      // Traverse even cached ancestors so the limit is independent of input order.
      const parent=b.parentId==null?null:solve(b.parentId,depth+1);
      visiting.delete(id);
      if(poses.has(id)) return poses.get(id);
      let x=finite(b.x,'Bone x'), y=finite(b.y,'Bone y'), rotation=finite(b.rotation,'Bone rotation');
      const length=finite(b.length,'Bone length'); if(length<=0) throw new Error('Bone length must be positive');
      if(parent) {
        const angle=(parent.rotation%360)*Math.PI/180, c=Math.cos(angle), s=Math.sin(angle);
        const localX=x, localY=y;
        x=parent.tipX+c*localX-s*localY; y=parent.tipY+s*localX+c*localY;
        rotation+=parent.rotation;
      }
      const angle=(rotation%360)*Math.PI/180;
      const pose={id:b.id,x:finite(x,'Bone world x'),y:finite(y,'Bone world y'),rotation:finite(rotation,'Bone world rotation'),length,tipX:finite(x+length*Math.cos(angle),'Bone tip x'),tipY:finite(y+length*Math.sin(angle),'Bone tip y')};
      poses.set(id,pose); return pose;
    }
    return bones.map(b=>solve(b.id,1));
  }
  // Hot-path FK for a structurally validated rig: `plan` is the order in which solveBones() creates poses
  // (parents first), so the arithmetic, the checks and the first reported error are the same as solveBones().
  function bonePlan(bones) {
    const index=new Map(bones.map((b,i)=>[b.id,i])), parents=bones.map(b=>b.parentId==null?-1:index.get(b.parentId)), order=[], seen=new Uint8Array(bones.length);
    const visit=i=>{if(parents[i]>=0)visit(parents[i]);if(!seen[i]){seen[i]=1;order.push(i);}};
    for(let i=0;i<bones.length;i++)visit(i);
    return {order,parents};
  }
  function solveBonesPlanned(bones, plan) {
    const out=new Array(bones.length), order=plan.order, parents=plan.parents;
    for(let k=0;k<order.length;k++) {
      const i=order[k], b=bones[i], parent=parents[i]<0?null:out[parents[i]];
      let x=finite(b.x,'Bone x'), y=finite(b.y,'Bone y'), rotation=finite(b.rotation,'Bone rotation');
      const length=finite(b.length,'Bone length'); if(length<=0) throw new Error('Bone length must be positive');
      if(parent) {
        const angle=(parent.rotation%360)*Math.PI/180, c=Math.cos(angle), s=Math.sin(angle);
        const localX=x, localY=y;
        x=parent.tipX+c*localX-s*localY; y=parent.tipY+s*localX+c*localY;
        rotation+=parent.rotation;
      }
      const angle=(rotation%360)*Math.PI/180;
      out[i]={id:b.id,x:finite(x,'Bone world x'),y:finite(y,'Bone world y'),rotation:finite(rotation,'Bone world rotation'),length,tipX:finite(x+length*Math.cos(angle),'Bone tip x'),tipY:finite(y+length*Math.sin(angle),'Bone tip y')};
    }
    return out;
  }
  // Same deterministic 64-chord arc-length approximation as splinely_document. Chord tables are built once per
  // (d, transform) and shared by every binding and runtime (small LRU); a lookup walks the chords like Rust, or
  // binary-searches the cumulative lengths on long paths (same chord, results within rounding).
  const MOTION_MAX_CHORDS=100000, MOTION_TOTAL_CHORDS=100000, MOTION_SEARCH=1024, MOTION_CACHE=64, MOTION_CACHE_CHORDS=400000;
  const motionCache=new Map();let motionCached=0;
  const numKey=v=>v===0 && 1/v<0?'-0':String(v);
  function motionBuild(d, tr) {
    if(typeof d!=='string' || d.replace(pathToken,'').replace(/[\s,]/g,''))throw new Error('Invalid motion path');
    if(/(?:^|[a-zA-Z,])\s*,|,\s*(?:[a-zA-Z]|$)/.test(d))throw new Error('Invalid motion path separators');
    if(!/^\s*[Mm]/.test(d))throw new Error('Motion path must start with M');
    const tokens=d.match(pathToken)||[];let i=0,cmd='',current=[0,0],start=[0,0],total=0,n=0,ch=new Float64Array(1024),lens=new Float64Array(256);
    const angle=(tr.rotation||0)*Math.PI/180,c=Math.cos(angle),s=Math.sin(angle);
    const sx=tr.scaleX==null?1:tr.scaleX, sy=tr.scaleY==null?1:tr.scaleY, tx=tr.x||0, ty=tr.y||0;
    const segment=(points)=>{
      const p0=points[0],p1=points[1],p2=points[2],p3=points[3];
      let ax=c*sx*p0[0]-s*sy*p0[1]+tx, ay=s*sx*p0[0]+c*sy*p0[1]+ty;
      for(let j=1;j<=64;j++){
        const t=j/64,u=1-t,qx=u*u*u*p0[0]+3*u*u*t*p1[0]+3*u*t*t*p2[0]+t*t*t*p3[0],qy=u*u*u*p0[1]+3*u*u*t*p1[1]+3*u*t*t*p2[1]+t*t*t*p3[1];
        const bx=c*sx*qx-s*sy*qy+tx, by=s*sx*qx+c*sy*qy+ty, len=Math.hypot(bx-ax,by-ay);
        if(len>1e-12){
          total+=len;
          if(n>=MOTION_MAX_CHORDS)throw new Error('Motion path too complex');
          if(n===lens.length){const l2=new Float64Array(n*2),c2=new Float64Array(n*8);l2.set(lens);c2.set(ch);lens=l2;ch=c2;}
          ch[4*n]=ax;ch[4*n+1]=ay;ch[4*n+2]=bx;ch[4*n+3]=by;lens[n++]=len;
        }
        ax=bx;ay=by;
      }
    };
    const line=(a,b)=>segment([a,[a[0]+(b[0]-a[0])/3,a[1]+(b[1]-a[1])/3],[a[0]+2*(b[0]-a[0])/3,a[1]+2*(b[1]-a[1])/3],b]);
    while(i<tokens.length){
      if(/^[a-z]$/i.test(tokens[i]))cmd=tokens[i++];
      if(!/^[MmLlQqCcZz]$/.test(cmd))throw new Error('Motion paths support M/L/Q/C/Z only');
      const upper=cmd.toUpperCase(),relative=cmd!==upper;
      if(upper==='Z'){if(Math.hypot(current[0]-start[0],current[1]-start[1])>1e-9)line(current,start);current=start;cmd='';continue;}
      const count={M:2,L:2,Q:4,C:6}[upper];const nums=tokens.slice(i,i+count).map(Number);
      if(nums.length!==count || !nums.every(Number.isFinite))throw new Error('Invalid motion path coordinates');i+=count;
      const points=[];for(let j=0;j<count;j+=2)points.push([nums[j]+(relative?current[0]:0),nums[j+1]+(relative?current[1]:0)]);
      const end=points[points.length-1];
      if(upper==='M'){current=end;start=end;cmd=relative?'l':'L';continue;}
      if(upper==='L')line(current,end);
      else if(upper==='Q'){const q=points[0];segment([current,[current[0]+2*(q[0]-current[0])/3,current[1]+2*(q[1]-current[1])/3],[end[0]+2*(q[0]-end[0])/3,end[1]+2*(q[1]-end[1])/3],end]);}
      else segment([current,...points]);current=end;
    }
    if(!Number.isFinite(total) || total<=1e-12)throw new Error('Motion path must have finite nonzero length');
    let cum=null;
    if(n>MOTION_SEARCH){cum=new Float64Array(n+1);for(let j=0;j<n;j++)cum[j+1]=cum[j]+lens[j];}
    return {n,ch,lens,cum,total};
  }
  // Table for (d, transform); failures are cached too (as their message), so a bad path is not re-parsed every frame.
  function motionTable(d, tr) {
    const x=tr.x,y=tr.y,r=tr.rotation,a=tr.scaleX,b=tr.scaleY,num=v=>v==null || typeof v==='number';
    if(typeof d!=='string' || !(num(x) && num(y) && num(r) && num(a) && num(b)))return motionBuild(d,tr);
    const key=d+'\u0000'+numKey(x||0)+','+numKey(y||0)+','+numKey(r||0)+','+numKey(a==null?1:a)+','+numKey(b==null?1:b);
    let hit=motionCache.get(key);
    if(hit!==undefined){motionCache.delete(key);motionCache.set(key,hit);}
    else {
      try{hit=motionBuild(d,tr);}catch(e){hit={error:e.message,n:0};}
      motionCache.set(key,hit);motionCached+=hit.n;
      for(const [k,v] of motionCache){if(motionCache.size<=MOTION_CACHE && motionCached<=MOTION_CACHE_CHORDS)break;if(v===hit)break;motionCache.delete(k);motionCached-=v.n;}
    }
    if(hit.error!==undefined)throw new Error(hit.error);
    return hit;
  }
  const motionOut=[0,0,0];
  function motionSample(d, transform, progress) {
    const tb=motionTable(d,transform||{}),n=tb.n,lens=tb.lens,ch=tb.ch,total=tb.total,eps=total*1e-12;
    let distance=clamp(progress,0,1)*total,j=0;
    if(tb.cum!==null){
      // First chord whose end is within the tolerance of the distance (the chord the walk below stops at).
      const cum=tb.cum;let hi=n-1;
      while(j<hi){const mid=(j+hi)>>1;if(distance-cum[mid]<=lens[mid]+eps)hi=mid;else j=mid+1;}
      distance-=cum[j];
    } else for(;j<n-1;j++){if(distance<=lens[j]+eps)break;distance-=lens[j];}
    const t=clamp(distance/lens[j],0,1),k=4*j,ax=ch[k],ay=ch[k+1],bx=ch[k+2],by=ch[k+3];
    motionOut[0]=ax+(bx-ax)*t;motionOut[1]=ay+(by-ay)*t;motionOut[2]=Math.atan2(by-ay,bx-ax)*180/Math.PI;
    return motionOut;
  }

  const blendParameter = v => {finite(v,'Blend-space parameter');if(Math.abs(v)>1e7)throw new Error('Blend-space parameter out of range');return v;};
  function spaceWeights(spaces, get) {
    const result=new Map();
    for(const s of spaces) {
      const p=s.parameters.map(k=>blendParameter(get(k)));
      const distances=s.samples.map(v=>(v.position[0]-p[0])**2+(v.position[1]-p[1])**2);
      const nearest=Math.min(...distances), exact=distances.indexOf(0);
      const raw=distances.map((d,i)=>exact<0?nearest/d:(i===exact?1:0)), sum=raw.reduce((a,b)=>a+b,0);
      s.samples.forEach((v,i)=>result.set(v.clipId,raw[i]/sum));
    }
    return result;
  }
  // Same as splinely_document MIN_CLIP_DURATION: bounds loop laps per second.
  const MIN_CLIP_DURATION=0.001;
  function normalize(project) {
    if (!project || typeof project !== 'object' || Array.isArray(project)) throw new Error('Project must be an object');
    if (project.schemaVersion != null && project.schemaVersion !== 1) throw new Error('Unsupported schemaVersion: ' + project.schemaVersion);
    const unique = (items, label) => {
      const ids=new Set(); for (const x of items) { if (typeof x.id !== 'string' || !x.id || ids.has(x.id)) throw new Error('Missing or duplicate ' + label + ' id: ' + x.id); ids.add(x.id); } return items;
    };
    const clips = unique(list(project.clips).map(c => {
      if(c.id==='@timeline')throw new Error('Reserved clip id: @timeline');
      const duration=finite(c.duration,'Clip duration'); if (duration < MIN_CLIP_DURATION) throw new Error('Clip duration must be at least '+MIN_CLIP_DURATION+' seconds');
      const blendMode=c.blendMode===undefined?'override':c.blendMode;
      if(!['override','weighted','additive'].includes(blendMode))throw new Error('Unsupported blendMode: '+blendMode);
      const weight=weightValue(c.weight===undefined?1:c.weight);
      const trackKeys=new Set();
      return {...c,duration,blendMode,weight,loop: !!(c.looping == null ? c.loop : c.looping),tracks:(c.tracks || []).map(t => {
        if(typeof (t.nodeId || t.target)!=='string' || typeof t.property!=='string')throw new Error('Track requires target and property');
        const trackKey=JSON.stringify([t.nodeId || t.target,t.property]);
        if(trackKeys.has(trackKey))throw new Error('Clip '+c.id+': duplicate property track: '+(t.nodeId || t.target)+'.'+t.property);
        trackKeys.add(trackKey);
        if(!t.keyframes || !t.keyframes.length)throw new Error('Track requires keyframes');
        const keyframes=t.keyframes.map(k => {
          if (finite(k.time,'Keyframe time') < 0 || k.time > duration) throw new Error('Keyframe outside clip');
          const value=unwrap(k.value);if(value===undefined)throw new Error('Keyframe requires value');
          if(typeof value==='number' || (k.value && k.value.type==='number'))finite(value,'Keyframe value');
          if(blendMode==='additive') {
            const v=vectorOf(value), color=['fill','stroke'].includes(t.property);
            if(!v || v.kind!==(color?'c':'n') || !['x','y','rotation','scaleX','scaleY','opacity','fill','stroke','width','height','radius','fontSize','motionProgress'].includes(t.property))throw new Error('Additive clips support matching numeric and RGBA properties only');
            if(v.nums.some(n=>!Number.isFinite(n) || Math.abs(n)>1e7 || (color && (n<0 || n>1))))throw new Error('Additive keyframe out of range');
            if(['opacity','motionProgress'].includes(t.property))weightValue(value);
            if(['width','height','radius','fontSize'].includes(t.property) && value<0)throw new Error('Additive geometry must be nonnegative');
          }
          if(blendMode==='weighted' && !vectorOf(value))throw new Error('Weighted keyframe must be numeric, RGBA, gradient or path data');
          if(Array.isArray(value) && value.some(v=>typeof v!=='number' || !Number.isFinite(v)))throw new Error('Keyframe array must contain finite numbers');
          easing(k.easing,0.5);return {...k};
        }).sort((a,b)=>a.time-b.time);
        return {...t,target:t.nodeId || t.target,property:t.property,keyframes};
      })};
    }), 'clip');
    const ownedClips=new Set();
    if(project.blendSpaces!==undefined && !Array.isArray(project.blendSpaces))throw new Error('blendSpaces must be an array');
    const blendSpaces=unique((project.blendSpaces || []).map(s=>{
      if(!s || typeof s!=='object' || Object.keys(s).some(k=>!['id','parameters','samples'].includes(k)) || typeof s.id!=='string' || !/^[A-Za-z_][A-Za-z0-9_.-]*$/.test(s.id))throw new Error('Invalid blend space fields or ID');
      if(!Array.isArray(s.parameters) || s.parameters.length!==2 || s.parameters.some(k=>typeof k!=='string' || !k) || s.parameters[0]===s.parameters[1])throw new Error('Blend space needs two distinct parameter names');
      s.parameters.forEach(k=>blendParameter((project.variables || {})[k]));
      if(!Array.isArray(s.samples) || !s.samples.length || s.samples.length>256)throw new Error('Blend space needs 1..256 samples');
      s.samples.forEach((v,i)=>{
        if(!v || typeof v!=='object' || Object.keys(v).some(k=>!['clipId','position'].includes(k)))throw new Error('Invalid blend-space sample fields');
        if(!clips.some(c=>c.id===v.clipId && c.blendMode==='weighted') || ownedClips.has(v.clipId))throw new Error('Blend-space sample needs a unique existing weighted clip');
        ownedClips.add(v.clipId);
        if(!Array.isArray(v.position) || v.position.length!==2)throw new Error('Blend-space sample needs a 2D position');
        v.position.forEach(blendParameter);
        if(s.samples.slice(0,i).some(a=>(a.position[0]-v.position[0])**2+(a.position[1]-v.position[1])**2<=1e-12))throw new Error('Blend-space samples must be more than 0.000001 apart');
      });
      return {id:s.id,parameters:[...s.parameters],samples:s.samples.map(v=>({clipId:v.clipId,position:[...v.position]}))};
    }),'blend space');
    const machines=unique(list(project.stateMachines || project.machines).map(m=>{
      const states=unique(list(m.states),'state'), ids=new Set(states.map(s=>s.id));
      if(!ids.has(m.initial))throw new Error('Unknown initial state: '+m.initial);
      for(const t of [...(m.transitions || []),...states.flatMap(s=>s.transitions || [])])if(!ids.has(t.to || t.target))throw new Error('Unknown state: '+(t.to || t.target));
      return {...m,states};
    }),'machine');
    for(const source of [project,...clips]) {
      for(const cue of source.audioCues || []) {
        if(finite(cue.time,'Cue time')<0)throw new Error('Cue time must be nonnegative');
        for(const k of ['offset','volume','duration'])if(cue[k]!=null && (finite(cue[k],'Cue '+k)<0 || (k==='duration' && cue[k]===0)))throw new Error('Invalid cue '+k);
      }
    }
    const nodes=unique(list(project.nodes),'node'), bones=unique(list(project.bones),'bone'), assets=unique(list(project.audioAssets || project.assets),'asset');
    unique([...nodes,...bones,...clips,...machines,...assets,...(project.audioCues || []),...clips.flatMap(c=>c.audioCues || [])].filter(x=>x.id!=null),'global');
    if(project.description!==undefined){if(typeof project.description!=='string')throw new Error('Project description must be a string');if(project.description!=='')a11yText(project.description,1000,'Project description');}
    const interactiveNodes=interactiveIds(machines,project.interactions);
    for(const n of nodes) if(n.a11y!==undefined) {
      const a=n.a11y,what='Node '+n.id+' a11y';
      if(!a || typeof a!=='object' || Array.isArray(a))throw new Error(what+' must be an object');
      for(const k of Object.keys(a))if(!A11Y_KEYS.includes(k))throw new Error(what+': unknown field '+k);
      if(a.label!==undefined)a11yText(a.label,200,what+'.label');
      if(a.description!==undefined)a11yText(a.description,1000,what+'.description');
      if(a.role!==undefined && !A11Y_ROLES.includes(a.role))throw new Error(what+'.role must be one of '+A11Y_ROLES.join(', '));
      if(a.hidden!==undefined && typeof a.hidden!=='boolean')throw new Error(what+'.hidden must be boolean');
      if(a.hidden){
        if(a.label!==undefined || a.description!==undefined || a.role!==undefined)throw new Error(what+': hidden nodes cannot also have label, description or role');
        if(interactiveNodes.has(n.id))throw new Error(what+': an interactive node cannot be hidden from assistive technology');
      }
    }
    for(const m of machines) for(const s of m.states) if(s.announce!==undefined)a11yText(s.announce,200,'Machine '+m.id+' state '+s.id+' announce');
    const boneIds=new Set(bones.map(b=>b.id));
    for(const b of bones) if(b.parentId!=null && (typeof b.parentId!=='string' || !boneIds.has(b.parentId))) throw new Error('Missing bone parent: '+b.parentId);
    solveBones(bones);
    for(const n of nodes) if(n.boneId!=null && !boneIds.has(n.boneId)) throw new Error('Missing node bone: '+n.boneId);
    // Group rigs (move → turn → attach bones) let a whole group be animated; see Project::rig_group.
    for(const g of list(project.groups)) if(g && g.rig!=null) {
      const r=g.rig, parent=id=>bones.find(b=>b.id===id);
      if(!r || typeof r!=='object' || ![r.moveBone,r.turnBone,r.attachBone].every(id=>typeof id==='string' && boneIds.has(id)))throw new Error('Group '+g.id+': rig needs existing moveBone, turnBone and attachBone');
      if(parent(r.turnBone).parentId!==r.moveBone || parent(r.attachBone).parentId!==r.turnBone)throw new Error('Group '+g.id+': rig bones must be chained move → turn → attach');
    }
    for(const n of nodes) if(n.motionPath!=null) {
      const b=n.motionPath,source=nodes.find(p=>p.id===b.path);
      if(!source || source===n || !source.kind || source.kind.type!=='path')throw new Error('Unknown or invalid motion path source');
      if(n.boneId!=null || source.boneId!=null || source.motionPath!=null)throw new Error('Motion path bindings cannot be chained or bone-bound');
      weightValue(b.progress===undefined?0:b.progress);
      if(b.orient!==undefined && typeof b.orient!=='boolean')throw new Error('Motion orientation must be boolean');
      motionSample(source.kind.d,source.transform,b.progress||0);
    }
    // Total chord budget over the distinct source paths, including their animated `d` values (keyframes, overrides).
    const motionSources=new Set(nodes.filter(n=>n.motionPath!=null).map(n=>n.motionPath.path));
    if(motionSources.size) {
      let chords=0;
      const count=(d,tr,strict)=>{
        try{chords+=motionTable(d,tr || {}).n;}catch(e){if(strict || e.message==='Motion path too complex')throw e;}
        if(chords>MOTION_TOTAL_CHORDS)throw new Error('Motion paths too complex: more than '+MOTION_TOTAL_CHORDS+' chords');
      };
      for(const n of nodes)if(motionSources.has(n.id))count(n.kind.d,n.transform,true);
      for(const c of clips)for(const t of c.tracks)if(t.property==='d' && motionSources.has(t.target)){const src=nodes.find(n=>n.id===t.target);for(const k of t.keyframes){const v=unwrap(k.value);if(typeof v==='string')count(v,src.transform,false);}}
      for(const m of machines)for(const st of m.states) {
        const o=st.overrides;
        if(Array.isArray(o)){for(const x of o)if(x && x.property==='d' && motionSources.has(x.nodeId || x.target)){const v=unwrap(x.value);if(typeof v==='string')count(v,nodes.find(n=>n.id===(x.nodeId || x.target)).transform,false);}}
        else if(o && typeof o==='object')for(const id of Object.keys(o))if(motionSources.has(id) && o[id] && typeof o[id]==='object' && own(o[id],'d')){const v=unwrap(o[id].d);if(typeof v==='string')count(v,nodes.find(n=>n.id===id).transform,false);}
      }
    }
    const validatePatch=(id,props)=>{if(boneIds.has(id)) bonePatch(props);if(own(props,'motionProgress')){if(!nodes.some(n=>n.id===id && n.motionPath))throw new Error('motionProgress requires a motionPath binding');weightValue(unwrap(props.motionProgress));}};
    for(const c of clips) for(const t of c.tracks) {
      if(c.blendMode==='additive' && !boneIds.has(t.target)) {
        const n=nodes.find(n=>n.id===t.target);
        if(!n)throw new Error('Unknown additive target: '+t.target);
        const kind={width:'rect',height:'rect',radius:'circle',fontSize:'text'}[t.property];
        if(kind && n.kind && n.kind.type!==kind)throw new Error('Additive property unsupported for node kind');
      }
      for(const k of t.keyframes) validatePatch(t.target,{[t.property]:k.value});
    }
    for(const m of machines) for(const s of m.states) {
      if(Array.isArray(s.overrides)) for(const o of s.overrides) validatePatch(o.nodeId || o.target,{[o.property]:o.value});
      else for(const [id,props] of Object.entries(s.overrides || {})) validatePatch(id,props);
    }
    const validateActions=items=>{for(const a of items || []){if((a.type==='setVariable' || a.type==='variable.set') && blendSpaces.some(s=>s.parameters.includes(a.name || a.variable)))blendParameter(a.value);if(a.type==='setProperty' && a.property==='motionProgress')validatePatch(a.nodeId || a.target,{[a.property]:a.value});if(a.type==='blendClip') {
      const c=clips.find(c=>c.id===a.clipId);
      if(!c || c.blendMode==='override')throw new Error('blendClip requires an existing weighted or additive clip: '+a.clipId);
      weightValue(a.weight);fadeDuration(a.duration===undefined?0:a.duration);
    }}};
    for(const m of machines) {
      for(const s of m.states) {for(const key of ['onEnter','entry','exit','onExit'])validateActions(s[key]);for(const t of s.transitions || [])validateActions(t.actions);}
      for(const t of m.transitions || [])validateActions(t.actions);
    }
    for(const b of project.interactions || [])validateActions(b.actions);
    for(const m of machines)for(const t of [...(m.transitions || []),...m.states.flatMap(s=>s.transitions || [])])
      {if(t.blend!=null && (finite(t.blend,'Transition blend')<0 || t.blend>60))throw new Error('Transition blend must be 0..60 seconds');
      if(t.after!=null && (finite(t.after,'Timer after')<0 || t.after>3600))throw new Error('Timer after must be 0..3600 seconds');}
    const nodeIds=new Set(nodes.map(n=>n.id));
    const followers=unique(list(project.followers),'follower').map(f=>{
      if(!nodeIds.has(f.target) && !boneIds.has(f.target))throw new Error('Unknown follower target: '+f.target);
      const mode=f.mode===undefined?'offset':f.mode;if(!['offset','rotate'].includes(mode))throw new Error('Unsupported follower mode: '+mode);
      const strength=f.strength===undefined?1:finite(f.strength,'Follower strength');if(strength<0 || strength>1)throw new Error('Follower strength must be 0..1');
      const max=finite(f.max,'Follower max');if(max<0 || (mode==='rotate' && max>180))throw new Error('Invalid follower max');
      const smoothing=f.smoothing===undefined?0:finite(f.smoothing,'Follower smoothing');if(smoothing<0 || smoothing>10)throw new Error('Follower smoothing must be 0..10');
      if(f.when!=null){const m=machines.find(m=>m.id===f.when.machine);if(!m || !Array.isArray(f.when.states) || !f.when.states.every(s=>m.states.some(st=>st.id===s)))throw new Error('Unknown follower condition: '+f.id);}
      return {...f,mode,strength,max,smoothing,restAngle:f.restAngle==null?0:finite(f.restAngle,'Follower restAngle')};
    });
    unique([...nodes,...bones,...clips,...machines,...followers,...blendSpaces,...assets,...(project.audioCues || []),...clips.flatMap(c=>c.audioCues || [])].filter(x=>x.id!=null),'global');
    return {...project,nodes,bones,clips,machines,assets,followers,blendSpaces};
  }
  function owningSvg(el) {
    for(let n=el;n;n=n.parentNode)if((n.tagName || '').toLowerCase()==='svg')return n;
    return el && el.ownerSVGElement;
  }
  function projectError(code, message, cause) {
    const e = new Error(message);
    e.name = 'SplinelyProjectError';
    e.code = code; // 'missing-metadata' | 'ambiguous-metadata' | 'unsupported-encoding' | 'invalid-json' | 'invalid-sply'
    if (cause) e.cause = cause;
    return e;
  }
  const isBinaryProject = v => v != null && typeof v === 'object' && (v instanceof Uint8Array || (typeof ArrayBuffer === 'function' && (v instanceof ArrayBuffer || ArrayBuffer.isView(v))) || Object.prototype.toString.call(v) === '[object ArrayBuffer]');
  function decodeBinaryProject(decode, input) {
    try { return decode(input); } catch (e) { throw projectError('invalid-sply', 'Invalid splinely-project SPLY: ' + (e && e.message), e); }
  }
  function readProject(svg) {
    // Same rule as Rust `Project::from_svg_metadata`: an SVG-namespace direct child of the root svg; exactly one.
    const candidates=[...svg.querySelectorAll('[id="splinely-project"], [data-splinely-project]')].filter(el=>el.parentNode===svg && (!el.namespaceURI || el.namespaceURI==='http://www.w3.org/2000/svg'));
    if (!candidates.length) throw projectError('missing-metadata', 'Missing splinely-project metadata');
    if (candidates.length>1) throw projectError('ambiguous-metadata', 'Multiple splinely-project metadata blocks');
    const block=candidates[0];
    const encoding = block.getAttribute ? block.getAttribute('data-encoding') : null;
    if (encoding === 'sply-base64') return decodeBinaryProject(SplyDecode.decodeSplyBase64, block.textContent);
    if (encoding != null && encoding !== '' && encoding !== 'json') throw projectError('unsupported-encoding', 'Unsupported splinely-project data-encoding: ' + encoding);
    try { return JSON.parse(block.textContent); } catch (e) { throw projectError('invalid-json', 'Invalid splinely-project JSON: ' + e.message, e); }
  }
  function createRuntime(svg, project, options) {
    options = options || {};
    if (!svg || typeof svg.querySelectorAll !== 'function') throw new Error('An SVG root is required');
    // The raw project object (passed in as an object or SPLY bytes, or decoded once from <metadata>) is exposed
    // as api.sourceProject so hosts need not parse/decode the same project a second time.
    const source = project == null ? readProject(svg) : isBinaryProject(project) ? decodeBinaryProject(SplyDecode.decodeSply, project) : project;
    project = normalize(source);
    if (svg.splinely) throw new Error('SVG already has a Splinely runtime');
    const env=options.window || (svg.ownerDocument && svg.ownerDocument.defaultView) || host;
    const diagnostics=[], subscriptions=new Map(), cleanup=[], explicit=new Map(), nodes=new Map(), clips=new Map(), machines=new Map();
    const bones=new Map(project.bones.map(b=>[b.id,b]));
    const groupRigs=new Map((project.groups || []).filter(g=>g && g.rig).map(g=>[g.id,g.rig]));
    const groupIds=new Set((project.groups || []).filter(g=>g).map(g=>g.id));
    let bonePoses=[];
    const variables=new Map(Object.entries(project.variables || {}).map(([k,v]) => [k,v]));
    let dead=false, raf=null, last=null, api, processing=false, ticking=false, liveRegion=null, afterRender=null, frameHooks=null;
    let syncClock=()=>{};
    const queue=[];
    const report = error => { const e=error instanceof Error ? error : new Error(String(error)); diagnostics.push(e); if (diagnostics.length>100) diagnostics.shift(); dispatch('error',{error:e}); return e; };
    function dispatch(name, detail, target) {
      const keys = target == null ? [name] : [target+'\u0000'+name,name];
      for (const key of keys) for (const fn of [...(subscriptions.get(key) || [])]) {
        try { const result=fn(detail,api); if (result && typeof result.catch === 'function') result.catch(e => { if (name !== 'error') report(e); }); }
        catch(e) { if (name !== 'error') report(e); }
      }
    }
    // Every API call passes through ensure(): it also wakes a sleeping auto-tick loop (see autoTick below).
    let wake=null;
    const ensure = () => { if (dead) throw new Error('Splinely runtime destroyed'); if (wake !== null) wake(); };
    function on(a,b,c) {
      ensure(); const key=typeof b === 'function' ? a : a+'\u0000'+b, fn=typeof b === 'function' ? b : c;
      if (typeof fn !== 'function') throw new Error('Event handler must be a function');
      if (!subscriptions.has(key)) subscriptions.set(key,new Set()); subscriptions.get(key).add(fn);
      return () => { const set=subscriptions.get(key); if(set) {set.delete(fn); if(!set.size) subscriptions.delete(key);} };
    }
    const elements=new Map(); if(svg.id) elements.set(svg.id,svg);
    for(const el of svg.querySelectorAll('[id]'))if((!owningSvg(el) || owningSvg(el)===svg) && (!el.namespaceURI || el.namespaceURI==='http://www.w3.org/2000/svg') && !['svg','script','metadata'].includes((el.tagName || '').toLowerCase()))elements.set(el.id,el);
    // Nested base values (colours, gradients, wrapped numbers) are private copies the runtime never mutates, so
    // snapshotProps() can keep them by reference (see `owned`).
    const owned=new WeakSet();
    const ownCopy=v=>{if(v===null || typeof v!=='object')return v;let c;try{c=snapshot1(v,0);}catch(_){return v;}const mark=x=>{if(x!==null && typeof x==='object'){owned.add(x);for(const k of Object.keys(x))mark(x[k]);}};mark(c);return c;};
    for (const node of project.nodes) {
      const el=elements.get(node.id); if (!el && node.visible!==false) {report('Missing SVG node: '+node.id); continue;}
      const core=!!node.kind;
      const base=core ? {x:0,y:0,rotation:0,scaleX:1,scaleY:1,opacity:1,fill:{r:80/255,g:140/255,b:240/255,a:1},...node.transform,...node.style} : {...node.properties};
      if(core) { for(const k of ['width','height','radius','fontSize','d']) if(node.kind[k]!=null) base[k]=node.kind[k]; if(node.gradient) base.gradient=node.gradient; if(node.motionPath) base.motionProgress=node.motionPath.progress===undefined?0:node.motionPath.progress; }
      for(const k of Object.keys(base))base[k]=ownCopy(base[k]);
      nodes.set(node.id,{id:node.id,el,base,core,visible:node.visible!==false,boneId:node.boneId,gradient:!!(core && node.gradient),original:new Map(),lastKeys:new Set(),spareKeys:new Set(),propsA:null,propsB:null,written:new Map(),lastProps:null,lastReports:null,atRest:false,restPoses:null,natMode:0,natStyle:null});
    }
    // Adapter allows exported elements without a corresponding node descriptor.
    function getNode(id) {
      if(!nodes.has(id) && elements.has(id)) nodes.set(id,{id,el:elements.get(id),base:{},core:false,original:new Map(),lastKeys:new Set(),spareKeys:new Set(),propsA:null,propsB:null,written:new Map(),lastProps:null,lastReports:null,atRest:false,restPoses:null,natMode:0,natStyle:null});
      if(!nodes.has(id)) throw new Error('Unknown node: '+id); return nodes.get(id);
    }
    function attr(node,name,value) {
      if(dead) return;
      if(!node.original.has(name)) node.original.set(name,node.el.getAttribute(name));
      // Skip writes that would leave the attribute unchanged (last value this runtime wrote).
      const text=value == null ? null : String(value), written=node.written;
      if(written.get(name)===text && written.has(name)) return;
      if(text===null) node.el.removeAttribute(name); else node.el.setAttribute(name,text);
      written.set(name,text);
    }
    const gradientWritten=new WeakMap();
    function put(el,name,value) {
      let w=gradientWritten.get(el);if(!w){w=new Map();gradientWritten.set(el,w);}
      const text=String(value);if(w.get(name)===text)return;
      el.setAttribute(name,value);w.set(name,text);
    }
    const aliases={radius:'r',fontSize:'font-size',strokeWidth:'stroke-width',scaleX:'scaleX',scaleY:'scaleY'};
    const attrName=k => Object.prototype.hasOwnProperty.call(aliases,k)?aliases[k]:k;
    const supported=new Set('x y cx cy r radius rx ry width height fill stroke opacity fill-opacity stroke-opacity strokeWidth stroke-width fontSize font-size rotation scaleX scaleY transform d points visibility'.split(' '));
    // Update the exported <linearGradient>/<radialGradient> referenced by fill="url(#…)".
    function applyGradient(node,g) {
      if(!node.el || !g || !Array.isArray(g.stops)) return;
      const ref=/url\(#([^)]+)\)/.exec(node.el.getAttribute('fill') || '');
      const el=ref && elements.get(ref[1]);
      if(!el) return;
      if(!g.radial) {
        const a=(Number(g.angle)||0)*Math.PI/180, r=v=>Math.round(v*1e6)/1e6+0;
        put(el,'x1',r(0.5-Math.cos(a)/2)); put(el,'y1',r(0.5-Math.sin(a)/2));
        put(el,'x2',r(0.5+Math.cos(a)/2)); put(el,'y2',r(0.5+Math.sin(a)/2));
      }
      const stops=[...(el.children || [])].filter(c=>(c.tagName || '').toLowerCase()==='stop');
      stops.forEach((stop,i)=>{
        const s=g.stops[i], c=s && color(s.color); if(!c) return;
        put(stop,'offset',clamp(Number(s.offset)||0,0,1));
        put(stop,'stop-color',`rgb(${c.slice(0,3).map(v=>Math.round(clamp(v,0,1)*255)).join(',')})`);
        put(stop,'stop-opacity',clamp(c[3],0,1));
      });
    }
    // Followers: current smoothed value per follower, [dx,dy] or [degrees,0].
    const follow=new Map(); let pointer=null;
    // Honour the viewer's "reduce motion" setting unless the host decides: no
    // looping autoplay, instant state changes, resting followers.
    const reducedMotion=options.reducedMotion!=null?!!options.reducedMotion:(()=>{try{return !!(env.matchMedia && env.matchMedia('(prefers-reduced-motion: reduce)').matches);}catch(_){return false;}})();
    // Node-based follower origins depend only on the static project descriptor: computed once per follower.
    const nodeOrigins=new Map();
    function followerOrigin(f) {
      if(Array.isArray(f.origin))return f.origin;
      let cached=nodeOrigins.get(f);if(cached!==undefined)return cached;
      const n=project.nodes.find(n=>n.id===f.target);
      if(n && n.kind){cached=nodeOrigin(n);nodeOrigins.set(f,cached);return cached;}
      const pose=bonePoses.find(p=>p.id===f.target);return pose?[pose.x,pose.y]:[0,0];
    }
    function nodeOrigin(n) {
        const k=n.kind,t={x:0,y:0,rotation:0,scaleX:1,scaleY:1,...n.transform};
        const box=k.type==='rect'?[k.x,k.y,k.width,k.height]:k.type==='circle'?[k.cx-k.radius,k.cy-k.radius,2*k.radius,2*k.radius]:k.type==='path'?k.bounds:[k.x,k.y-k.fontSize,String(k.text).length*k.fontSize*0.6,k.fontSize*1.2];
        const a=t.rotation*Math.PI/180,c=Math.cos(a),s=Math.sin(a),xs=[],ys=[];
        for(const [px,py] of [[box[0],box[1]],[box[0]+box[2],box[1]],[box[0],box[1]+box[3]],[box[0]+box[2],box[1]+box[3]]]){const sx=px*t.scaleX,sy=py*t.scaleY;xs.push(t.x+sx*c-sy*s);ys.push(t.y+sx*s+sy*c);}
        return [(Math.min(...xs)+Math.max(...xs))/2,(Math.min(...ys)+Math.max(...ys))/2];
    }
    function followerGoal(f) {
      if(!pointer || reducedMotion)return [0,0];
      if(f.when){const m=machines.get(f.when.machine);if(!m || !f.when.states.includes(m.state))return [0,0];}
      const [ox,oy]=followerOrigin(f),dx=pointer[0]-ox,dy=pointer[1]-oy;
      if(f.mode==='rotate'){
        if(dx===0 && dy===0)return [0,0];
        let angle=(Math.atan2(dy,dx)*180/Math.PI-f.restAngle)%360;if(angle>180)angle-=360;if(angle<-180)angle+=360;
        return [clamp(angle*f.strength,-f.max,f.max),0];
      }
      let x=dx*f.strength,y=dy*f.strength;const length=Math.hypot(x,y);
      if(length>f.max && length>0){x*=f.max/length;y*=f.max/length;}
      return [x,y];
    }
    function updateFollowers(dt) {
      let moved=false;
      for(const f of project.followers) {
        const goal=followerGoal(f);let cur=follow.get(f.id);
        if(cur===undefined){cur=[0,0];follow.set(f.id,cur);}
        const keep=f.smoothing>0?Math.exp(-4*dt/f.smoothing):0;
        let x=goal[0]+(cur[0]-goal[0])*keep,y=goal[1]+(cur[1]-goal[1])*keep;
        if(Math.abs(x)<1e-6)x=0;if(Math.abs(y)<1e-6)y=0;
        // Updated in place: the stored pair is only read by renderFrame().
        if(x!==cur[0] || y!==cur[1]){moved=true;cur[0]=x;cur[1]=y;}
      }
      return moved;
    }
    function fireTimers() {
      for(const m of machines.values()) {
        if(m.halted)continue;
        const state=m.states.get(m.state);
        if(!m.timers)m.timers=new Map();
        let list=m.timers.get(state);
        if(!list){list=[...(state.transitions || []),...(m.transitions || [])].filter(t=>t.after!=null);m.timers.set(state,list);}
        let t;
        for(let i=0;i<list.length;i++){const x=list[i];if((x.from==null || x.from===m.state) && x.after<=m.elapsed+1e-9 && guard(x.guard)){t=x;break;}}
        if(t)enqueue({id:m.id,timer:t,event:'after'});
      }
    }
    function advanceBlends(dt) {
      let changed=false;
      for(const m of machines.values())m.elapsed=(m.elapsed || 0)+dt;
      fireTimers();
      for(const m of machines.values())if(m.fade){m.fade.elapsed=Math.min(m.fade.duration,m.fade.elapsed+dt);if(m.fade.elapsed>=m.fade.duration)m.fade=null;changed=true;}
      // Followers count as at rest only after a real time step left them unchanged (dt=0 never moves them).
      // While catching up a slept (idle) span they are at their resting fixed point and stay untouched.
      if(!creditTick){const moved=updateFollowers(dt);if(moved || dt>0)followMoving=moved;if(moved)changed=true;}
      if(changed)dirty=true;
      return changed;
    }
    // Frame cache: render() recomputes only after something that feeds a frame changed
    // (dirty); an unchanged frame replays the diagnostics the last full render produced.
    // Native offload state (see nativeCheck); pendingDelta/armInterval are set by the auto-tick loop.
    const nat={on:false,hidden:false,sig:null,anims:new Map(),boneNative:false,clockMs:()=>null};
    let explicitRev=0, hiddenTicking=false, shadowing=false, pendingDelta=()=>0, armInterval=()=>{};
    let followMoving=true, creditTick=false, dirty=true, renderDepth=0, reentered=false, renderReports=[], reportSink=renderReports, boneCache=null, renderBudget=Infinity;
    const rr=x=>{if(shadowing)return;reportSink.push(x);report(x);};
    const emptyMap=new Map(), noPoses=[], noEntries=[], stateEntries=new WeakMap();
    const transformKeys=new Set(['x','y','rotation','scaleX','scaleY']);
    const motionNodes=project.nodes.filter(n=>n.motionPath), motionTr={x:0,y:0,rotation:0,scaleX:1,scaleY:1};
    const entries=state=>{
      if(!state)return noEntries;
      let out=stateEntries.get(state);if(out)return out;
      const overrides=state.overrides;out=[];
      if(Array.isArray(overrides)) for(const o of overrides) out.push([o.nodeId || o.target,o.property,o.value]);
      else for(const [id,p] of Object.entries(overrides || {})) for(const [k,v] of Object.entries(p)) out.push([id,k,v]);
      stateEntries.set(state,out);return out;
    };
    // Snapshot of a node's last applied properties; null when it cannot be compared safely. `props` is this frame's
    // own object, so it is kept as the snapshot: only nested values the runtime does not own are replaced by copies.
    function snapshot1(v,depth) {
      if(v===null || typeof v!=='object')return v;
      if(depth>8)throw 0;
      if(Array.isArray(v))return v.map(x=>snapshot1(x,depth+1));
      const out={};for(const k of Object.keys(v))out[k]=snapshot1(v[k],depth+1);return out;
    }
    function snapshotProps(props) {
      for(const k in props){const v=props[k];if(v!==null && typeof v==='object' && !owned.has(v)){try{props[k]=snapshot1(v,0);}catch(_){return null;}}}
      return props;
    }
    function sameProps(a,b) {
      let n=0;
      for(const k in b) {
        if(!own(a,k))return false;
        const x=a[k],y=b[k];
        if(x!==y && (x===null || y===null || typeof x!=='object' || typeof y!=='object' || !equal(x,y)))return false;
        n++;
      }
      for(const k in a)n--;
      return n===0;
    }
    function render() {
      if(dead) return;
      // Clocks advanced by the hidden figure's timers draw nothing: the frame stays dirty until it is shown.
      if(hiddenTicking)return;
      // Over the tick's budget of intermediate frames: the tick's final render draws the result.
      if(renderBudget<=0)return;
      renderBudget--;
      // Native animations are re-planned and re-synced once clip times are final: not during a tick's
      // intermediate segment renders (the tick checks again when it ends).
      if(nat.on && renderDepth===0 && !ticking)nativeCheck();
      if(renderDepth>0)reentered=true;
      if(!dirty) {
        const again=renderReports;
        for(let i=0;i<again.length;i++)report(again[i]);
        return;
      }
      dirty=false;
      const outer=reportSink, sink=[];
      renderReports=sink;reportSink=sink;renderDepth++;
      try {renderFrame();}
      catch(e) {dirty=true;throw e;}
      finally {reportSink=outer;if(--renderDepth===0 && reentered){dirty=true;reentered=false;}}
      if(afterRender)afterRender();
    }
    // Per-frame working set. `values` holds property objects (fresh copies of the node base) only for nodes
    // that something touches this frame; every other node reads its base and, when it is already showing that
    // base, is skipped by the apply loop. Bone working objects are reused and reset from the rig each frame.
    const boneList=[...bones.values()], plan=bonePlan(boneList), boneSolved=new Float64Array(boneList.length*3);
    let values=new Map(), boneValues=null, boneWork=null;
    const freshBones=()=>{boneValues=new Map();for(const [id,b] of bones)boneValues.set(id,{...b});boneWork=[...boneValues.values()];};
    freshBones();
    const baseOf=id=>{const n=nodes.get(id);return n===undefined?undefined:n.base;};
    const touch=id=>{let t=values.get(id);if(t===undefined){const n=getNode(id);t={...n.base};values.set(id,t);}return t;};
    function renderFrame() {
      if(renderDepth<=1){values.clear();computeFrame();return;}
      // Re-entered from an error handler during a frame: use private working state so the outer frame is untouched.
      const saved=[values,boneValues,boneWork];values=new Map();freshBones();
      try{computeFrame();}finally{values=saved[0];boneValues=saved[1];boneWork=saved[2];}
    }
    function computeFrame(shadow) {
      for(let i=0;i<boneList.length;i++){const b=boneList[i],w=boneWork[i];w.x=b.x;w.y=b.y;w.rotation=b.rotation;}
      const merge=(id,props) => { try {
        if(bones.has(id)) Object.assign(boneValues.get(id),bonePatch(props));
        else Object.assign(touch(id),props);
      } catch(e){rr(e);} };
      // Single-property merge for the hot track loop; same checks and errors as merge().
      const mergeOne=(id,k,v) => { try {
        if(bones.has(id)) {
          if(!boneProperties.has(k)) throw new Error('Unsupported bone property: '+k);
          if(v && typeof v==='object' && (v.type!=='number' || !own(v,'value'))) throw new Error('Bone '+k+' must be numeric');
          boneValues.get(id)[k]=finite(unwrap(v),'Bone '+k);
        } else touch(id)[k]=v;
      } catch(e){rr(e);} };
      const current=(id,property)=>{const v=bones.has(id)?boneValues.get(id):(values.get(id) || baseOf(id));return v && v[property];};
      for(const m of machines.values()) {
        const now=entries(m.states.get(m.state));
        if(!m.fade) {for(let i=0;i<now.length;i++)mergeOne(now[i][0],now[i][1],now[i][2]);continue;}
        // Crossfade every property either state sets; a property only one state
        // sets fades between that value and the value underneath.
        const before=entries(m.states.get(m.fade.from)), t=smoothstep(m.fade.elapsed/m.fade.duration), touched=new Map();
        for(const [id,k] of [...before,...now])touched.set(id+'\u0000'+k,[id,k]);
        for(const [id,k] of touched.values()) {
          const under=current(id,k), pick=list=>{const hit=list.filter(([i,p])=>i===id && p===k).pop();return hit?hit[2]:under;};
          const a=pick(before), b=pick(now);
          merge(id,{[k]:a===undefined?b:b===undefined?a:mixValues(a,b,t)});
        }
      }
      const spaceFactors=project.blendSpaces.length?spaceWeights(project.blendSpaces,k=>variables.get(k)):emptyMap;
      const restValue=(id,property)=>{
        if(bones.has(id))return bones.get(id)[property];
        const n=getNode(id);let v=n.base[property];
        if(v===undefined && !n.core && n.el){const name=attrName(property);v=n.original.has(name)?n.original.get(name):n.el.getAttribute(name);}
        if(v==null || v==='none')return property==='stroke'?{r:0,g:0,b:0,a:0}:['scaleX','scaleY','opacity'].includes(property)?1:0;
        return ['fill','stroke'].includes(property)?v:Number(unwrap(v));
      };
      const additive=(id,property,sample,weight)=>{
        if(weight===0)return;
        const reference=restValue(id,property), under=current(id,property);
        const a=vectorOf(under==null?reference:under), b=vectorOf(sample), r=vectorOf(reference);
        if(!a || !b || !r || a.shape!==b.shape || a.shape!==r.shape)throw new Error('Additive reference and sample must have matching numeric/RGBA types');
        const nums=a.nums.map((v,i)=>v+weight*(b.nums[i]-r.nums[i]));
        if(a.kind==='c' || ['opacity','motionProgress'].includes(property))nums.forEach((v,i)=>nums[i]=clamp(v,0,1));
        else if(['width','height','radius','fontSize'].includes(property))nums[0]=Math.max(0,nums[0]);
        nums.forEach(v=>finite(v,'Additive result'));
        merge(id,{[property]:fromVector(a,nums)});
      };
      let group=null;
      const flush=()=>{
        if(group===null)return;
        for(const [id,props] of group)for(const [property,samples] of props) {
          const total=samples.reduce((s,v)=>s+v.weight,0);if(total===0)continue;
          try {
            let base=(bones.has(id)?boneValues.get(id):values.get(id) || getNode(id).base)[property];
            const residual=Math.max(0,1-total), divisor=Math.max(1,total);
            // Adapter baselines come from original DOM, never a previously animated frame.
            if(residual>0 && base===undefined && !bones.has(id)) {
              const node=getNode(id), name=attrName(property);
              if(!node.core && node.el) {
                base=node.original.has(name)?node.original.get(name):node.el.getAttribute(name);
                if(typeof samples[0].value==='number' && base!=null)base=Number(base);
                if(property==='stroke' && base==='none')base=null;
              }
            }
            let value;
            if(typeof samples[0].value==='number') {
              const fallback=['scaleX','scaleY','opacity'].includes(property)?1:0;
              const baseline=residual===0?0:finite(base==null?fallback:unwrap(base),'Weighted baseline');
              value=baseline*residual+samples.reduce((s,v)=>s+v.value*(v.weight/divisor),0);
              finite(value,'Weighted result');
            } else if(['gradient','d'].includes(property)) {
              // Structured values blend number by number when shapes match;
              // otherwise the heaviest contribution wins.
              const vectors=samples.map(v=>vectorOf(v.value)), baseVector=vectorOf(base);
              const shape=vectors[0] && vectors[0].shape;
              if(vectors.every(v=>v && v.shape===shape) && (residual===0 || (baseVector && baseVector.shape===shape))) {
                const nums=vectors[0].nums.map((_,i)=>(residual===0?0:baseVector.nums[i]*residual)+vectors.reduce((s,v,k)=>s+v.nums[i]*(samples[k].weight/divisor),0));
                value=fromVector(vectors[0],nums);
              } else {
                const heaviest=samples.reduce((a,b)=>b.weight>a.weight?b:a);
                value=heaviest.weight>=residual?heaviest.value:base;
              }
            } else {
              const baseline=residual===0?[0,0,0,0]:color(unwrap(base)) || (property==='stroke' && base==null?[0,0,0,0]:null);
              if(!baseline)throw new Error('Weighted baseline must be RGBA');
              const channels=baseline.map((b,i)=>b*residual+samples.reduce((s,v)=>s+color(v.value)[i]*(v.weight/divisor),0));
              value={r:channels[0],g:channels[1],b:channels[2],a:channels[3]};
            }
            merge(id,{[property]:value});
          } catch(e){rr(e);}
        }
        group=null;
      };
      for(let ci=0;ci<clipOrder.length;ci++) {
        const c=clipOrder[ci];if(!c.active)continue;
        const mode=c.blendMode, table=tableOf(c), time=c.time;
        if(mode!=='weighted')flush();
        if(mode==='weighted') {
          // A clip contributes at most once per property (last duplicate track wins).
          const sampled=new Map();
          for(let i=0;i<table.length;i++) {
            const t=table[i], value=t.ct?evalCompiled(t.ct,time):evaluateTrack(t.track,time);if(value===undefined)continue;
            if(!sampled.has(t.target))sampled.set(t.target,new Map());sampled.get(t.target).set(t.property,value);
          }
          if(c.weight>0)for(const [id,props] of sampled) {
            if(group===null)group=new Map();
            if(!group.has(id))group.set(id,new Map());
            for(const [property,value] of props) {
              const target=group.get(id);if(!target.has(property))target.set(property,[]);
              const weight=c.weight*(spaceFactors.has(c.id)?spaceFactors.get(c.id):1);
              if(weight>0)target.get(property).push({value,weight});
            }
          }
        } else if(mode==='additive') {
          for(let i=0;i<table.length;i++) {
            const t=table[i];if(t.nat && !shadow)continue;
            const value=t.ct?evalCompiled(t.ct,time):evaluateTrack(t.track,time);if(value===undefined)continue;
            try{additive(t.target,t.property,value,c.weight);}catch(e){rr(e);}
          }
        } else {
          for(let i=0;i<table.length;i++) {
            const t=table[i];if(t.nat && !shadow)continue;
            const value=t.ct?evalCompiled(t.ct,time):evaluateTrack(t.track,time);if(value===undefined)continue;
            mergeOne(t.target,t.property,value);
          }
        }
      }
      flush();
      for(const [id,p] of explicit) merge(id,p);
      for(const f of project.followers) {
        const [a,b]=follow.get(f.id) || [0,0];if(a===0 && b===0)continue;
        const v=bones.has(f.target)?boneValues.get(f.target):(values.get(f.target) || baseOf(f.target) || {});
        const num=k=>{const x=unwrap(v[k]);return typeof x==='number'?x:0;};
        if(f.mode==='rotate')merge(f.target,{rotation:num('rotation')+a});
        else merge(f.target,{x:num('x')+a,y:num('y')+b});
      }
      for(const n of motionNodes) {
        try {
          const binding=n.motionPath,source=values.get(binding.path) || baseOf(binding.path),props=nodes.has(n.id)?touch(n.id):undefined;
          motionTr.x=unwrap(source.x);motionTr.y=unwrap(source.y);motionTr.rotation=unwrap(source.rotation);motionTr.scaleX=unwrap(source.scaleX);motionTr.scaleY=unwrap(source.scaleY);
          const [x,y,angle]=motionSample(unwrap(source.d),motionTr,unwrap(props.motionProgress));
          props.x=(unwrap(props.x)||0)+x;props.y=(unwrap(props.y)||0)+y;
          if(binding.orient)props.rotation=(unwrap(props.rotation)||0)+angle;
        } catch(e){rr(e);}
      }
      let poses;
      try {
        if(!bones.size){bonePoses=noPoses;poses=emptyMap;}
        else {
          // Bone solving is pure in (x, y, rotation) of each bone (id, parent and length are fixed per rig):
          // reuse the last poses, and the same poses Map, when none of them changed.
          let same=boneCache!==null;
          for(let i=0,j=0;same && i<boneWork.length;i++,j+=3){const b=boneWork[i];same=Object.is(boneSolved[j],b.x) && Object.is(boneSolved[j+1],b.y) && Object.is(boneSolved[j+2],b.rotation);}
          if(same){bonePoses=boneCache.poses;poses=boneCache.map;}
          else {
            bonePoses=solveBonesPlanned(boneWork,plan); poses=new Map(bonePoses.map(p=>[p.id,p]));
            for(let i=0,j=0;i<boneWork.length;i++,j+=3){const b=boneWork[i];boneSolved[j]=b.x;boneSolved[j+1]=b.y;boneSolved[j+2]=b.rotation;}
            boneCache={poses:bonePoses,map:poses};
          }
        }
      }
      catch(e) {rr(e);return;} // Keep the last valid frame on FK overflow.
      if(shadow)return;
      for(const node of nodes.values()) {
        if(dead) return;
        let props=values.get(node.id);
        const touched=props!==undefined, binding=node.boneId;
        if(!touched) {
          // Nothing contributed and the node already shows its base (with the same bone poses): only the
          // diagnostics of that application are replayed, exactly as the sameProps() path below would.
          if(node.atRest && (binding==null || node.restPoses===poses)) {
            const again=node.lastReports;
            if(again!==null)for(let i=0;i<again.length;i++)rr(again[i]);
            continue;
          }
          // Core nodes reuse two property objects (base keys only; never the one held as lastProps).
          if(node.core) {
            const base=node.base;props=node.propsA===node.lastProps?node.propsB:node.propsA;
            if(props===null){props={...base};if(node.propsA===null)node.propsA=props;else node.propsB=props;}
            else for(const k in base)props[k]=base[k];
          } else props={...node.base};
        }
        node.atRest=false;
        if(binding!=null && node.natMode!==1 && node.natMode!==3) {
          try {
            const pose=poses.get(binding), angle=(pose.rotation%360)*Math.PI/180, c=Math.cos(angle), s=Math.sin(angle);
            const x=finite(unwrap(props.x==null?0:props.x),'Transform x'), y=finite(unwrap(props.y==null?0:props.y),'Transform y');
            props.x=finite(pose.x+c*x-s*y,'Attached x'); props.y=finite(pose.y+s*x+c*y,'Attached y');
            props.rotation=finite(pose.rotation+finite(unwrap(props.rotation==null?0:props.rotation),'Transform rotation'),'Attached rotation');
          } catch(e) {rr(e);continue;}
        }
        if(!node.el) {if(!touched){node.atRest=true;node.restPoses=poses;}continue;} // Hidden descriptors remain valid animation/override targets.
        // Same properties as the last application: the DOM already shows the result.
        if(node.lastProps!==null && props.gradient===undefined && sameProps(node.lastProps,props)) {
          const again=node.lastReports;
          if(again!==null)for(let i=0;i<again.length;i++)rr(again[i]);
          if(!touched){node.atRest=true;node.restPoses=poses;}
          continue;
        }
        const keys=node.spareKeys, mark=reportSink.length;keys.clear();
        if(node.visible===false) {attr(node,'visibility','hidden'); keys.add('visibility');}
        const composed=node.core || binding!=null;
        if(composed) {
          try {
            const component=(key,fallback)=>finite(props[key]==null?fallback:unwrap(props[key]),'Transform '+key);
            // natMode 2: the pose is carried by the CSS translate/rotate/scale (and native animations of them).
            if(node.natMode===2)attr(node,'transform','translate(0 0) rotate(0) scale(1 1)');
            else attr(node,'transform',`translate(${component('x',0)} ${component('y',0)}) rotate(${component('rotation',0)}) scale(${component('scaleX',1)} ${component('scaleY',1)})`);
          } catch(error){rr(error);} keys.add('transform');
        }
        const propKeys=Object.keys(props);
        for(let pi=0;pi<propKeys.length;pi++) {
          if(dead) return;
          const key=propKeys[pi], raw=props[key];
          if(composed && transformKeys.has(key)) continue;
          if(composed && key==='transform') {rr('Raw transform is unsupported on composed nodes');continue;}
          // Never turn schema properties into scripts, URLs, or markup.
          if(key==='motionProgress') continue;
          if(key==='gradient') {applyGradient(node,unwrap(raw));continue;}
          if(!supported.has(key)) {rr('Unsupported property: '+key);continue;}
          // A gradient fill is static SVG; colour values must not replace its url().
          if(node.gradient && (key==='fill' || key==='fill-opacity')) continue;
          if(node.visible===false && key==='visibility') continue;
          const name=attrName(key), value=unwrap(raw), col=color(value);
          if((key==='fill'||key==='stroke') && col) {
            attr(node,name,`rgb(${Math.round(clamp(col[0],0,1)*255)},${Math.round(clamp(col[1],0,1)*255)},${Math.round(clamp(col[2],0,1)*255)})`);
            attr(node,name+'-opacity',clamp(col[3],0,1)); keys.add(name+'-opacity');
          } else {
            if(Array.isArray(value) && !value.every(v=>typeof v==='number' && Number.isFinite(v))){rr('Property arrays must contain finite numbers');continue;}
            const serialized=Array.isArray(value)?value.join(' '):(node.core && key==='stroke' && value==null?'none':value);
            if(typeof serialized === 'string' && (/url\s*\(/i.test(serialized) || serialized.includes('\\'))){rr('URL properties are unsupported');continue;}
            if(typeof serialized==='number' && !Number.isFinite(serialized)){rr('Property must be finite');continue;}
            if(serialized && typeof serialized==='object'){rr('Unsupported property value');continue;}
            attr(node,name,serialized);
          }
          keys.add(name);
        }
        for(const key of node.lastKeys) if(!keys.has(key)) attr(node,key,node.original.get(key));
        node.spareKeys=node.lastKeys;node.lastKeys=keys;
        node.lastProps=snapshotProps(props);
        node.lastReports=reportSink.length>mark?reportSink.slice(mark):null;
        if(!touched && node.lastProps!==null && props.gradient===undefined){node.atRest=true;node.restPoses=poses;}
      }
    }
    // A group is set through its rig: x/y offset the move bone, rotation turns the turn bone.
    function setGroup(id,props) {
      const rig=groupRigs.get(id);
      if(!rig)throw new Error('Group '+id+' has no rig; animate the group once (e.g. splinely-cli keyframe --target '+id+') to create one');
      explicitRev++;
      if(props===null){explicit.delete(rig.moveBone);explicit.delete(rig.turnBone);dirty=true;render();return;}
      if(!props || typeof props!=='object' || Array.isArray(props)) throw new Error('Properties must be an object or null');
      const patch=bonePatch(props), move={}, turn={};
      for(const k of Object.keys(patch))(k==='rotation'?turn:move)[k]=patch[k];
      if(Object.keys(move).length)explicit.set(rig.moveBone,{...explicit.get(rig.moveBone),...move});
      if(Object.keys(turn).length)explicit.set(rig.turnBone,{...explicit.get(rig.turnBone),...turn});
      dirty=true;render();
    }
    function set(id,props) {ensure();explicitRev++;if(!bones.has(id) && !nodes.has(id) && groupIds.has(id))return setGroup(id,props);if(props && own(props,'motionProgress')) {if(!project.nodes.some(n=>n.id===id && n.motionPath))throw new Error('motionProgress requires a motionPath binding');weightValue(unwrap(props.motionProgress));}if(!bones.has(id))getNode(id); if(props===null) explicit.delete(id); else {if(!props || typeof props!=='object' || Array.isArray(props)) throw new Error('Properties must be an object or null');explicit.set(id,{...explicit.get(id),...(bones.has(id)?bonePatch(props):props)});}dirty=true;render();}
    function variable(id) {ensure();if(!variables.has(id)) throw new Error('Unknown variable: '+id);return {get:()=>variables.get(id),set:v=>{ensure();const old=variables.get(id);if((old===null?v!==null:typeof old!==typeof v) || Array.isArray(old)!==Array.isArray(v))throw new Error('Variable type mismatch: '+id);if(v===undefined || typeof v==='function' || (typeof v==='number' && !Number.isFinite(v)))throw new Error('Variable must be a JSON value');if(project.blendSpaces.some(s=>s.parameters.includes(id)))blendParameter(v);variables.set(id,v);dirty=true;render();emit('variable.change',{id,value:v});}};}
    function actions(items,detail) {
      for(const a of items || []) {
        try {
          switch(a.type) {
            case 'setProperty': set(a.nodeId,{[a.property]:a.value});break;
            case 'set': set(a.target,a.properties);break;
            case 'playClip': case 'clip.play': clip(a.clipId || a.clip).play();break;
            case 'blendClip': clip(a.clipId).fadeTo(a.weight,a.duration===undefined?0:a.duration);break;
            case 'stopClip': case 'clip.stop': clip(a.clipId || a.clip).stop();break;
            case 'clip.pause': clip(a.clipId || a.clip).pause();break;
            case 'clip.seek': clip(a.clipId || a.clip).seek(a.time);break;
            case 'setVariable': case 'variable.set': variable(a.name || a.variable).set(a.value);break;
            case 'emit': emit(typeof a.event === 'object'?a.event.name:a.event,detail,typeof a.event==='object'?a.event.target:undefined);break;
            case 'machine.send': send(a.machine,a.event,detail);break;
            case 'playAudio': case 'audio.play': audio.play(a.assetId || a.asset);break;
            default: throw new Error('Unsupported action: '+a.type);
          }
        } catch(e) {report(e);}
      }
    }
    function eventMatches(spec,event,target) { return typeof spec==='string' ? spec===event : spec && spec.name===event && (spec.target==null || spec.target===target); }
    function guard(g) {if(!g)return true; if(typeof g !== 'object' || !own(g,'variable')) {report('Unsupported guard');return false;} return variables.has(g.variable) && equal(variables.get(g.variable),g.equals);}
    function send(id,event,detail,target) {ensure();if(!machines.has(id)) throw new Error('Unknown machine: '+id);enqueue({id,event,detail,target});}
    function enqueue(item) {if(!processing){syncClock();ensure();}queue.push(item);drain();}
    function drain() {
      if(processing)return; processing=true;
      try {
        let count=0;
        while(queue.length) {
          if(++count>256) {queue.length=0;report('Event cycle limit exceeded (256)');cycleHit=true;break;}
          const e=queue.shift();
          if(e.broadcast) {dispatch(e.event,e.detail,e.target);queue.unshift(...[...machines.keys()].map(id=>({...e,id,broadcast:false})));continue;}
          const m=machines.get(e.id); if(!m || m.halted)continue;
          const state=m.states.get(m.state);
          const transitions=[...(state.transitions || []),...(m.transitions || [])];
          // Timer transitions fire from advanceBlends, never on events.
          const t=e.timer?(transitions.includes(e.timer) && guard(e.timer.guard)?e.timer:null):transitions.find(t=>t.after==null && (t.from==null || t.from===m.state || t.from==='*') && eventMatches(t.event,e.event,e.target) && guard(t.guard));
          if(!t)continue;
          const next=t.to || t.target; if(!m.states.has(next)) {report('Unknown state: '+next);continue;}
          if(count>128)(cycling || (cycling=new Set())).add(m);
          actions(state.exit || state.onExit,e.detail); const previous=m.state; m.state=next;dirty=true;
          const blend=t.blend==null?0:t.blend;m.fade=blend>0 && !reducedMotion?{from:previous,elapsed:0,duration:blend}:null;m.elapsed=0;
          actions(t.actions,e.detail); actions(m.states.get(next).onEnter || m.states.get(next).entry,e.detail);
          render();dispatch('state.change',{machine:m.id,from:previous,to:next,event:e.timer?'after':e.event});if(liveRegion)announceState(m.id,next);
        }
      } finally {processing=false;}
    }
    // Sustained cycles (the limit hit on CYCLE_TICKS consecutive ticks, e.g. after:0 timers restarting an emit
    // ping-pong every frame) stop the machines that kept transitioning, so they cannot burn every frame.
    const CYCLE_TICKS=3;let cycleHit=false,cycleTicks=0,cycling=null;
    function cycleGuard() {
      if(!cycleHit){cycleTicks=0;cycling=null;return;}
      cycleHit=false;if(++cycleTicks<CYCLE_TICKS)return;
      cycleTicks=0;for(const m of cycling || []){m.halted=true;report('State machine '+m.id+' stopped: event cycle limit exceeded on '+CYCLE_TICKS+' consecutive ticks');}cycling=null;
    }
    function emit(event,detail,target) {ensure();enqueue({broadcast:true,event,detail,target});}
    for(const m of project.machines) {
      const states=new Map(m.states.map(s=>[s.id,s])); if(!states.has(m.initial)) throw new Error('Unknown initial state: '+m.initial);
      machines.set(m.id,{...m,states,state:m.initial,elapsed:0});
    }
    function machine(id) {ensure();const m=machines.get(id);if(!m) throw new Error('Unknown machine: '+id);return {send:(event,detail)=>send(id,typeof event==='object'?event.name:event,detail,typeof event==='object'?event.target:undefined),get state(){return m.state;}};}
    const callbacks=new Set();
    for(const [index,c] of project.clips.entries()) clips.set(c.id,{...c,index,priority:c.priority || 0,time:0,playing:false,active:false,cycle:0,revision:0,ramp:null,sources:new Map(),table:null});
    // Track tables are compiled the first time a clip is rendered, keeping load time low.
    const tableOf=c=>c.table || (c.table=c.tracks.map(t=>({target:t.target,property:t.property,track:t,ct:compileTrack(t)})));
    // Clip order is fixed at load: (priority, index) with stable ties.
    const clipList=[...clips.values()], clipOrder=[...clipList].sort((a,b)=>a.priority-b.priority || a.index-b.index);
    const activeBuf=[], fadingBuf=[], fromBuf=[], revBuf=[];
    const first=project.clips[0] && project.clips[0].id;
    const transport={id:'@timeline',duration:Infinity,time:0,playing:false,manual:false,loop:false,cycle:0,revision:0,sources:new Map(),audioCues:project.audioCues || []};
    const cueList=c => c.audioCues || [];
    function transportPlay(){if(!transport.playing){transport.playing=true;transport.revision++;if(transport.time===0)crossing(transport,0,0,true);schedule(transport);}}
    function transportPause(){transport.playing=false;transport.revision++;stopSources(transport);}
    function getClip(id) {ensure();const c=clips.get(id);if(!c)throw new Error('Unknown clip: '+id);return c;}
    function stopSources(c) {for(const source of c.sources.values()) source.stop(); c.sources.clear();}
    function crossing(c,from,to,zero) {
      const rev=c.revision;
      if(!callbacks.size)return c.revision===rev && !dead;
      const items=[...callbacks].filter(cb=>cb.clip===c.id);
      items.sort((a,b)=>a.time-b.time);
      for(const item of items) if((item.time>from && item.time<=to) || (zero && item.time===0)) {
        if(c.revision!==rev || dead) return false;
        if(callbacks.has(item.ref)) try{const result=item.fn({clip:c.id,time:item.time,cycle:c.cycle},api);if(result && typeof result.catch==='function')result.catch(report);}catch(e){report(e);}
      }
      return c.revision===rev && !dead;
    }
    const failedLoopCues=new Set();
    function schedule(c) {
      if(!c.playing || !audio.ready)return;
      const cues=cueList(c);
      for(let lap=0;lap<=(c.loop && !audio.context.createBuffer?1:0);lap++) for(let i=0;i<cues.length;i++) {
        const cue=cues[i], key=(c.loop?'loop':c.cycle+lap)+':'+i;
        if(c.sources.has(key))continue;
        if(c.loop && audio.context.createBuffer) {
          const failure=c.id+':'+i;if(failedLoopCues.has(failure))continue;
          try {const buffer=audio.loopBuffer(c,i,cue);c.sources.set(key,audio.start(cue.assetId || cue.asset,{buffer,loop:true,offset:c.time%c.duration,volume:cue.volume}));}catch(e){failedLoopCues.add(failure);report(e);}
          continue;
        }
        const start=finite(cue.time,'Audio cue time')+(lap?lap*c.duration:0);
        const asset=cue.assetId || cue.asset;
        const buffer=audio.buffers.get(asset);if(!buffer){report('Unknown or unavailable audio asset: '+asset);continue;}
        const offset=cue.offset || 0, length=Math.min(cue.duration==null?buffer.duration-offset:cue.duration,c.duration-cue.time);
        const elapsed=Math.max(0,c.time-start);
        if(start<0 || cue.time>=c.duration || length<=elapsed)continue;
        try {
          const handle=audio.start(asset,{offset:offset+elapsed,volume:cue.volume,duration:length-elapsed,when:audio.context.currentTime+Math.max(0,start-c.time)});
          c.sources.set(key,handle);
        } catch(e){report(e);}
      }
    }
    function clip(id) {
      const c=getClip(id);
      return {
        get time(){return liveTime(c);},get duration(){return c.duration;},get playing(){return c.playing;},get loop(){return c.loop;},
        get weight(){ensure();syncClock();return c.weight;},
        setWeight(weight){ensure();weightValue(weight);if(c.blendMode==='override')throw new Error('Weight controls require a weighted or additive clip');syncClock();c.ramp=null;c.weight=weight;dirty=true;render();return this;},
        fadeTo(weight,seconds){ensure();weightValue(weight);fadeDuration(seconds);if(c.blendMode==='override')throw new Error('Weight controls require a weighted or additive clip');syncClock();c.ramp=seconds===0?null:{from:c.weight,to:weight,duration:seconds,elapsed:0};if(seconds===0)c.weight=weight;dirty=true;render();if(c.ramp)armInterval();return this;},
        set loop(v){ensure();c.loop=!!v;if(c.loop)c.time%=c.duration;c.revision++;stopSources(c);dirty=true;render();schedule(c);},
        play(){ensure();syncClock();if(c.playing)return this;if(c.time>=c.duration){c.time=0;c.cycle=0;}c.active=true;c.playing=true;c.revision++;const revision=c.revision;transportPlay();if(dead || c.revision!==revision || !c.playing)return this;dirty=true;render();if(c.time===0)crossing(c,0,0,true);schedule(c);return this;},
        pause(){ensure();syncClock();c.playing=false;c.revision++;stopSources(c);if(!transport.manual && ![...clips.values()].some(c=>c.playing))transportPause();if(nat.on){nativeCheck();if(dirty)render();}return this;},
        seek(time){ensure();syncClock();finite(time,'Seek time');c.time=c.loop?Math.max(0,time)%c.duration:clamp(time,0,c.duration);c.cycle=0;c.active=true;c.revision++;stopSources(c);dirty=true;render();schedule(c);return this;},
        stop(){this.pause();c.ramp=null;c.time=0;c.cycle=0;c.active=false;dirty=true;render();return this;}
      };
    }
    // Catch-up work per tick is bounded. Up to LAP_EXACT segments run exactly as always (a frame per segment and
    // loop wrap). After that, or from the start when a looping clip would wrap LAP_EXACT times, looping clips without callbacks advance whole laps arithmetically instead of one
    // segment per lap, and intermediate frames are drawn only when a callback or a clip.end can observe them.
    // At most TICK_RENDERS intermediate frames per tick; the tick's final frame is always drawn.
    const LAP_EXACT=8, TICK_RENDERS=64, freeBuf=[];
    function tick(delta) {
      ensure();if(finite(delta,'Tick delta')<0)throw new Error('Tick delta must be nonnegative');
      if(ticking)throw new Error('Reentrant tick is unsupported');
      ticking=true;renderBudget=TICK_RENDERS;
      try {
      if(advanceBlends(delta))render();
      let remaining=delta,segments=0,fast=false;
      do {
        if(++segments>10000){report('Transport catch-up limit exceeded (10000)');for(const c of clips.values())if(c.playing)clip(c.id).pause();transportPause();break;}
        const active=activeBuf, fading=fadingBuf;
        active.length=0;fading.length=0;
        if(transport.playing)active.push(transport);
        for(let i=0;i<clipList.length;i++){const c=clipList[i];if(c.playing)active.push(c);if(c.ramp)fading.push(c);}
        if(!active.length && !fading.length)break;
        if(!fast){if(segments>LAP_EXACT)fast=true;else if(segments===1)for(let i=0;i<active.length;i++)if(active[i].loop && remaining>=LAP_EXACT*active[i].duration)fast=true;}
        let step=remaining,observe=!fast || callbacks.size>0;
        for(const c of fading)step=Math.min(step,c.ramp.duration-c.ramp.elapsed);
        for(let i=0;i<active.length;i++) {
          const c=active[i];let cbs=false;
          for(const cb of callbacks)if(cb.clip===c.id){cbs=true;if(cb.time>c.time)step=Math.min(step,cb.time-c.time);}
          freeBuf[i]=fast && c.loop && !cbs;
          if(!freeBuf[i])step=Math.min(step,c.duration-c.time);
        }
        for(let i=0;i<active.length;i++){const c=active[i];fromBuf[i]=c.time;revBuf[i]=c.revision;if(!c.loop && c.time+step>=c.duration)observe=true;}
        for(const c of active){c.time+=step;if(step!==0 && c!==transport && c.tracks.length && !c.allNative)dirty=true;}
        if(fading.length)dirty=true;
        for(const c of fading) {
          const r=c.ramp;r.elapsed=Math.min(r.duration,r.elapsed+step);
          c.weight=r.elapsed===r.duration?r.to:r.from*(1-r.elapsed/r.duration)+r.to*(r.elapsed/r.duration);
          if(r.elapsed===r.duration)c.ramp=null;
        }
        remaining=Math.max(0,remaining-step);if(observe)render();
        for(let i=0,n=active.length;i<n;i++) {
          const c=active[i],from=fromBuf[i],revision=revBuf[i];
          if(dead)return;if(c.revision!==revision || !c.playing)continue;
          if(freeBuf[i]) {
            // Whole laps at once (fmod is exact); the same per-wrap work as below, done once.
            if(c.time>=c.duration){const r=c.time%c.duration;c.cycle+=Math.round((c.time-r)/c.duration);c.time=r;if(!c.allNative)dirty=true;for(const [key,h] of c.sources)if(Number(key.split(':')[0])<c.cycle){h.stop();c.sources.delete(key);}}
            continue;
          }
          if(!crossing(c,from,c.time,false))continue;
          if(c.time===c.duration) {
            if(!c.loop){c.playing=false;c.revision++;stopSources(c);emit('clip.end',{clip:c.id},c.id);}
            else {c.time=0;if(!c.allNative)dirty=true;c.cycle++;for(const [key,h] of c.sources)if(Number(key.split(':')[0])<c.cycle){h.stop();c.sources.delete(key);}if(!fast || callbacks.size)render();crossing(c,0,0,true);}
          }
        }
      } while(remaining>0 && !dead);
      if(dead)return;schedule(transport);for(let i=0;i<clipList.length;i++)schedule(clipList[i]);renderBudget=Infinity;render();cycleGuard();
      if(frameHooks)runFrameHooks(delta);
      } finally {ticking=false;renderBudget=Infinity;}
      if(nat.on && !dead){nativeCheck();if(dirty)render();}
    }
    function at(time,fn,opts) {
      ensure();finite(time,'Callback time');if(typeof fn!=='function')throw new Error('Timeline callback must be a function');
      const id=opts && opts.clip || transport.id,c=id===transport.id?transport:getClip(id);if(time<0 || time>c.duration)throw new Error('Callback outside clip');
      const cb={clip:id,time,fn};cb.ref=cb;callbacks.add(cb);return()=>callbacks.delete(cb);
    }
    let context=null, unlocking=null,loopBytes=0;const buffers=new Map(),independent=new Set(),loopBuffers=new Map();
    const audio={
      ready:false,buffers,get context(){return context;},
      loopBuffer(c,index,cue){
        const key=c.id+':'+index;if(loopBuffers.has(key))return loopBuffers.get(key);
        const original=buffers.get(cue.assetId || cue.asset);if(!original)throw new Error('Unknown loop audio asset');
        const frames=Math.ceil(c.duration*original.sampleRate),bytes=frames*original.numberOfChannels*4;
        if(loopBytes+bytes>128*1024*1024)throw new Error('Loop audio padding exceeds 128 MiB limit');
        const start=Math.round(cue.time*original.sampleRate),offset=Math.round((cue.offset || 0)*original.sampleRate);
        const count=Math.min(frames-start,original.length-offset,Math.round((cue.duration==null?c.duration:cue.duration)*original.sampleRate));
        if(start<0 || offset<0 || count<=0)throw new Error('Invalid loop audio cue range');
        const buffer=context.createBuffer(original.numberOfChannels,frames,original.sampleRate);
        for(let ch=0;ch<original.numberOfChannels;ch++)buffer.getChannelData(ch).set(original.getChannelData(ch).subarray(offset,offset+count),start);
        loopBytes+=bytes;loopBuffers.set(key,buffer);return buffer;
      },
      async unlock(){
        ensure();if(unlocking)return unlocking;
        unlocking=(async()=>{
          const C=env.AudioContext || env.webkitAudioContext;if(!C)throw new Error('Web Audio unavailable');if(!context)context=new C();
          await context.resume();
          for(const a of project.assets) if(!buffers.has(a.id)) {
            const data=a.dataBase64 || a.data;if(a.url || typeof data!=='string' || !/^[a-z\d+/]*={0,2}$/i.test(data) || data.length%4!==0)throw new Error('Asset must contain embedded base64: '+a.id);
            if(data.length>64*1024*1024)throw new Error('Audio asset exceeds 64 MiB encoded limit: '+a.id);
            const raw=env.atob ? env.atob(data) : (typeof Buffer!=='undefined'?Buffer.from(data,'base64').toString('binary'):null);if(raw===null)throw new Error('Base64 decoder unavailable');
            const bytes=Uint8Array.from(raw,c=>c.charCodeAt(0));const buffer=await context.decodeAudioData(bytes.buffer);
            if(dead)throw new Error('Splinely runtime destroyed');buffers.set(a.id,buffer);
          }
          if(dead)throw new Error('Splinely runtime destroyed');audio.ready=true;nativeOff();armInterval();for(const c of [transport,...clips.values()])schedule(c);return api;
        })();
        try{return await unlocking;}catch(e){report(e);throw e;}finally{unlocking=null;}
      },
      start(id,opts){
        ensure();if(!audio.ready)throw new Error('Audio is locked; await audio.unlock()');opts=opts || {};const buffer=opts.buffer || buffers.get(id);if(!buffer)throw new Error('Unknown audio asset: '+id);
        const offset=finite(opts.offset==null?0:opts.offset,'Audio offset'),volume=finite(opts.volume==null?1:opts.volume,'Audio volume');
        const duration=finite(opts.duration==null?buffer.duration-offset:opts.duration,'Audio duration');
        if(offset<0 || offset>=buffer.duration || duration<=0 || volume<0)throw new Error('Invalid audio range or volume');
        const source=context.createBufferSource(),gain=context.createGain();source.buffer=buffer;source.loop=!!opts.loop;gain.gain.value=volume;source.connect(gain);gain.connect(context.destination);
        let stopped=false;const handle={stop(){if(stopped)return;stopped=true;try{source.stop();}catch(_){}source.disconnect();gain.disconnect();independent.delete(handle);},get stopped(){return stopped;}};
        source.onended=()=>{stopped=true;source.disconnect();gain.disconnect();independent.delete(handle);};
        try {if(opts.loop)source.start(opts.when==null?context.currentTime:opts.when,offset);
          else source.start(opts.when==null?context.currentTime:opts.when,offset,Math.min(duration,buffer.duration-offset));}
        catch(e){handle.stop();throw e;}return handle;
      },
      play(id,opts){const handle=audio.start(id,opts);independent.add(handle);return handle;},
      pause(){for(const h of [...independent])h.stop();},seek(time){audio.pause();if(time!=null)api.timeline.seek(time);}
    };
    const pointerStarts=new Map();
    function wantsPointerCapture(target) {
      if(options.pointerCapture!=null)return options.pointerCapture===true;
      const dragEvents=['dragstart','drag','dragend'];
      return dragEvents.some(name=>subscriptions.has(name)||subscriptions.has(target+'\u0000'+name)) ||
        (project.interactions||[]).some(b=>b.target===target && dragEvents.includes(b.event)) ||
        (project.stateMachines||[]).some(m=>(m.transitions||[]).some(t=>t.event && dragEvents.includes(t.event.name) && (!t.event.target||t.event.target===target)));
    }
    const eventTypes=['click','dblclick','pointerenter','pointerleave','pointerdown','pointermove','pointerup','pointercancel','lostpointercapture','wheel','keydown','keyup','focus','blur'];
    function coordinates(event) {
      let x=event.clientX || 0,y=event.clientY || 0;
      try {const matrix=svg.getScreenCTM();if(matrix){const p=svg.createSVGPoint();p.x=x;p.y=y;const q=p.matrixTransform(matrix.inverse());x=q.x;y=q.y;}}catch(e){report(e);}
      return {x,y};
    }
    // DOM events nobody consumes (no subscription, interaction or machine transition for the name) are not
    // emitted: emitting syncs the clock, which ticks and draws once per event, between and beyond frames,
    // and even while the figure is hidden. Hosts that forward pointer moves at a high rate pay nothing then.
    const machineEvents=new Set();
    for(const m of project.machines)for(const t of [...(m.transitions || []),...m.states.flatMap(s=>s.transitions || [])])if(t.event!=null)machineEvents.add(typeof t.event==='string'?t.event:t.event.name);
    const heard=(type,target)=>machineEvents.has(type) || subscriptions.has(type) || subscriptions.has(target+'\u0000'+type) || (project.interactions || []).some(b=>b.target===target && b.event===type);
    const listeners=new Map();
    for(const type of eventTypes) {
      const listener=event=>{
        if(dead)return;
        // Hidden figures stay asleep: the pointer is only recorded (followers catch up once visible).
        if(wake!==null && !nat.hidden)wake();
        for(let control=event.target;control && control!==svg;control=control.parentNode)if(control.getAttribute && control.getAttribute('data-splinely-host-control')!=null)return;
        if(event.target && owningSvg(event.target) && owningSvg(event.target)!==svg)return;
        let el=event.target;while(el && el!==svg && elements.get(el.id)!==el)el=el.parentNode;
        const target=el && el.id || svg.id,point=coordinates(event),detail={...point,event,target,key:event.key,pointerId:event.pointerId,buttons:event.buttons};
        if(type==='pointermove' || type==='pointerdown' || type==='pointerenter')pointer=[point.x,point.y];
        if(type==='pointerleave' && event.target===svg)pointer=null;
        if(project.followers.length && /^pointer/.test(type)){followMoving=true;if(updateFollowers(0))dirty=true;if(!nat.hidden)render();}
        // Keep keyboard handling scoped to focus within this SVG. Focusability remains exporter's choice.
        if(heard(type,target)){emit(type,detail,target);if(dead)return;}
        for(const binding of project.interactions || [])if(binding.target===target && binding.event===type)actions(binding.actions,detail);
        if(dead)return;
        if(type==='pointerdown') {pointerStarts.set(event.pointerId,{...point,target,el});try{if(el && el.setPointerCapture && wantsPointerCapture(target))el.setPointerCapture(event.pointerId);}catch(_){}emit('dragstart',{...detail,dx:0,dy:0},target);}
        if(type==='pointermove' && pointerStarts.has(event.pointerId)){const start=pointerStarts.get(event.pointerId);emit('drag',{...detail,dx:point.x-start.x,dy:point.y-start.y},start.target);}
        if((type==='pointerup'||type==='pointercancel'||type==='lostpointercapture') && pointerStarts.has(event.pointerId)){const start=pointerStarts.get(event.pointerId);pointerStarts.delete(event.pointerId);try{if(start.el.releasePointerCapture)start.el.releasePointerCapture(event.pointerId);}catch(_){}emit('dragend',{...detail,dx:point.x-start.x,dy:point.y-start.y,cancelled:type!=='pointerup'},start.target);}
      };
      listeners.set(type,listener);svg.addEventListener(type,listener,true);cleanup.push(()=>svg.removeEventListener(type,listener,true));
    }
    function snapshot() {
      // Observational only: don't advance clocks, invoke callbacks or expose handles.
      return {version:1,destroyed:dead,time:liveTime(transport),playing:transport.playing,
        clips:[...clips.values()].map(c=>({id:c.id,time:liveTime(c),duration:c.duration,active:c.active,playing:c.playing,weight:c.weight,fading:!!c.ramp})),
        states:Object.fromEntries([...machines].map(([id,m])=>[id,m.state])),
        audio:{ready:audio.ready,state:context?context.state:'locked',embeddedAssets:project.assets.length},
        reducedMotion,
        diagnostics:diagnostics.map(e=>e.message)};
    }
    api={version:1,snapshot,reportError(error){ensure();return report(error);},diagnostics,on,emit,bones(){ensure();if(nat.on && nat.boneNative)return shadowBones();return bonePoses.map(p=>({...p}));},onDestroy(fn){ensure();if(typeof fn!=='function')throw new Error('Disposer must be a function');cleanup.push(fn);},get destroyed(){return dead;}, get sourceProject(){return source;}, clip,machine,set,variable,timeline:{at,get time(){return liveTime(transport);},get playing(){return transport.playing;},play(){ensure();syncClock();transport.manual=true;transportPlay();return this;},pause(){ensure();transport.manual=false;for(const c of clips.values())clip(c.id).pause();transportPause();return this;},seek(time){ensure();syncClock();finite(time,'Timeline seek time');if(time<0)throw new Error('Timeline seek must be nonnegative');transport.time=time;transport.revision++;stopSources(transport);for(const c of clips.values())if(c.active)clip(c.id).seek(c.loop?time%c.duration:Math.min(time,c.duration));schedule(transport);return this;}},audio,tick,
      destroy(){if(dead)return;if(nat.on)try{nativeOff(true);}catch(e){report(e);}dead=true;for(const fn of cleanup)try{fn();}catch(error){report(error);}for(const c of [transport,...clips.values()])stopSources(c);audio.pause();loopBuffers.clear();buffers.clear();if(context && context.close)Promise.resolve(context.close()).catch(()=>{});subscriptions.clear();callbacks.clear();for(const [id,start] of pointerStarts)try{if(start.el.releasePointerCapture)start.el.releasePointerCapture(id);}catch(_){}pointerStarts.clear();if(svg.splinely===api)delete svg.splinely;unregister(env,api);}
    };
    // Screen-reader semantics, keyboard activation, focus ring and live announcements.
    // Opt-in: projects without description, node a11y or state announce are left untouched.
    let announceTimer=null;
    function announceState(machineId,stateId) {
      const m=machines.get(machineId),s=m && m.states.get(stateId),text=s && s.announce;
      if(!liveRegion || typeof text!=='string')return;
      const set=()=>{if(!dead)liveRegion.textContent=text;};
      // One pending re-announcement per runtime: a newer announcement replaces it.
      if(announceTimer!==null){if(env.clearTimeout)env.clearTimeout(announceTimer);announceTimer=null;}
      if(liveRegion.textContent!==text){set();return;}
      // Identical text is only re-announced when the region changes in between.
      liveRegion.textContent='';
      if(typeof env.setTimeout==='function')announceTimer=env.setTimeout(()=>{announceTimer=null;set();},50);else set();
    }
    function setupAccessibility() {
      const states=project.machines.flatMap(m=>m.states);
      if(!project.description && !project.nodes.some(n=>n.a11y!==undefined) && !states.some(s=>s.announce!==undefined))return;
      const saved=[],put=(el,name,value)=>{saved.push([el,name,el.getAttribute(name)]);el.setAttribute(name,value);};
      const doc=svg.ownerDocument,SVGNS='http://www.w3.org/2000/svg',created=[];
      cleanup.push(()=>{for(const [el,name,old] of saved.reverse())if(old==null)el.removeAttribute(name);else el.setAttribute(name,old);for(const el of created)if(el.parentNode && el.parentNode.removeChild)el.parentNode.removeChild(el);});
      const interactive=interactiveIds(project.machines,project.interactions),hover=interactiveIds(project.machines,project.interactions,['pointerenter','pointerleave']);
      const focusables=new Set();
      for(const n of project.nodes) {
        const el=elements.get(n.id);if(!el)continue;
        const a=n.a11y || {},isInteractive=interactive.has(n.id);
        if(a.hidden){put(el,'aria-hidden','true');continue;}
        if(a.role!==undefined)put(el,'role',a.role);else if(isInteractive && el.getAttribute('role')==null)put(el,'role','button');
        if(a.label!==undefined)put(el,'aria-label',a.label);else if(isInteractive && el.getAttribute('aria-label')==null)put(el,'aria-label',n.name || n.id);
        if(a.description!==undefined)put(el,'aria-description',a.description);
        if(isInteractive){if(el.getAttribute('tabindex')==null)put(el,'tabindex','0');put(el,'data-splinely-focusable','');focusables.add(el);}
      }
      const live=states.some(s=>s.announce!==undefined);
      if(svg.getAttribute('role')==null)put(svg,'role',focusables.size || live?'group':'img');
      if(project.name && svg.getAttribute('aria-labelledby')==null && svg.getAttribute('aria-label')==null)put(svg,'aria-label',project.name);
      if(project.description && svg.getAttribute('aria-describedby')==null && svg.getAttribute('aria-description')==null)put(svg,'aria-description',project.description);
      const make=(tag,attrs)=>{const el=doc.createElementNS(SVGNS,tag);for(const k of Object.keys(attrs))el.setAttribute(k,attrs[k]);return el;};
      if(live && doc && typeof doc.createElementNS==='function' && svg.appendChild) {
        liveRegion=make('text',{'data-splinely-live':'',role:'status','aria-live':'polite','aria-atomic':'true',x:'0',y:'0','font-size':'1','fill-opacity':'0','pointer-events':'none'});
        svg.appendChild(liveRegion);created.push(liveRegion);
        cleanup.push(()=>{if(announceTimer!==null && env.clearTimeout)env.clearTimeout(announceTimer);announceTimer=null;});
      }
      if(!focusables.size)return;
      let modality=false,ring=null,ringFor=null,spaceDown=null;const entered=new Set();
      const focusVisible=el=>{try{return !!el.matches(':focus-visible');}catch(_){return modality;}};
      const forced=()=>{try{return !!(env.matchMedia && env.matchMedia('(forced-colors: active)').matches);}catch(_){return false;}};
      // The ring is an SVG overlay next to the focused node: it renders in every engine (CSS outline on SVG
      // children does not), needs no stylesheet (CSP-safe) and uses system colours in forced-colors mode.
      function drawRing() {
        if(!ring || !ringFor)return;
        try {
          const b=ringFor.getBBox(),pad=4,t=ringFor.getAttribute('transform');
          for(const r of ring.children){r.setAttribute('x',b.x-pad);r.setAttribute('y',b.y-pad);r.setAttribute('width',b.width+2*pad);r.setAttribute('height',b.height+2*pad);}
          if(t)ring.setAttribute('transform',t);else ring.removeAttribute('transform');
          ring.removeAttribute('visibility');
        } catch(_) {ring.setAttribute('visibility','hidden');}
      }
      function hideRing() {
        if(ring && ring.parentNode && ring.parentNode.removeChild)ring.parentNode.removeChild(ring);
        ring=ringFor=null;afterRender=null;
      }
      function showRing(el) {
        hideRing();
        // A host that draws its own focus can switch the ring off: createRuntime(svg, project, { focusRing: false }).
        if(options.focusRing===false)return;
        if(!doc || typeof doc.createElementNS!=='function' || !el.parentNode || !el.parentNode.insertBefore)return;
        const f=forced(),shape=(stroke,width)=>make('rect',{fill:'none',stroke,'stroke-width':width,'vector-effect':'non-scaling-stroke',rx:'3'});
        ring=make('g',{'data-splinely-focus-ring':'','pointer-events':'none','aria-hidden':'true'});
        ring.appendChild(shape(f?'Canvas':'#000000',6));ring.appendChild(shape(f?'Highlight':'#ffbf47',3));
        el.parentNode.insertBefore(ring,el.nextSibling || null);ringFor=el;afterRender=drawRing;drawRing();
      }
      cleanup.push(hideRing);
      const center=el=>{try{const r=el.getBoundingClientRect();return {clientX:r.left+r.width/2,clientY:r.top+r.height/2};}catch(_){return {clientX:0,clientY:0};}};
      function activate(el) {
        const point=center(el),Mouse=env.MouseEvent;
        if(typeof Mouse==='function' && el.dispatchEvent) {
          try {el.dispatchEvent(new Mouse('click',{bubbles:true,cancelable:true,composed:true,detail:0,...point}));return;} catch(e) {report(e);}
        }
        const l=listeners.get('click');if(l)l({type:'click',target:el,detail:0,...point});
      }
      const onKeyDown=event=>{
        const el=event.target;if(dead || !focusables.has(el) || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey)return;
        if(event.key==='Enter'){if(!event.repeat)activate(el);}
        else if(event.key===' ' || event.key==='Spacebar'){event.preventDefault && event.preventDefault();spaceDown=el;}
      };
      const onKeyUp=event=>{
        if(dead || (event.key!==' ' && event.key!=='Spacebar') || !spaceDown)return;
        const el=spaceDown;spaceDown=null;if(event.target===el)activate(el);
      };
      const onFocus=event=>{
        const el=event.target;if(dead || !focusables.has(el) || !focusVisible(el))return;
        showRing(el);
        if(hover.has(el.id)){entered.add(el);const l=listeners.get('pointerenter');if(l)l({type:'pointerenter',target:el,...center(el)});}
      };
      const onBlur=event=>{
        const el=event.target;if(!focusables.has(el))return;
        if(ringFor===el)hideRing();
        if(entered.delete(el)){const l=listeners.get('pointerleave');if(l && !dead)l({type:'pointerleave',target:el,...center(el)});}
        if(spaceDown===el)spaceDown=null;
      };
      const mode=name=>()=>{modality=name==='key';};
      for(const [type,fn] of [['keydown',onKeyDown],['keyup',onKeyUp],['focus',onFocus],['blur',onBlur]]){svg.addEventListener(type,fn,true);cleanup.push(()=>svg.removeEventListener(type,fn,true));}
      if(doc && doc.addEventListener)for(const [type,fn] of [['keydown',mode('key')],['pointerdown',mode('pointer')]]){doc.addEventListener(type,fn,true);cleanup.push(()=>doc.removeEventListener(type,fn,true));}
    }
    // ---- Opt-in runtime plugins (docs/plugins.md). Never read from project data: only options.plugins or api.use().
    // Without plugins this costs one null check per tick and nothing else.
    const plugins=[];
    function pluginNow(){return env.performance && env.performance.now?env.performance.now():Date.now();}
    function plainDetail(detail,type){
      // Read-only, primitive-only copy: plugins never receive DOM events, handles or mutable project objects.
      const out={};if(type!==undefined)out.type=type;
      if(detail && typeof detail==='object')for(const k of Object.keys(detail)){const v=detail[k];if(v===null || typeof v==='string' || typeof v==='boolean' || (typeof v==='number' && Number.isFinite(v)))out[k]=v;}
      return Object.freeze(out);
    }
    function deepFreeze(o){if(o && typeof o==='object' && !Object.isFrozen(o)){Object.freeze(o);for(const k of Object.keys(o))deepFreeze(o[k]);}return o;}
    function pluginFail(p,where,error){
      if(p.disabled || dead)return;
      p.errors++;report(new Error('Splinely plugin '+p.id+' '+where+': '+(error && error.message || String(error))));
      if(p.errors>=PLUGIN_MAX_ERRORS)pluginDisable(p,'too many errors');
    }
    function pluginDisable(p,why){
      if(p.disabled)return;p.disabled=true;p.reason=why;
      for(const off of p.offs.splice(0))try{off();}catch(_){}
      if(frameHooks){frameHooks=frameHooks.filter(q=>q!==p);if(!frameHooks.length)frameHooks=null;}
      report(new Error('Splinely plugin '+p.id+' disabled: '+why));
    }
    function pluginCall(p,where,fn,arg){
      if(p.disabled || dead)return;
      try{const result=fn.call(p.def,arg,p.ctx);if(result && typeof result.then==='function')result.then(null,e=>pluginFail(p,where,e));}
      catch(e){pluginFail(p,where,e);}
    }
    function runFrameHooks(delta){
      const list=frameHooks;
      for(let i=0;i<list.length;i++){
        const p=list[i];if(p.disabled)continue;
        const t0=pluginNow();
        pluginCall(p,'onFrame',p.def.onFrame,Object.freeze({delta,time:transport.time,playing:transport.playing}));
        const spent=pluginNow()-t0;
        // Synchronous code cannot be preempted: the budget is measured after the fact and repeated overruns disable the hook.
        if(spent>p.budget){p.overruns++;report(new Error('Splinely plugin '+p.id+' onFrame took '+spent.toFixed(1)+'ms (budget '+p.budget+'ms)'));if(p.overruns>=PLUGIN_MAX_OVERRUNS)pluginDisable(p,'frame budget exceeded');}
      }
    }
    function pluginName(name,what){if(typeof name!=='string' || !PLUGIN_NAME.test(name))throw new Error(what+' must match '+PLUGIN_NAME);return name;}
    function use(def,meta){
      ensure();
      if(!def || typeof def!=='object' || Array.isArray(def))throw new Error('Plugin must be an object');
      for(const k of Object.keys(def))if(!PLUGIN_KEYS.includes(k))throw new Error('Unknown plugin field: '+k);
      if(typeof def.id!=='string' || !PLUGIN_ID.test(def.id))throw new Error('Plugin id must match '+PLUGIN_ID);
      if(def.apiVersion!==PLUGIN_API_VERSION)throw new Error('Unsupported plugin apiVersion '+String(def.apiVersion)+' (runtime supports '+PLUGIN_API_VERSION+')');
      if(plugins.some(q=>q.id===def.id))throw new Error('Duplicate plugin id: '+def.id);
      if(plugins.length>=PLUGIN_MAX)throw new Error('At most '+PLUGIN_MAX+' plugins per runtime');
      for(const h of ['setup','onEvent','onState','onFrame'])if(def[h]!==undefined && typeof def[h]!=='function')throw new Error('Plugin '+h+' must be a function');
      let events=[];
      if(def.events!==undefined){
        if(!Array.isArray(def.events) || def.events.length>PLUGIN_MAX_EVENTS)throw new Error('Plugin events must be an array of at most '+PLUGIN_MAX_EVENTS+' names');
        events=[...new Set(def.events.map(n=>pluginName(n,'Plugin event name')))];
      }
      if(def.onEvent && !events.length)throw new Error('Plugin onEvent requires a non-empty events list');
      let budget=PLUGIN_FRAME_BUDGET_MS;
      if(def.frameBudgetMs!==undefined){if(typeof def.frameBudgetMs!=='number' || !(def.frameBudgetMs>=0.1 && def.frameBudgetMs<=50))throw new Error('frameBudgetMs must be a number in 0.1..50');budget=def.frameBudgetMs;}
      const m=meta && typeof meta==='object'?meta:{};
      const p={id:def.id,def,events,budget,errors:0,overruns:0,disabled:false,reason:'',offs:[],ctx:null,
        source:typeof m.source==='string'?m.source.slice(0,128):'',sha256:typeof m.sha256==='string'?m.sha256.slice(0,80):''};
      const pid=def.id;
      p.ctx=Object.freeze({
        pluginId:pid,apiVersion:PLUGIN_API_VERSION,
        // Every call below goes through the same validated host API a page script would use.
        emit(name,detail){pluginName(name,'Event name');if(name==='error')throw new Error('Plugins cannot emit error events');api.emit(name,plainDetail(detail));},
        getVariable(name){return api.variable(name).get();},
        setVariable(name,value){api.variable(name).set(value);},
        send(machineId,event,detail){if(typeof event!=='string')throw new Error('Event must be a string');api.machine(machineId).send(event,detail===undefined?undefined:plainDetail(detail));},
        playClip(id){api.clip(id).play();},stopClip(id){api.clip(id).stop();},
        set(id,props){api.set(id,props);},
        snapshot(){return deepFreeze(snapshot());},
        report(error){if(!p.disabled)report(new Error('Splinely plugin '+pid+': '+(error && error.message || String(error))));},
        onDestroy(fn){if(typeof fn!=='function')throw new Error('Disposer must be a function');cleanup.push(()=>{try{fn();}catch(e){pluginFail(p,'onDestroy',e);}});}
      });
      plugins.push(p);
      if(def.setup){
        try{const result=def.setup.call(def,p.ctx);if(result && typeof result.then==='function')result.then(null,e=>pluginFail(p,'setup',e));}
        catch(e){plugins.splice(plugins.indexOf(p),1);p.disabled=true;p.reason='setup failed';report(new Error('Splinely plugin '+pid+' setup failed: '+(e && e.message || String(e))));return Object.freeze({id:pid,disabled:true});}
      }
      if(def.onEvent)for(const name of events)p.offs.push(on(name,detail=>{pluginCall(p,'onEvent',def.onEvent,plainDetail(detail,name));}));
      if(def.onState)p.offs.push(on('state.change',detail=>{pluginCall(p,'onState',def.onState,plainDetail(detail));}));
      if(def.onFrame)frameHooks=(frameHooks||[]).concat(p);
      return Object.freeze({id:pid,disabled:false});
    }
    function pluginList(){return plugins.map(p=>({id:p.id,apiVersion:PLUGIN_API_VERSION,source:p.source,sha256:p.sha256,events:[...p.events],hooks:['setup','onEvent','onState','onFrame'].filter(h=>typeof p.def[h]==='function'),disabled:p.disabled,reason:p.reason,errors:p.errors,overruns:p.overruns}));}
    api.use=use;api.plugins=pluginList;
    setupAccessibility();
    svg.splinely=api;register(env,api);
    if(options.plugins!=null){
      if(!Array.isArray(options.plugins))report(new Error('options.plugins must be an array'));
      else for(const def of options.plugins)try{use(def);}catch(e){report(e);}
    }
    try {
    processing=true;
    try {for(const m of machines.values())actions(m.states.get(m.state).onEnter || m.states.get(m.state).entry);}
    finally {processing=false;drain();}
    // Preservation-only customJs is expected, not an error. Trusted hosts decide execution separately.
    render();for(const c of project.clips)if(c.autoplay && !(reducedMotion && (c.looping || c.loop)))clip(c.id).play();
    } catch(error) {api.destroy();throw error;}
    // ---- Native offload (Web Animations API) ----
    // Clip tracks that only move bones, move/turn/scale unbound shapes or fade shapes can run as browser
    // animations (element.animate) instead of per-frame JavaScript. JavaScript keeps the clocks, events and
    // everything else; it skips those tracks when rendering. A channel (a bone tree, a shape's pose, a shape's
    // opacity) is offloaded only when every contribution to it is such a track, so blending stays exact:
    //  - bone trees: the bound shapes get CSS `translate`/`rotate` = their bone's rest pose plus additive
    //    animations of the bone deltas; the transform attribute carries only the shape's own (local) transform.
    //  - unbound shapes: CSS translate/rotate/scale carry the pose, the attribute stays at identity.
    //  - opacity: one animation with the absolute value.
    // Anything else touching a channel (state overrides, set(), followers, weighted or fading clips, motion
    // paths, a focus ring) keeps it on the JavaScript path. Animations are re-planned when what plays
    // changes and re-synchronised with the runtime clock on play/pause/seek/end.
    function natEligibleTrack(c,t) {
      const ct=t.ct;if(!ct || !(c.duration>0) || !Number.isFinite(c.duration))return false;
      const times=ct.times;
      for(let i=0;i<times.length;i++) {
        const v=ct.vals[i];if(typeof v!=='number' || !Number.isFinite(v))return false;
        if(!(times[i]>=0 && times[i]<=c.duration) || (i>0 && !(times[i]>times[i-1])))return false;
      }
      return true;
    }
    const natNum=v=>Math.abs(v)<1e-9?0:+v.toPrecision(12);
    // Keyframes for one or two tracks with the same times and easings; map(i) returns the CSS value.
    function natFrames(ct,D,prop,map) {
      const out=[], n=ct.times.length, frame=(offset,easing,i)=>{const f={offset,easing};f[prop]=map(i);out.push(f);};
      if(ct.times[0]>0)frame(0,'linear',0);
      for(let i=0;i<n;i++){const e=ct.ease[i];frame(ct.times[i]/D,e===0?'linear':e===1?'steps(1, end)':'cubic-bezier('+ct.pts[i].join(', ')+')',i);}
      if(ct.times[n-1]<D)frame(1,'linear',n-1);
      return out;
    }
    const sameTiming=(a,b)=>{
      if(a.times.length!==b.times.length)return false;
      for(let i=0;i<a.times.length;i++){if(a.times[i]!==b.times[i] || a.ease[i]!==b.ease[i])return false;if(a.ease[i]===2 && a.pts[i].some((v,k)=>v!==b.pts[i][k]))return false;}
      return true;
    };
    const boneChildren=new Map();for(const b of boneList)if(b.parentId!=null && bones.has(b.parentId)){if(!boneChildren.has(b.parentId))boneChildren.set(b.parentId,[]);boneChildren.get(b.parentId).push(b.id);}
    const boneRoot=id=>{let b=bones.get(id),n=0;while(b && b.parentId!=null && bones.has(b.parentId) && n++<130)b=bones.get(b.parentId);return b?b.id:id;};
    let natRest=null;
    function natRestPoses() {
      if(natRest===null){natRest=new Map();try{for(const p of solveBonesPlanned(boneList.map(b=>({...b})),plan))natRest.set(p.id,p);}catch(_){}}
      return natRest;
    }
    function natPlan() {
      if(afterRender!==null)return {specs:new Map(),natTracks:new Set(),modes:new Map(),lists:new Map(),boneNative:false};
      const contrib=new Map(), block={block:true};
      const add=(id,prop,x)=>{const k=id+'\u0000'+prop;let l=contrib.get(k);if(l===undefined)contrib.set(k,l=[]);l.push(x);};
      const get=(id,prop)=>contrib.get(id+'\u0000'+prop);
      for(const m of machines.values()) {
        for(const [id,k] of entries(m.states.get(m.state)))add(id,k,block);
        if(m.fade)for(const [id,k] of entries(m.states.get(m.fade.from)))add(id,k,block);
      }
      for(let ci=0;ci<clipOrder.length;ci++) {
        const c=clipOrder[ci];if(!c.active)continue;
        const table=tableOf(c), ok=c.blendMode!=='weighted' && !c.ramp;
        for(let i=0;i<table.length;i++){const t=table[i];add(t.target,t.property,ok && natEligibleTrack(c,t)?{c,t,i,over:c.blendMode!=='additive'}:block);}
      }
      for(const [id,p] of explicit)for(const k of Object.keys(p))add(id,k,block);
      for(const f of project.followers){if(f.mode==='rotate')add(f.target,'rotation',block);else{add(f.target,'x',block);add(f.target,'y',block);}}
      for(const n of motionNodes) {
        for(const k of ['x','y','rotation','motionProgress'])add(n.id,k,block);
        if(n.motionPath)for(const k of ['x','y','rotation','scaleX','scaleY','d'])add(n.motionPath.path,k,block);
      }
      const clean=l=>l!==undefined && l.every(x=>!x.block);
      // Contributions that reach the output: everything from the last override on (an override replaces what is under it).
      const effective=l=>{let s=0;for(let i=0;i<l.length;i++)if(l[i].over)s=i;return l.slice(s).filter(x=>!(x.c.blendMode==='additive' && x.c.weight===0));};
      const specs=new Map(), natTracks=new Set(), modes=new Map();
      const spec=(node,prop,composite,x,frames,extra)=>{
        const key=node.id+'|'+prop+'|'+x.c.index+'|'+x.i+'|'+(extra || '')+'|'+(x.c.blendMode==='additive'?x.c.weight:'')+'|'+x.c.loop;
        specs.set(key,{key,node,prop,composite,c:x.c,frames});
      };
      const delta=(x,rest)=>x.over?(v=>v-rest):(v=>x.c.weight*(v-rest));
      // Bone trees. A tree runs natively when every contribution to its bones is native. Nodes bound to it:
      // natMode 1 (no ancestor of the node's bone rotates natively): CSS translate/rotate hold the bone's rest
      // pose plus added deltas (an ancestor's translation is rotated by its parent's rest rotation), the
      // attribute keeps the node's own transform. natMode 3 (an ancestor rotates): CSS translate/rotate hold
      // the root bone, the CSS transform list holds each further bone (offset, rotation) and the node's own
      // transform, which must then be static; deltas accumulate into the matching list entries.
      const trees=new Map(), treeOf=id=>{const r=boneRoot(id);let t=trees.get(r);if(!t)trees.set(r,t={js:false,native:new Map()});return t;};
      const chainOf=id=>{const c=[];for(let b=bones.get(id),d=0;b && d<130;b=b.parentId!=null?bones.get(b.parentId):null,d++)c.unshift(b);return c;};
      for(const b of boneList) {
        const tree=treeOf(b.id);
        for(const [k,l] of contrib) {
          if(!k.startsWith(b.id+'\u0000'))continue;
          const prop=k.slice(b.id.length+1);
          if(!boneProperties.has(prop) || !clean(l)){tree.js=true;continue;}
          tree.native.set(b.id+'\u0000'+prop,l);
        }
      }
      // natMode 3 when an ancestor bone rotates natively, or when the node's own transform is animated by native
      // tracks only (then it goes into the list too instead of being written each frame).
      const ancestorTurns=n=>{const c=chainOf(n.boneId), t=treeOf(n.boneId);for(let i=0;i<c.length-1;i++)if(t.native.has(c[i].id+'\u0000rotation'))return true;return false;};
      const localState=n=>{let any=false;for(const k of transformKeys){const l=get(n.id,k);if(l===undefined)continue;if(!clean(l))return -1;any=true;}return any?1:0;};
      const listed=n=>ancestorTurns(n) || localState(n)===1?chainOf(n.boneId):null;
      for(const n of nodes.values()) {
        if(n.boneId==null || !n.el)continue;
        if(!n.el.style || (!n.core && !bones.has(n.boneId))){treeOf(n.boneId).js=true;continue;}
        if(bones.has(n.boneId) && ancestorTurns(n) && localState(n)<0)treeOf(n.boneId).js=true;
      }
      const rest=natRestPoses();
      for(const tree of trees.values()) {
        if(tree.js || !tree.native.size)continue;
        for(const l of tree.native.values())for(const x of l)natTracks.add(x.t);
      }
      const lists=new Map(), deg=v=>natNum(v)+'deg', px=(x,y)=>natNum(x)+'px '+natNum(y)+'px';
      const listOf=(chain,vals,local)=>{let out='';for(let i=1;i<chain.length;i++)out+='translate('+natNum(vals[i][0])+'px, '+natNum(vals[i][1])+'px) rotate('+deg(vals[i][2])+') ';return out+'translate('+natNum(local[0])+'px, '+natNum(local[1])+'px) rotate('+deg(local[2])+') scale('+natNum(local[3])+', '+natNum(local[4])+')';};
      for(const node of nodes.values()) {
        if(!node.el || !node.el.style)continue;
        let mode=0;
        if(node.boneId!=null && bones.has(node.boneId)) {
          const tree=treeOf(node.boneId), chain=!tree.js && tree.native.size?listed(node):null;
          if(chain) {
            mode=3;
            const zero=chain.map(()=>[0,0,0]), local=['x','y','rotation','scaleX','scaleY'].map((k,i)=>finite(node.base[k]==null?(i>2?1:0):unwrap(node.base[k]),'Transform '+k));
            const restVals=chain.map((b,i)=>i?[chain[i-1].length+b.x,b.y,b.rotation]:[0,0,0]);
            lists.set(node,{translate:px(chain[0].x,chain[0].y),rotate:deg(chain[0].rotation),transform:listOf(chain,restVals,local)});
            // The node's own transform: native clip tracks accumulate into the last list entries.
            for(const [slot,prop] of ['x','y','rotation','scaleX','scaleY'].entries()) {
              const l=get(node.id,prop);if(!l)continue;
              for(const x of l)natTracks.add(x.t);
              for(const x of effective(l)) {
                const d=delta(x,local[slot]), ct=x.t.ct, D=x.c.duration;
                spec(node,'transform','accumulate',x,natFrames(ct,D,'transform',k=>{const v=[0,0,0,1,1];v[slot]+=d(ct.nums[k]);return listOf(chain,zero,v);}),'local');
              }
            }
            for(let i=0;i<chain.length;i++) {
              const b=chain[i];
              for(const prop of ['x','y','rotation']) {
                const l=tree.native.get(b.id+'\u0000'+prop);if(!l)continue;
                for(const x of effective(l)) {
                  const d=delta(x,b[prop]), ct=x.t.ct, D=x.c.duration;
                  if(i===0) {
                    if(prop==='rotation')spec(node,'rotate','add',x,natFrames(ct,D,'rotate',k=>deg(d(ct.nums[k]))),b.id);
                    else spec(node,'translate','add',x,natFrames(ct,D,'translate',k=>{const v=d(ct.nums[k]);return prop==='x'?px(v,0):px(0,v);}),b.id);
                  } else {
                    const slot=prop==='x'?0:prop==='y'?1:2;
                    spec(node,'transform','accumulate',x,natFrames(ct,D,'transform',k=>{const v=zero.map(z=>z.slice());v[i][slot]=d(ct.nums[k]);return listOf(chain,v,[0,0,0,1,1]);}),b.id);
                  }
                }
              }
            }
          } else if(!tree.js && tree.native.size) {
            for(let b=bones.get(node.boneId),depth=0;b && depth<130;b=b.parentId!=null?bones.get(b.parentId):null,depth++) {
              const parent=b.parentId!=null?rest.get(b.parentId):null, angle=parent?(parent.rotation%360)*Math.PI/180:0, cos=Math.cos(angle), sin=Math.sin(angle);
              for(const prop of ['x','y','rotation']) {
                const l=tree.native.get(b.id+'\u0000'+prop);if(!l)continue;
                mode=1;
                if(prop==='rotation' && b.id!==node.boneId)continue; // ancestors rotating: natMode 3
                const restValue=b[prop];
                for(const x of effective(l)) {
                  const d=delta(x,restValue), ct=x.t.ct, D=x.c.duration;
                  if(prop==='rotation')spec(node,'rotate','add',x,natFrames(ct,D,'rotate',i=>deg(d(ct.nums[i]))),b.id);
                  else {const ux=prop==='x'?cos:-sin, uy=prop==='x'?sin:cos;spec(node,'translate','add',x,natFrames(ct,D,'translate',i=>{const v=d(ct.nums[i]);return px(ux*v,uy*v);}),b.id);}
                }
              }
            }
          }
        } else if(node.boneId==null && node.core) {
          const lists={};let any=false,ok=true;
          for(const prop of ['x','y','rotation','scaleX','scaleY']){const l=get(node.id,prop);if(l===undefined)continue;any=true;if(!clean(l))ok=false;lists[prop]=l;}
          let sx=null,sy=null;
          if(ok && any) {
            sx=lists.scaleX?effective(lists.scaleX):[];sy=lists.scaleY?effective(lists.scaleY):[];
            if(sx.length>1 || sy.length>1 || (sx.length && sy.length && (sx[0].c!==sy[0].c || !sameTiming(sx[0].t.ct,sy[0].t.ct))))ok=false;
          }
          if(ok && any) {
            mode=2;
            for(const prop of ['x','y','rotation'])if(lists[prop])for(const x of lists[prop])natTracks.add(x.t);
            for(const prop of ['scaleX','scaleY'])if(lists[prop])for(const x of lists[prop])natTracks.add(x.t);
            for(const prop of ['x','y','rotation'])if(lists[prop])for(const x of effective(lists[prop])) {
              const d=delta(x,finite(unwrap(node.base[prop]),'Transform '+prop)), ct=x.t.ct, D=x.c.duration;
              if(prop==='rotation')spec(node,'rotate','add',x,natFrames(ct,D,'rotate',i=>natNum(d(ct.nums[i]))+'deg'));
              else spec(node,'translate','add',x,natFrames(ct,D,'translate',i=>{const v=natNum(d(ct.nums[i]));return prop==='x'?v+'px 0px':'0px '+v+'px';}));
            }
            if(sx.length || sy.length) {
              const x=sx[0] || sy[0], baseX=unwrap(node.base.scaleX), baseY=unwrap(node.base.scaleY);
              const abs=(e,base)=>e?(e.over?(v=>v):(v=>base+e.c.weight*(v-base))):null, fx=abs(sx[0],baseX), fy=abs(sy[0],baseY);
              spec(node,'scale','replace',x,natFrames(x.t.ct,x.c.duration,'scale',i=>natNum(fx?fx(sx[0].t.ct.nums[i]):baseX)+' '+natNum(fy?fy(sy[0].t.ct.nums[i]):baseY)),(sx[0]?sx[0].i:'')+'+'+(sy[0]?sy[0].i:''));
            }
          }
        }
        modes.set(node,mode);
        // Opacity: a single contribution that reaches the output (JavaScript clamps, so does CSS).
        if(node.core) {
          const l=get(node.id,'opacity');
          if(clean(l)) {
            const e=effective(l);
            if(e.length<=1) {
              for(const x of l)natTracks.add(x.t);
              if(e.length) {
                const x=e[0], base=finite(unwrap(node.base.opacity==null?1:node.base.opacity),'Opacity'), ct=x.t.ct;
                spec(node,'opacity','replace',x,natFrames(ct,x.c.duration,'opacity',i=>String(natNum(x.over?ct.nums[i]:base+x.c.weight*(ct.nums[i]-base)))));
              }
            }
          }
        }
      }
      return {specs,natTracks,modes,lists,boneNative:[...trees.values()].some(t=>!t.js && t.native.size)};
    }
    const natSigBuf=[];
    function natSignature() {
      const s=natSigBuf;let i=0,changed=nat.sig===null;
      const put=v=>{if(!changed && s[i]!==v)changed=true;s[i++]=v;};
      put(explicitRev);put(afterRender!==null);put(nat.hidden);
      for(let k=0;k<clipList.length;k++){const c=clipList[k];put(c.active);put(c.playing);put(c.revision);put(c.ramp!==null?-1:c.weight);put(c.loop);}
      for(const m of machines.values()){put(m.state);put(m.fade?m.fade.from:null);}
      if(s.length!==i){s.length=i;changed=true;}
      nat.sig=s;return changed;
    }
    function natSync(rec) {
      const c=rec.c, a=rec.anim, t=c.time*1000;
      try {
        if(!c.playing || nat.hidden){a.pause();a.currentTime=t;}
        else {
          const tl=env.document && env.document.timeline, base=nat.clockMs();
          if(base!=null && tl && typeof tl.currentTime==='number')a.startTime=base-t;
          else {a.currentTime=t;a.play();}
        }
      } catch(e){report(e);}
      rec.sync=c.revision+'|'+c.playing+'|'+nat.hidden;
    }
    function natStyle(node,mode,list) {
      const el=node.el, st=el.style;let key='';
      if(mode===1){const p=natRestPoses().get(node.boneId);key=p?natNum(p.x)+'px '+natNum(p.y)+'px|'+natNum(p.rotation)+'deg||':'0px 0px|0deg||';}
      else if(mode===2){const b=node.base;key=natNum(unwrap(b.x))+'px '+natNum(unwrap(b.y))+'px|'+natNum(unwrap(b.rotation))+'deg|'+natNum(unwrap(b.scaleX))+' '+natNum(unwrap(b.scaleY))+'|';}
      else if(mode===3 && list)key=list.translate+'|'+list.rotate+'||'+list.transform;
      if(node.natStyle===key)return;
      const [tr,rot,sc,tf]=key?key.split('|'):['','','',''];
      try{st.translate=tr;st.rotate=rot;st.scale=sc;st.transform=tf;}catch(e){report(e);}
      node.natStyle=key || null;
    }
    // Brings animations, flags and styles in line with what currently plays. Runs at the start of render().
    function nativeCheck() {
      if(!nat.on || dead || !natSignature())return;
      let np;
      try{np=natPlan();}catch(e){report(e);nativeOff();return;}
      const anims=nat.anims;
      for(const [key,rec] of anims)if(!np.specs.has(key)){try{rec.anim.cancel();}catch(_){}anims.delete(key);}
      for(const node of nodes.values()) {
        const mode=np.modes.get(node) || 0;
        if(node.natMode!==mode){node.natMode=mode;node.lastProps=null;node.atRest=false;dirty=true;}
        if(node.el && node.el.style && (mode!==0 || node.natStyle!==null))natStyle(node,mode,np.lists.get(node));
      }
      for(const c of clipList) {
        const table=tableOf(c);let all=true;
        for(let i=0;i<table.length;i++){const t=table[i], on=np.natTracks.has(t);if(t.nat!==on){t.nat=on;dirty=true;}if(!on)all=false;}
        c.allNative=all;
      }
      nat.boneNative=np.boneNative;
      for(const [key,s] of np.specs) {
        let rec=anims.get(key);
        if(rec===undefined) {
          try {
            const anim=s.node.el.animate(s.frames,{duration:s.c.duration*1000,iterations:s.c.loop?Infinity:1,fill:'both',composite:s.composite,easing:'linear'});
            rec={anim,c:s.c,sync:null};anims.set(key,rec);
          } catch(e){report(e);nativeOff();return;}
        }
        if(rec.sync!==s.c.revision+'|'+s.c.playing+'|'+nat.hidden)natSync(rec);
      }
    }
    // Back to the JavaScript path for everything (audio unlock, destroy).
    function nativeOff(final) {
      if(!nat.on)return;
      if(final)for(const c of clipList)c.time=liveTime(c);
      nat.on=false;nat.sig=null;nat.boneNative=false;
      for(const rec of nat.anims.values())try{rec.anim.cancel();}catch(_){}
      nat.anims.clear();
      for(const node of nodes.values()){if(node.natMode!==0 || node.natStyle!==null){node.natMode=0;natStyle(node,0);node.lastProps=null;node.atRest=false;}}
      for(const c of clipList){c.allNative=false;if(c.table)for(const t of c.table)t.nat=false;}
      dirty=true;render();
    }
    // Clip time as the clock stands now: while the frame loop sleeps (native animations only, or hidden),
    // the stored time lags behind and is projected forward without ticking.
    function liveTime(c) {
      if(!c.playing)return c.time;
      const p=pendingDelta();if(!(p>0))return c.time;
      return c.loop?(c.time+p)%c.duration:Math.min(c.duration,c.time+p);
    }
    // Poses including natively animated bones (computed on demand; the render skips those tracks).
    function shadowBones() {
      const saved=[values,boneValues,boneWork,bonePoses,boneCache], solved=boneSolved.slice();
      values=new Map();freshBones();shadowing=true;
      try{computeFrame(true);return bonePoses.map(p=>({...p}));}
      finally{shadowing=false;[values,boneValues,boneWork,bonePoses,boneCache]=saved;boneSolved.set(solved);}
    }
    // Seconds until the next thing JavaScript must do while clips run natively: timers, one-shot clip
    // ends and timeline callbacks. Capped so a long sleep never needs a huge catch-up tick.
    function nextEvent() {
      let due=nextTimer();if(due<0)return due;
      let playing=false;
      for(let i=0;i<clipList.length;i++) {
        const c=clipList[i];if(!c.playing)continue;
        playing=true;
        const left=c.duration-c.time;if(!c.loop && left>=0 && left<due)due=left;
        for(const cb of callbacks)if(cb.clip===c.id){let l=cb.time-c.time;if(l<=0 && c.loop)l+=c.duration;if(l>0 && l<due)due=l;}
      }
      if(playing && due>10)due=10;
      return due;
    }
    // Idle detection for the auto-tick loop: true when a tick can change nothing visible or observable —
    // no clip playing or fading, no state-machine fade, followers at rest, no queued events, no plugin
    // frame hooks and no diagnostics that an unchanged frame would replay. A playing timeline transport
    // only advances its clock, which the catch-up tick on wake reproduces; its audio cues keep it awake.
    function quiet() {
      if(dirty || queue.length || frameHooks || renderReports.length || followMoving)return false;
      if(transport.playing && audio.ready && transport.audioCues.length)return false;
      for(let i=0;i<clipList.length;i++){const c=clipList[i];if(c.ramp || (c.playing && !c.allNative))return false;}
      for(const m of machines.values())if(m.fade)return false;
      return true;
    }
    // Seconds until the next timed (`after`) transition or timeline callback can fire; -1 when unknown.
    function nextTimer() {
      let due=Infinity;
      if(transport.playing)for(const cb of callbacks)if(cb.clip===transport.id){const left=cb.time-transport.time;if(left>0 && left<due)due=left;}
      for(const m of machines.values()) {
        if(m.halted)continue;
        const list=m.timers && m.timers.get(m.states.get(m.state));
        if(!list)return -1;
        for(let i=0;i<list.length;i++) {
          const x=list[i];
          if(x.from!=null && x.from!==m.state)continue;
          if(x.guard && (typeof x.guard!=='object' || !own(x.guard,'variable')))return -1;
          const left=x.after-(m.elapsed || 0);
          if(left>0 && left<due)due=left;
        }
      }
      return due;
    }
    const maxFps=options.maxFps==null?0:Number(options.maxFps);
    if(options.maxFps!=null && !(Number.isFinite(maxFps) && maxFps>0))throw new Error('maxFps must be a positive number');
    if(options.autoTick!==false && !dead){
      const now=()=>env.performance && env.performance.now?env.performance.now():Date.now();last=now();let clock=env;
      let sleeping=false,sleepTimer=null,frameStep=1000/60,framePending=false,frameAt=-Infinity,vsync=1000/60,woke=true;
      let hidden=false,hiddenTimer=null,hiddenBusy=false,hiddenPoke=false,intervalTimer=null;
      const advance=()=>{if(dead)return;const nextClock=audio.ready && context.state==='running'?context:env;
        const current=nextClock===env?now():context.currentTime*1000;
        if(clock!==nextClock){clock=nextClock;last=current;return;}
        const delta=Math.max(0,(current-last)/1000);if(!sleeping && delta>0)frameStep=Math.min(delta*1000,100);last=current;try{tick(delta);}catch(e){report(e);}
        if(sleeping && !hidden && wake!==null && !quiet())wake();};
      syncClock=()=>{if(!ticking && !processing && !dead)advance();};
      // Time the clock has run since the last tick while the frame loop sleeps (see liveTime).
      pendingDelta=()=>sleeping && clock===env && !ticking && !dead?Math.max(0,(now()-last)/1000):0;
      // Ramps (fadeTo) and audio clocks also advance from a 25 ms timer, so they progress while frames are
      // throttled. Armed only while one of them is in use.
      armInterval=()=>{
        if(intervalTimer!==null || dead || !env.setInterval)return;
        intervalTimer=env.setInterval(()=>{
          if(!audio.ready && !clipList.some(c=>c.ramp)){env.clearInterval(intervalTimer);intervalTimer=null;return;}
          if(hidden && !audio.ready)return;
          advance();
        },25);
      };
      cleanup.push(()=>{if(intervalTimer!==null)env.clearInterval(intervalTimer);intervalTimer=null;});
      if(audio.ready || clipList.some(c=>c.ramp))armInterval();
      const scheduler=env.requestAnimationFrame?frameScheduler(env):null;
      if(scheduler){
        const request=()=>{framePending=true;scheduler.request(frame);};
        // maxFps: one tick per slot of a grid shared by every runtime in the window (slot = floor((time+1 ms of
        // vsync jitter)/interval) of the frame's rAF timestamp), so throttled runtimes all tick in the same vsync
        // and the page draws at most maxFps frames instead of one per runtime phase.
        const interval=maxFps?1000/maxFps:0;let lastSlot=NaN;
        function frame(time){
          framePending=false;if(dead || hidden)return;
          if(interval){const slot=Math.floor(((typeof time==='number' && Number.isFinite(time)?time:now())+1)/interval);if(slot===lastSlot){request();return;}lastSlot=slot;}
          advance();
          if(clock===env){const d=last-frameAt;if(!woke && d>0 && d<100)vsync+=(d-vsync)*0.1;frameAt=last;woke=false;}
          if(dead || hidden || framePending)return;
          // Idle fast path: stop requesting frames while nothing animates in JavaScript. The clock (`last`)
          // keeps running, so the first tick after waking covers the slept time exactly like the skipped
          // ticks would have. Clips running as native animations count as idle; the loop wakes for their
          // ends and callbacks. Wakes: any API call or DOM event (ensure/listener), a non-quiet transport
          // tick, or the next timer.
          const due=quiet()?nextEvent():-1;
          if(due>=0 && (due===Infinity || env.setTimeout)) {
            sleeping=true;
            if(due!==Infinity)sleepTimer=env.setTimeout(()=>{sleepTimer=null;if(wake!==null)wake();},Math.max(0,due*1000-20));
            return;
          }
          request();
        }
        // Hidden (off screen or background tab): no frames and paused native animations. Clocks still
        // advance on timers at the next due event (state timers, clip ends, callbacks), so events fire on
        // time; nothing is drawn until the figure is visible again.
        const hiddenAdvance=()=>{if(hiddenBusy || ticking || processing || dead)return;hiddenBusy=true;hiddenTicking=hidden;try{advance();}finally{hiddenBusy=false;hiddenTicking=false;}};
        const hiddenArm=()=>{
          if(hiddenTimer!==null){if(env.clearTimeout)env.clearTimeout(hiddenTimer);hiddenTimer=null;}
          if(!hidden || dead || !env.setTimeout)return;
          let due=nextEvent();if(due<0)due=1;
          if(due===Infinity)return;
          // At least 1 ms: a due time a rounding error away must not re-arm without the clock moving.
          hiddenTimer=env.setTimeout(()=>{hiddenTimer=null;hiddenAdvance();hiddenArm();},Math.max(1,due*1000));
        };
        const setHidden=h=>{
          if(dead || h===hidden)return;
          if(h) {
            hiddenAdvance();if(dead)return;
            hidden=true;nat.hidden=true;sleeping=true;
            if(framePending){scheduler.cancel(frame);framePending=false;}
            if(sleepTimer!==null){if(env.clearTimeout)env.clearTimeout(sleepTimer);sleepTimer=null;}
            if(nat.on){nativeCheck();if(dirty)render();}
            hiddenArm();
          } else {
            hidden=false;nat.hidden=false;
            if(hiddenTimer!==null){if(env.clearTimeout)env.clearTimeout(hiddenTimer);hiddenTimer=null;}
            if(wake!==null)wake();
            if(nat.on && !dead){nativeCheck();if(dirty)render();}
          }
        };
        wake=()=>{
          if(!sleeping || hiddenBusy)return;
          if(hidden) {
            // An API call while hidden: bring the clocks up to date, then re-plan the next timer after it.
            hiddenAdvance();
            if(!hiddenPoke && env.setTimeout){hiddenPoke=true;env.setTimeout(()=>{hiddenPoke=false;hiddenArm();},0);}
            return;
          }
          sleeping=false;woke=true;
          if(sleepTimer!==null){if(env.clearTimeout)env.clearTimeout(sleepTimer);sleepTimer=null;}
          // Catch the clock up to the last frame the loop would have run (the last frame plus whole frame
          // periods, `vsync` being the running average; one step before now without a frame yet) with a quiet tick, so the next frame advances by a normal
          // frame step instead of the whole slept time — as if the loop had kept ticking.
          if(clock===env && !ticking && !processing && !dead) {
            const t=now(), tl=env.document && env.document.timeline, ft=tl && typeof tl.currentTime==='number'?tl.currentTime:NaN;
            // Browsers report the current frame's time as document.timeline.currentTime (same time origin).
            const target=ft<=t && ft>t-100?ft:frameAt>-Infinity && t>=frameAt?frameAt+Math.floor((t-frameAt)/vsync)*vsync:t-frameStep;
            if(target>last){const delta=(target-last)/1000;last=target;creditTick=true;try{tick(delta);}catch(e){report(e);}finally{creditTick=false;}}
          }
          if(!dead && !framePending)request();
        };
        cleanup.push(()=>{wake=null;if(framePending)scheduler.cancel(frame);framePending=false;if(sleepTimer!==null && env.clearTimeout)env.clearTimeout(sleepTimer);if(hiddenTimer!==null && env.clearTimeout)env.clearTimeout(hiddenTimer);});
        request();
        if(nativeSupported()){nat.on=true;nat.clockMs=()=>clock===env?last:null;nat.sig=null;dirty=true;render();}
        if(options.pauseWhenHidden!==false) {
          const doc=svg.ownerDocument || env.document;
          let offscreen=false,docHidden=!!(doc && doc.visibilityState==='hidden');
          const update=()=>setHidden(offscreen || docHidden);
          if(doc && typeof doc.addEventListener==='function') {
            const onVisibility=()=>{docHidden=doc.visibilityState==='hidden';update();};
            doc.addEventListener('visibilitychange',onVisibility);cleanup.push(()=>doc.removeEventListener('visibilitychange',onVisibility));
          }
          if(typeof env.IntersectionObserver==='function') {
            try {
              const observer=new env.IntersectionObserver(list=>{const e=list[list.length-1];if(e && !dead){offscreen=!e.isIntersecting;update();}});
              observer.observe(svg);cleanup.push(()=>observer.disconnect());
            } catch(e){report(e);}
          }
          if(docHidden)update();
        }
      }
    }
    // Native animations need: the auto-tick loop (it keeps their clocks), element.animate with composite
    // modes and the individual CSS transform properties. options.nativeAnimations === false opts out.
    function nativeSupported() {
      if(options.nativeAnimations===false || audio.ready || typeof svg.animate!=='function')return false;
      try {
        const css=env.CSS, effect=env.KeyframeEffect;
        if(!css || typeof css.supports!=='function' || !css.supports('translate','1px 2px') || !css.supports('rotate','1deg') || !css.supports('scale','1 2'))return false;
        return !!(effect && effect.prototype && 'composite' in effect.prototype && svg.style);
      } catch(_){return false;}
    }
    return api;
  }
  // Runtime plugin API (docs/plugins.md). apiVersion must match exactly; additions bump it.
  const PLUGIN_API_VERSION=1,PLUGIN_ID=/^[A-Za-z][A-Za-z0-9_.-]{0,63}$/,PLUGIN_NAME=/^[A-Za-z][A-Za-z0-9._:-]{0,63}$/;
  const PLUGIN_KEYS=['id','apiVersion','events','frameBudgetMs','setup','onEvent','onState','onFrame'];
  const PLUGIN_MAX=8,PLUGIN_MAX_EVENTS=32,PLUGIN_MAX_ERRORS=5,PLUGIN_MAX_OVERRUNS=3,PLUGIN_FRAME_BUDGET_MS=4;
  // One requestAnimationFrame loop per window for every runtime in it (also across copies of this file):
  // runtimes register a frame callback; a single browser frame dispatches all pending ones.
  const schedulerKey=typeof Symbol==='function'?Symbol.for('splinely.frameScheduler.v1'):'__splinelyFrameSchedulerV1';
  const schedulerFallback=typeof WeakMap==='function'?new WeakMap():null;
  function frameScheduler(env) {
    let s=null;
    try{s=env[schedulerKey] || null;}catch(_){}
    if(s===null && schedulerFallback)s=schedulerFallback.get(env) || null;
    if(s!==null && typeof s.request==='function' && typeof s.cancel==='function')return s;
    let pending=new Set(), handle=null;
    const dispatch=time=>{
      handle=null;const run=pending;pending=new Set();
      for(const fn of run)try{fn(time);}catch(e){if(env.console && env.console.error)env.console.error(e);}
    };
    s={
      request(fn){pending.add(fn);if(handle===null)handle=env.requestAnimationFrame(dispatch);},
      cancel(fn){pending.delete(fn);if(!pending.size && handle!==null){if(env.cancelAnimationFrame)env.cancelAnimationFrame(handle);handle=null;}},
      get size(){return pending.size;}
    };
    try{Object.defineProperty(env,schedulerKey,{value:s,configurable:true});}catch(_){if(schedulerFallback)schedulerFallback.set(env,s);}
    return s;
  }
  const registryKey=typeof Symbol==='function'?Symbol.for('splinely.runtime.instances.v1'):'__splinelyRuntimeInstancesV1';
  function register(env,api){if(!env)return;if(!env[registryKey])env[registryKey]={instances:new Set(),alias:null};const r=env[registryKey];r.instances.add(api);syncAlias(env,r);}
  function unregister(env,api){const r=env && env[registryKey];if(r){r.instances.delete(api);syncAlias(env,r);}}
  function syncAlias(env,r){if(r.alias && env.splinely===r.alias)delete env.splinely;r.alias=null;if(r.instances.size===1 && env.splinely==null){r.alias=[...r.instances][0];env.splinely=r.alias;}}
  // Options for the runtime's own start from the root <svg> (exporter host attributes): data-splinely-max-fps="N"
  // (N > 0) and data-splinely-native-animations="false". Other values keep the defaults.
  function autoStartOptions(svg) {
    const options={},get=name=>typeof svg.getAttribute==='function'?svg.getAttribute(name):null;
    const fps=get('data-splinely-max-fps');
    if(fps!=null && /^\s*\d+(\.\d+)?\s*$/.test(fps) && Number(fps)>0)options.maxFps=Number(fps);
    if(get('data-splinely-native-animations')==='false')options.nativeAnimations=false;
    return options;
  }
  const exported={createRuntime,readProject,normalize,evaluateTrack,interpolate,easing,PLUGIN_API_VERSION};
  if(typeof module==='object' && module.exports)module.exports=exported;
  else {
    if(!host.SplinelyRuntime)host.SplinelyRuntime=exported;
    const script=host.document && host.document.currentScript;
    const svg=script && (script.ownerSVGElement || (script.parentNode && script.parentNode.closest && script.parentNode.closest('svg')));
    if(svg && !svg.splinely)try{createRuntime(svg,undefined,autoStartOptions(svg));}catch(e){if(host.console)host.console.error('Splinely initialization failed; static SVG retained:',e);}
  }
})(typeof window!=='undefined'?window:globalThis);
