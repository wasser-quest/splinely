'use strict';
// Auto-tick loop: requestAnimationFrame stops while nothing animates (idle fast path) and resumes on any
// API call, DOM event or due timer; `maxFps` caps how often a frame actually ticks.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const rt=require('../src/splinely.js');

class Element {
  constructor(id){this.id=id;this.attrs=new Map();this.parentNode=null;this.children=[];this.listeners=new Map();}
  getAttribute(k){return this.attrs.has(k)?this.attrs.get(k):null;}
  setAttribute(k,v){this.attrs.set(k,String(v));}
  removeAttribute(k){this.attrs.delete(k);}
  addEventListener(k,fn){this.listeners.set(k,fn);}removeEventListener(){}
}
// Fake browser clock: frames every 1000/60 ms, timeouts run when due.
function harness(project,options){
  const svg=new Element('root'),els=project.nodes.map(n=>{const e=new Element(n.id);e.ownerSVGElement=svg;e.parentNode=svg;return e;});
  let wall=0,pending=null,frames=0,timeouts=[];
  const env={performance:{now:()=>wall},requestAnimationFrame:fn=>{pending=fn;return 1;},cancelAnimationFrame(){pending=null;},
    setTimeout:(fn,ms)=>{const t={fn,at:wall+ms};timeouts.push(t);return t;},clearTimeout:t=>{timeouts=timeouts.filter(x=>x!==t);},
    setInterval:()=>1,clearInterval(){}};
  svg.ownerDocument={defaultView:env};
  svg.querySelectorAll=sel=>sel==='[id]'?els:[];svg.querySelector=()=>null;
  const api=rt.createRuntime(svg,project,{reducedMotion:false,...options});
  const run=ms=>{const end=wall+ms;while(wall<end-1e-9){wall+=1000/60;
    for(const t of timeouts.filter(t=>t.at<=wall)){timeouts=timeouts.filter(x=>x!==t);t.fn();}
    const fn=pending;pending=null;if(fn){frames++;fn();}}};
  return {api,run,el:id=>els.find(e=>e.id===id),frames:()=>frames,sleeping:()=>pending===null};
}
const circle=id=>({id,kind:{type:'circle',cx:0,cy:0,radius:1}});
const num=v=>({type:'number',value:v});
const clipProject=()=>({schemaVersion:1,nodes:[circle('a')],clips:[{id:'move',duration:1,tracks:[{nodeId:'a',property:'x',keyframes:[{time:0,value:num(0)},{time:1,value:num(10)}]}]}]});

test('the frame loop sleeps while idle and an API call resumes it without skipping time',()=>{
  const h=harness(clipProject());
  h.run(1000);
  assert.ok(h.frames()<5,'idle runtime keeps requesting frames: '+h.frames());
  assert.ok(h.sleeping());
  h.api.clip('move').play();
  assert.ok(!h.sleeping(),'play() must wake the loop');
  h.run(500);
  const x=Number(/translate\(([-\d.e]+)/.exec(h.el('a').getAttribute('transform'))[1]);
  assert.ok(x>4 && x<6,'clip must advance by elapsed frames, not by the slept time: x='+x);
  h.run(1000); // clip ends -> idle again
  const after=h.frames();h.run(1000);
  assert.equal(h.frames(),after);
  h.api.destroy();
});

test('timed transitions still fire on time while the loop sleeps',()=>{
  const project={schemaVersion:1,nodes:[circle('a')],
    machines:[{id:'m',initial:'one',states:[{id:'one',transitions:[{to:'two',after:2}]},{id:'two'}]}]};
  const h=harness(project);
  h.run(1900);
  assert.equal(h.api.machine('m').state,'one');
  h.run(200);
  assert.equal(h.api.machine('m').state,'two');
  assert.ok(h.frames()<20,'expected only a few frames around the timer, got '+h.frames());
  h.api.destroy();
});

test('maxFps caps the tick rate',()=>{
  const h=harness(clipProject(),{maxFps:30});
  h.api.clip('move').play();
  // Count frames whose callback actually ticked (the transform changes on every tick of the playing clip).
  let ticks=0,last=h.el('a').getAttribute('transform');
  for(let i=0;i<30;i++){h.run(1000/60);const t=h.el('a').getAttribute('transform');if(t!==last)ticks++;last=t;}
  assert.equal(h.frames(),30);
  assert.ok(ticks>=14 && ticks<=16,'expected ~15 ticks in 30 frames at maxFps 30, got '+ticks);
  h.api.destroy();
  assert.throws(()=>harness(clipProject(),{maxFps:0}),/maxFps/);
});

test('timeline callbacks fire on time after the clips end and the loop sleeps',()=>{
  const h=harness(clipProject());
  let fired=null;
  h.api.clip('move').play();
  h.api.timeline.at(3,({time})=>{fired=time;});
  h.run(2900);
  assert.equal(fired,null);
  h.run(200);
  assert.equal(fired,3);
  assert.ok(h.frames()<100,'loop should sleep between the clip end and the callback: '+h.frames());
  h.api.destroy();
});
