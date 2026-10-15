'use strict';
// Hot-path behaviour: idle ticks are free, unchanged attributes are never rewritten, the project JSON
// is read from <metadata> once and exposed as api.sourceProject, and compiled tracks match evaluateTrack.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const rt=require('../src/splinely.js');
const ROOT=path.join(__dirname,'..','..');

class Element {
  constructor(id){this.id=id;this.attrs=new Map();this.calls=0;this.parentNode=null;this.children=[];}
  getAttribute(k){this.calls++;return this.attrs.has(k)?this.attrs.get(k):null;}
  setAttribute(k,v){this.calls++;this.attrs.set(k,String(v));}
  removeAttribute(k){this.calls++;this.attrs.delete(k);}
  addEventListener(){}removeEventListener(){}
}
function dom(project,metadata){
  const svg=new Element('root'),els=project.nodes.map(n=>{const e=new Element(n.id);e.ownerSVGElement=svg;return e;});
  svg.ownerDocument={defaultView:{}};
  let reads=0,all=metadata?[{get textContent(){reads++;return metadata;},ownerSVGElement:svg,parentNode:svg}]:[];
  svg.querySelectorAll=sel=>sel==='[id]'?els:sel.includes('splinely-project')?all:[];
  svg.querySelector=()=>null;
  return {svg,els,reads:()=>reads,calls:()=>els.reduce((s,e)=>s+e.calls,0)};
}
const load=name=>JSON.parse(fs.readFileSync(path.join(ROOT,'examples',name+'.splinely.json'),'utf8'));

test('idle ticks perform no DOM reads or writes',()=>{
  const project=load('desert-stickfight'),d=dom(project);
  const api=rt.createRuntime(d.svg,project,{autoTick:false});
  for(const c of project.clips)api.clip(c.id).play();
  api.tick(0.25);
  for(const c of project.clips)api.clip(c.id).stop();
  const before=d.calls();
  for(let i=0;i<100;i++)api.tick(1/60);
  assert.equal(d.calls(),before);
});

test('a playing clip only rewrites attributes whose values changed',()=>{
  const project={schemaVersion:1,nodes:[{id:'a',kind:{type:'circle',cx:1,cy:2,radius:3}},{id:'b',kind:{type:'circle',cx:1,cy:2,radius:3}}],
    clips:[{id:'c',duration:10,looping:true,tracks:[{nodeId:'a',property:'x',keyframes:[{time:0,value:0},{time:10,value:100}]},{nodeId:'b',property:'x',keyframes:[{time:0,value:5}]}]}]};
  const d=dom(project),api=rt.createRuntime(d.svg,project,{autoTick:false});
  api.clip('c').play();api.tick(0.1);
  const b=d.els[1];const before=b.calls;
  for(let i=0;i<50;i++)api.tick(0.1);
  assert.equal(b.calls,before,'constant track must not touch its node');
  assert.ok(d.els[0].calls>before,'animated node keeps updating');
});

test('project JSON is read from <metadata> once and exposed as sourceProject',()=>{
  const project=load('demo'),text=JSON.stringify(project),d=dom(project,text);
  const api=rt.createRuntime(d.svg,null,{autoTick:false});
  assert.equal(d.reads(),1);
  assert.deepEqual(api.sourceProject,project);
  const given=load('rig'),d2=dom(given),api2=rt.createRuntime(d2.svg,given,{autoTick:false});
  assert.equal(api2.sourceProject,given);
});

test('compiled keyframe lookup agrees with evaluateTrack across many keyframes',()=>{
  const frames=[];for(let i=0;i<40;i++)frames.push({time:i*0.05,value:{type:'number',value:Math.sin(i)*10},easing:i%3===0?'step':i%3===1?[0.2,0.1,0.7,1]:'linear'});
  const project={schemaVersion:1,nodes:[{id:'n',kind:{type:'circle',cx:0,cy:0,radius:1}}],clips:[{id:'c',duration:2,tracks:[{nodeId:'n',property:'x',keyframes:frames}]}]};
  const d=dom(project),api=rt.createRuntime(d.svg,project,{autoTick:false});
  const track=rt.normalize(project).clips[0].tracks[0];
  for(let i=0;i<=400;i++){
    const t=i*0.005;api.clip('c').seek(t);
    const v=rt.evaluateTrack(track,t),m=/translate\(([^ ]+) /.exec(d.els[0].getAttribute('transform'));
    assert.equal(Number(m[1]),v,`t=${t}`);
  }
});
