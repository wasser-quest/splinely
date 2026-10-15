'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {createRuntime,normalize}=require('../src/splinely.js');
const fixture=require('./fixtures/blend-spaces.json');
const clone=()=>JSON.parse(JSON.stringify(fixture));
const near=(a,b)=>assert.ok(Math.abs(Number(a)-b)<1e-10,`${a} != ${b}`);
function mount(p=clone()) {
  const attrs=new Map();
  const node={id:'n',tagName:'rect',getAttribute:k=>attrs.get(k)??null,setAttribute:(k,v)=>attrs.set(k,String(v)),removeAttribute:k=>attrs.delete(k)};
  const svg={tagName:'svg',querySelectorAll:()=>[node],addEventListener(){},removeEventListener(){}};node.parentNode=svg;
  const f=createRuntime(svg,p,{autoTick:false});
  const pose=()=>attrs.get('transform').match(/[-+]?\d*\.?\d+(?:e[-+]?\d+)?/gi).map(Number);
  return {f,attrs,pose};
}
test('shared Rust fixture: additive reference, 2D factors, sparse residual, RGBA, bones, patches',()=>{
  const p=clone(),before=JSON.stringify(p),{f,attrs,pose}=mount(p);
  near(pose()[0],42);near(pose()[1],32);near(pose()[2],15);near(pose()[3],1.25);
  near(attrs.get('opacity'),0.6);near(attrs.get('fill-opacity'),0.75);assert.equal(attrs.get('fill'),'rgb(102,77,204)');
  near(f.bones()[0].x,5);
  f.variable('speed').set(0);near(pose()[0],32);near(pose()[1],42);
  f.variable('speed').set(2);near(pose()[0],52);near(pose()[1],22);
  f.variable('speed').set(1);f.clip('walk').setWeight(0.5);near(pose()[0],39.5);
  f.set('n',{x:99});near(pose()[0],99);f.set('n',null);near(pose()[0],39.5);
  assert.equal(f.diagnostics.length,0);assert.equal(JSON.stringify(p),before);f.destroy();
});
test('live parameter changes and additive fades match Rust; bad changes are atomic',()=>{
  const {f,pose}=mount();
  f.clip('offset').fadeTo(0,1);f.tick(0.5);near(pose()[0],41);near(f.bones()[0].x,4);
  f.clip('offset').pause();f.tick(0.5);near(pose()[0],40);
  for(const value of [null,'bad',NaN,Infinity,10000001])assert.throws(()=>f.variable('speed').set(value));
  near(f.variable('speed').get(),1);near(pose()[0],40);
  f.clip('offset').stop();f.clip('walk').stop();f.clip('run').stop();near(pose()[0],20);
  f.variable('speed').set(0);near(pose()[0],20); // space does not activate anything
  assert.equal(f.diagnostics.length,0);f.destroy();
});
test('additive boundary order, absent stroke and clamps',()=>{
  const p=clone();delete p.blendSpaces;p.clips[0].weight=.5;p.clips[1].weight=.5;
  [p.clips[1],p.clips[2]]=[p.clips[2],p.clips[1]];
  p.stateMachines[0].states[0].overrides.push({nodeId:'n',property:'opacity',value:{type:'number',value:0}});
  p.clips[1].tracks.push({nodeId:'n',property:'stroke',keyframes:[{time:0,value:{type:'color',value:{r:1,g:0,b:0,a:1}}}]});
  const {f,pose,attrs}=mount(p);
  near(pose()[0],38.5);near(attrs.get('opacity'),0);near(attrs.get('stroke-opacity'),.5);
  assert.equal(f.diagnostics.length,0);f.destroy();
});
test('strict new validation: inactive clips, types, sample ownership, actions, IDs',()=>{
  const cases=[
    p=>p.blendSpaces=null,
    p=>p.blendSpaces[0].unknown=1,
    p=>p.blendSpaces[0].samples[0].unknown=1,
    p=>p.blendSpaces[0].id='not valid',
    p=>p.blendSpaces[0].parameters=['speed','speed'],
    p=>p.blendSpaces[0].samples[1].position=[.0000001,0],
    p=>p.blendSpaces[0].samples[1].clipId='offset',
    p=>p.blendSpaces[0].samples[1].clipId='walk',
    p=>p.blendSpaces[0].samples=[],
    p=>p.variables.speed='bad',
    p=>p.variables.speed=10000001,
    p=>p.clips[2].tracks[0].keyframes[0].value={type:'color',value:{r:0,g:0,b:0,a:1}},
    p=>p.clips[2].tracks[5].keyframes[0].value={type:'number',value:1},
    p=>p.clips[2].tracks[0].property='gradient',
    p=>p.clips[2].tracks[0].keyframes[0].value={type:'number',value:1e8},
    p=>p.stateMachines[0].states[0].onEnter=[{type:'setVariable',name:'speed',value:null}],
    p=>p.audioAssets=[{id:'locomotion',data:'',mime:'audio/wav'}],
  ];
  for(const mutate of cases){const p=clone();mutate(p);assert.throws(()=>normalize(p));}
});
test('outside hull and single-point spaces stay finite and normalized',()=>{
  const {f,pose}=mount();
  f.variable('speed').set(4);near(pose()[0],48);
  f.variable('speed').set(1);f.variable('turn').set(100);near(pose()[0],42);
  f.variable('speed').set(1e-200);f.variable('turn').set(0);near(pose()[0],32);
  assert.equal(f.diagnostics.length,0);f.destroy();
  const p=clone();p.blendSpaces[0].samples.pop();p.clips.splice(1,1);
  const m=mount(p);m.f.variable('speed').set(123);near(m.pose()[0],32);m.f.destroy();
});
test('review regressions: duplicate tracks, deep-copied spaces, variable.set validation',()=>{
  for(const mode of ['additive','weighted']){
    const p=clone();const c=p.clips.find(c=>c.blendMode===mode);
    c.tracks.push(JSON.parse(JSON.stringify(c.tracks[0])));
    assert.throws(()=>normalize(p),/duplicate property track/);
  }
  const p=clone();p.clips[2].tracks[0].keyframes[0].value={type:'number',value:1e7+1};
  assert.throws(()=>normalize(p),/out of range/);
  const q=clone(),n=normalize(q);
  assert.notEqual(n.blendSpaces[0],q.blendSpaces[0]);
  assert.notEqual(n.blendSpaces[0].parameters,q.blendSpaces[0].parameters);
  assert.notEqual(n.blendSpaces[0].samples[0],q.blendSpaces[0].samples[0]);
  assert.notEqual(n.blendSpaces[0].samples[0].position,q.blendSpaces[0].samples[0].position);
  q.blendSpaces[0].samples[0].position[0]=5;q.blendSpaces[0].parameters[0]='turn';
  assert.deepEqual(n.blendSpaces[0].samples[0].position,[0,0]);assert.equal(n.blendSpaces[0].parameters[0],'speed');
  for(const action of [{type:'variable.set',variable:'speed',value:null},{type:'variable.set',name:'speed',value:'bad'},{type:'setVariable',name:'turn',value:1e8}]){
    const r=clone();r.stateMachines[0].states[0].onEnter=[action];assert.throws(()=>normalize(r));
    const i=clone();i.interactions=[{actions:[action]}];assert.throws(()=>normalize(i));
  }
  const ok=clone();ok.stateMachines[0].states[0].onEnter=[{type:'variable.set',variable:'speed',value:2},{type:'variable.set',variable:'unrelated',value:null}];
  normalize(ok);
});
