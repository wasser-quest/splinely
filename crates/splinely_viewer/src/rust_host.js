// Page glue for `splinely-viewer --runtime rust`: applies frames pushed by the native
// host and forwards pointer/keyboard input back over the narrow IPC channel.
//
// Page -> host messages (one JSON object per `post`, validated natively):
//   {"t":"ready"}  {"t":"ack"}  {"t":"error","message":"..."}
//   {"t":"input","pointer":{"x":n,"y":n}?,"leave":true?,"events":[{"name":"click","target":"id"?}]}
// Input is batched once per animation frame. pointermove/drag/wheel are coalesced.
(function (root) {
  'use strict';
  const EVENT_TYPES = ['click', 'dblclick', 'pointerenter', 'pointerleave', 'pointerdown', 'pointermove',
    'pointerup', 'pointercancel', 'lostpointercapture', 'wheel', 'keydown', 'keyup', 'focus', 'blur'];
  const COALESCED = new Set(['pointermove', 'drag', 'wheel']);
  const MAX_EVENTS = 64;
  const MAX_DIAGNOSTICS = 5;

  /**
   * env: {window, document, svg, applier, post(string), config:{dragAll, dragTargets}}
   */
  function createRustHost(env) {
    const { window: win, document: doc, svg, applier, post } = env;
    const config = env.config || {};
    const dragTargets = new Set(config.dragTargets || []);
    const groupOrNode = new Set();
    for (const el of svg.querySelectorAll('[id]')) {
      if (el.getAttribute('data-splinely-node') != null || el.getAttribute('data-splinely-group') != null) groupOrNode.add(el.id);
    }
    let pendingNodes = new Map();
    let pending = { pointer: undefined, leave: false, events: [] };
    let scheduled = false;
    let ackDue = false;
    const starts = new Map();
    const raf = win.requestAnimationFrame ? cb => win.requestAnimationFrame(cb) : cb => setTimeout(cb, 16);
    const send = message => { try { post(JSON.stringify(message)); } catch (_) { /* no host attached */ } };

    function diagnostic(text) {
      let box = doc.getElementById && doc.getElementById('splinely-diagnostics');
      if (!box || !box.appendChild) return;
      const line = doc.createElement('div');
      line.textContent = String(text).slice(0, 300);
      box.appendChild(line);
      while (box.children && box.children.length > MAX_DIAGNOSTICS) box.removeChild(box.children[0]);
      box.hidden = false;
    }
    function fail(text) {
      diagnostic(text);
      send({ t: 'error', message: String(text).slice(0, 400) });
    }
    function onFrame(message) {
      if (!message || !Array.isArray(message.nodes)) return;
      for (const n of message.nodes) pendingNodes.set(n.id, n);
      ackDue = true;
      schedule();
    }
    function queue(name, target) {
      if (COALESCED.has(name)) {
        const i = pending.events.findIndex(e => e.name === name && e.target === target);
        if (i >= 0) pending.events.splice(i, 1);
      }
      if (pending.events.length >= MAX_EVENTS) return;
      pending.events.push(target ? { name, target } : { name });
      schedule();
    }
    function schedule() {
      if (scheduled) return;
      scheduled = true;
      raf(step);
    }
    function step() {
      scheduled = false;
      if (pendingNodes.size) {
        const nodes = [...pendingNodes.values()];
        pendingNodes = new Map();
        try {
          const result = applier.apply({ v: 1, nodes });
          if (result.errors.length) fail('Frame applier: ' + result.errors[0]);
        } catch (error) { fail('Frame applier: ' + error.message); }
      }
      if (ackDue) { ackDue = false; send({ t: 'ack' }); }
      const p = pending;
      if (p.events.length || p.pointer !== undefined || p.leave) {
        pending = { pointer: undefined, leave: false, events: [] };
        const message = { t: 'input', events: p.events };
        if (p.leave) message.leave = true;
        else if (p.pointer) message.pointer = p.pointer;
        send(message);
      }
    }
    function targetOf(el) {
      while (el && el !== svg) {
        if (el.id && groupOrNode.has(el.id)) return { id: el.id, el };
        el = el.parentNode;
      }
      return { id: '', el: null };
    }
    function coordinates(event) {
      let x = event.clientX || 0, y = event.clientY || 0;
      try {
        const matrix = svg.getScreenCTM();
        if (matrix) {
          const p = svg.createSVGPoint();
          p.x = x; p.y = y;
          const q = p.matrixTransform(matrix.inverse());
          x = q.x; y = q.y;
        }
      } catch (error) { diagnostic('Pointer mapping failed: ' + error.message); }
      return { x, y };
    }
    function listener(type, event) {
      const hit = targetOf(event.target);
      const target = hit.id;
      const point = coordinates(event);
      if (type === 'pointermove' || type === 'pointerdown' || type === 'pointerenter') {
        if (Number.isFinite(point.x) && Number.isFinite(point.y)) { pending.pointer = point; pending.leave = false; }
      }
      if (type === 'pointerleave' && event.target === svg) { pending.pointer = undefined; pending.leave = true; }
      queue(type, target);
      const id = event.pointerId;
      if (type === 'pointerdown') {
        starts.set(id, { target, el: hit.el });
        try {
          if (hit.el && hit.el.setPointerCapture && (config.dragAll || dragTargets.has(target))) hit.el.setPointerCapture(id);
        } catch (_) { /* capture is best effort */ }
        queue('dragstart', target);
      }
      if (type === 'pointermove' && starts.has(id)) queue('drag', starts.get(id).target);
      if ((type === 'pointerup' || type === 'pointercancel' || type === 'lostpointercapture') && starts.has(id)) {
        const start = starts.get(id);
        starts.delete(id);
        try { if (start.el && start.el.releasePointerCapture) start.el.releasePointerCapture(id); } catch (_) { /* ignore */ }
        queue('dragend', start.target);
      }
    }
    const cleanups = [];
    function listen(owner, type) {
      const fn = event => listener(type, event);
      owner.addEventListener(type, fn, true);
      cleanups.push(() => owner.removeEventListener(type, fn, true));
    }
    return {
      /** Apply the first (full) frame, attach input listeners, tell the host to start ticking. */
      start(initial) {
        if (initial) {
          const result = applier.apply(initial);
          if (result.errors.length) fail('Frame applier: ' + result.errors[0]);
        }
        for (const type of EVENT_TYPES) {
          if (type === 'keydown' || type === 'keyup') listen(doc, type);
          else if (type === 'focus' || type === 'blur') listen(win, type);
          else listen(svg, type);
        }
        send({ t: 'ready' });
      },
      onFrame,
      diagnostic,
      destroy() { for (const fn of cleanups.splice(0)) fn(); },
    };
  }

  const api = { createRustHost, EVENT_TYPES };
  root.SplinelyRustHost = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
