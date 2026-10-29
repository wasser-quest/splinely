#!/usr/bin/env node
// Real Chromium check of runtime/src/splinely-mount.js: an exported graphic is mounted inline in a
// page (no iframe), runs its clips, and is removed again. PLAYWRIGHT_MODULE may point to an install.
const playwright = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');

const root = path.resolve(__dirname, '..');
const graphic = fs.readFileSync(path.join(root, 'examples', 'demo.svg'), 'utf8');
const page = `<!doctype html><meta charset="utf-8"><title>inline mount</title>
<div id="stage" style="width:320px"></div>
<script src="/runtime/src/splinely.js"></script>
<script src="/runtime/src/splinely-mount.js"></script>`;

const server = http.createServer((req, res) => {
  if (req.url === '/') {
    res.setHeader('content-type', 'text/html; charset=utf-8');
    return res.end(page);
  }
  const file = path.join(root, decodeURIComponent(req.url.split('?')[0]));
  if (!file.startsWith(root) || !fs.existsSync(file)) { res.statusCode = 404; return res.end(); }
  res.setHeader('content-type', file.endsWith('.js') ? 'text/javascript' : 'text/plain');
  res.end(fs.readFileSync(file));
});

(async () => {
  await new Promise(done => server.listen(0, '127.0.0.1', done));
  const origin = `http://127.0.0.1:${server.address().port}`;
  const browser = await playwright.chromium.launch({ headless: true, args: ['--no-sandbox'] });
  let failures = 0;
  const check = (name, ok, detail = '') => {
    console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${!ok && detail ? ` [${detail}]` : ''}`);
    if (!ok) failures++;
  };
  try {
    const context = await browser.newContext();
    const tab = await context.newPage();
    const errors = [];
    tab.on('pageerror', error => errors.push(error.message));
    await tab.goto(origin + '/', { waitUntil: 'load' });
    await tab.evaluate(async (svgText) => {
      const container = document.getElementById('stage');
      window.graphic = window.SplinelyMount.mount(container, svgText);
    }, graphic);

    const state = await tab.evaluate(() => ({
      svgInside: !!document.querySelector('#stage > svg'),
      iframes: document.querySelectorAll('iframe').length,
      scriptsInside: document.querySelectorAll('#stage script').length,
      error: window.graphic.error ? String(window.graphic.error.message || window.graphic.error) : null,
      hasRuntime: !!window.graphic.runtime,
      clipIds: window.graphic.runtime ? window.graphic.runtime.snapshot().clips.map(clip => clip.id) : [],
    }));
    check('the graphic is an inline SVG in the page', state.svgInside);
    check('no iframe is used', state.iframes === 0, `iframes=${state.iframes}`);
    check('the graphic ships no script inside the page', state.scriptsInside === 0, `scripts=${state.scriptsInside}`);
    check('the runtime starts', state.hasRuntime && state.error === null, state.error || 'no runtime');

    if (state.clipIds.length) {
      const clipId = state.clipIds[0];
      const before = await tab.evaluate(id => window.graphic.runtime.clip(id).time, clipId);
      await tab.evaluate(id => window.graphic.runtime.clip(id).play(), clipId);
      await tab.evaluate(() => new Promise(done => setTimeout(done, 300)));
      const after = await tab.evaluate(id => window.graphic.runtime.clip(id).time, clipId);
      check('a clip plays in the page', after > before, `time ${before} -> ${after}`);
    } else {
      check('the graphic has a clip to play', false, 'no clips');
    }

    await tab.evaluate(() => window.graphic.destroy());
    const gone = await tab.evaluate(() => document.querySelectorAll('#stage > svg').length === 0);
    check('destroy removes the graphic', gone);

    // Sanitizer: markup the exporter never writes does not reach the page.
    const hostile = await tab.evaluate(() => {
      const text = '<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" onload="window.hit=1">' +
        '<style>#stage{display:none}</style><rect id="r" onclick="window.hit=1" style="position:fixed" fill="url(https://example.invalid/x)" stroke="url(#r)"/>' +
        '<foreignObject><div xmlns="http://www.w3.org/1999/xhtml">x</div></foreignObject><a href="javascript:window.hit=1"><circle r="1"/></a>' +
        '<use href="https://example.invalid/s.svg#a"/><use xlink:href="#r"/><image href="https://example.invalid/i.png"/>' +
        '<animate attributeName="href" to="javascript:x"/><set attributeName="fill" to="red"/><script>window.hit=1</script>' +
        '<metadata id="splinely-project">{"schemaVersion":1,"nodes":[]}</metadata></svg>';
      const g = window.SplinelyMount.mount(document.getElementById('stage'), text);
      const html = g.svg.outerHTML;
      g.destroy();
      return { html, hit: !!window.hit, display: getComputedStyle(document.getElementById('stage')).display };
    });
    check('sanitizer removes scripts, styles, handlers, foreign content and external references',
      !/style|onload|onclick|foreignObject|<a |<image|<animate|<set|<script|example\.invalid/.test(hostile.html) &&
      /<use xlink:href="#r"/.test(hostile.html) && /stroke="url\(#r\)"/.test(hostile.html) && !hostile.hit && hostile.display !== 'none',
      hostile.html);

    // decodeSplyBase64: the native Uint8Array.fromBase64 path and the atob fallback agree.
    await tab.addScriptTag({ url: '/runtime/src/sply_decode.js' });
    const base64 = await tab.evaluate((valid) => {
      const native = Uint8Array.fromBase64;
      const run = (s, useNative) => {
        if (!useNative) delete Uint8Array.fromBase64;
        try { return JSON.stringify(window.SplyDecode.decodeSplyBase64(s)); } catch (e) { return 'E:' + e.message; }
        finally { if (native) Uint8Array.fromBase64 = native; }
      };
      const cases = [valid, ' ' + valid + '\n', valid.slice(0, -1), valid.slice(0, -2) + '@=', '', 'AA==', 'AB==', 'AA=A', 'A===', '\u00a0' + valid];
      return { native: typeof native, mismatches: cases.filter(s => run(s, true) !== run(s, false)).length };
    }, /<metadata id="splinely-project" data-encoding="sply-base64"[^>]*>([^<]*)</.exec(fs.readFileSync(path.join(root, 'examples', 'water-drop.svg'), 'utf8'))[1]);
    check('native and fallback base64 decoding agree', base64.mismatches === 0, JSON.stringify(base64));
    check('no page errors', errors.length === 0, errors.join(' | '));
  } finally {
    await browser.close();
    server.close();
  }
  if (failures) {
    console.error(`${failures} check(s) failed`);
    process.exit(1);
  }
  console.log('inline mount: all checks passed');
})().catch(error => {
  console.error(error);
  server.close();
  process.exit(1);
});
