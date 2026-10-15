'use strict';
// Web-player accessibility: validation, ARIA semantics, keyboard activation, focus ring, live region,
// reduced motion and byte-identical behaviour for projects without accessibility data (fake DOM).
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {createRuntime,normalize}=require('../src/splinely.js');

class Element {
  constructor(id,tag='rect'){this.id=id;this.tagName=tag;this.attrs=new Map();this.listeners=new Map();this.parentNode=null;this.children=[];this.textContent='';}
  getAttribute(k){return this.attrs.has(k)?this.attrs.get(k):null;}
  setAttribute(k,v){this.attrs.set(k,String(v));}
  removeAttribute(k){this.attrs.delete(k);}
  addEventListener(k,fn){if(!this.listeners.has(k))this.listeners.set(k,new Set());this.listeners.get(k).add(fn);}
  removeEventListener(k,fn){this.listeners.get(k)?.delete(fn);}
  appendChild(c){c.parentNode=this;this.children.push(c);return c;}
  insertBefore(c,ref){c.parentNode=this;const i=ref?this.children.indexOf(ref):-1;if(i<0)this.children.push(c);else this.children.splice(i,0,c);return c;}
  removeChild(c){const i=this.children.indexOf(c);if(i>=0)this.children.splice(i,1);c.parentNode=null;return c;}
  get nextSibling(){const i=this.parentNode.children.indexOf(this);return this.parentNode.children[i+1]||null;}
  getBBox(){return {x:1,y:2,width:10,height:20};}
  getBoundingClientRect(){return {left:100,top:50,width:20,height:10};}
  dispatchEvent(event){event.target=this;let el=this;const path=[];while(el){path.unshift(el);el=el.parentNode;}
    for(const e of path)for(const fn of [...(e.listeners.get(event.type)||[])])fn(event);
    return true;}
  fire(type,event={}){event.type=type;this.dispatchEvent(event);}
}
function mount(project,{env={},ids=['a','b','c']}={}){
  const svg=new Element('root','svg');
  const children=ids.map(id=>{const e=new Element(id);e.ownerSVGElement=svg;e.parentNode=svg;return e;});
  svg.children=[...children];
  const doc={defaultView:env,createElementNS:(ns,tag)=>new Element(null,tag),addEventListener(){},removeEventListener(){}};
  svg.ownerDocument=doc;
  svg.querySelectorAll=s=>s==='[id]'?children:[];svg.querySelector=()=>null;
  svg.getScreenCTM=()=>({inverse:()=>({})});svg.createSVGPoint=()=>({x:0,y:0,matrixTransform(){return {x:this.x,y:this.y};}});
  const f=createRuntime(svg,project,{autoTick:false,...(env.reducedMotion!==undefined?{reducedMotion:env.reducedMotion}:{})});
  return {f,svg,els:Object.fromEntries(children.map(e=>[e.id,e]))};
}
const a11yKeys=e=>[...e.attrs.keys()].filter(k=>/^(role|tabindex|aria-|data-splinely)/.test(k));
const node=(id,extra={})=>({id,name:id.toUpperCase(),kind:{type:'rect',x:0,y:0,width:10,height:10},visible:true,...extra});
const click=(target,extra)=>({name:'click',target,...extra});
function demo(extra={}){
  return {schemaVersion:1,name:'Scene',nodes:[node('a'),node('b'),node('c')],
    stateMachines:[{id:'m',initial:'off',states:[{id:'off'},{id:'on',overrides:[{nodeId:'a',property:'opacity',value:{type:'number',value:0.5}}]}],
      transitions:[{from:'off',to:'on',event:click('a')},{from:'on',to:'off',event:click('b')}]}],...extra};
}
const key=(k,extra)=>({key:k,preventDefault(){this.prevented=true;},...extra});

test('without a11y data nothing is added: no attributes, no listeners beyond the runtime ones',()=>{
  const {els,svg,f}=mount(demo());
  for(const e of [svg,...Object.values(els)])assert.deepEqual(a11yKeys(e),[],e.id);
  assert.equal(svg.children.length,3);
  assert.equal(svg.listeners.get('keyup')?.size,1,'only the runtime keyup listener');
  f.destroy();
});

test('validation: description, node a11y and announce are strict',()=>{
  const bad=(p,re)=>assert.throws(()=>normalize(p),re);
  bad(demo({description:5}),/description must be a string/);
  bad(demo({description:' padded'}),/Project description/);
  bad(demo({description:'x'.repeat(1001)}),/1\.\.1000/);
  bad(demo({nodes:[node('a',{a11y:{label:'x',foo:1}})]}),/unknown field foo/);
  bad(demo({nodes:[node('a',{a11y:{role:'banner'}})]}),/role must be one of/);
  bad(demo({nodes:[node('a',{a11y:{hidden:'yes'}})]}),/hidden must be boolean/);
  bad(demo({nodes:[node('a',{a11y:{label:''}})]}),/label must be 1\.\.200/);
  bad(demo({nodes:[node('a',{a11y:{label:'a\nb'}})]}),/control characters/);
  bad(demo({nodes:[node('a',{a11y:{label:'x'.repeat(201)}})]}),/1\.\.200/);
  bad(demo({nodes:[node('c',{a11y:{hidden:true,label:'x'}})]}),/hidden nodes cannot/);
  bad(demo({nodes:[node('a',{a11y:{hidden:true}})]}),/interactive node cannot be hidden/);
  bad(demo({nodes:[node('a',{a11y:null})]}),/must be an object/);
  const p=demo();p.stateMachines[0].states[0].announce='';bad(p,/announce must be 1\.\.200/);
  normalize(demo({description:'ok',nodes:[node('a',{a11y:{}}),node('c',{a11y:{hidden:true}})]}));
});

test('semantics: root group role, labels, default role button, tabindex, hidden, overrides; restored on destroy',()=>{
  const {els,svg,f}=mount(demo({description:'A toggle scene',nodes:[node('a',{a11y:{label:'Switch on',description:'Turns the lamp on'}}),node('b'),node('c',{a11y:{hidden:true}})]}));
  assert.equal(svg.getAttribute('role'),'group');
  assert.equal(svg.getAttribute('aria-label'),'Scene');
  assert.equal(svg.getAttribute('aria-description'),'A toggle scene');
  assert.equal(els.a.getAttribute('role'),'button');assert.equal(els.a.getAttribute('aria-label'),'Switch on');
  assert.equal(els.a.getAttribute('aria-description'),'Turns the lamp on');assert.equal(els.a.getAttribute('tabindex'),'0');
  assert.equal(els.b.getAttribute('aria-label'),'B','falls back to the node name');
  assert.equal(els.c.getAttribute('aria-hidden'),'true');assert.equal(els.c.getAttribute('tabindex'),null);
  f.destroy();
  for(const e of [svg,...Object.values(els)])assert.deepEqual(a11yKeys(e),[],e.id+' restored');
});

test('semantics: image role without interactive nodes, role override, existing exporter attributes win',()=>{
  const {svg,els,f}=mount({schemaVersion:1,name:'Pic',description:'A picture',nodes:[node('a',{a11y:{role:'img',label:'Sun'}})]});
  assert.equal(svg.getAttribute('role'),'img');assert.equal(els.a.getAttribute('role'),'img');assert.equal(els.a.getAttribute('tabindex'),null);f.destroy();
  const s=mount(demo({description:'d'}),{});s.f.destroy();
  const pre=new Element('root','svg');pre.setAttribute('role','img');pre.setAttribute('aria-labelledby','t');pre.setAttribute('aria-describedby','d');
  const e=new Element('a');e.parentNode=pre;pre.children=[e];pre.ownerDocument={defaultView:{}};pre.querySelectorAll=q=>q==='[id]'?[e]:[];pre.querySelector=()=>null;
  pre.getScreenCTM=()=>({inverse:()=>({})});pre.createSVGPoint=()=>({x:0,y:0,matrixTransform(){return this;}});
  createRuntime(pre,{schemaVersion:1,name:'Z',description:'Y',nodes:[node('a')]},{autoTick:false});
  assert.equal(pre.getAttribute('role'),'img');assert.equal(pre.getAttribute('aria-label'),null);assert.equal(pre.getAttribute('aria-description'),null);
});

test('keyboard: Enter on keydown and Space on keyup dispatch the same click a pointer would; other keys and modifiers do not',()=>{
  const env={MouseEvent:class{constructor(type,init){this.type=type;Object.assign(this,init);}}};
  const {els,svg,f}=mount(demo({description:'d'}),{env});
  const seen=[];f.on('a','click',d=>seen.push(d));
  els.a.fire('keydown',key('Enter'));
  assert.equal(f.machine('m').state,'on');assert.equal(seen.length,1);assert.equal(seen[0].x,110);assert.equal(seen[0].y,55,'click at the node centre');
  els.b.fire('keydown',key(' '));assert.equal(f.machine('m').state,'on','Space waits for keyup');
  els.b.fire('keyup',key(' '));assert.equal(f.machine('m').state,'off');
  const space=key(' ');els.a.fire('keydown',space);assert.equal(space.prevented,true,'Space must not scroll the page');
  els.a.fire('keyup',key(' '));assert.equal(f.machine('m').state,'on');
  els.b.fire('keydown',key('Enter',{ctrlKey:true}));els.b.fire('keydown',key('a'));els.b.fire('keydown',key('Enter',{repeat:true}));
  assert.equal(f.machine('m').state,'on');
  els.c.fire('keydown',key('Enter'));assert.equal(f.machine('m').state,'on','non-focusable nodes are not activated');
  els.a.fire('keydown',key(' '));els.b.fire('keyup',key(' '));assert.equal(f.machine('m').state,'on','keyup on another node does not activate');
  f.destroy();els.b.fire('keydown',key('Enter'));assert.equal(f.snapshot().destroyed,true);
});

test('keyboard activation falls back to the runtime listener when MouseEvent is unavailable and fires interactions bindings',()=>{
  const {els,f}=mount(demo({description:'d',variables:{hit:false},interactions:[{target:'c',event:'click',actions:[{type:'setVariable',name:'hit',value:true}]}]}));
  els.c.fire('keydown',key('Enter'));assert.equal(f.variable('hit').get(),true);
  f.destroy();
});

test('focus ring: shown for keyboard focus only, follows the node transform, system colours in forced-colors mode, removed on blur/destroy',()=>{
  const env={matchMedia:q=>({matches:/forced-colors/.test(q)})};
  const {svg,els,f}=mount(demo({description:'d'}),{env});
  els.a.matches=()=>false;els.a.fire('focus');assert.equal(svg.children.length,3,'mouse focus: no ring');
  els.a.matches=()=>true;els.a.fire('focus');
  const ring=svg.children.find(e=>e.getAttribute&&e.getAttribute('data-splinely-focus-ring')!=null);
  assert.ok(ring);assert.equal(svg.children.indexOf(ring),svg.children.indexOf(els.a)+1);
  assert.deepEqual(ring.children.map(r=>r.getAttribute('stroke')),['Canvas','Highlight']);
  assert.equal(ring.children[1].getAttribute('x'),'-3');assert.equal(ring.children[1].getAttribute('width'),'18');
  assert.equal(ring.getAttribute('pointer-events'),'none');
  els.a.setAttribute('transform','translate(5 5)');f.set('b',{x:1});
  assert.equal(ring.getAttribute('transform'),'translate(5 5)');
  els.a.fire('blur');assert.equal(svg.children.some(e=>e===ring),false);
  els.a.fire('focus');f.destroy();assert.equal(svg.children.some(e=>e.getAttribute&&e.getAttribute('data-splinely-focus-ring')!=null),false);
  const plain=mount(demo({description:'d'}));plain.els.b.matches=()=>true;plain.els.b.fire('focus');
  assert.deepEqual(plain.svg.children.find(e=>e.tagName==='g').children.map(r=>r.getAttribute('stroke')),['#000000','#ffbf47']);
});

test('focus-visible falls back to keyboard modality when matches() is unsupported',()=>{
  const {svg,els}=mount(demo({description:'d'}));
  els.a.fire('focus');assert.equal(svg.children.length,3);
});

test('keyboard focus mirrors hover handlers; blur ends them',()=>{
  const p=demo({description:'d'});p.stateMachines.push({id:'h',initial:'out',states:[{id:'out'},{id:'in'}],transitions:[{from:'out',to:'in',event:{name:'pointerenter',target:'b'}},{from:'in',to:'out',event:{name:'pointerleave',target:'b'}}]});
  const {els,f}=mount(p);els.b.matches=()=>true;
  els.b.fire('focus');assert.equal(f.machine('h').state,'in');els.b.fire('blur');assert.equal(f.machine('h').state,'out');
  els.b.matches=()=>false;els.b.fire('focus');assert.equal(f.machine('h').state,'out','pointer focus does not double-fire hover');
  f.destroy();
});

test('live region: created only when a state announces, polite, updated on transitions, same text re-announced, removed on destroy',()=>{
  const timers=[];const env={setTimeout:(fn)=>{timers.push(fn);return timers.length;},clearTimeout(){}};
  const p=demo();p.stateMachines[0].states[1].announce='Lamp is on';p.stateMachines[0].states[0].announce='Lamp is off';
  const {svg,els,f}=mount(p,{env});
  const live=svg.children.find(e=>e.getAttribute&&e.getAttribute('data-splinely-live')!=null);
  assert.ok(live);assert.equal(live.getAttribute('aria-live'),'polite');assert.equal(live.getAttribute('role'),'status');assert.equal(live.getAttribute('aria-atomic'),'true');
  assert.equal(live.textContent,'','initial state is not announced');
  els.a.fire('click');assert.equal(live.textContent,'Lamp is on');
  els.b.fire('click');assert.equal(live.textContent,'Lamp is off');
  f.machine('m').send('click');
  assert.equal(svg.getAttribute('role'),'group');
  f.destroy();assert.equal(svg.children.includes(live),false);
  const none=mount(demo({description:'d'}));assert.equal(none.svg.children.some(e=>e.getAttribute('data-splinely-live')!=null),false);
});

test('live region re-announces identical text after a clear',()=>{
  const timers=[];const env={setTimeout:fn=>{timers.push(fn);return timers.length;},clearTimeout(){}};
  const p=demo();p.stateMachines[0].states[1].announce='Same';p.stateMachines[0].transitions.push({from:'on',to:'on',event:click('c')});
  const {svg,els}=mount(p,{env});const live=svg.children.find(e=>e.getAttribute('data-splinely-live')!=null);
  els.a.fire('click');assert.equal(live.textContent,'Same');els.c.fire('click');assert.equal(live.textContent,'');timers.shift()();assert.equal(live.textContent,'Same');
});

test('reduced motion stays honoured with accessibility data: instant states, no looping autoplay, announcements still work',()=>{
  const p=demo({description:'d',clips:[{id:'loop',duration:1,looping:true,autoplay:true,tracks:[]}]});
  p.stateMachines[0].transitions[0].blend=1;p.stateMachines[0].states[1].announce='On';
  const {els,f,svg}=mount(p,{env:{reducedMotion:true}});
  assert.equal(f.snapshot().reducedMotion,true);assert.equal(f.clip('loop').playing,false);
  els.a.fire('keydown',key('Enter'));
  assert.equal(svg.children.find(e=>e.getAttribute&&e.getAttribute('data-splinely-live')!=null).textContent,'On');
  f.destroy();
  const motion=mount(p,{env:{reducedMotion:false}});assert.equal(motion.f.clip('loop').playing,true);motion.f.destroy();
});
