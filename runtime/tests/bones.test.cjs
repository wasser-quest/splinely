'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const {createRuntime,normalize}=require('../src/splinely.js');
const number=value=>({type:'number',value});
const bone=(id,extra={})=>({id,x:0,y:0,rotation:0,length:10,...extra});
const node=(extra={})=>({id:'limb',kind:{type:'rect',width:5,height:2},transform:{x:1,y:2,rotation:15,scaleX:2,scaleY:3},style:{opacity:1},...extra});
const rig=(extra={})=>({schemaVersion:1,bones:[bone('rootBone')],nodes:[node({boneId:'rootBone'})],...extra});
const track=(id,property,a,b,easing)=>({nodeId:id,property,keyframes:[{time:0,value:number(a),easing},{time:2,value:number(b)}]});
function element(id,tagName='rect') {
  const attrs=new Map(), listeners=new Map();
  return {id,tagName,attrs,listeners,getAttribute:k=>attrs.has(k)?attrs.get(k):null,setAttribute:(k,v)=>attrs.set(k,String(v)),removeAttribute:k=>attrs.delete(k),
    addEventListener(k,fn){if(!listeners.has(k))listeners.set(k,new Set());listeners.get(k).add(fn);},removeEventListener(k,fn){listeners.get(k)?.delete(fn);}};
}
function root(env={}) {
  const svg=element('scene','svg'), limb=element('limb');limb.parentNode=svg;limb.ownerSVGElement=svg;
  svg.children=[limb];svg.ownerDocument={defaultView:env};
  svg.querySelectorAll=selector=>selector==='[id]'?svg.children:svg.children.filter(n=>n.id==='splinely-project');
  return svg;
}
function mount(p=rig(),env={}) {const svg=root(env);return {svg,limb:svg.children[0],f:createRuntime(svg,p,{autoTick:false})};}
function near(actual,expected) {assert.ok(Math.abs(actual-expected)<1e-9,`${actual} != ${expected}`);}
function transform(limb,expected) {
  const values=limb.getAttribute('transform').match(/[-+]?\d*\.?\d+(?:e[-+]?\d+)?/gi).map(Number);
  assert.equal(values.length,expected.length);values.forEach((v,i)=>near(v,expected[i]));
}

test('canonical FK is order independent, children start at parent tips, attachments stay local',()=>{
  for(const reverse of [false,true]) {
    const bones=[bone('rootBone',{x:10,y:20,rotation:90}),bone('child',{parentId:'rootBone',x:2,y:3,rotation:-90,length:5}),bone('grandchild',{parentId:'child',x:1,y:2,rotation:45,length:2})];
    const p=rig({bones:reverse?bones.reverse():bones,nodes:[node({boneId:'child',locked:true,originalKind:{type:'rawSvg',svg:'<script>bad()</script>'}})]});
    const before=JSON.stringify(p),{f,limb}=mount(p);
    const poses=new Map(f.bones().map(b=>[b.id,b]));
    near(poses.get('rootBone').tipX,10);near(poses.get('rootBone').tipY,30);
    near(poses.get('child').x,7);near(poses.get('child').y,32);near(poses.get('child').rotation,0);
    near(poses.get('child').tipX,12);near(poses.get('child').tipY,32);assert.equal(poses.get('child').length,5);
    near(poses.get('grandchild').x,13);near(poses.get('grandchild').y,34);near(poses.get('grandchild').rotation,45);
    transform(limb,[8,34,15,2,3]);assert.equal(limb.getAttribute('x'),null);assert.equal(limb.getAttribute('width'),'5');
    f.set('child',{rotation:90});transform(limb,[6,30,195,2,3]);
    f.set('child',null);transform(limb,[8,34,15,2,3]);f.tick(0);transform(limb,[8,34,15,2,3]);
    const copy=f.bones();copy[0].x=999;assert.notEqual(f.bones()[0].x,999);
    assert.equal(JSON.stringify(p),before);assert.equal(f.diagnostics.length,0);f.destroy();
  }
});

test('state, clip and explicit bone/node patches compose before one FK pass',()=>{
  const {f,limb}=mount(rig({nodes:[node({transform:{x:0,y:0,rotation:0,scaleX:1,scaleY:1},boneId:'rootBone'})],
    stateMachines:[{id:'machine',initial:'posed',states:[{id:'posed',overrides:[{nodeId:'rootBone',property:'x',value:number(10)},{nodeId:'rootBone',property:'rotation',value:number(90)},{nodeId:'limb',property:'x',value:number(2)}]}]}],
    clips:[{id:'pose',duration:2,tracks:[track('rootBone','rotation',0,180),track('rootBone','y',0,20),track('limb','x',0,4)]},{id:'later',duration:2,tracks:[track('rootBone','x',20,40)]}]}));
  transform(limb,[10,2,90,1,1]);f.clip('pose').seek(1);transform(limb,[10,12,90,1,1]);
  f.clip('later').seek(1);transform(limb,[30,12,90,1,1]);
  f.set('rootBone',{x:number(50),y:30,rotation:180});f.set('limb',{x:3});transform(limb,[47,30,180,1,1]);
  f.set('rootBone',null);f.set('limb',null);transform(limb,[30,12,90,1,1]);
  f.clip('later').stop();f.clip('pose').stop();transform(limb,[10,2,90,1,1]);f.destroy();
});

test('bone rotations interpolate numerically (not shortest arc) with existing easing and transport',()=>{
  const {f}=mount(rig({clips:[{id:'pose',duration:2,looping:true,tracks:[track('rootBone','rotation',350,10)]}]}));
  const c=f.clip('pose');c.seek(1);near(f.bones()[0].rotation,180);c.play();f.tick(0.5);near(f.bones()[0].rotation,95);
  c.pause();f.tick(3);near(f.bones()[0].rotation,95);c.seek(0).play();f.tick(2.5);near(f.bones()[0].rotation,265);
  f.timeline.pause();f.timeline.seek(1);near(f.bones()[0].rotation,180);c.stop();near(f.bones()[0].rotation,0);f.destroy();
  for(const [easing,time,expected] of [[{type:'step'},1,0],[{type:'step'},2,100],[{type:'cubicBezier',x1:0.25,y1:0.1,x2:0.25,y2:1},1,80.2403387584857]]) {
    const {f:g}=mount(rig({clips:[{id:'ease',duration:2,tracks:[track('rootBone','rotation',0,100,easing)]}]}));
    g.clip('ease').seek(time);near(g.bones()[0].rotation,expected);g.destroy();
  }
});

test('malformed rigs fail before mount: IDs, parents, attachments, cycles, finite geometry and overflow',()=>{
  const cases=[
    [rig({bones:[bone('b'),bone('b')]}),/duplicate bone/],
    [rig({bones:[bone('limb')],nodes:[] ,clips:[{id:'limb',duration:1}]}),/duplicate global/],
    [rig({bones:[bone('rootBone',{parentId:'missing'})]}),/Missing bone parent/],
    [rig({bones:[bone('rootBone',{parentId:'rootBone'})]}),/cycle/],
    [rig({bones:[bone('rootBone',{parentId:'b'}),bone('b',{parentId:'rootBone'})]}),/cycle/],
    [rig({nodes:[node({boneId:'missing'})]}),/Missing node bone/],
    [rig({bones:[bone('rootBone',{length:0})]}),/positive/],
    [rig({bones:[bone('rootBone',{length:-1})]}),/positive/],
    [rig({bones:[bone('rootBone',{x:NaN})]}),/finite/],
    [rig({bones:[bone('rootBone',{rotation:Infinity})]}),/finite/],
    [rig({bones:[bone('rootBone',{length:Infinity})]}),/finite/],
    [rig({bones:[bone('rootBone',{x:1e308,length:1e308})]}),/finite/]
  ];
  for(const [p,error] of cases) {const svg=root();assert.throws(()=>createRuntime(svg,p,{autoTick:false}),error);assert.equal(svg.splinely,undefined);}
  for(const reverse of [false,true]) {
    const bones=Array.from({length:128},(_,i)=>bone('b'+i,{parentId:i?'b'+(i-1):null}));
    assert.equal(normalize({bones:reverse?[...bones].reverse():bones}).bones.length,128);
    bones.push(bone('b128',{parentId:'b127'}));assert.throws(()=>normalize({bones:reverse?bones.reverse():bones}),/depth limit/);
  }
});

test('bone patches use numeric allowlist for API, keyframes, states and actions',()=>{
  const {f,limb}=mount();const before=limb.getAttribute('transform');
  for(const props of [{length:2},{parentId:'other'},{fill:'red'},{onclick:'bad'},{x:'1'},{y:Infinity},{rotation:{type:'color',value:42}},{rotation:{type:'number',value:'42'}}]) assert.throws(()=>f.set('rootBone',props),/Unsupported bone property|numeric|finite/);
  assert.equal(limb.getAttribute('transform'),before);
  f.set('rootBone',{rotation:number(90)});transform(limb,[-2,1,105,2,3]);
  f.set('limb',{transform:'translate(999 999)',onclick:'bad',fill:'url(https://invalid)'});transform(limb,[-2,1,105,2,3]);assert.equal(limb.getAttribute('onclick'),null);assert.equal(limb.getAttribute('fill'),null);
  assert.ok(f.diagnostics.some(e=>/Raw transform/.test(e.message)));f.destroy();
  for(const value of [{type:'color',value:42},number(Infinity),'42']) {
    const t=track('rootBone','rotation',0,1);t.keyframes[0].value=value;
    assert.throws(()=>normalize(rig({clips:[{id:'bad',duration:2,tracks:[t]}]})),/numeric|finite/);
    assert.throws(()=>normalize(rig({stateMachines:[{id:'m',initial:'s',states:[{id:'s',overrides:[{nodeId:'rootBone',property:'rotation',value}]}]}]})),/numeric|finite/);
  }
  const {f:g}=mount(rig({stateMachines:[{id:'m',initial:'s',states:[{id:'s',onEnter:[{type:'setProperty',nodeId:'rootBone',property:'rotation',value:{type:'color',value:42}}]}]}]}));
  assert.equal(g.bones()[0].rotation,0);assert.match(g.diagnostics[0].message,/numeric/);g.destroy();
});

test('FK overflow keeps last valid frame, diagnoses safely and can recover',()=>{
  const {f:g,limb:n}=mount(rig({bones:[bone('rootBone',{x:1e308,length:1}),bone('child',{parentId:'rootBone',length:1})]}));
  const valid=n.getAttribute('transform'),validPoses=g.bones();g.set('child',{x:1e308});
  assert.equal(n.getAttribute('transform'),valid);assert.deepEqual(g.bones(),validPoses);assert.ok(g.diagnostics.some(e=>/finite/.test(e.message)));
  g.set('child',null);assert.equal(g.bones().length,2);g.destroy();
});

test('nested SVG IDs stay scoped and destroying from an error stops rendering',()=>{
  const svg=root(),nested=root(),other=nested.children[0];nested.parentNode=svg;svg.children.push(other);
  const f=createRuntime(svg,rig(),{autoTick:false});assert.equal(other.getAttribute('transform'),null);
  const limb=svg.children[0],before=limb.getAttribute('opacity');f.on('error',()=>f.destroy());
  f.set('limb',{scaleX:Infinity,opacity:0.25});assert.equal(f.destroyed,true);assert.equal(limb.getAttribute('opacity'),before);assert.equal(svg.splinely,undefined);
  assert.ok([...svg.listeners.values()].every(s=>s.size===0));assert.throws(()=>f.bones(),/destroyed/);assert.throws(()=>f.set('rootBone',{x:0}),/destroyed/);f.destroy();
});

test('old no-bones documents still render and invalid auto-mounted rigs retain static fallback',()=>{
  const {f,limb}=mount({schemaVersion:1,nodes:[node()],clips:[]});assert.deepEqual(f.bones(),[]);transform(limb,[1,2,15,2,3]);f.destroy();
  const errors=[],env={console:{error:e=>errors.push(e)}},svg=root(env),metadata=element('splinely-project','metadata');metadata.parentNode=svg;
  metadata.textContent=JSON.stringify(rig({bones:[bone('rootBone',{parentId:'missing'})]}));svg.children.push(metadata);svg.children[0].setAttribute('transform','translate(42 0)');
  env.document={currentScript:{ownerSVGElement:svg}};env.window=env;
  vm.runInNewContext(fs.readFileSync(require.resolve('../src/splinely.js'),'utf8'),env);
  assert.equal(svg.splinely,undefined);assert.equal(svg.children[0].getAttribute('transform'),'translate(42 0)');assert.equal(errors.length,1);
});
