'use strict';
// Guard: interactions, state-machine transitions and timers never depend on the
// timeline transport (no Play button / host start). Example pages rely on this.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {createRuntime}=require('../src/splinely.js');
class Element {
  constructor(id,tag='circle'){this.id=id;this.tagName=tag;this.attrs=new Map();this.listeners=new Map();this.parentNode=null;}
  getAttribute(k){return this.attrs.has(k)?this.attrs.get(k):null;}
  setAttribute(k,v){this.attrs.set(k,String(v));}
  removeAttribute(k){this.attrs.delete(k);}
  addEventListener(k,fn){if(!this.listeners.has(k))this.listeners.set(k,new Set());this.listeners.get(k).add(fn);}
  removeEventListener(k,fn){this.listeners.get(k)?.delete(fn);}
  fire(type,event={}){for(const fn of this.listeners.get(type)||[])fn({target:this,type,...event});}
}
function mount(project,ids=['n']){
  const svg=new Element('root','svg');const children=ids.map(id=>new Element(id));
  for(const e of children){e.ownerSVGElement=svg;e.parentNode=svg;}
  svg.ownerDocument={defaultView:{}};svg.children=children;
  svg.querySelectorAll=s=>s==='[id]'?children:[];svg.querySelector=()=>null;
  svg.getScreenCTM=()=>({inverse:()=>({})});svg.createSVGPoint=()=>({x:0,y:0,matrixTransform(){return {x:0,y:0};}});
  return {f:createRuntime(svg,project,{autoTick:false}),svg,n:children[0]};
}
const frame=(time,value)=>({time,value,easing:{type:'linear'}});
const number=value=>({type:'number',value});
const clip=(extra={})=>({id:'a',duration:2,tracks:[{target:'n',property:'cx',keyframes:[frame(0,0),frame(2,20)]}],...extra});
const node={id:'n',kind:{type:'circle',cx:5,cy:5,radius:2},transform:{x:0,y:0,rotation:0,scaleX:1,scaleY:1}};

test('pointer events, interaction bindings and machine transitions work while the timeline never played',()=>{
  const {f,svg,n}=mount({schemaVersion:1,nodes:[node],clips:[clip()],
    interactions:[{target:'n',event:'click',actions:[{type:'playClip',clipId:'a'}]}],
    stateMachines:[{id:'m',initial:'off',states:[{id:'off'},{id:'on',overrides:[{nodeId:'n',property:'opacity',value:number(0.5)}]}],
      transitions:[{from:'off',to:'on',event:{name:'click',target:'n'}}]}]});
  assert.equal(f.timeline.playing,false);assert.equal(f.clip('a').playing,false);
  const seen=[];f.on('n','click',()=>seen.push('click'));
  svg.fire('click',{target:n});
  assert.deepEqual(seen,['click']);
  assert.equal(f.machine('m').state,'on');
  assert.equal(f.clip('a').playing,true,'an interaction may itself start the timeline');
  f.tick(1);assert.equal(f.clip('a').time,1);
  assert.deepEqual(f.diagnostics,[]);f.destroy();
});

test('timer transitions fire without any transport and initial onEnter playClip self-starts',()=>{
  const {f}=mount({schemaVersion:1,nodes:[node],clips:[clip()],
    stateMachines:[{id:'m',initial:'idle',states:[{id:'idle'},{id:'busy'}],transitions:[{from:'idle',to:'busy',after:2}]},
      {id:'show',initial:'first',states:[{id:'first',onEnter:[{type:'playClip',clipId:'a'}]},{id:'done'}],transitions:[{from:'first',to:'done',event:{name:'clip.end'}}]}]});
  assert.equal(f.timeline.playing,true,'initial onEnter playClip starts the timeline on mount');
  assert.equal(f.machine('show').state,'first');
  f.tick(1.9);assert.equal(f.machine('m').state,'idle');
  f.tick(0.2);assert.equal(f.machine('m').state,'busy');
  f.tick(1);assert.equal(f.machine('show').state,'done');
  f.destroy();
});

test('an idle machine with no autoplay clip still reacts to events and keeps the timeline stopped',()=>{
  const {f}=mount({schemaVersion:1,nodes:[node],clips:[clip()],
    stateMachines:[{id:'m',initial:'a',states:[{id:'a'},{id:'b'}],transitions:[{from:'a',to:'b',event:{name:'go'}}]}]});
  assert.equal(f.timeline.playing,false);
  f.emit('go');assert.equal(f.machine('m').state,'b');
  assert.equal(f.timeline.playing,false);f.destroy();
});
