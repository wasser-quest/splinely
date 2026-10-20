// Mounts a Splinely graphic inline in the page, like a Rive artboard: no iframe and no document of its
// own. The graphic is ordinary SVG in the host's DOM; the host decides where it goes and when it is removed.
//
// Load splinely.js first (it sets window.SplinelyRuntime). This file adds one function:
//   const graphic = SplinelyMount.mount(container, svgText, { autoTick: true });
//   graphic.runtime.on('clip.end', ...);   // the same API as SplinelyRuntime.createRuntime
//   graphic.destroy();                     // stops the graphic and removes it from the page
//
// Scripts in the source are removed: the host runs the graphic, the file's own runtime copy is not used.
// A graphic that cannot start stays visible as its static drawing; `error` then says why.
(function (host) {
  'use strict';

  const SplinelyRuntime = host.SplinelyRuntime;
  if (!SplinelyRuntime) throw new Error('splinely-mount.js needs splinely.js loaded first');

  /** Parses exported SVG text into its root <svg>, without the scripts. */
  function parse(source) {
    const doc = new host.DOMParser().parseFromString(source, 'image/svg+xml');
    const root = doc.documentElement;
    if (!root || root.localName !== 'svg' || doc.getElementsByTagName('parsererror').length) {
      throw new Error('Not an SVG graphic');
    }
    for (const script of [...root.getElementsByTagName('script')]) script.remove();
    return root;
  }

  /**
   * Shows the graphic inside `container`, replacing what it held. `source` is the SVG text of an
   * exported graphic, or an SVG element. `options` are passed to createRuntime.
   */
  function mount(container, source, options) {
    const root = typeof source === 'string' ? parse(source) : source;
    const svg = host.document.importNode(root, true);
    if (svg.localName === 'svg' && svg.getElementsByTagName('script').length) {
      for (const script of [...svg.getElementsByTagName('script')]) script.remove();
    }
    container.replaceChildren(svg);
    try {
      const runtime = SplinelyRuntime.createRuntime(svg, undefined, options);
      return {
        svg,
        runtime,
        error: null,
        destroy() {
          runtime.destroy();
          svg.remove();
        },
      };
    } catch (error) {
      // The static drawing stays in the container; the host can show the error or ignore it.
      return { svg, runtime: null, error, destroy() { svg.remove(); } };
    }
  }

  const exported = { mount };
  if (typeof module === 'object' && module.exports) module.exports = exported;
  else host.SplinelyMount = exported;
})(typeof window !== 'undefined' ? window : globalThis);
