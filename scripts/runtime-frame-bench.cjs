#!/usr/bin/env node
'use strict';
// Micro-benchmark for the browser runtime's per-frame cost (runtime/src/splinely.js) on a fake DOM.
//   node scripts/runtime-frame-bench.cjs [--runtime path/to/splinely.js] [--frames N] [--only scenario-prefix] [--figure f.svg=f.sply] [--json]
// Each scenario mounts a checked-in example with autoTick:false and calls tick(1/60) repeatedly,
// reporting microseconds per frame and DOM attribute writes per frame. The "idle-raf" scenario mounts
// the mascot with autoTick:true and a fake requestAnimationFrame and counts how many frames are still
// requested over 10 simulated seconds once nothing animates (the idle fast path).
// The "autotick:" scenarios mount a figure with the real auto-tick loop on the fake browser of
// runtime/tests/fake-waapi.cjs (rAF at 60 Hz, timers, Web Animations) and report JavaScript time and
// DOM writes per displayed frame over 20 simulated seconds — once with Web Animations available
// ("native") and once without ("js"). Add figures with --figure file.svg=file.sply (repeatable);
// stickfight is built in.
const fs=require('node:fs');
const path=require('node:path');
const ROOT=path.join(__dirname,'..');
const args=process.argv.slice(2);
const opt=(name,def)=>{const i=args.indexOf(name);return i<0?def:args[i+1];};
const runtimePath=path.resolve(opt('--runtime',path.join(ROOT,'runtime/src/splinely.js')));
const FRAMES=Number(opt('--frames',3000));
const rt=require(runtimePath);

class Element {
  constructor(id,tag,attrs){this.id=id;this.tagName=tag;this.attrs=new Map(Object.entries(attrs||{}));this.parentNode=null;this.children=[];}
  getAttribute(k){return this.attrs.has(k)?this.attrs.get(k):null;}
  setAttribute(k,v){writes++;this.attrs.set(k,String(v));}
  removeAttribute(k){writes++;this.attrs.delete(k);}
  addEventListener(k,fn){(this.listeners||(this.listeners=new Map())).set(k,fn);}
  removeEventListener(){}
  fire(type,event){const fn=this.listeners&&this.listeners.get(type);if(fn)fn({target:this,type,pointerId:1,...event});}
  setPointerCapture(){}releasePointerCapture(){}
}
let writes=0;
// Minimal SVG reader: elements, attributes and nesting (enough for the exported examples).
function domFromSvg(file){
  let text=fs.readFileSync(file,'utf8');
  text=text.replace(/<!--[\s\S]*?-->/g,'').replace(/<metadata[\s\S]*?<\/metadata>/g,'').replace(/<script[\s\S]*?<\/script>/g,'').replace(/<title[\s\S]*?<\/title>/g,'').replace(/<desc[\s\S]*?<\/desc>/g,'');
  const svg=new Element('root','svg'),all=[],stack=[svg];let first=true;
  svg.ownerDocument={defaultView:{}};
  for(const m of text.matchAll(/<(\/?)([A-Za-z][\w:-]*)([^>]*?)(\/?)>/g)){
    if(m[1]){if(stack.length>1)stack.pop();continue;}
    if(first){first=false;continue;}
    const attrs={};for(const a of m[3].matchAll(/([\w:-]+)="([^"]*)"/g))attrs[a[1]]=a[2];
    const el=new Element(attrs.id,m[2],attrs),parent=stack[stack.length-1];
    el.parentNode=parent;el.ownerSVGElement=svg;parent.children.push(el);all.push(el);
    if(!m[4])stack.push(el);
  }
  const withId=all.filter(e=>e.id);
  svg.querySelectorAll=sel=>sel==='[id]'?withId:[];
  svg.querySelector=()=>null;
  svg.getScreenCTM=()=>null;
  return svg;
}
const readProject=name=>{
  const json=path.join(ROOT,'examples',name+'.splinely.json');
  if(fs.existsSync(json))return JSON.parse(fs.readFileSync(json,'utf8'));
  const bytes=fs.readFileSync(path.join(ROOT,'examples',name+'.sply'));
  return bytes[0]===0x7b?JSON.parse(bytes.toString('utf8')):bytes; // text or binary SPLY
};
// Mascot: curious-face (followers + blink timers) plus a looping "breathe" clip on two properties.
function mascot(){
  const p=readProject('curious-face');
  p.clips=[{id:'breathe',duration:3,looping:true,autoplay:true,tracks:[
    {nodeId:'head',property:'scaleY',keyframes:[{time:0,value:{type:'number',value:1}},{time:1.5,value:{type:'number',value:1.03},easing:{type:'cubicBezier',x1:0.42,y1:0,x2:0.58,y2:1}},{time:3,value:{type:'number',value:1}}]},
    {nodeId:'mouth',property:'y',keyframes:[{time:0,value:{type:'number',value:0}},{time:1.5,value:{type:'number',value:1.5}},{time:3,value:{type:'number',value:0}}]},
  ]}];
  return p;
}
const scenarios=[
  {name:'mascot-breathe+pointer',svg:'curious-face',project:mascot,pointer:true},
  {name:'stickfight-loop',svg:'stickfight',project:()=>readProject('stickfight')},
  {name:'water-drop-loop',svg:'water-drop',project:()=>readProject('water-drop')},
  {name:'cactus-runner-loops',svg:'cactus-runner',project:()=>readProject('cactus-runner')},
  {name:'blaze-vs-volt-loop',svg:'blaze-vs-volt',project:()=>readProject('blaze-vs-volt')},
  {name:'mascot-idle(no clips)',svg:'curious-face',project:()=>readProject('curious-face')},
];
function run(s){
  const svg=domFromSvg(path.join(ROOT,'examples',s.svg+'.svg'));
  const api=rt.createRuntime(svg,s.project(),{autoTick:false,reducedMotion:false});
  for(const c of api.snapshot().clips)if(!c.playing && s.name.endsWith('loop'))api.clip(c.id).play();
  if(s.pointer)svg.fire('pointermove',{clientX:300,clientY:40});
  for(let i=0;i<300;i++)api.tick(1/60); // warm up the JIT
  writes=0;
  const t0=process.hrtime.bigint();
  for(let i=0;i<FRAMES;i++){
    if(s.pointer && i%120===0)svg.fire('pointermove',{clientX:100+(i%240),clientY:40+(i%90)});
    api.tick(1/60);
  }
  const ms=Number(process.hrtime.bigint()-t0)/1e6;
  api.destroy();
  return {scenario:s.name,usPerFrame:+(ms*1000/FRAMES).toFixed(2),writesPerFrame:+(writes/FRAMES).toFixed(2)};
}
// Idle: count rAF callbacks requested over 10 s of simulated time with nothing animating.
function idleRaf(){
  const svg=domFromSvg(path.join(ROOT,'examples','curious-face.svg'));
  let wall=0,pending=null,requested=0,timeouts=[];
  const env={performance:{now:()=>wall},requestAnimationFrame:fn=>{pending=fn;requested++;return requested;},cancelAnimationFrame(){pending=null;},
    setTimeout:(fn,ms)=>{const t={fn,at:wall+ms};timeouts.push(t);return t;},clearTimeout:t=>{timeouts=timeouts.filter(x=>x!==t);},setInterval:()=>1,clearInterval(){}};
  svg.ownerDocument={defaultView:env};
  const api=rt.createRuntime(svg,readProject('curious-face'),{reducedMotion:false});
  const t0=process.hrtime.bigint();let calls=0;
  for(let i=0;i<600;i++){ // 10 s at 60 Hz
    wall+=1000/60;
    for(const t of timeouts.filter(t=>t.at<=wall)){timeouts=timeouts.filter(x=>x!==t);t.fn();}
    const fn=pending;pending=null;if(fn){calls++;fn();}
  }
  const ms=Number(process.hrtime.bigint()-t0)/1e6;
  const state=api.machine('blink').state;api.destroy();
  return {scenario:'idle-raf(mascot,10s@60Hz)',rafCallbacks:calls,usTotal:+(ms*1000).toFixed(0),blinkState:state};
}
// Auto-tick loop on the fake browser: JS µs and DOM writes (attributes + style properties) per frame.
function autotick(label,svgFile,projectBytes,waapi){
  const {createEnv}=require(path.join(ROOT,'runtime/tests/fake-waapi.cjs'));
  const {env,svg}=createEnv(fs.readFileSync(svgFile,'utf8'),{waapi,io:true});
  const api=rt.createRuntime(svg,projectBytes(),{reducedMotion:false});
  env.advanceTo(env.time+2000); // warm up the JIT, let autoplay settle
  const w0=env.writes,f0=env.frames,a0=env.animateCalls||0,t0=process.hrtime.bigint();
  env.advanceTo(env.time+20000);
  const ms=Number(process.hrtime.bigint()-t0)/1e6,frames=env.frames-f0||1,displayed=1200;
  api.destroy();
  return {scenario:'autotick:'+label+(waapi?' native':' js'),usPerFrame:+(ms*1000/displayed).toFixed(2),writesPerFrame:+((env.writes-w0)/displayed).toFixed(2),rafCallbacksPer20s:frames,animateCalls:(env.animateCalls||0)-a0};
}
const figures=[['stickfight',path.join(ROOT,'examples/stickfight.svg'),()=>readProject('stickfight')]];
for(let i=0;i<args.length;i++)if(args[i]==='--figure'){const [svgFile,sply]=args[i+1].split('=');figures.push([path.basename(sply).replace(/\.sply$/,''),svgFile,()=>fs.readFileSync(sply)]);}
const only=opt('--only',null);
const results=scenarios.filter(s=>!only || s.name.startsWith(only)).map(run);if(!only || only==='idle')results.push(idleRaf());
if(!only || only.startsWith('autotick'))for(const [label,svgFile,project] of figures)for(const waapi of [false,true])results.push(autotick(label,svgFile,project,waapi));
if(args.includes('--json'))console.log(JSON.stringify(results,null,1));
else {console.log('runtime:',path.relative(process.cwd(),runtimePath),'frames:',FRAMES);console.table(results);}
