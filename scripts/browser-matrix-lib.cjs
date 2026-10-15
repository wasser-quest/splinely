'use strict';
// Shared helpers of scripts/browser-matrix.cjs and scripts/browser-audio-latency.cjs.
// No dependencies besides Node and an external Playwright install (PLAYWRIGHT_MODULE).
const fs = require('node:fs');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');
const {execFileSync} = require('node:child_process');

const ROOT = path.resolve(__dirname, '..');
const ENGINES = ['chromium', 'firefox', 'webkit'];
const MIME = {
  '.html': 'text/html; charset=utf-8', '.svg': 'image/svg+xml', '.json': 'application/json',
  '.js': 'text/javascript', '.cjs': 'text/javascript', '.png': 'image/png', '.sply': 'application/octet-stream',
  '.wav': 'audio/wav',
};

function loadPlaywright() {
  try {
    return {pw: require(process.env.PLAYWRIGHT_MODULE || 'playwright'), error: null};
  } catch (error) {
    return {pw: null, error: `Playwright not loadable (set PLAYWRIGHT_MODULE): ${error.message.split('\n')[0]}`};
  }
}

// Scratch space: never the 1 GB tmpfs unless the caller says so.
function scratchDir(prefix) {
  const base = process.env.SPLINELY_SCRATCH || path.join(os.homedir(), '.cache', 'splinely-scratch');
  fs.mkdirSync(base, {recursive: true});
  return fs.mkdtempSync(path.join(base, prefix));
}

function cliPath() {
  return path.resolve(process.env.SPLINELY_CLI || path.join(ROOT, 'target/debug/splinely-cli'));
}

function cliInfo() {
  const file = cliPath();
  try {
    const stat = fs.statSync(file);
    const version = execFileSync(file, ['--version'], {encoding: 'utf8'}).trim();
    return {path: file, version, mtime: stat.mtime.toISOString(), bytes: stat.size};
  } catch (error) {
    return {path: file, error: `unavailable: ${String(error.message).split('\n')[0]} (cargo build -p splinely-cli)`};
  }
}

function cli(args) {
  return execFileSync(cliPath(), args.map(String), {encoding: 'utf8', maxBuffer: 256 * 1024 * 1024});
}

// Serves the repository (read-only) under "/" and an optional generated directory under "/gen/".
function startServer(genDir) {
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, 'http://x');
    // /__jump?to=URL : renderer-initiated navigation (script, no input), used for the audio gesture test.
    if (url.pathname === '/__jump') {
      res.writeHead(200, {'content-type': 'text/html'});
      res.end(`<!doctype html><title>jump</title><script>setTimeout(()=>location.replace(${JSON.stringify(url.searchParams.get('to'))}),50)</script>`);
      return;
    }
    // /__blank : real HTML+inline-SVG document with the runtime script, for DOM-level runtime tests.
    if (url.pathname === '/__blank') {
      res.writeHead(200, {'content-type': 'text/html'});
      res.end('<!doctype html><title>blank</title><svg id="s" xmlns="http://www.w3.org/2000/svg" width="100" height="100" viewBox="0 0 100 100"><circle id="n" cx="5" cy="5" r="2"/></svg><script src="/runtime/src/splinely.js"></script>');
      return;
    }
    let rel = decodeURIComponent(url.pathname);
    let base = ROOT;
    if (rel.startsWith('/gen/') && genDir) { base = genDir; rel = rel.slice(4); }
    const file = path.resolve(base, '.' + rel);
    if (!file.startsWith(base + path.sep) || !fs.existsSync(file) || !fs.statSync(file).isFile()) {
      res.writeHead(404); res.end('not found'); return;
    }
    res.writeHead(200, {'content-type': MIME[path.extname(file)] || 'application/octet-stream', 'cache-control': 'no-store'});
    fs.createReadStream(file).pipe(res);
  });
  return new Promise(resolve => server.listen(0, '127.0.0.1', () => resolve({
    server, origin: `http://127.0.0.1:${server.address().port}`,
    close: () => new Promise(done => { server.closeAllConnections?.(); server.close(done); }),
  })));
}

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

// Classifies launch failures so a missing/broken browser is reported as SKIP with the reason.
async function launchEngine(pw, engine, options = {}) {
  if (!pw) throw Object.assign(new Error('Playwright unavailable'), {skip: true});
  try {
    return await pw[engine].launch({headless: true, timeout: 60000, ...options});
  } catch (error) {
    let message = String(error.message).split('\n').map(l => l.replace(/[║╔╗╚╝═]/g, '').trim()).filter(Boolean);
    const hostCheck = message.some(l => /missing dependencies to run browsers/.test(l));
    message = message.slice(0, hostCheck ? 2 : 6).join(' | ');
    if (hostCheck) message += ' | (Playwright host validation failed. On an unsupported distro with the libraries present, PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=1 bypasses only the check; on Ubuntu use `npx playwright install --with-deps`.)';
    throw Object.assign(new Error(message), {skip: true});
  }
}

module.exports = {ROOT, ENGINES, loadPlaywright, scratchDir, cliPath, cliInfo, cli, startServer, sleep, launchEngine};
