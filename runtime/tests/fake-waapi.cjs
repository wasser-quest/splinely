'use strict';
// Minimal browser stand-in for runtime tests: a manual clock, requestAnimationFrame/setTimeout queues,
// IntersectionObserver/visibility hooks and a small Web Animations implementation (element.animate with
// keyframes, offsets, linear/steps/cubic-bezier easing, iterations, fill and add/replace composite) that
// can evaluate what an element shows: its transform attribute combined with CSS translate/rotate/scale.
const fs = require('node:fs');

function bezier(x1, y1, x2, y2) {
  const curve = (s, a, b) => 3 * (1 - s) * (1 - s) * s * a + 3 * (1 - s) * s * s * b + s * s * s;
  return t => {
    if (t <= 0 || t >= 1) return t;
    let lo = 0, hi = 1;
    for (let i = 0; i < 60; i++) { const mid = (lo + hi) / 2; if (curve(mid, x1, x2) < t) lo = mid; else hi = mid; }
    return curve((lo + hi) / 2, y1, y2);
  };
}
function easingFn(spec) {
  if (!spec || spec === 'linear') return t => t;
  if (spec === 'steps(1, end)') return t => (t < 1 ? 0 : 1);
  const m = /^cubic-bezier\(([^)]*)\)$/.exec(spec);
  if (m) return bezier(...m[1].split(',').map(Number));
  throw new Error('fake WAAPI: unsupported easing ' + spec);
}
const nums = s => String(s).trim().split(/\s+/).map(v => parseFloat(v));

class FakeAnimation {
  constructor(env, el, frames, opts) {
    this.env = env; this.el = el; this.frames = frames.map(f => ({ ...f })); this.opts = { ...opts };
    this.prop = Object.keys(frames[0]).find(k => k !== 'offset' && k !== 'easing' && k !== 'composite');
    this.startTime = env.document.timeline.currentTime; this.holdTime = null; this.cancelled = false;
    for (const f of this.frames) f.ease = easingFn(f.easing);
  }
  get playState() { return this.cancelled ? 'idle' : this.holdTime != null ? 'paused' : 'running'; }
  get currentTime() { return this.holdTime != null ? this.holdTime : this.env.document.timeline.currentTime - this.startTime; }
  set currentTime(v) { if (this.holdTime != null) this.holdTime = v; else this.startTime = this.env.document.timeline.currentTime - v; }
  pause() { if (this.holdTime == null) this.holdTime = this.currentTime; }
  play() { if (this.holdTime != null) { this.startTime = this.env.document.timeline.currentTime - this.holdTime; this.holdTime = null; } }
  cancel() { this.cancelled = true; const list = this.el.animations; list.splice(list.indexOf(this), 1); }
  progress() {
    const d = this.opts.duration, n = this.opts.iterations == null ? 1 : this.opts.iterations, t = this.currentTime;
    if (t < 0) return 0;
    if (n !== Infinity && t >= d * n) return n - Math.floor(n) || 1;
    return (t % d) / d;
  }
  value() {
    const p = this.progress(), fr = this.frames;
    let i = 0;
    while (i + 1 < fr.length && fr[i + 1].offset <= p) i++;
    if (i + 1 === fr.length || p < fr[0].offset) return this.prop === 'transform' ? parseList(fr[i].transform) : nums(fr[i][this.prop]);
    const a = fr[i], b = fr[i + 1], span = b.offset - a.offset, t = span > 0 ? a.ease((p - a.offset) / span) : 1;
    if (this.prop === 'transform') {
      const la = parseList(a.transform), lb = parseList(b.transform);
      if (la.length !== lb.length || la.some((f, k) => f.fn !== lb[k].fn)) throw new Error('fake WAAPI: mismatched transform lists');
      return la.map((f, k) => ({ fn: f.fn, args: f.args.map((v, j) => v + (lb[k].args[j] - v) * t) }));
    }
    const va = nums(a[this.prop]), vb = nums(b[this.prop]);
    return va.map((v, k) => v + (vb[k] - v) * t);
  }
}
// Hooks the animation property setter so `animation.startTime = x` works like the real API.
Object.defineProperty(FakeAnimation.prototype, 'startTime', {
  get() { return this._start; },
  set(v) { this._start = v; this.holdTime = null; },
});

// CSS transform list: [{fn, args}] (lengths in px, angles in deg).
function parseList(text) {
  return [...String(text || '').matchAll(/(\w+)\(([^)]*)\)/g)].map(f => ({ fn: f[1], args: f[2].split(/[\s,]+/).filter(Boolean).map(v => parseFloat(v)) }));
}
function listMatrix(list) {
  let m = [1, 0, 0, 1, 0, 0];
  for (const f of list) {
    const a = f.args;
    if (f.fn === 'translate') m = multiply(m, T(a[0], a[1] || 0));
    else if (f.fn === 'rotate') m = multiply(m, R(a[0]));
    else if (f.fn === 'scale') m = multiply(m, S(a[0], a.length > 1 ? a[1] : a[0]));
    else if (f.fn === 'matrix') m = multiply(m, a);
    else throw new Error('fake WAAPI: unsupported transform function ' + f.fn);
  }
  return m;
}
function multiply(a, b) {
  return [a[0] * b[0] + a[2] * b[1], a[1] * b[0] + a[3] * b[1], a[0] * b[2] + a[2] * b[3], a[1] * b[2] + a[3] * b[3], a[0] * b[4] + a[2] * b[5] + a[4], a[1] * b[4] + a[3] * b[5] + a[5]];
}
const T = (x, y) => [1, 0, 0, 1, x, y];
const R = deg => { const r = deg * Math.PI / 180; return [Math.cos(r), Math.sin(r), -Math.sin(r), Math.cos(r), 0, 0]; };
const S = (x, y) => [x, 0, 0, y, 0, 0];
function parseTransform(text) {
  let m = [1, 0, 0, 1, 0, 0];
  for (const f of String(text || '').matchAll(/(\w+)\(([^)]*)\)/g)) {
    const a = f[2].split(/[\s,]+/).filter(Boolean).map(Number);
    if (f[1] === 'translate') m = multiply(m, T(a[0], a[1] || 0));
    else if (f[1] === 'rotate') m = multiply(m, R(a[0]));
    else if (f[1] === 'scale') m = multiply(m, S(a[0], a.length > 1 ? a[1] : a[0]));
    else if (f[1] === 'matrix') m = multiply(m, a);
  }
  return m;
}

class Element {
  constructor(env, id, tag, attrs) {
    this.env = env; this.id = id; this.tagName = tag; this.attrs = new Map(Object.entries(attrs || {}));
    this.parentNode = null; this.children = []; this.animations = []; this.listeners = new Map();
    // Style property writes count as DOM writes too.
    const values = { translate: '', rotate: '', scale: '', transform: '' }, style = {};
    for (const k of Object.keys(values)) Object.defineProperty(style, k, { enumerable: true, get: () => values[k], set: v => { this.env.writes++; values[k] = String(v); } });
    this.style = style;
  }
  getAttribute(k) { return this.attrs.has(k) ? this.attrs.get(k) : null; }
  setAttribute(k, v) { this.env.writes++; this.attrs.set(k, String(v)); }
  removeAttribute(k) { this.env.writes++; this.attrs.delete(k); }
  addEventListener(k, fn) { this.listeners.set(k, fn); }
  removeEventListener() {}
  fire(type, event) { const fn = this.listeners.get(type); if (fn) fn({ target: this, type, pointerId: 1, ...event }); }
  setPointerCapture() {} releasePointerCapture() {}
  animate(frames, opts) {
    if (!this.env.waapi) throw new Error('animate disabled');
    this.env.animateCalls++;
    const a = new FakeAnimation(this.env, this, frames, opts);
    a.startTime = this.env.document.timeline.currentTime;
    this.animations.push(a); return a;
  }
  getAnimations() { return this.animations.slice(); }
  // What the element shows: [a,b,c,d,e,f] of translate·rotate·scale·transform-attribute, and opacity.
  shown() {
    let tr = this.style.translate ? nums(this.style.translate) : [0, 0];
    let rot = this.style.rotate ? nums(this.style.rotate)[0] : 0;
    let sc = this.style.scale ? nums(this.style.scale) : [1, 1];
    let opacity = this.attrs.has('opacity') ? Number(this.attrs.get('opacity')) : 1;
    if (tr.length < 2) tr = [tr[0], 0];
    if (sc.length < 2) sc = [sc[0], sc[0]];
    let list = this.style.transform ? parseList(this.style.transform) : null;
    for (const a of this.animations) {
      const v = a.value(), add = a.opts.composite === 'add';
      if (a.prop === 'transform') {
        // replace, add (append) and accumulate (pairwise for matching lists; scale accumulates s1 + s2 - 1).
        if (a.opts.composite === 'accumulate' && list) {
          if (list.length !== v.length || list.some((f, k) => f.fn !== v[k].fn)) throw new Error('fake WAAPI: accumulate needs matching lists');
          list = list.map((f, k) => ({ fn: f.fn, args: f.args.map((x, j) => f.fn === 'scale' ? x + v[k].args[j] - 1 : x + v[k].args[j]) }));
        } else if (add && list) list = list.concat(v);
        else list = v;
        continue;
      }
      if (a.prop === 'translate') tr = add ? [tr[0] + v[0], tr[1] + (v[1] || 0)] : [v[0], v[1] || 0];
      else if (a.prop === 'rotate') rot = add ? rot + v[0] : v[0];
      else if (a.prop === 'scale') sc = add ? [sc[0] * v[0], sc[1] * (v.length > 1 ? v[1] : v[0])] : [v[0], v.length > 1 ? v[1] : v[0]];
      else if (a.prop === 'opacity') opacity = add ? opacity + v[0] : v[0];
    }
    // The CSS transform property (style or animation) overrides the transform attribute.
    const own = list ? listMatrix(list) : parseTransform(this.attrs.get('transform'));
    const m = multiply(multiply(multiply(T(tr[0], tr[1]), R(rot)), S(sc[0], sc[1])), own);
    return { m, opacity: Math.max(0, Math.min(1, opacity)) };
  }
}

// Builds a fake window + parsed SVG. options: {waapi: true|false, io: true|false}
function createEnv(svgText, options = {}) {
  const env = {
    writes: 0, animateCalls: 0, frames: 0, waapi: options.waapi !== false, time: 1000,
    rafQueue: new Map(), rafId: 0, rafCalls: 0, timers: new Map(), timerId: 0, intervals: new Map(), observers: [],
  };
  env.performance = { now: () => env.time };
  // Like browsers, the document timeline's time is the time of the current (last started) animation frame.
  env.frameTime = env.time;
  env.document = { timeline: { get currentTime() { return env.frameTime; } }, visibilityState: 'visible', listeners: new Map(),
    addEventListener(k, fn) { if (!this.listeners.has(k)) this.listeners.set(k, new Set()); this.listeners.get(k).add(fn); },
    removeEventListener(k, fn) { const l = this.listeners.get(k); if (l) l.delete(fn); } };
  env.requestAnimationFrame = fn => { env.rafCalls++; const id = ++env.rafId; env.rafQueue.set(id, fn); return id; };
  env.cancelAnimationFrame = id => { env.rafQueue.delete(id); };
  env.setTimeout = (fn, ms) => { const id = ++env.timerId; env.timers.set(id, { fn, at: env.time + Math.max(0, ms || 0) }); return id; };
  env.clearTimeout = id => { env.timers.delete(id); };
  env.setInterval = (fn, ms) => { const id = ++env.timerId; env.intervals.set(id, { fn, ms, at: env.time + ms }); return id; };
  env.clearInterval = id => { env.intervals.delete(id); };
  if (options.waapi !== false) {
    env.CSS = { supports: () => true };
    env.KeyframeEffect = function KeyframeEffect() {}; env.KeyframeEffect.prototype.composite = 'replace';
  }
  if (options.io) {
    env.IntersectionObserver = class { constructor(cb) { this.cb = cb; env.observers.push(this); } observe(el) { this.el = el; } disconnect() { env.observers.splice(env.observers.indexOf(this), 1); } };
  }
  // Reports `visible` to the IntersectionObservers (of one svg only when `el` is given).
  env.setVisible = (visible, el) => { for (const o of [...env.observers]) if (!el || o.el === el) o.cb([{ isIntersecting: visible, target: o.el }]); };
  env.setDocumentHidden = hidden => { env.document.visibilityState = hidden ? 'hidden' : 'visible'; for (const fn of [...(env.document.listeners.get('visibilitychange') || [])]) fn(); };
  // Advances the clock to `to` (ms), running timers, intervals and one animation frame per 1000/60 ms.
  // Vsync: frame k at start + k·frameMs, optionally displaced by up to `jitter` ms (deterministic), like real
  // frame times that rarely land exactly on keyframe or loop boundaries.
  env.frameMs = 1000 / 60; env.frameIndex = 1; env.frameStart = env.time; const jitter = options.jitter || 0;
  const frameAt = k => env.frameStart + k * env.frameMs + jitter * Math.sin(k * 2.39996);
  env.nextFrame = frameAt(1);
  const passFrame = () => { env.frameTime = env.nextFrame; env.nextFrame = frameAt(++env.frameIndex); };
  const skipFrames = () => { while (env.nextFrame <= env.time + 1e-6) passFrame(); };
  env.advanceTo = to => {
    for (let guard = 0; ; guard++) {
      if (guard > 1e6) throw new Error('fake env: no progress at ' + env.time + ' (timers ' + [...env.timers.values()].map(t => t.at).join(',') + ')');
      let next = Infinity, kind = null, id = null;
      for (const [k, t] of env.timers) if (t.at < next) { next = t.at; kind = 'timer'; id = k; }
      for (const [k, t] of env.intervals) if (t.at < next) { next = t.at; kind = 'interval'; id = k; }
      if (env.rafQueue.size && env.nextFrame <= next) { next = env.nextFrame; kind = 'raf'; }
      if (next > to + 1e-6) break;
      env.time = Math.max(env.time, next);
      if (kind === 'timer') { const t = env.timers.get(id); env.timers.delete(id); t.fn(); }
      else if (kind === 'interval') { const t = env.intervals.get(id); t.at += t.ms; t.fn(); }
      else { const q = [...env.rafQueue.values()]; env.rafQueue.clear(); env.frames = (env.frames || 0) + 1; for (const fn of q) fn(env.time); }
      if (kind === 'raf') skipFrames();
      else while (env.nextFrame < env.time) passFrame();
    }
    env.time = Math.max(env.time, to);
    skipFrames();
  };
  // Parses another figure into the same window (several runtimes on one page).
  env.addSvg = text => {
    const other = parseSvg(env, text);
    other.ownerDocument = svg.ownerDocument;
    return other;
  };
  const svg = parseSvg(env, svgText);
  svg.ownerDocument = { defaultView: env, ...env.document, addEventListener: (k, fn) => env.document.addEventListener(k, fn), removeEventListener: (k, fn) => env.document.removeEventListener(k, fn) };
  Object.defineProperty(svg.ownerDocument, 'visibilityState', { get: () => env.document.visibilityState });
  return { env, svg };
}

function parseSvg(env, text) {
  text = text.replace(/<!--[\s\S]*?-->/g, '').replace(/<metadata[\s\S]*?<\/metadata>/g, '').replace(/<script[\s\S]*?<\/script>/g, '')
    .replace(/<title[\s\S]*?<\/title>/g, '').replace(/<desc[\s\S]*?<\/desc>/g, '');
  const svg = new Element(env, 'root', 'svg'), all = [], stack = [svg];
  let first = true;
  for (const m of text.matchAll(/<(\/?)([A-Za-z][\w:-]*)([^>]*?)(\/?)>/g)) {
    if (m[1]) { if (stack.length > 1) stack.pop(); continue; }
    const attrs = {};
    for (const a of m[3].matchAll(/([\w:-]+)="([^"]*)"/g)) attrs[a[1]] = a[2];
    if (first) { first = false; for (const [k, v] of Object.entries(attrs)) svg.attrs.set(k, v); continue; }
    const el = new Element(env, attrs.id, m[2], attrs), parent = stack[stack.length - 1];
    el.parentNode = parent; el.ownerSVGElement = svg; parent.children.push(el); all.push(el);
    if (!m[4]) stack.push(el);
  }
  svg.querySelectorAll = sel => (sel === '[id]' ? all.filter(e => e.id) : []);
  svg.getElementsByTagName = () => [];
  svg.querySelector = () => null;
  svg.getScreenCTM = () => null;
  svg.all = all;
  return svg;
}

const readFile = f => fs.readFileSync(f, 'utf8');
module.exports = { createEnv, parseTransform, multiply, readFile };
