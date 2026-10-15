// Dependency-free frame applier for `splinely-viewer --runtime rust`.
//
// The Rust runtime (crates/splinely_runtime) evaluates the project; this file only
// writes the evaluated nodes into the static SVG produced by
// `Project::to_static_svg`. Attribute names, number formats and colour rounding
// deliberately mirror renderFrame()/applyGradient() in runtime/src/splinely.js, so a
// Rust-driven page shows the same attributes as the JS runtime would write.
//
// Message (version 1), built by crates/splinely_viewer/src/rust_mode.rs:
//   {v:1, nodes:[{id, tf:[x,y,rotation,scaleX,scaleY], op, sw, fill:[r,g,b,a],
//                 stroke:[r,g,b,a]|null, grad?:{radial,angle,stops:[[offset,r,g,b,a]]},
//                 geo?:{width,height,r,fs,d}}]}
// Nodes not listed are unchanged. Nodes without an element (hidden at export) are skipped.
(function (root) {
  'use strict';
  const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
  const finite = (v, label) => {
    if (typeof v !== 'number' || !Number.isFinite(v)) throw new Error(label + ' must be finite');
    return v;
  };
  const channel = v => Math.round(clamp(finite(v, 'Colour channel'), 0, 1) * 255);
  const rgb = c => {
    if (!Array.isArray(c) || c.length !== 4) throw new Error('Colour must be [r,g,b,a]');
    return `rgb(${channel(c[0])},${channel(c[1])},${channel(c[2])})`;
  };
  const alpha = c => clamp(finite(c[3], 'Colour alpha'), 0, 1);
  // Same guard as the JS runtime: data must never turn into URLs or script.
  const safeText = v => {
    if (typeof v !== 'string' || /url\s*\(/i.test(v) || v.includes('\\')) throw new Error('URL properties are unsupported');
    return v;
  };

  function createFrameApplier(svg) {
    if (!svg || typeof svg.querySelectorAll !== 'function') throw new Error('An SVG root is required');
    const byId = new Map();
    const nodes = new Map();
    const written = new WeakMap();
    for (const el of svg.querySelectorAll('[id]')) {
      if (byId.has(el.id)) continue;
      byId.set(el.id, el);
      if (el.getAttribute('data-splinely-node') === el.id) {
        nodes.set(el.id, { el, original: new Map(), written: new Map(), lastKeys: new Set() });
      }
    }
    // Gradient elements are shared writes; skip unchanged values like the JS runtime.
    function put(el, name, value) {
      let w = written.get(el);
      if (!w) { w = new Map(); written.set(el, w); }
      const text = String(value);
      if (w.get(name) === text) return;
      el.setAttribute(name, value);
      w.set(name, text);
    }
    function applyGradient(st, g) {
      if (!g || !Array.isArray(g.stops)) throw new Error('Gradient must have stops');
      const ref = /url\(#([^)]+)\)/.exec(st.el.getAttribute('fill') || '');
      const el = ref && byId.get(ref[1]);
      if (!el) throw new Error('Gradient element missing for ' + st.el.id);
      if (!g.radial) {
        const a = finite(Number(g.angle) || 0, 'Gradient angle') * Math.PI / 180;
        const r = v => Math.round(v * 1e6) / 1e6 + 0;
        put(el, 'x1', r(0.5 - Math.cos(a) / 2)); put(el, 'y1', r(0.5 - Math.sin(a) / 2));
        put(el, 'x2', r(0.5 + Math.cos(a) / 2)); put(el, 'y2', r(0.5 + Math.sin(a) / 2));
      }
      const stops = [...(el.children || [])].filter(c => (c.tagName || '').toLowerCase() === 'stop');
      stops.forEach((stop, i) => {
        const s = g.stops[i];
        if (!s) return;
        const c = [s[1], s[2], s[3], s[4]];
        put(stop, 'offset', clamp(Number(s[0]) || 0, 0, 1));
        put(stop, 'stop-color', rgb(c));
        put(stop, 'stop-opacity', clamp(finite(c[3], 'Colour alpha'), 0, 1));
      });
    }
    function applyNode(st, n) {
      const keys = new Set();
      const attr = (name, value) => {
        if (!st.original.has(name)) st.original.set(name, st.el.getAttribute(name));
        const text = value == null ? null : String(value);
        if (st.written.has(name) && st.written.get(name) === text) return;
        if (text === null) st.el.removeAttribute(name); else st.el.setAttribute(name, text);
        st.written.set(name, text);
      };
      const set = (name, value) => { attr(name, value); keys.add(name); };
      if (!Array.isArray(n.tf) || n.tf.length !== 5) throw new Error('Transform must be [x,y,rotation,scaleX,scaleY]');
      const [x, y, r, sx, sy] = n.tf.map(v => finite(v, 'Transform'));
      set('transform', `translate(${x} ${y}) rotate(${r}) scale(${sx} ${sy})`);
      set('opacity', finite(n.op, 'Opacity'));
      if (n.grad) {
        applyGradient(st, n.grad); // a gradient fill replaces fill/fill-opacity
      } else {
        set('fill', rgb(n.fill)); set('fill-opacity', alpha(n.fill));
      }
      if (n.stroke) { set('stroke', rgb(n.stroke)); set('stroke-opacity', alpha(n.stroke)); }
      else set('stroke', 'none');
      set('stroke-width', finite(n.sw, 'Stroke width'));
      const geo = n.geo || {};
      for (const [key, name] of [['width', 'width'], ['height', 'height'], ['r', 'r'], ['fs', 'font-size']]) {
        if (geo[key] !== undefined) set(name, finite(geo[key], name));
      }
      if (geo.d !== undefined) set('d', safeText(geo.d));
      // Attributes a previous frame wrote that this frame no longer sets go back to export values.
      for (const key of st.lastKeys) if (!keys.has(key)) attr(key, st.original.get(key));
      st.lastKeys = keys;
    }
    return {
      ids() { return [...byId.keys()]; },
      has(id) { return nodes.has(id); },
      /** Apply one message; returns `{applied, errors}` and never throws for a bad node. */
      apply(message) {
        if (!message || message.v !== 1 || !Array.isArray(message.nodes)) throw new Error('Unsupported frame message');
        const errors = [];
        let applied = 0;
        for (const n of message.nodes) {
          const st = n && nodes.get(n.id);
          if (!st) continue;
          try { applyNode(st, n); applied++; } catch (error) { errors.push(`${n.id}: ${error.message}`); }
        }
        return { applied, errors };
      },
    };
  }

  const api = { createFrameApplier };
  root.SplinelyFrameApplier = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
