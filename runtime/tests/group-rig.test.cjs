'use strict';
// Groups animate as one piece through a move → turn → attach bone chain
// (`Project::rig_group`); `set(groupId, …)` drives that chain.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {createRuntime}=require('../src/splinely.js');
const number=value=>({type:'number',value});
const bone=(id,extra={})=>({id,x:0,y:0,rotation:0,length:1,...extra});
// Group centre (100, 50); the rig is the identity at rest.
const rigBones=()=>[bone('g-move'),bone('g-turn',{parentId:'g-move',x:99,y:50}),bone('g-attach',{parentId:'g-turn',x:-101,y:-50})];
const rigged=(extra={})=>({schemaVersion:1,bones:rigBones(),
  groups:[{id:'g',name:'G',rig:{moveBone:'g-move',turnBone:'g-turn',attachBone:'g-attach'}}],
  nodes:[{id:'limb',group:'g',boneId:'g-attach',kind:{type:'rect',width:5,height:2},transform:{x:110,y:50,rotation:0,scaleX:1,scaleY:1},style:{opacity:1}}],...extra});
function element(id,tagName='rect') {
  const attrs=new Map();
  return {id,tagName,attrs,getAttribute:k=>attrs.has(k)?attrs.get(k):null,setAttribute:(k,v)=>attrs.set(k,String(v)),removeAttribute:k=>attrs.delete(k),addEventListener(){},removeEventListener(){}};
}
function mount(p) {
  const svg=element('scene','svg'), limb=element('limb');limb.parentNode=svg;limb.ownerSVGElement=svg;
  svg.children=[limb];svg.ownerDocument={defaultView:{}};
  svg.querySelectorAll=selector=>selector==='[id]'?svg.children:[];
  return {limb,f:createRuntime(svg,p,{autoTick:false})};
}
function transform(limb,expected) {
  const values=limb.getAttribute('transform').match(/[-+]?\d*\.?\d+(?:e[-+]?\d+)?/gi).map(Number);
  assert.equal(values.length,expected.length);
  values.forEach((v,i)=>assert.ok(Math.abs(v-expected[i])<1e-9,`${values} != ${expected}`));
}

test('a group rig is the identity at rest and turns around the group centre',()=>{
  const {f,limb}=mount(rigged());
  transform(limb,[110,50,0,1,1]);
  f.set('g',{rotation:90});transform(limb,[100,60,90,1,1]);
  f.set('g',{x:5,y:-10});transform(limb,[105,50,90,1,1]);
  f.set('g',null);transform(limb,[110,50,0,1,1]);
  assert.equal(f.diagnostics.length,0);f.destroy();
});

test('group clips and states animate through the rig bones',()=>{
  const {f,limb}=mount(rigged({
    clips:[{id:'spin',duration:2,looping:false,autoplay:true,tracks:[{nodeId:'g-turn',property:'rotation',keyframes:[{time:0,value:number(0),easing:{type:'linear'}},{time:2,value:number(180)}]}]}],
    stateMachines:[{id:'m',initial:'up',states:[{id:'up',overrides:[{nodeId:'g-move',property:'y',value:number(-20)}]}],transitions:[]}]}));
  f.tick(1);transform(limb,[100,40,90,1,1]);
  f.destroy();
});

test('group patches accept only x, y and rotation; groups need a rig',()=>{
  const p=rigged();p.groups.push({id:'plain',name:'Plain'});
  const {f}=mount(p);
  assert.throws(()=>f.set('g',{opacity:0.5}),/Unsupported bone property/);
  assert.throws(()=>f.set('plain',{x:1}),/has no rig/);
  f.destroy();
});

test('broken group rigs are rejected',()=>{
  const missing=rigged();missing.groups[0].rig.turnBone='nope';
  assert.throws(()=>mount(missing),/rig needs existing/);
  const unchained=rigged();unchained.bones[2].parentId='g-move';
  assert.throws(()=>mount(unchained),/chained move → turn → attach/);
});
