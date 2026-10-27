'use strict';
// Native (Web Animations) offload, visibility pause and the shared frame scheduler, on the fake browser of
// fake-waapi.cjs. Every native result is compared with the JavaScript path (nativeAnimations:false), which
// writes the same pose as attributes each frame.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const rt=require('../src/splinely.js');
const {createEnv}=require('./fake-waapi.cjs');

const SVG='<svg viewBox="0 0 100 100"><circle id="a" cx="0" cy="0" r="1"/><circle id="b" cx="0" cy="0" r="1"/><circle id="c" cx="0" cy="0" r="1"/></svg>';
const circle=(id,extra)=>({id,kind:{type:'circle',cx:0,cy:0,radius:1},...extra});
const num=v=>({type:'number',value:v});
const cubic={type:'cubicBezier',x1:0.42,y1:0,x2:0.58,y2:1};
const track=(target,property,values,dur,extra)=>({...target,property,keyframes:values.map((v,i)=>({time:dur*i/(values.length-1),value:num(v),easing:cubic})),...extra});
// a: looping x/opacity clip; b: bound to a two-bone rig whose bones move; c: static.
function project(extra={}){
  return {schemaVersion:1,
    bones:[{id:'root',x:10,y:20,rotation:0,length:5},{id:'arm',parentId:'root',x:2,y:1,rotation:15,length:4}],
    nodes:[circle('a'),circle('b',{boneId:'arm',transform:{x:3,y:0,rotation:0,scaleX:1,scaleY:1}}),circle('c')],
    clips:[
      {id:'sway',duration:2,looping:true,autoplay:true,tracks:[track({nodeId:'a'},'x',[0,10,0],2),track({nodeId:'a'},'opacity',[1,0.4,1],2)]},
      {id:'rig',duration:1.5,looping:true,autoplay:true,tracks:[track({nodeId:'root'},'y',[20,26,20],1.5),track({nodeId:'root'},'rotation',[0,12,0],1.5),track({nodeId:'arm'},'rotation',[15,-30,15],1.5)]},
      {id:'pop',duration:0.5,tracks:[track({nodeId:'c'},'y',[0,8],0.5)]},
    ],
    ...extra};
}
function mount(options={},envOptions={},proj=project()){
  const {env,svg}=createEnv(SVG,{io:true,...envOptions});
  const events=[];
  const api=rt.createRuntime(svg,proj,{reducedMotion:false,...options});
  api.on('clip.end',d=>events.push([Math.round(env.time),d.clip]));
  const el=id=>svg.all.find(e=>e.id===id);
  return {env,svg,api,el,events,anims:id=>el(id).animations.length};
}
// Native and JavaScript runtimes driven through the same script; asserts the shown pose matches every frame.
function compare(script,proj=project,ms=4000,label='',skip=()=>false){
  const sides=[mount({nativeAnimations:true},{},proj()),mount({nativeAnimations:false},{},proj())];
  // API calls land between frames (as they do in browsers), not exactly on a frame time.
  for(const s of sides)s.env.advanceTo(s.env.time+5);
  let max=0,where='';
  for(let t=0;t<ms;t+=1000/60){
    for(const s of sides){script(s,t);s.env.advanceTo(s.env.time+1000/60);}
    if(skip(t))continue;
    for(const id of ['a','b','c']){
      const x=sides[0].el(id).shown(),y=sides[1].el(id).shown();
      let d=Math.abs(x.opacity-y.opacity);for(let k=0;k<6;k++)d=Math.max(d,Math.abs(x.m[k]-y.m[k]));
      if(d>max){max=d;where=id+' at '+Math.round(t)+' ms';}
    }
  }
  assert.ok(max<1e-6,label+': native and JavaScript output differ by '+max+' ('+where+')');
  assert.deepEqual(sides[0].events,sides[1].events);
  return sides;
}

test('transform/opacity clips run as Web Animations with no per-frame JavaScript',()=>{
  const m=mount();
  m.env.advanceTo(m.env.time+500);
  assert.ok(m.anims('a')>=2,'x and opacity of a animate natively');
  assert.ok(m.anims('b')>=2,'bone motion of b animates natively');
  assert.equal(m.anims('c'),0);
  const frames=m.env.frames,writes=m.env.writes;
  m.env.advanceTo(m.env.time+3000);
  assert.ok(m.env.frames-frames<3,'frame loop keeps running: '+(m.env.frames-frames));
  assert.equal(m.env.writes,writes,'DOM writes while offloaded');
  // Loops keep the timeline API: clip time still advances with the clock.
  assert.ok(Math.abs(m.api.clip('sway').time-((m.env.time-1000)/1000)%2)<0.02);
  compare(()=>{});
});

test('the native path matches the JavaScript path through play, pause, seek, stop and one-shot clip.end',()=>{
  const sides=compare((s,t)=>{
    const at=x=>Math.abs(t-x)<1000/120;
    if(at(310))s.api.clip('pop').play();
    if(at(1000))s.api.clip('sway').pause();
    if(at(1400))s.api.clip('sway').seek(0.75);
    if(at(1700))s.api.clip('sway').play();
    if(at(2100))s.api.clip('rig').stop();
    if(at(2610)){s.api.clip('rig').play();s.api.clip('pop').seek(0.2);s.api.clip('pop').play();}
  });
  assert.deepEqual(sides[0].events.map(e=>e[1]),['pop','pop']);
});

test('nodes touched by anything not offloadable fall back to JavaScript',()=>{
  // An explicit set() on a takes all of a's transform back to the JS path; opacity stays native.
  let m=mount();m.env.advanceTo(m.env.time+100);
  m.api.set('a',{x:4});m.env.advanceTo(m.env.time+100);
  assert.ok(m.el('a').animations.every(a=>a.prop==='opacity'),'a keeps only its opacity animation');
  m.env.advanceTo(m.env.time+500);
  assert.ok(Math.abs(m.el('a').shown().m[4]-4)<1e-9,'the explicit x is written by JavaScript');
  compare((s,t)=>{if(t<1)s.api.set('a',{x:4});},project,4000,'explicit');
  // A weighted clip on the rig: the whole bone tree goes to JavaScript.
  const weighted=()=>{const p=project();p.clips.push({id:'mix',duration:1,looping:true,autoplay:true,blendMode:'weighted',weight:0.5,tracks:[track({nodeId:'arm'},'rotation',[15,40],1)]});return p;};
  m=mount({},{},weighted());m.env.advanceTo(m.env.time+100);
  assert.equal(m.anims('b'),0,'b has no native animation');
  assert.ok(m.anims('a')>=2,'unrelated nodes stay native');
  compare(()=>{},weighted,4000,'weighted');
  // A fade (setWeight ramp) on an additive clip moves its tracks to JavaScript while it ramps.
  const additive=()=>{const p=project();p.clips[0].blendMode='additive';return p;};
  // (While the ramp runs, the 25 ms ramp timer also moves the JavaScript pose between frames, so only
  // poses after it are compared.)
  compare((s,t)=>{if(Math.abs(t-500)<1000/120)s.api.clip('sway').fadeTo(0.3,0.6);},additive,4000,'fade',t=>t>=490 && t<1150);
});

test('nativeAnimations:false and missing element.animate keep everything in JavaScript',()=>{
  for(const [options,env] of [[{nativeAnimations:false},{}],[{},{waapi:false}]]) {
    const m=mount(options,env);
    m.env.advanceTo(m.env.time+1000);
    assert.equal(m.env.animateCalls,0);
    for(const id of ['a','b','c'])assert.equal(m.anims(id),0);
    const frames=m.env.frames;m.env.advanceTo(m.env.time+1000);
    assert.ok(m.env.frames-frames>50,'JavaScript path ticks every frame');
    assert.notEqual(m.el('a').getAttribute('transform'),null);
  }
});

test('destroy() cancels native animations and leaves the current pose in the attributes',()=>{
  const m=mount();m.env.advanceTo(m.env.time+700);
  const shown=m.el('b').shown();
  m.api.destroy();
  assert.equal(m.anims('a')+m.anims('b'),0);
  assert.equal(m.el('b').style.translate,'');
  const after=m.el('b').shown();
  for(let k=0;k<6;k++)assert.ok(Math.abs(after.m[k]-shown.m[k])<1e-6,'pose kept after destroy');
});

test('an offscreen or hidden figure stops ticking and resumes with the clock caught up',()=>{
  for(const hide of [(env,v)=>env.setVisible(!v),(env,v)=>env.setDocumentHidden(v)]) {
    const m=mount({nativeAnimations:false});
    m.env.advanceTo(m.env.time+500);
    hide(m.env,true);
    const frames=m.env.frames,writes=m.env.writes;
    m.env.advanceTo(m.env.time+3000);
    assert.equal(m.env.frames,frames,'no frames while hidden');
    assert.equal(m.env.writes,writes,'no DOM writes while hidden');
    // Time keeps running (observed through the API) and events still fire.
    m.api.clip('pop').play();
    m.env.advanceTo(m.env.time+1000);
    assert.equal(m.events.length,1,'clip.end fires while hidden');
    hide(m.env,false);
    m.env.advanceTo(m.env.time+1000/60);
    assert.ok(Math.abs(m.api.clip('sway').time-((m.env.time-1000)/1000)%2)<0.02,'clock caught up');
    assert.ok(m.env.frames>frames);
  }
  // Native animations pause while hidden and resynchronise when shown.
  const m=mount();m.env.advanceTo(m.env.time+500);
  m.env.setVisible(false);
  assert.ok(m.el('a').animations.every(a=>a.playState==='paused'));
  m.env.advanceTo(m.env.time+1234);
  m.env.setVisible(true);m.env.advanceTo(m.env.time+1000/60);
  assert.ok(m.el('a').animations.every(a=>a.playState!=='paused'));
  const js=mount({nativeAnimations:false});js.env.advanceTo(m.env.time);
  for(let k=0;k<6;k++)assert.ok(Math.abs(m.el('a').shown().m[k]-js.el('a').shown().m[k])<1e-6);
  // pauseWhenHidden:false keeps ticking.
  const keep=mount({nativeAnimations:false,pauseWhenHidden:false});
  keep.env.setVisible(false);const f=keep.env.frames;keep.env.advanceTo(keep.env.time+1000);
  assert.ok(keep.env.frames-f>50);
});

test('runtimes in one window share a single requestAnimationFrame loop and keep their own maxFps',()=>{
  const {env,svg}=createEnv(SVG,{io:true,waapi:false});
  const second=env.addSvg(SVG),third=env.addSvg(SVG);
  const apis=[rt.createRuntime(svg,project(),{reducedMotion:false}),rt.createRuntime(second,project(),{reducedMotion:false}),rt.createRuntime(third,project(),{reducedMotion:false,maxFps:20})];
  env.advanceTo(env.time+100);
  const raf=env.rafCalls,frames=env.frames;
  env.advanceTo(env.time+2000);
  assert.ok(env.rafCalls-raf<=env.frames-frames+1,'one rAF per frame for all runtimes: '+(env.rafCalls-raf)+' for '+(env.frames-frames)+' frames');
  // maxFps:20 writes about a third as often as the others.
  const w=e=>e.all.find(x=>x.id==='a');
  let count=[0,0,0];const last=[svg,second,third].map(s=>w(s).getAttribute('transform'));
  for(let i=0;i<120;i++){env.advanceTo(env.time+1000/60);[svg,second,third].forEach((s,k)=>{const v=w(s).getAttribute('transform');if(v!==last[k]){count[k]++;last[k]=v;}});}
  assert.ok(count[0]>100 && count[1]>100,'display-rate runtimes update every frame: '+count);
  assert.ok(count[2]>=35 && count[2]<=45,'maxFps:20 runtime updates at ~20 fps: '+count[2]);
  // Destroying one runtime does not stop the others; destroying all stops the loop.
  apis[0].destroy();
  const v=w(second).getAttribute('transform');env.advanceTo(env.time+100);
  assert.notEqual(w(second).getAttribute('transform'),v);
  apis[1].destroy();apis[2].destroy();
  env.advanceTo(env.time+100);
  assert.equal(env.rafQueue.size,0,'no frame requested after every runtime is destroyed');
});
