// Mounts a Splinely graphic inline in the page, like a Rive artboard: no iframe and no document of its
// own. The graphic is ordinary SVG in the host's DOM; the host decides where it goes and when it is removed.
//
// Load splinely.js first (it sets window.SplinelyRuntime). This file adds one function:
//   const graphic = SplinelyMount.mount(container, svgText, { autoTick: true });
//   graphic.runtime.on('clip.end', ...);   // the same API as SplinelyRuntime.createRuntime
//   graphic.destroy();                     // stops the graphic and removes it from the page
//
// The SVG is sanitized before it reaches the page: only the elements and attributes the Splinely exporter
// writes are kept (scripts, styles, foreignObject, links, images, animation elements and event-handler
// attributes go), references must stay inside the document ('#id'), and url() may only name '#id'. The host
// runs the graphic; the file's own runtime copy is not used.
// A graphic that cannot start stays visible as its static drawing; `error` then says why.
(function (host) {
  'use strict';

  const SplinelyRuntime = host.SplinelyRuntime;
  if (!SplinelyRuntime) throw new Error('splinely-mount.js needs splinely.js loaded first');

  const SVG_NS = 'http://www.w3.org/2000/svg';
  const META_NS = 'urn:splinely:metadata';
  // What the exporter writes (svg.rs, gradients, groups, a11y), plus inert shape and paint-server elements.
  const ELEMENTS = new Set(('svg g defs title desc metadata path rect circle ellipse line polyline polygon text tspan ' +
    'linearGradient radialGradient stop clipPath mask symbol use').split(' '));
  const ATTRIBUTES = new Set(('id version viewBox preserveAspectRatio width height x y x1 y1 x2 y2 cx cy r rx ry fx fy ' +
    'd points pathLength transform gradientTransform gradientUnits spreadMethod offset stop-color stop-opacity ' +
    'fill fill-opacity fill-rule stroke stroke-opacity stroke-width stroke-linecap stroke-linejoin stroke-miterlimit ' +
    'stroke-dasharray stroke-dashoffset opacity visibility display clip-path clip-rule clipPathUnits mask maskUnits ' +
    'maskContentUnits paint-order vector-effect font-size font-family font-weight font-style text-anchor ' +
    'dominant-baseline xml:space role tabindex focusable name schemaVersion href xlink:href').split(' '));
  // Attributes whose value is CSS and may carry url(): only same-document 'url(#id)' references survive.
  const PAINT = new Set(['fill', 'stroke', 'clip-path', 'mask']);
  const LOCAL_URL = /url\(\s*(['"]?)#[A-Za-z0-9_.:-]+\1\s*\)/gi;

  function allowedAttribute(el, name, value) {
    if (/^on/i.test(name) || name === 'style' || name === 'class') return false;
    if (name === 'href' || name === 'xlink:href') return /^#[^\s'"()\\]*$/.test(value);
    if (name === 'xmlns' || name.startsWith('xmlns:') || name.startsWith('data-') || name.startsWith('aria-')) return true;
    if (!ATTRIBUTES.has(name)) return false;
    if (PAINT.has(name) && (value.includes('\\') || /url\(/i.test(value.replace(LOCAL_URL, '')))) return false;
    return true;
  }
  /** Removes everything the exporter does not write from `root` (in place) and returns it. */
  function sanitize(root) {
    const walk = (el, inMetadata) => {
      for (const attr of [...el.attributes]) if (!allowedAttribute(el, attr.name, attr.value)) el.removeAttribute(attr.name);
      for (let child = el.firstChild; child; ) {
        const next = child.nextSibling;
        if (child.nodeType === 1) {
          const ns = child.namespaceURI;
          // <metadata> holds the project text and the generator element of the Splinely namespace.
          if (ns === SVG_NS ? ELEMENTS.has(child.localName) : inMetadata && ns === META_NS) walk(child, inMetadata || child.localName === 'metadata');
          else el.removeChild(child);
        } else if (child.nodeType !== 3 && child.nodeType !== 4 && child.nodeType !== 8) el.removeChild(child);
        child = next;
      }
    };
    walk(root, false);
    return root;
  }

  /** Parses exported SVG text into its sanitized root <svg>. */
  function parse(source) {
    const doc = new host.DOMParser().parseFromString(source, 'image/svg+xml');
    const root = doc.documentElement;
    if (!root || root.localName !== 'svg' || root.namespaceURI !== SVG_NS || doc.getElementsByTagName('parsererror').length) {
      throw new Error('Not an SVG graphic');
    }
    return sanitize(root);
  }

  /**
   * Shows the graphic inside `container`, replacing what it held. `source` is the SVG text of an
   * exported graphic, or an SVG element. `options` are passed to createRuntime.
   */
  function mount(container, source, options) {
    const root = typeof source === 'string' ? parse(source) : source;
    const svg = host.document.importNode(root, true);
    if (typeof source !== 'string') sanitize(svg);
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

  const exported = { mount, sanitize };
  if (typeof module === 'object' && module.exports) module.exports = exported;
  else host.SplinelyMount = exported;
})(typeof window !== 'undefined' ? window : globalThis);
