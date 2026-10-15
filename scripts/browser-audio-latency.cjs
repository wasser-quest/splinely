#!/usr/bin/env node
'use strict';
// Measures REAL audible output and cue latency of a Splinely export (default examples/anime-hello.svg).
//   * A PulseAudio/PipeWire null sink is created; the browser is routed to it (PULSE_SINK) and its
//     ".monitor" source is recorded with parec. RMS/peak/onset come from those captured samples.
//   * Gesture policy is verified, not bypassed: audio must be silent BEFORE the real Playwright click
//     (runtime audio locked, a gesture-less AudioContext stays suspended) and audible AFTER it.
//   * Latency = first captured sample above threshold minus (a) the in-page click event time and
//     (b) the AudioBufferSourceNode.start() call time (+ its scheduled `when`).
//   * Optional native leg: the editor's ffplay command line, spawn -> first sound.
// Result JSON: status pass|fail|skip + reason. No audio server => status "skip" (loud, exit 3), never "pass".
//
// node scripts/browser-audio-latency.cjs [--browser chromium|firefox] [--runs 3] [--out file.json]
//     [--svg examples/anime-hello.svg] [--threshold 0.003] [--no-ffplay]
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {spawn, spawnSync} = require('node:child_process');
const lib = require('./browser-matrix-lib.cjs');

const SINK = 'splinely_matrix_sink';
const RATE = 48000;

function parseArgs(argv) {
  const args = {browser: 'chromium', runs: 3, out: null, svg: 'examples/anime-hello.svg', threshold: 0.003, ffplay: true};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i], next = () => argv[++i];
    if (a === '--browser') args.browser = next();
    else if (a === '--runs') args.runs = Number(next());
    else if (a === '--out') args.out = next();
    else if (a === '--svg') args.svg = next();
    else if (a === '--threshold') args.threshold = Number(next());
    else if (a === '--no-ffplay') args.ffplay = false;
    else throw new Error('unknown argument ' + a);
  }
  return args;
}

function pactl(args) {
  const r = spawnSync('pactl', args, {encoding: 'utf8', timeout: 8000});
  return {ok: r.status === 0, out: (r.stdout || '').trim(), err: (r.stderr || String(r.error || '')).trim()};
}

// ---- recording -----------------------------------------------------------------------------------
class Recorder {
  start() {
    this.chunks = []; this.frames = 0; this.buffers = [];
    this.proc = spawn('parec', ['-d', SINK + '.monitor', '--raw', '--format=s16le', `--rate=${RATE}`,
      '--channels=2', '--latency-msec=5'], {stdio: ['ignore', 'pipe', 'pipe']});
    this.stderr = '';
    this.proc.stderr.on('data', d => { this.stderr += d; });
    this.proc.stdout.on('data', data => {
      const arrival = performance.timeOrigin + performance.now();
      this.buffers.push(data);
      this.frames += Math.floor(data.length / 4);
      this.chunks.push({arrival, endFrame: this.frames});
    });
    this.startedAt = performance.timeOrigin + performance.now();
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => this.chunks.length ? resolve() : reject(new Error('parec produced no data: ' + this.stderr)), 3000);
      this.proc.once('error', reject);
      const poll = setInterval(() => { if (this.chunks.length > 2) { clearInterval(poll); clearTimeout(timer); resolve(); } }, 10);
    });
  }
  async stop() {
    this.proc.kill('SIGTERM');
    await new Promise(resolve => this.proc.once('close', resolve));
    const raw = Buffer.concat(this.buffers);
    const frames = Math.floor(raw.length / 4);
    // Delivery only ever adds delay, so the earliest-arriving line (arrival - frames/rate) is the best
    // estimate of the capture clock; this removes pipe/scheduler jitter. Residual error is about the
    // capture period (5 ms) plus pipe/scheduling latency (typically < 5 ms).
    let base = Infinity;
    for (const c of this.chunks) base = Math.min(base, c.arrival - c.endFrame / RATE * 1000);
    this.pcm = new Int16Array(raw.buffer.slice(raw.byteOffset, raw.byteOffset + frames * 4));
    this.base = base;
    this.frameCount = frames;
    return this;
  }
  timeOfFrame(i) { return this.base + i / RATE * 1000; }
  frameAt(epochMs) { return Math.max(0, Math.min(this.frameCount, Math.round((epochMs - this.base) * RATE / 1000))); }
  stats(fromMs, toMs) {
    const a = this.frameAt(fromMs), b = this.frameAt(toMs);
    const sum = [0, 0]; let peak = 0;
    for (let i = a; i < b; i++) for (let c = 0; c < 2; c++) {
      const v = this.pcm[2 * i + c] / 32768; sum[c] += v * v; peak = Math.max(peak, Math.abs(v));
    }
    const n = Math.max(1, b - a), rms = sum.map(s => Math.sqrt(s / n));
    return {frames: b - a, ms: (b - a) / RATE * 1000, rms: Math.sqrt((rms[0] ** 2 + rms[1] ** 2) / 2), rmsPerChannel: rms, peak,
      rmsDbfs: db(Math.sqrt((rms[0] ** 2 + rms[1] ** 2) / 2)), peakDbfs: db(peak)};
  }
  firstAbove(threshold, fromMs) {
    for (let i = this.frameAt(fromMs); i < this.frameCount; i++)
      if (Math.max(Math.abs(this.pcm[2 * i]), Math.abs(this.pcm[2 * i + 1])) / 32768 > threshold) return {frame: i, epochMs: this.timeOfFrame(i)};
    return null;
  }
  lastAbove(threshold) {
    for (let i = this.frameCount - 1; i >= 0; i--)
      if (Math.max(Math.abs(this.pcm[2 * i]), Math.abs(this.pcm[2 * i + 1])) / 32768 > threshold) return {frame: i, epochMs: this.timeOfFrame(i)};
    return null;
  }
}
const db = v => v > 0 ? Math.round(20 * Math.log10(v) * 10) / 10 : -Infinity;
const epochNow = () => performance.timeOrigin + performance.now();
const round = (v, d = 2) => v == null ? v : Math.round(v * 10 ** d) / 10 ** d;
const summary = values => {
  const v = values.filter(x => Number.isFinite(x)).sort((a, b) => a - b);
  return v.length ? {n: v.length, min: round(v[0]), median: round(v[Math.floor((v.length - 1) / 2)]), max: round(v[v.length - 1]), mean: round(v.reduce((s, x) => s + x, 0) / v.length)} : {n: 0};
};

// Which sink does the browser's playback stream feed? (proves audio really went to our null sink)
function sinkInputEvidence(sinkIndex) {
  const r = pactl(['--format=json', 'list', 'sink-inputs']);
  if (!r.ok) return {error: r.err};
  try {
    return JSON.parse(r.out).map(i => ({binary: i.properties?.['application.process.binary'] || i.properties?.['application.name'],
      sink: i.sink, onTestSink: i.sink === sinkIndex, corked: i.corked, muted: i.mute}));
  } catch (error) { return {error: String(error)}; }
}

// ---- browser leg ---------------------------------------------------------------------------------
const INIT = () => {
  window.__audioProbe = {clicks: [], starts: []};
  const epoch = () => performance.timeOrigin + performance.now();
  document.addEventListener('click', e => window.__audioProbe.clicks.push({trusted: e.isTrusted, target: e.target?.id || e.target?.tagName,
    eventEpochMs: performance.timeOrigin + e.timeStamp, seenEpochMs: epoch()}), true);
  // Gesture-less probe, started from a DOM event of the page itself (NOT page.evaluate: Playwright runs
  // evaluate() with userGesture=true in Chromium, which would legitimately unlock audio).
  document.addEventListener('DOMContentLoaded', () => setTimeout(async () => {
    try {
      const c = new AudioContext(), o = c.createOscillator(), g = c.createGain();
      g.gain.value = 0.5; o.frequency.value = 880; o.connect(g); g.connect(c.destination); o.start();
      await new Promise(r => setTimeout(r, 700));
      window.__audioProbe.policy = {state: c.state, currentTime: c.currentTime, userActivation: navigator.userActivation ? navigator.userActivation.hasBeenActive : null};
      o.stop(); await c.close();
    } catch (e) { window.__audioProbe.policy = {error: String(e)}; }
  }, 50));
  const start = AudioBufferSourceNode.prototype.start;
  AudioBufferSourceNode.prototype.start = function (when, offset, duration) {
    window.__audioProbe.starts.push({callEpochMs: epoch(), when: when ?? 0, offset: offset ?? 0, duration: duration ?? null,
      contextTime: this.context.currentTime, contextState: this.context.state, bufferDuration: this.buffer?.duration});
    return start.apply(this, arguments);
  };
};

async function browserRun(pw, args, genOrigin, sinkIndex, run) {
  const engine = args.browser;
  const launchOptions = {env: {...process.env, PULSE_SINK: SINK}};
  if (engine === 'chromium') {
    // The product requires a user gesture, so keep Chromium's own default policy explicitly and unmute.
    launchOptions.args = ['--autoplay-policy=document-user-activation-required'];
    launchOptions.ignoreDefaultArgs = ['--mute-audio'];
  } else if (engine === 'firefox') {
    launchOptions.firefoxUserPrefs = {'media.autoplay.default': 1, 'media.autoplay.blocking_policy': 0};
  }
  const browser = await lib.launchEngine(pw, engine, launchOptions);
  const recorder = new Recorder();
  const result = {run, checks: {}};
  try {
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.addInitScript(INIT);
    await recorder.start();
    const loadAt = epochNow();
    // Cross-origin (127.0.0.1 -> localhost) renderer-initiated hop: Chromium carries sticky user activation across
    // same-origin navigations, which would unlock audio without any gesture.
    await page.goto(`${genOrigin}/__jump?to=${encodeURIComponent(genOrigin.replace('127.0.0.1', 'localhost') + '/' + args.svg)}`);
    await page.waitForFunction(() => !!document.documentElement.splinely, null, {timeout: 15000});
    const mountedAt = epochNow();
    const pre = await page.evaluate(() => {
      const f = document.documentElement.splinely;
      return {ready: f.audio.ready, context: f.audio.context ? f.audio.context.state : 'none', controls: document.querySelectorAll('[data-splinely-host-control]').length, playing: f.timeline.playing};
    });
    // Gesture-less AudioContext + oscillator (init script above): must stay suspended and silent.
    await page.waitForFunction(() => !!window.__audioProbe.policy, null, {timeout: 5000}).catch(() => {});
    const policy = await page.evaluate(() => window.__audioProbe.policy || {error: 'probe did not finish'});
    await lib.sleep(500);
    const clickNode = epochNow();
    await page.click('[data-splinely-host-control] rect');
    const clickAfter = epochNow();
    await page.waitForFunction(() => window.__audioProbe.starts.length > 0, null, {timeout: 8000}).catch(() => {});
    await lib.sleep(450);
    result.sinkInputs = sinkInputEvidence(sinkIndex);
    await lib.sleep(1200);
    const post = await page.evaluate(() => {
      const f = document.documentElement.splinely, ctx = f.audio.context, probe = window.__audioProbe;
      const buf = [...f.audio.buffers.values()][0];
      let sum = 0, peak = 0, n = 0;
      if (buf) { // RMS over the sounding part only (first..last sample above 0.003), comparable with the capture
        const d = buf.getChannelData(0); let a = 0, b = d.length - 1;
        while (a < b && Math.abs(d[a]) < 0.003) a++;
        while (b > a && Math.abs(d[b]) < 0.003) b--;
        for (let ch = 0; ch < buf.numberOfChannels; ch++) for (let i = a; i <= b; i++) { const v = buf.getChannelData(ch)[i]; sum += v * v; peak = Math.max(peak, Math.abs(v)); n++; }
      }
      return {ready: f.audio.ready, context: ctx ? ctx.state : 'none', sampleRate: ctx?.sampleRate, baseLatency: ctx?.baseLatency ?? null,
        outputLatency: ctx?.outputLatency ?? null, probe, timeline: f.timeline.time, playing: f.timeline.playing,
        asset: buf ? {duration: buf.duration, channels: buf.numberOfChannels, sampleRate: buf.sampleRate, rms: Math.sqrt(sum / Math.max(1, n)), peak} : null,
        diagnostics: f.diagnostics.map(d => d.message)};
    });
    await recorder.stop();
    // ---- analysis ----
    const threshold = args.threshold;
    // Pre-gesture window: from shortly after the runtime mounted (so it includes the gesture-less AudioContext
    // probe) to just before the click. Whatever the browser itself emits while loading its page is reported
    // separately as loadTransient (observed with Playwright's Firefox: ~50 ms burst unrelated to page code).
    const preStats = recorder.stats(mountedAt + 150, clickNode - 20);
    const loadTransient = recorder.stats(loadAt, mountedAt + 150);
    const click = post.probe.clicks[0];
    const start = post.probe.starts[0];
    const clickEpoch = click ? click.eventEpochMs : clickNode;
    const onset = recorder.firstAbove(threshold, clickNode - 20);
    const end = recorder.lastAbove(threshold);
    const audible = onset ? recorder.stats(onset.epochMs, Math.min(end.epochMs, onset.epochMs + (post.asset ? post.asset.duration * 1000 : 400))) : null;
    const scheduledStartMs = start ? start.callEpochMs + Math.max(0, start.when - start.contextTime) * 1000 : null;
    Object.assign(result, {
      pre, policy, baseLatencyMs: post.baseLatency == null ? null : round(post.baseLatency * 1000), outputLatencyMs: post.outputLatency == null ? null : round(post.outputLatency * 1000),
      contextSampleRate: post.sampleRate, contextState: post.context, asset: post.asset, diagnostics: post.diagnostics,
      recordedMs: round(recorder.frameCount / RATE * 1000, 0),
      loadTransient: {peak: round(loadTransient.peak, 4), ms: round(loadTransient.ms, 0)}, preGesture: {...preStats, windowMs: round(preStats.ms, 0)},
      audible: audible && {rmsDbfs: audible.rmsDbfs, peakDbfs: audible.peakDbfs, rms: round(audible.rms, 5), peak: round(audible.peak, 5), ms: round(audible.ms, 0),
        soundEndsAfterOnsetMs: round(end.epochMs - onset.epochMs, 0)},
      assetRmsDbfs: post.asset ? db(post.asset.rms) : null,
      click: click ? {trusted: click.trusted, target: click.target} : null,
      latencyMs: onset && {
        clickToOnset: round(onset.epochMs - clickEpoch),
        clickToStartCall: start ? round(start.callEpochMs - clickEpoch) : null,
        startCallToOnset: start ? round(onset.epochMs - start.callEpochMs) : null,
        // Pure output pipeline: scheduled AudioBufferSourceNode start -> first sample on the sink monitor.
        scheduledStartToOnset: scheduledStartMs == null ? null : round(onset.epochMs - scheduledStartMs),
        scheduledOffsetMs: start ? round(Math.max(0, start.when - start.contextTime) * 1000) : null,
        clickNodeVsPageDeltaMs: click ? round(clickEpoch - clickNode) : null},
    });
    const lat = result.latencyMs;
    const c = result.checks;
    c.runtimeLockedBeforeGesture = pre.ready === false && (pre.context === 'none' || pre.context === 'suspended') && pre.controls === 1;
    c.gesturelessAudioContextSuspended = policy.state === 'suspended';
    c.silentBeforeGesture = preStats.peak < threshold;
    c.realTrustedClick = !!click && click.trusted === true;
    c.audioRunningAfterGesture = post.ready === true && post.context === 'running';
    c.streamOnTestSink = Array.isArray(result.sinkInputs) && result.sinkInputs.some(i => i.onTestSink);
    c.audibleAfterGesture = !!audible && audible.peak > threshold * 3 && audible.rmsDbfs > -60;
    c.audibleLevelPlausible = !!audible && post.asset != null && Math.abs(audible.rmsDbfs - result.assetRmsDbfs) < 3;
    c.latencyPlausible = !!lat && lat.clickToOnset > 0 && lat.clickToOnset < 3000 && (lat.scheduledStartToOnset == null || (lat.scheduledStartToOnset > -20 && lat.scheduledStartToOnset < 500));
    c.noRuntimeDiagnostics = post.diagnostics.length === 0;
    result.status = Object.values(c).every(Boolean) ? 'pass' : 'fail';
    result.failedChecks = Object.entries(c).filter(([, v]) => !v).map(([k]) => k);
    result.recorderTimingNote = 'onset time = earliest-delivery capture clock; uncertainty about +-10 ms';
    return result;
  } catch (error) {
    try { await recorder.stop(); } catch (_) { /* recorder may not have started */ }
    result.status = 'fail'; result.error = String(error.stack || error).split('\n').slice(0, 6).join('\n');
    return result;
  } finally { await browser.close().catch(() => {}); }
}

// ---- native ffplay leg ---------------------------------------------------------------------------
async function ffplayRun(wavFile, threshold, sinkIndex) {
  const recorder = new Recorder();
  await recorder.start();
  await lib.sleep(500);
  const tSpawn = epochNow();
  // Same arguments as crates/splinely_editor/src/audio_preview.rs (Ffplay::command).
  const child = spawn('ffplay', ['-hide_banner', '-loglevel', 'error', '-nodisp', '-autoexit', '-vn', '-sn', '-max_alloc', '33554432',
    '-threads', '1', '-filter_threads', '1', '-protocol_whitelist', 'file', '-f', 'wav', '-ss', '0.000000000', '-t', '0.400000000',
    '-volume', '100', '-i', wavFile], {env: {...process.env, PULSE_SINK: SINK, SDL_AUDIODRIVER: 'pulseaudio'}, stdio: ['ignore', 'ignore', 'pipe']});
  let stderr = ''; child.stderr.on('data', d => { stderr += d; });
  let evidence = null;
  setTimeout(() => { evidence = sinkInputEvidence(sinkIndex); }, 500);
  const exit = await new Promise(resolve => { const t = setTimeout(() => { child.kill('SIGKILL'); resolve('timeout'); }, 8000); child.once('close', code => { clearTimeout(t); resolve(code); }); });
  await lib.sleep(200);
  await recorder.stop();
  const onset = recorder.firstAbove(threshold, tSpawn - 20), end = recorder.lastAbove(threshold);
  const audible = onset && recorder.stats(onset.epochMs, end.epochMs);
  return {exit, stderr: stderr.trim().slice(0, 300), spawnToOnsetMs: onset ? round(onset.epochMs - tSpawn) : null,
    audible: audible && {rmsDbfs: audible.rmsDbfs, peakDbfs: audible.peakDbfs, ms: round(onset && end.epochMs - onset.epochMs, 0)},
    preSpawnPeak: round(recorder.stats(recorder.timeOfFrame(0), tSpawn - 20).peak, 6), sinkInputs: evidence};
}

// ---- main ----------------------------------------------------------------------------------------
async function main() {
  const args = parseArgs(process.argv.slice(2));
  const report = {schema: 'splinely-audio-latency/1', startedAt: new Date().toISOString(), browser: args.browser, svg: args.svg,
    thresholdFullScale: args.threshold, host: {platform: os.platform(), release: os.release()}, status: 'skip', reason: '', runs: []};
  const finish = code => {
    const text = JSON.stringify(report, null, 2) + '\n';
    if (args.out) { fs.mkdirSync(path.dirname(path.resolve(args.out)), {recursive: true}); fs.writeFileSync(args.out, text); }
    console.log(`AUDIO ${report.status.toUpperCase()}${report.reason ? ': ' + report.reason : ''}`);
    if (report.latencyMs) console.log('latency (ms):', JSON.stringify(report.latencyMs));
    process.exitCode = code;
  };
  let moduleId = null;
  const cleanup = () => { if (moduleId) { pactl(['unload-module', moduleId]); moduleId = null; } };
  process.on('SIGINT', () => { cleanup(); process.exit(130); });
  const info = pactl(['info']);
  if (!info.ok) { report.reason = 'SKIPPED, NOT PASSED: no PulseAudio/PipeWire server reachable (pactl info failed: ' + info.err.slice(0, 160) + ')'; return finish(3); }
  report.audioServer = (info.out.match(/Server Name: (.*)/) || [])[1];
  if (spawnSync('parec', ['--version']).status !== 0) { report.reason = 'SKIPPED, NOT PASSED: parec not installed'; return finish(3); }
  const loaded = pactl(['load-module', 'module-null-sink', `sink_name=${SINK}`, 'sink_properties=device.description=SplinelyAudioTest', `rate=${RATE}`, 'channels=2']);
  if (!loaded.ok) { report.reason = 'SKIPPED, NOT PASSED: cannot create null sink: ' + loaded.err.slice(0, 160); return finish(3); }
  moduleId = loaded.out;
  const sinks = pactl(['--format=json', 'list', 'sinks']);
  const sinkIndex = JSON.parse(sinks.out).find(s => s.name === SINK)?.index;
  report.nullSink = {name: SINK, module: moduleId, index: sinkIndex};
  const {pw, error} = lib.loadPlaywright();
  if (!pw) { report.reason = 'SKIPPED, NOT PASSED: ' + error; cleanup(); return finish(3); }
  const server = await lib.startServer(null);
  try {
    for (let run = 1; run <= args.runs; run++) {
      let r;
      try { r = await browserRun(pw, args, server.origin, sinkIndex, run); }
      catch (e) {
        if (e.skip) { report.reason = `SKIPPED, NOT PASSED: ${args.browser} cannot start: ${e.message}`; report.runs = []; return finish(3); }
        throw e;
      }
      report.runs.push(r);
    }
    const ok = report.runs.filter(r => r.status === 'pass');
    report.status = ok.length === report.runs.length ? 'pass' : 'fail';
    report.reason = report.status === 'pass' ? '' : `${report.runs.length - ok.length}/${report.runs.length} runs failed: ` + [...new Set(report.runs.flatMap(r => r.failedChecks || [r.error]))].join(', ');
    const pick = k => report.runs.map(r => r.latencyMs?.[k]);
    report.latencyMs = {clickToOnset: summary(pick('clickToOnset')), clickToStartCall: summary(pick('clickToStartCall')), startCallToOnset: summary(pick('startCallToOnset')),
      scheduledStartToOnset: summary(pick('scheduledStartToOnset')), scheduledOffset: summary(pick('scheduledOffsetMs'))};
    report.contextLatencyMs = {base: summary(report.runs.map(r => r.baseLatencyMs)), output: summary(report.runs.map(r => r.outputLatencyMs))};
    report.audible = {rmsDbfs: summary(report.runs.map(r => r.audible?.rmsDbfs)), peakDbfs: summary(report.runs.map(r => r.audible?.peakDbfs)),
      preGesturePeak: Math.max(...report.runs.map(r => r.preGesture?.peak ?? 0))};
    if (args.ffplay && args.browser === 'chromium') {
      if (spawnSync('ffplay', ['-version']).status !== 0) report.ffplay = {status: 'skip', reason: 'SKIPPED, NOT PASSED: ffplay not installed'};
      else {
        const project = JSON.parse(fs.readFileSync(path.join(lib.ROOT, 'examples/anime-hello.splinely.json'), 'utf8'));
        const asset = project.audioAssets[0];
        const dir = lib.scratchDir('audio-');
        const wav = path.join(dir, 'cue.wav');
        fs.writeFileSync(wav, Buffer.from(asset.dataBase64, 'base64'));
        const runs = [];
        for (let i = 0; i < args.runs; i++) runs.push(await ffplayRun(wav, args.threshold, sinkIndex));
        fs.rmSync(dir, {recursive: true, force: true});
        const good = runs.every(r => r.exit === 0 && r.spawnToOnsetMs != null && r.preSpawnPeak < args.threshold);
        report.ffplay = {status: good ? 'pass' : 'fail', reason: good ? '' : 'ffplay produced no audible sound on the null sink or exited nonzero', runs,
          spawnToOnsetMs: summary(runs.map(r => r.spawnToOnsetMs)),
          note: 'spawn -> first sample on null-sink monitor; includes process start, WAV probe/decode and SDL/Pulse open. Editor preview is frame-driven, not sample accurate.'};
        if (!good) { report.status = 'fail'; report.reason = (report.reason ? report.reason + '; ' : '') + 'ffplay leg failed'; }
      }
    }
  } finally { await server.close(); cleanup(); }
  finish(report.status === 'pass' ? 0 : 1);
}

main().catch(error => { console.error(error); process.exitCode = 1; });
