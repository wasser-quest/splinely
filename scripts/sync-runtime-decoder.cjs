#!/usr/bin/env node
'use strict';
// Injects runtime/src/sply_decode.js (the single source of truth for the SPLY decoder) into
// runtime/src/splinely.js between the `// <sply-decode>` and `// </sply-decode>` marker lines.
//   node scripts/sync-runtime-decoder.cjs           rewrite splinely.js if stale
//   node scripts/sync-runtime-decoder.cjs --check   exit 1 if the injected copy differs
// The decoder is wrapped so its CommonJS export is captured in a local `module` object:
// no global (`SplyDecode`) is created and the host runtime's own module.exports is untouched.
const fs = require('node:fs');
const path = require('node:path');

const root = path.join(__dirname, '..');
const DECODER = path.join(root, 'runtime/src/sply_decode.js');
const RUNTIME = path.join(root, 'runtime/src/splinely.js');
const OPEN = '  // <sply-decode>';
const CLOSE = '  // </sply-decode>';

function block(decoderSource) {
  const body = decoderSource.replace(/\r\n/g, '\n').replace(/\n+$/, '');
  if (body.includes('\u0000')) throw new Error('decoder contains NUL');
  return [
    OPEN,
    '  // GENERATED from runtime/src/sply_decode.js by scripts/sync-runtime-decoder.cjs. Do not edit; run the script.',
    '  const SplyDecode = (function () {',
    '    const module = { exports: {} }; // captures the decoder export; keeps it off the global object',
    body,
    '    return module.exports;',
    '  })();',
    CLOSE,
  ].join('\n');
}

// Returns runtime source with the marker region replaced by the freshly generated block.
function inject(runtimeSource, decoderSource) {
  const text = runtimeSource.replace(/\r\n/g, '\n');
  const start = text.indexOf(OPEN + '\n');
  const end = text.indexOf(CLOSE);
  if (start < 0 || end < start || text.indexOf(OPEN + '\n', start + 1) >= 0 || text.indexOf(CLOSE, end + 1) >= 0)
    throw new Error('splinely.js must contain exactly one `// <sply-decode>` ... `// </sply-decode>` marker pair');
  return text.slice(0, start) + block(decoderSource) + text.slice(end + CLOSE.length);
}

if (require.main === module) {
  const current = fs.readFileSync(RUNTIME, 'utf8');
  const next = inject(current, fs.readFileSync(DECODER, 'utf8'));
  if (process.argv.includes('--check')) {
    if (next !== current) {
      console.error('runtime/src/splinely.js is out of date; run: node scripts/sync-runtime-decoder.cjs');
      process.exit(1);
    }
  } else if (next !== current) {
    fs.writeFileSync(RUNTIME, next);
    console.log('updated runtime/src/splinely.js');
  }
}
module.exports = { inject, block, DECODER, RUNTIME };
