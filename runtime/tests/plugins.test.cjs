'use strict';
// Runtime plugin API v1 (docs/plugins.md): explicit opt-in, versioned hooks, read-only snapshots,
// errors become diagnostics, and runtimes without plugins are untouched.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {createRuntime,PLUGIN_API_VERSION}=require('../src/splinely.js');

class Element {
  constructor(id,tag='circle'){this.id=id;this.tagName=tag;this.attrs=new Map();this.listeners=new Map();this.parentNode=null;}
  getAttribute(k){return this.attrs.has(k)?this.attrs.get(k):null;}
  setAttribute(k,v){this.attrs.set(k,String(v));}
  removeAttribute(k){this.attrs.delete(k);}
  addEventListener(k,fn){if(!this.listeners.has(k))this.listeners.set(k,new Set());this.listeners.get(k).add(fn);}
  removeEventListener(k,fn){this.listeners.get(k)?.delete(fn);}
  fire(type,event={}){for(const fn of this.listeners.get(type)||[])fn({target:this,type,...event});}
}
function mount(project,options={},ids=['n']){
  const svg=new Element('root','svg');const children=ids.map(id=>new Element(id));
  for(const e of children){e.ownerSVGElement=svg;e.parentNode=svg;}
  const clock={t:0};
  svg.ownerDocument={defaultView:{performance:{now:()=>clock.t}}};svg.children=children;
  svg.querySelectorAll=s=>s==='[id]'?children:[];svg.querySelector=()=>null;
  svg.getScreenCTM=()=>({inverse:()=>({})});svg.createSVGPoint=()=>({x:0,y:0,matrixTransform(){return {x:0,y:0};}});
  return {f:createRuntime(svg,project,{autoTick:false,...options}),svg,n:children[0],clock};
}
const node={id:'n',kind:{type:'circle',cx:5,cy:5,radius:2},transform:{x:0,y:0,rotation:0,scaleX:1,scaleY:1}};
const base=(extra={})=>({schemaVersion:1,nodes:[node],variables:{count:0,label:''},
  stateMachines:[{id:'m',initial:'a',states:[{id:'a'},{id:'b'}],transitions:[{from:'a',to:'b',event:{name:'go'}},{from:'b',to:'a',event:{name:'back'}}]}],...extra});
const plugin=(extra={})=>({id:'p1',apiVersion:1,...extra});
const messages=f=>f.diagnostics.map(e=>e.message);

test('the API version constant is exported and plugins are strictly opt-in',()=>{
  assert.equal(PLUGIN_API_VERSION,1);
  const {f}=mount(base());
  assert.deepEqual(f.plugins(),[]);
  assert.equal(typeof f.use,'function');
  f.tick(0.5);assert.deepEqual(f.diagnostics,[]);f.destroy();
  // The runtime never loads code by itself: no eval-like construction in its source.
  const source=fs.readFileSync(path.join(__dirname,'../src/splinely.js'),'utf8');
  assert.ok(!source.includes('new Function'),'runtime must not compile code');
});

test('without plugins behaviour is identical (events, state, variables, ticks)',()=>{
  const run=options=>{
    const {f,svg,n}=mount(base({clips:[{id:'c',duration:2,tracks:[{target:'n',property:'x',keyframes:[{time:0,value:0,easing:{type:'linear'}},{time:2,value:10,easing:{type:'linear'}}]}]}]}),options);
    const log=[];f.on('go',()=>log.push('go'));f.on('state.change',d=>log.push(d.to));
    f.emit('go');f.clip('c').play();f.tick(1);f.variable('count').set(3);svg.fire('click',{target:n});f.tick(1.5);
    const out={log,state:f.machine('m').state,snapshot:f.snapshot()};f.destroy();return out;
  };
  assert.deepEqual(run({}),run({plugins:[]}));
});

test('use() validates the plugin shape strictly',()=>{
  const {f}=mount(base());
  const bad=[
    [null,/object/],[[],/object/],['x',/object/],
    [{apiVersion:1},/id/],[{id:'1x',apiVersion:1},/id/],[{id:'a b',apiVersion:1},/id/],[{id:'x'.repeat(65),apiVersion:1},/id/],
    [{id:'a'},/apiVersion/],[{id:'a',apiVersion:2},/apiVersion/],[{id:'a',apiVersion:'1'},/apiVersion/],
    [plugin({extra:1}),/Unknown plugin field/],[plugin({url:'x'}),/Unknown plugin field/],
    [plugin({setup:1}),/function/],[plugin({onFrame:'x'}),/function/],
    [plugin({onEvent(){}}),/events/],[plugin({events:'click',onEvent(){}}),/events/],
    [plugin({events:['bad name'],onEvent(){}}),/Plugin event name/],
    [plugin({events:Array.from({length:33},(_,i)=>'e'+i),onEvent(){}}),/at most 32/],
    [plugin({frameBudgetMs:0}),/frameBudgetMs/],[plugin({frameBudgetMs:100}),/frameBudgetMs/],
  ];
  for(const [def,re] of bad)assert.throws(()=>f.use(def),re,JSON.stringify(def));
  assert.deepEqual(f.plugins(),[]);
  f.use(plugin());
  assert.throws(()=>f.use(plugin()),/Duplicate plugin id/);
  for(let i=0;i<7;i++)f.use(plugin({id:'q'+i}));
  assert.throws(()=>f.use(plugin({id:'overflow'})),/At most 8/);
  assert.equal(f.plugins().length,8);
  f.destroy();
  assert.throws(()=>f.use(plugin({id:'late'})),/destroyed/);
});

test('onEvent receives frozen primitive-only copies, never DOM events',()=>{
  const seen=[];
  const {f,svg,n}=mount(base(),{plugins:[plugin({events:['click','go','variable.change'],onEvent(e,ctx){seen.push(e);assert.equal(ctx.pluginId,'p1');}})]});
  svg.fire('click',{target:n,clientX:1,key:'k',pointerId:2,buttons:1});
  f.emit('go',{note:'hi',obj:{deep:1},fn(){}});
  f.variable('count').set(4);
  assert.equal(seen[0].type,'click');assert.equal(seen[0].target,'n');
  assert.ok(!('event' in seen[0]),'raw DOM event is not exposed');
  assert.equal(seen[1].type,'go');assert.equal(seen[1].note,'hi');assert.ok(!('obj' in seen[1]) && !('fn' in seen[1]));
  assert.deepEqual([seen[2].type,seen[2].id,seen[2].value],['variable.change','count',4]);
  for(const e of seen){assert.ok(Object.isFrozen(e));assert.throws(()=>{'use strict';e.type='x';});}
  assert.deepEqual(f.diagnostics,[]);f.destroy();
});

test('onState reports machine transitions; setup runs once with a frozen ctx',()=>{
  const states=[],setups=[];
  const {f}=mount(base(),{plugins:[plugin({setup(ctx){setups.push(ctx);},onState(s){states.push(s);}})]});
  assert.equal(setups.length,1);assert.ok(Object.isFrozen(setups[0]));
  f.emit('go');f.emit('back');
  assert.deepEqual(states.map(s=>[s.machine,s.from,s.to]),[['m','a','b'],['m','b','a']]);
  assert.ok(Object.isFrozen(states[0]));
  assert.deepEqual(f.plugins()[0].hooks,['setup','onState']);
  f.destroy();
});

test('ctx calls go through the same validation as host calls',()=>{
  let ctx;
  const {f}=mount(base(),{plugins:[plugin({setup(c){ctx=c;}})]});
  ctx.setVariable('count',5);assert.equal(f.variable('count').get(),5);assert.equal(ctx.getVariable('count'),5);
  assert.throws(()=>ctx.setVariable('count','text'),/type mismatch/);
  assert.throws(()=>ctx.setVariable('ghost',1),/Unknown variable/);
  assert.throws(()=>ctx.send('ghost','go'),/Unknown machine/);
  ctx.send('m','go');assert.equal(f.machine('m').state,'b');
  assert.throws(()=>ctx.emit('error'),/error events/);
  assert.throws(()=>ctx.emit('bad name'),/Event name/);
  assert.throws(()=>ctx.set('ghost',{x:1}),/Unknown node/);
  assert.throws(()=>ctx.playClip('ghost'),/Unknown clip/);
  const snap=ctx.snapshot();assert.ok(Object.isFrozen(snap)&&Object.isFrozen(snap.clips));
  assert.equal(snap.states.m,'b');
  assert.throws(()=>{'use strict';snap.time=5;});
  assert.throws(()=>{'use strict';ctx.emit=null;});
  // Reporting from a plugin lands in diagnostics with the plugin id.
  ctx.report('custom');assert.match(messages(f).at(-1),/Splinely plugin p1: custom/);
  f.destroy();
});

test('exceptions never break the runtime: reported, counted, plugin disabled after repeated failures',()=>{
  let calls=0;
  const other=[];
  const {f}=mount(base(),{plugins:[
    plugin({events:['go'],onEvent(){calls++;throw new Error('boom');}}),
    plugin({id:'p2',events:['go'],onEvent(e){other.push(e.type);}}),
  ]});
  for(let i=0;i<8;i++)f.emit('go');
  assert.equal(calls,5,'disabled after 5 errors');
  assert.equal(other.length,8,'a neighbouring plugin keeps working');
  assert.equal(f.machine('m').state,'b','the host state machine is unaffected');
  const msgs=messages(f);
  assert.equal(msgs.filter(m=>/p1 onEvent: boom/.test(m)).length,5);
  assert.ok(msgs.some(m=>/p1 disabled: too many errors/.test(m)));
  const info=f.plugins()[0];assert.equal(info.disabled,true);assert.equal(info.errors,5);
  f.destroy();
});

test('setup failure: reported, plugin not activated, runtime and other plugins continue',()=>{
  const ran=[];
  const {f}=mount(base(),{plugins:[
    plugin({setup(){throw new Error('nope');},onFrame(){ran.push('bad');}}),
    plugin({id:'ok',onFrame(){ran.push('ok');}}),
  ]});
  assert.ok(messages(f).some(m=>/p1 setup failed: nope/.test(m)));
  assert.deepEqual(f.plugins().map(p=>p.id),['ok']);
  f.tick(0.1);assert.deepEqual(ran,['ok']);f.destroy();
});

test('invalid plugins in options.plugins become diagnostics, not exceptions',()=>{
  const {f}=mount(base(),{plugins:[{id:'x',apiVersion:99},plugin({id:'fine'}),'junk']});
  const msgs=messages(f);
  assert.ok(msgs.some(m=>/apiVersion 99/.test(m)));assert.ok(msgs.some(m=>/must be an object/.test(m)));
  assert.deepEqual(f.plugins().map(p=>p.id),['fine']);f.destroy();
  const {f:g}=mount(base(),{plugins:'nope'});assert.ok(messages(g).some(m=>/must be an array/.test(m)));g.destroy();
});

test('async hook rejections are reported too',async()=>{
  const {f}=mount(base(),{plugins:[plugin({events:['go'],async onEvent(){throw new Error('late');}})]});
  f.emit('go');await new Promise(r=>setTimeout(r,5));
  assert.ok(messages(f).some(m=>/p1 onEvent: late/.test(m)));f.destroy();
});

test('onFrame runs once per tick with delta/time and is time-budgeted',()=>{
  const frames=[];
  const {f,clock}=mount(base(),{plugins:[plugin({frameBudgetMs:2,onFrame(fr){frames.push(fr);if(fr.slow)clock.t+=5;}})]});
  f.tick(0.25);f.tick(0);
  assert.equal(frames.length,2);assert.equal(frames[0].delta,0.25);assert.ok(Object.isFrozen(frames[0]));
  assert.deepEqual(messages(f),[]);
  // Over budget (measured with performance.now): reported, disabled after 3 overruns.
  const {f:g,clock:c2}=mount(base(),{plugins:[plugin({frameBudgetMs:2,onFrame(){c2.t+=5;}})]});
  for(let i=0;i<5;i++)g.tick(0.016);
  assert.equal(g.plugins()[0].overruns,3);assert.equal(g.plugins()[0].disabled,true);
  assert.ok(messages(g).some(m=>/budget 2ms/.test(m)));assert.ok(messages(g).some(m=>/disabled: frame budget exceeded/.test(m)));
  f.destroy();g.destroy();
});

test('a plugin cannot loop the host: emit feedback hits the existing cycle limit',()=>{
  const {f}=mount(base(),{plugins:[plugin({events:['ping'],onEvent(e,ctx){ctx.emit('ping');}})]});
  f.emit('ping');
  assert.ok(messages(f).some(m=>/Event cycle limit exceeded/.test(m)));
  assert.equal(f.machine('m').state,'a');f.destroy();
});

test('destroy stops all hooks and runs plugin disposers',()=>{
  const calls=[];let disposed=0;
  const {f}=mount(base(),{plugins:[plugin({events:['go'],setup(ctx){ctx.onDestroy(()=>disposed++);},onEvent(){calls.push('e');},onFrame(){calls.push('f');}})]});
  f.tick(0.1);f.destroy();assert.equal(disposed,1);
  const n=calls.length;assert.throws(()=>f.emit('go'),/destroyed/);assert.equal(calls.length,n);
});

test('shipped hover-wiggle runtime file works against a project with its variables',()=>{
  const dir=path.join(__dirname,'../../examples/plugins/hover-wiggle');
  const source=fs.readFileSync(path.join(dir,'runtime.js'),'utf8');
  const load=()=>new Function('"use strict";\n'+source)();   // what the trusted export loader does
  const {f,svg,n}=mount(base({variables:{wiggleCount:0,wiggleLast:''}}),{plugins:[load()]});
  svg.fire('pointerenter',{target:n});svg.fire('click',{target:n});svg.fire('pointermove',{target:n});
  assert.equal(f.variable('wiggleCount').get(),2);assert.equal(f.variable('wiggleLast').get(),'click:n');
  assert.deepEqual(f.diagnostics,[]);f.destroy();
  // Without the editor part's variables the plugin stays passive and says why (once).
  const {f:g,svg:s2,n:n2}=mount(base({variables:{}}),{plugins:[load()]});
  s2.fire('click',{target:n2});
  assert.equal(g.diagnostics.length,1);assert.match(g.diagnostics[0].message,/variables wiggleCount\/wiggleLast are missing/);
  g.destroy();
});
