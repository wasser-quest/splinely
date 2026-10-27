'use strict';
// Hosts (Locora's bridge) forward the pointer as synthetic DOM events, often many per frame and to figures
// that are off screen. Events nobody consumes must not tick or draw; hidden figures stay asleep; followers
// still end where they would have.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const rt=require('../src/splinely.js');
const {createEnv}=require('./fake-waapi.cjs');

const SVG='<svg viewBox="0 0 100 100"><circle id="body" cx="0" cy="0" r="1"/><circle id="eye" cx="0" cy="0" r="1"/><circle id="lid" cx="0" cy="0" r="1"/></svg>';
const circle=(id,extra)=>({id,kind:{type:'circle',cx:0,cy:0,radius:1},...extra});
const num=v=>({type:'number',value:v});
const track=(target,property,values,dur)=>({...target,property,keyframes:values.map((v,i)=>({time:dur*i/(values.length-1),value:num(v),easing:{type:'linear'}}))});
// Like the Locora mascots: a breathing bone, an eye follower bound to it, a blink machine on timers.
function project(extra={}){
  return {schemaVersion:1,
    bones:[{id:'torso',x:50,y:50,rotation:0,length:10}],
    nodes:[circle('body',{boneId:'torso'}),circle('eye',{boneId:'torso',transform:{x:2,y:-3,rotation:0,scaleX:1,scaleY:1}}),circle('lid',{boneId:'torso'})],
    clips:[{id:'breathe',duration:2,looping:true,autoplay:true,tracks:[track({nodeId:'torso'},'y',[50,53,50],2)]}],
    followers:[{id:'look',target:'eye',mode:'offset',max:3,smoothing:0.12,strength:0.12}],
    stateMachines:[{id:'blink',initial:'open',states:[
      {id:'open',transitions:[{to:'shut',after:3.8}]},
      {id:'shut',overrides:[{nodeId:'lid',property:'opacity',value:0}],transitions:[{to:'open',after:0.13}]},
    ]}],
    ...extra};
}
function mount(options={},proj=project()){
  const {env,svg}=createEnv(SVG,{io:true});
  const api=rt.createRuntime(svg,proj,{reducedMotion:false,maxFps:30,...options});
  const el=id=>svg.all.find(e=>e.id===id);
  return {env,svg,api,el};
}
const pose=m=>['body','eye','lid'].map(id=>m.el(id).shown());
function same(a,b,label){
  for(let i=0;i<a.length;i++){
    assert.ok(Math.abs(a[i].opacity-b[i].opacity)<1e-6,label+': opacity of node '+i);
    for(let k=0;k<6;k++)assert.ok(Math.abs(a[i].m[k]-b[i].m[k])<1e-6,label+': matrix of node '+i+' '+a[i].m+' vs '+b[i].m);
  }
}

test('pointer events nobody consumes draw at most once per frame, with the same pose as one event per frame',()=>{
  for(const nativeAnimations of [true,false]) {
    const flood=mount({nativeAnimations}), single=mount({nativeAnimations});
    for(const m of [flood,single])m.env.advanceTo(m.env.time+500);
    let between=0;
    for(let f=0;f<60;f++) {
      const x=20+f,y=30-f/2;
      const before=flood.env.writes;
      // Ten moves land between two frames: none of them may write.
      for(let i=0;i<10;i++)flood.svg.fire('pointermove',{clientX:x-(9-i)*0.1,clientY:y});
      between+=flood.env.writes-before;
      single.svg.fire('pointermove',{clientX:x,clientY:y});
      for(const m of [flood,single])m.env.advanceTo(m.env.time+1000/60);
    }
    assert.equal(between,0,'no DOM writes from forwarded pointer events (nativeAnimations '+nativeAnimations+')');
    same(pose(flood),pose(single),'nativeAnimations '+nativeAnimations);
    // The follower did move (it runs in JavaScript), and the rest stays native.
    assert.notDeepEqual(flood.el('eye').shown().m,mount({nativeAnimations}).el('eye').shown().m);
    if(nativeAnimations)assert.ok(flood.el('body').animations.length>0,'the bone still animates natively');
  }
});

test('a hidden figure does no work for forwarded pointer events and its followers catch up once visible',()=>{
  const m=mount(), ref=mount();
  for(const s of [m,ref])s.env.advanceTo(s.env.time+500);
  m.env.setVisible(false);
  const frames=m.env.frames, writes=m.env.writes;
  for(let i=0;i<300;i++){
    m.svg.fire(i%50===0?'pointerenter':'pointermove',{clientX:1e9*(i%2?1:-1),clientY:-2e9});
    if(i%3===0)m.env.advanceTo(m.env.time+1000/60);
  }
  m.svg.fire('pointermove',{clientX:90,clientY:10});
  m.env.advanceTo(m.env.time+2000);
  assert.equal(m.env.frames,frames,'no frames while hidden');
  assert.equal(m.env.writes,writes,'no DOM writes while hidden');
  // The blink timers kept running: the machine is where the visible reference is.
  ref.svg.fire('pointermove',{clientX:90,clientY:10});
  ref.env.advanceTo(m.env.time);
  assert.equal(m.api.machine('blink').state,ref.api.machine('blink').state);
  assert.equal(m.env.writes,writes,'an API read while hidden draws nothing either');
  m.env.setVisible(true);
  for(const s of [m,ref])s.env.advanceTo(s.env.time+3000);
  same(pose(m),pose(ref),'after becoming visible');
});

test('consumed pointer events still reach subscriptions, interactions and machine transitions',()=>{
  const proj=project({
    stateMachines:[{id:'mood',initial:'calm',states:[
      {id:'calm',transitions:[{to:'curious',event:{name:'pointermove',target:'eye'}}]},
      {id:'curious',overrides:[{nodeId:'lid',property:'opacity',value:0.5}]},
    ]}],
  });
  const m=mount({},proj);
  m.env.advanceTo(m.env.time+100);
  const seen=[];
  m.api.on('pointerdown',d=>seen.push(d.target));
  m.svg.fire('pointerdown',{clientX:1,clientY:1});
  assert.equal(seen.length,1);
  // A machine listening for pointermove on the eye still switches state.
  m.svg.fire('pointermove',{clientX:2,clientY:2,target:m.el('eye')});
  assert.equal(m.api.snapshot().states.mood,'curious');
  m.env.advanceTo(m.env.time+100);
  assert.ok(Math.abs(m.el('lid').shown().opacity-0.5)<1e-6);
});
