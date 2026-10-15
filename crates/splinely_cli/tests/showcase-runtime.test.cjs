'use strict';
// Tests the generated canonical data and reviewed controller against the actual runtime.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {execFileSync} = require('node:child_process');
const path = require('node:path');
const {createRuntime} = require('../../../runtime/src/splinely.js');
const cli = process.env.SPLINELY_CLI || path.resolve(__dirname, '../../../target/debug/splinely-cli');
function project(command, ...args) {
  return JSON.parse(execFileSync(cli, [command, ...args, '--output', '-'], {encoding:'utf8'}));
}
class Element {
  constructor(id, tag='circle') { this.id=id; this.tagName=tag; this.attrs=new Map(); this.listeners=new Map(); this.style={}; }
  getAttribute(key) { return this.attrs.get(key) ?? null; }
  setAttribute(key, value) { this.attrs.set(key, String(value)); }
  removeAttribute(key) { this.attrs.delete(key); }
  addEventListener(key, fn) { if(!this.listeners.has(key))this.listeners.set(key,new Set());this.listeners.get(key).add(fn); }
  removeEventListener(key, fn) { this.listeners.get(key)?.delete(fn); }
}
function mount(p, speech=true, clock=null) {
  const spoken=[];
  const env={speechSynthesis:{speak:u=>spoken.push(u.text),cancel(){}},SpeechSynthesisUtterance:class {constructor(text){this.text=text;}}};
  if(clock)env.performance={now:()=>clock.now};
  const svg=new Element('root','svg');
  const children=p.nodes.map(n=>new Element(n.id,n.kind.type));
  const metadata=new Element('splinely-project','metadata');metadata.textContent=JSON.stringify(p);children.push(metadata);
  children.forEach(n=>{n.ownerSVGElement=svg;n.parentNode=svg;});
  svg.ownerDocument={defaultView:env};
  svg.querySelectorAll=selector=>selector==='[id]'?children:[metadata];
  svg.querySelector=selector=>children.find(n=>selector===`[id="${n.id}"]`) || null;
  const f=createRuntime(svg,p,{autoTick:!!clock,speech});
  // Like trusted export: the callback receives only the public instance API.
  const document={currentScript:{ownerSVGElement:svg}};
  new Function('splinely','document',p.customJs)(f,document);
  return {f,svg,spoken};
}

test('seeded gallery automatically visits each state, wraps, speaks and pauses via API',()=>{
  for(const count of [3,6,12]) {
    const p=project('random-figures','--seed','0','--count',String(count));
    const {f,svg,spoken}=mount(p);
    assert.equal(f.machine('gallery').state,'idle');assert.equal(spoken.length,0);
    const seen=[];f.on('state.change',e=>seen.push(e.to));
    f.emit('showcase.start');
    assert.equal(spoken.length,1);
    // Repeated START cannot select an unchecked state or skip the running one.
    f.emit('showcase.start');assert.equal(f.machine('gallery').state,'figure-0');
    f.tick(1.2);const before=f.clip('motion-0').time;
    f.emit('showcase.pause');f.tick(10);assert.equal(f.clip('motion-0').time,before);
    f.emit('showcase.pause'); // Idempotent pause retains the resume list.
    f.emit('showcase.start');
    f.tick(4.8 * count);
    assert.deepEqual(seen,Array.from({length:count+1},(_,i)=>`figure-${i%count}`));
    assert.equal(f.machine('gallery').state,'figure-0');
    assert.equal(spoken.length,count+1);
    for(let i=1;i<count;i++)assert.equal(svg.querySelector(`[id="phrase-${i}"]`).getAttribute('opacity'),'0');
    const unexpected=f.diagnostics.filter(d=>!d.message.includes('customJs is preserved'));
    assert.deepEqual(unexpected,[]);
    f.destroy();
    assert.equal(svg.querySelector('[id="start-control"]').listeners.get('keydown').size,0);
  }
});

test('wall-clock pause at a completion boundary holds the next state atomically',()=>{
  const clock={now:0}, p=project('random-figures','--count','3');
  const {f}=mount(p,true,clock);
  f.emit('showcase.start');clock.now=4810;
  f.emit('showcase.pause');
  assert.equal(f.machine('gallery').state,'figure-1');
  assert.equal(f.timeline.playing,false);
  assert.ok(p.clips.every(c=>!f.clip(c.id).playing));
  clock.now=8000;f.emit('showcase.start');
  assert.deepEqual(p.clips.filter(c=>f.clip(c.id).playing).map(c=>c.id),['motion-1']);
  f.destroy();
});

test('speech failure never gates automatic transitions and does not opt itself in',()=>{
  const {f,spoken}=mount(project('random-figures','--count','3'),false);
  f.emit('showcase.start');f.tick(4.9);
  assert.equal(f.machine('gallery').state,'figure-1');assert.equal(f.speech.enabled,false);
  assert.deepEqual(spoken,[]);assert.ok(f.diagnostics.some(d=>d.message.includes('Speech is opt-in')));
  f.destroy();
});

test('water drop deforms at impact, loops and resumes without reset',()=>{
  const {f,svg}=mount(project('water-drop'),false);
  const drop=svg.querySelector('[id="drop"]');const first=drop.getAttribute('transform');
  f.tick(1.02);assert.notEqual(drop.getAttribute('transform'),first);
  f.emit('showcase.pause');const before=drop.getAttribute('transform');f.tick(10);
  assert.equal(drop.getAttribute('transform'),before);
  f.emit('showcase.start');f.tick(1.78);assert.equal(drop.getAttribute('transform'),first);
  assert.equal(f.speech.enabled,false);f.destroy();
});
