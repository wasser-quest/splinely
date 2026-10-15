// hover-wiggle runtime part (Splinely runtime plugin API v1).
//
// File format: the BODY of a function that returns the plugin object. The trusted export
// loader compiles it and calls `splinely.use(plugin)`. It runs ONLY when the SVG was exported
// with `splinely-cli export ... --plugin <this folder> --trusted-js`. No project data can
// load or enable it. See docs/plugins.md.
//
// The editor part (splinely-plugin.json tools) merges clips/state machines plus the variables
// `wiggleCount` (number) and `wiggleLast` (string); this part reacts to the same pointer events.
let declared = false;
let count = 0;
return {
  id: 'hover-wiggle',
  apiVersion: 1,
  events: ['pointerenter', 'click'],
  setup(ctx) {
    try {
      ctx.getVariable('wiggleCount');
      ctx.getVariable('wiggleLast');
      declared = true;
    } catch (_) {
      // The behavior tool was never applied to this project: stay passive but report once.
      ctx.report('variables wiggleCount/wiggleLast are missing; apply a hover-wiggle tool first');
    }
  },
  onEvent(event, ctx) {
    if (!declared || !event.target || event.target === '') return;
    count += 1;
    ctx.setVariable('wiggleCount', count);
    ctx.setVariable('wiggleLast', event.type + ':' + event.target);
  },
};
