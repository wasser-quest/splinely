'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {createRuntime,normalize}=require('../src/forma.js');
const num=value=>({type:'number',value});
const rgba=(r,g,b,a)=>({type:'color',value:{r,g,b,a}});
const track=(nodeId,property,value)=>({nodeId,property,keyframes:[{time:0,value}]});
const weighted=(id,weight,tracks)=>({id,duration:2,blendMode:'weighted',weight,tracks});
function mount(extra={},options={}) {
  const attrs=new Map(),listeners=new Map();
  const node={id:'n',tagName:'rect',getAttribute:k=>attrs.get(k)??null,setAttribute:(k,v)=>attrs.set(k,String(v)),removeAttribute:k=>attrs.delete(k)};
  const svg={id:'scene',tagName:'svg',querySelectorAll:()=>[node],addEventListener:(k,v)=>listeners.set(k,v),removeEventListener:k=>listeners.delete(k)};node.parentNode=svg;
  const p={schemaVersion:1,nodes:[{id:'n',properties:{x:10,y:20,fill:{r:0,g:0,b:1,a:1}}}],...extra};
  const before=JSON.stringify(p),f=createRuntime(svg,p,{autoTick:false,...options});
  return {f,attrs,p,before,svg};
}
const near=(a,b)=>assert.ok(Math.abs(Number(a)-b)<1e-10,`${a} != ${b}`);
test('spec residual/midpoint/normalized overweight, sparse channels, immutable defaults',()=>{
  const {f,attrs,p,before}=mount({clips:[weighted('a',0.25,[track('n','x',num(30)),track('n','y',num(40))]),weighted('b',0.25,[track('n','x',num(50))])]});
  f.clip('a').seek(0);f.clip('b').seek(0);near(attrs.get('x'),25);near(attrs.get('y'),25);
  f.clip('a').setWeight(0.5);f.clip('b').setWeight(0.5);near(attrs.get('x'),40);
  f.clip('a').setWeight(1);f.clip('b').setWeight(1);near(attrs.get('x'),40);near(attrs.get('y'),40);
  f.clip('a').setWeight(0);f.clip('b').setWeight(0);near(attrs.get('x'),10);near(attrs.get('y'),20);
  assert.equal(JSON.stringify(p),before);assert.equal(normalize({clips:[{id:'c',duration:1}]}).clips[0].blendMode,'override');assert.equal(normalize({clips:[{id:'c',duration:1}]}).clips[0].weight,1);f.destroy();
});
test('mixed override boundaries, state baseline and explicit patches last',()=>{
  const {f,attrs}=mount({stateMachines:[{id:'m',initial:'s',states:[{id:'s',overrides:[{nodeId:'n',property:'x',value:num(20)}]}]}],clips:[weighted('a',0.5,[track('n','x',num(40))]),{id:'o',duration:2,tracks:[track('n','y',num(70))]},weighted('b',0.5,[track('n','x',num(60))])]});
  for(const id of ['a','o','b'])f.clip(id).seek(0);near(attrs.get('x'),45);near(attrs.get('y'),70);
  f.set('n',{x:99});near(attrs.get('x'),99);f.set('n',null);near(attrs.get('x'),45);f.destroy();
});
test('RGBA channels and positive absent-stroke transparent baseline; zero preserves absence',()=>{
  const {f,attrs}=mount({clips:[weighted('a',0.5,[track('n','fill',rgba(1,0,0,0.5)),track('n','stroke',rgba(1,0,0,1))])]});
  f.clip('a').seek(0);assert.equal(attrs.get('fill'),'rgb(128,0,128)');near(attrs.get('fill-opacity'),0.75);assert.equal(attrs.get('stroke'),'rgb(128,0,0)');near(attrs.get('stroke-opacity'),0.5);
  f.clip('a').setWeight(0);assert.equal(attrs.has('stroke'),false);assert.equal(attrs.has('stroke-opacity'),false);f.destroy();
});
test('local bones weighted before FK, numeric rotation and sparse node channels',()=>{
  const {f,attrs}=mount({bones:[{id:'root',x:0,y:0,rotation:0,length:10},{id:'child',parentId:'root',x:0,y:0,rotation:0,length:5}],nodes:[{id:'n',boneId:'child',kind:{type:'rect'},transform:{x:2,y:0,rotation:0,scaleX:1,scaleY:1},style:{}}],clips:[weighted('a',0.5,[track('root','rotation',num(180)),track('n','y',num(4))]),weighted('b',0.5,[track('root','rotation',num(0))])]});
  f.clip('a').seek(0);f.clip('b').seek(0);near(f.bones()[0].rotation,90);near(f.bones()[1].x,0);near(f.bones()[1].y,10);
  const values=attrs.get('transform').match(/[-+]?\d*\.?\d+(?:e[-+]?\d+)?/gi).map(Number);near(values[0],-2);near(values[1],12);f.destroy();
});
test('paused/inactive/ended ramps, interruption, seek preserves fade; stop cancels retaining weight',()=>{
  const {f,attrs}=mount({clips:[weighted('a',1,[track('n','x',num(30))])]});const c=f.clip('a');
  assert.equal(c.fadeTo(0,2),c);f.tick(1);near(c.weight,0.5);assert.equal(c.playing,false);near(attrs.get('x'),10);
  c.seek(1).fadeTo(1,2);f.tick(1);near(c.weight,0.75);near(c.time,1);c.seek(0);f.tick(1);near(c.weight,1);
  c.play();f.tick(2);assert.equal(c.playing,false);c.fadeTo(0,2);f.tick(1);near(c.weight,0.5);c.stop();f.tick(10);near(c.weight,0.5);near(attrs.get('x'),10);
  c.seek(0).fadeTo(0,0);near(c.weight,0);near(attrs.get('x'),10);c.play();assert.equal(c.playing,true);f.destroy();
});
test('segmented fade actions parity; parallel scroll continues through sparse jump',()=>{
  function run(parts) {
    const {f,attrs}=mount({clips:[{id:'scroll',duration:20,tracks:[{nodeId:'n',property:'x',keyframes:[{time:0,value:num(0)},{time:20,value:num(200)}]}]},weighted('walk',1,[track('n','y',num(20))]),weighted('jump',0,[track('n','y',num(100))])],stateMachines:[{id:'motion',initial:'walk',states:[{id:'walk'},{id:'jump'}],transitions:[{from:'walk',to:'jump',event:{name:'JUMP'},actions:[{type:'blendClip',clipId:'walk',weight:0,duration:1},{type:'blendClip',clipId:'jump',weight:1,duration:1}]}]},{id:'scrolling',initial:'s',states:[{id:'s',onEnter:[{type:'playClip',clipId:'scroll'}]}]}]});
    f.clip('walk').seek(0);f.clip('jump').seek(0);f.timeline.at(0.5,()=>f.emit('JUMP'));f.timeline.at(1,()=>{near(f.clip('jump').weight,0.5);f.clip('jump').fadeTo(0,1);});
    for(const d of parts)f.tick(d);const result=[Number(attrs.get('x')),Number(attrs.get('y')),f.clip('jump').weight,f.clip('walk').weight];f.destroy();return result;
  }
  const a=run([2]),b=run([0.5,0.5,0.5,0.5]);a.forEach((v,i)=>near(v,b[i]));near(a[0],20);near(a[1],20);
});
test('strict inactive fields/actions and API inputs reject before mutation',()=>{
  for(const weight of [null,-1,1.1,NaN,Infinity,'0.5'])assert.throws(()=>normalize({clips:[weighted('a',weight,[])]}),/weight/);
  for(const blendMode of [null,'additive',false])assert.throws(()=>normalize({clips:[{id:'a',duration:1,blendMode}]}),/blendMode/);
  for(const action of [{type:'blendClip',clipId:'missing',weight:0},{type:'blendClip',clipId:'a',weight:0,duration:-1},{type:'blendClip',clipId:'a',weight:Infinity}])assert.throws(()=>normalize({clips:[weighted('a',1,[])],interactions:[{actions:[action]}]}),/weighted clip|duration|weight/);
  assert.throws(()=>normalize({clips:[{id:'a',duration:1}],interactions:[{actions:[{type:'blendClip',clipId:'a',weight:0}]}]}),/weighted clip/);
  const {f}=mount({clips:[weighted('a',1,[]),{id:'o',duration:1}]});const c=f.clip('a');
  for(const value of [-1,2,NaN,Infinity,'1',null])assert.throws(()=>c.setWeight(value),/weight/);
  for(const seconds of [-1,NaN,Infinity,'1',null])assert.throws(()=>c.fadeTo(0,seconds),/duration/);
  assert.throws(()=>f.clip('o').setWeight(0),/weighted clip/);near(c.weight,1);f.destroy();assert.throws(()=>c.fadeTo(0,1),/destroyed/);
});
test('clip.end return fade consumes leftover delta with duration callbacks before end',()=>{
  function run(parts) {
    const jump={...weighted('jump',1,[track('n','y',num(100))]),duration:1};
    const {f,attrs}=mount({clips:[weighted('walk',0,[track('n','y',num(20))]),jump],stateMachines:[{id:'motion',initial:'jump',states:[{id:'jump'},{id:'walk'}],transitions:[{from:'jump',to:'walk',event:{name:'clip.end',target:'jump'},actions:[{type:'blendClip',clipId:'jump',weight:0,duration:2},{type:'blendClip',clipId:'walk',weight:1,duration:2}]}]}]});
    const events=[];f.clip('walk').seek(0);f.clip('jump').play().fadeTo(0.5,1);
    f.timeline.at(1,()=>{events.push('duration');near(f.clip('jump').weight,0.5);near(attrs.get('y'),60);assert.equal(f.clip('jump').playing,true);},{clip:'jump'});
    f.on('jump','clip.end',()=>{events.push('end');near(f.timeline.time,1);near(f.clip('jump').weight,0.5);near(attrs.get('y'),60);assert.equal(f.clip('jump').playing,false);});
    for(const d of parts)f.tick(d);
    assert.deepEqual(events,['duration','end']);assert.equal(f.machine('motion').state,'walk');
    const result=[Number(attrs.get('y')),f.clip('jump').weight,f.clip('walk').weight];assert.equal(f.diagnostics.length,0);f.destroy();return result;
  }
  const a=run([2]),b=run([0.25,0.75,0.5,0.5]);a.forEach((v,i)=>near(v,b[i]));near(a[0],40);near(a[1],0.25);near(a[2],0.5);
});
test('loop-zero callbacks see reset weighted pose after duration and fade completion',()=>{
  function run(parts) {
    const {f,attrs}=mount({clips:[{...weighted('a',1,[{nodeId:'n',property:'x',keyframes:[{time:0,value:num(0)},{time:1,value:num(100)}]}]),duration:1,looping:true}]});
    const c=f.clip('a'),events=[];
    f.timeline.at(0,({cycle})=>{events.push(['zero',cycle,Number(attrs.get('x')),c.weight]);near(attrs.get('x'),10*(1-c.weight));},{clip:'a'});
    f.timeline.at(1,({cycle})=>{events.push(['duration',cycle,Number(attrs.get('x')),c.weight]);near(attrs.get('x'),10*(1-c.weight)+100*c.weight);},{clip:'a'});
    c.play().fadeTo(0.5,1);for(const d of parts)f.tick(d);
    assert.deepEqual(events.map(e=>e.slice(0,2)),[['zero',0],['duration',0],['zero',1],['duration',1],['zero',2]]);
    assert.equal(f.diagnostics.length,0);f.destroy();return events;
  }
  assert.deepEqual(run([2]),run([0.5,0.5,0.5,0.5]));
});
test('production external machine send/broadcast catches up end fade before new actions',()=>{
  for(const broadcast of [false,true]) {
    let now=0;const env={performance:{now:()=>now}};
    const {f,attrs}=mount({clips:[{...weighted('jump',1,[track('n','y',num(100))]),duration:1},weighted('other',1,[])],stateMachines:[{id:'m',initial:'s',states:[{id:'s'}],transitions:[{event:'GO',to:'s',actions:[{type:'blendClip',clipId:'other',weight:0,duration:2}]}]}]},{autoTick:true,window:env});
    f.clip('jump').play();let ended=0;
    f.on('jump','clip.end',()=>{ended++;near(f.timeline.time,1);f.clip('jump').fadeTo(0,2);});
    now=2000;if(broadcast)f.emit('GO');else f.machine('m').send('GO');
    assert.equal(ended,1);near(f.clip('jump').weight,0.5);near(attrs.get('y'),60);near(f.clip('other').weight,1);
    now=3000;near(f.clip('jump').weight,0);near(f.clip('other').weight,0.5);assert.equal(f.diagnostics.length,0);f.destroy();
  }
});
test('duration callback restart suppresses stale end and preserves remaining fade time',()=>{
  const {f,attrs}=mount({clips:[{...weighted('a',1,[track('n','x',num(30))]),duration:1}]});const c=f.clip('a');let ends=0;
  f.on('a','clip.end',()=>ends++);f.timeline.at(1,()=>c.seek(0).fadeTo(0,1),{clip:'a'});c.play();f.tick(1.5);
  assert.equal(ends,0);near(c.time,0.5);near(c.weight,0.5);near(attrs.get('x'),20);assert.equal(f.diagnostics.length,0);f.destroy();
});
test('production timer advances paused ramps without audio or RAF; destroy releases timer',()=>{
  let now=0,interval,cleared=false;const env={performance:{now:()=>now},setInterval(fn){interval=fn;return 7;},clearInterval(id){assert.equal(id,7);cleared=true;}};
  const {f}=mount({clips:[weighted('a',1,[])]},{autoTick:true,window:env});const c=f.clip('a');now=10000;c.fadeTo(0,2);now=11000;interval();near(c.weight,0.5);now=12000;interval();near(c.weight,0);f.destroy();assert.equal(cleared,true);
});
