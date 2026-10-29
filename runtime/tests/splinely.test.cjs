'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const {createRuntime,evaluateTrack,easing,normalize}=require('../src/splinely.js');
class Element {
  constructor(id,tag='circle'){this.id=id;this.tagName=tag;this.attrs=new Map();this.listeners=new Map();this.parentNode=null;}
  getAttribute(k){return this.attrs.has(k)?this.attrs.get(k):null;}
  setAttribute(k,v){this.attrs.set(k,String(v));}
  removeAttribute(k){this.attrs.delete(k);}
  addEventListener(k,fn){if(!this.listeners.has(k))this.listeners.set(k,new Set());this.listeners.get(k).add(fn);}
  removeEventListener(k,fn){this.listeners.get(k)?.delete(fn);}
  fire(type,event={}){for(const fn of this.listeners.get(type)||[])fn({target:this,type,...event});}
  setPointerCapture(id){this.captured=id;}
  releasePointerCapture(){this.captured=null;}
}
function root(ids=['n'],env={}){
  const svg=new Element('root','svg');const children=ids.map(id=>new Element(id));
  for(const e of children){e.ownerSVGElement=svg;e.parentNode=svg;}
  svg.ownerDocument={defaultView:env};svg.children=children;
  svg.querySelectorAll=selector=>selector==='[id]'?children:children.filter(e=>e.id==='splinely-project' || e.dataSplinelyProject);
  svg.querySelector=selector=>children.find(e=>selector==='[id="splinely-project"]'?e.id==='splinely-project':!!e.dataSplinelyProject)||null;
  svg.getScreenCTM=()=>({inverse:()=>({})});svg.createSVGPoint=()=>({x:0,y:0,matrixTransform(){return {x:this.x/2,y:this.y/2};}});
  return svg;
}
const frame=(time,value,easing)=>({time,value,easing});
const number=value=>({type:'number',value});
function project(extra={}){return {schemaVersion:1,nodes:[{id:'n',properties:{cx:5,opacity:1}}],clips:[{id:'a',duration:2,tracks:[{target:'n',property:'cx',keyframes:[frame(0,0),frame(2,20)]}]}],...extra};}
function runtime(p=project(),env={},ids=['n'],opts={}){const svg=root(ids,env);const f=createRuntime(svg,p,{autoTick:false,...opts});return {f,svg,n:svg.children[0]};}
function near(actual,expected,tolerance=1e-10){assert.ok(Math.abs(actual-expected)<tolerance,`${actual} != ${expected}`);}

test('pure evaluation: boundaries, tagged values, sorted adapter keys, step, cubic, color, arrays',()=>{
  const t={keyframes:[frame(0,number(10)),frame(2,number(30))]};
  assert.equal(evaluateTrack(t,-1),10);assert.equal(evaluateTrack(t,1),20);assert.equal(evaluateTrack(t,4),30);
  assert.equal(evaluateTrack({keyframes:[frame(0,0,{type:'step'}),frame(1,10)]},0.99),0);
  assert.equal(evaluateTrack({keyframes:[frame(0,0,'step'),frame(1,10)]},1),10);
  near(easing({type:'cubicBezier',x1:0.42,y1:0,x2:0.58,y2:1},0.5),0.5);
  near(easing([0.25,0.1,0.25,1],0.5),0.802403387584857);
  assert.deepEqual(evaluateTrack({keyframes:[frame(0,'#0000'),frame(1,'#ffff')]},0.5),{r:0.5,g:0.5,b:0.5,a:0.5});
  assert.deepEqual(evaluateTrack({keyframes:[frame(0,[0,2]),frame(1,[2,4])]},0.5),[1,3]);
  assert.equal(evaluateTrack({keyframes:[frame(0,'hello'),frame(1,'world')]},0.5),'hello');
  assert.equal(evaluateTrack({keyframes:[frame(0,1),frame(0,2),frame(1,3)]},0),2);
  assert.deepEqual(normalize(project({clips:[{id:'x',duration:1,tracks:[{target:'n',property:'cx',keyframes:[frame(1,2),frame(0,1)]}]}]})).clips[0].tracks[0].keyframes.map(k=>k.time),[0,1]);
});

test('canonical schema transforms, geometry properties, color alpha, priority and static attribute restoration',()=>{
  const {f,n}=runtime(project({nodes:[{id:'n',kind:{type:'circle',cx:30,cy:40,radius:4},transform:{x:2,y:3,rotation:10,scaleX:1,scaleY:2},style:{fill:{r:1,g:0,b:0,a:0.4},opacity:1,stroke:null,strokeWidth:2}}],clips:[{id:'a',duration:2,tracks:[{nodeId:'n',property:'x',keyframes:[frame(0,number(0)),frame(2,number(20))]}]}],stateMachines:[{id:'m',initial:'s',states:[{id:'s',overrides:[{nodeId:'n',property:'x',value:number(7)}]}]}]}));
  assert.equal(n.getAttribute('transform'),'translate(7 3) rotate(10) scale(1 2)');
  assert.equal(n.getAttribute('r'),'4');assert.equal(n.getAttribute('fill'),'rgb(255,0,0)');assert.equal(n.getAttribute('fill-opacity'),'0.4');assert.equal(n.getAttribute('stroke-width'),'2');
  f.clip('a').seek(1);assert.equal(n.getAttribute('transform'),'translate(10 3) rotate(10) scale(1 2)');
  f.set('n',{x:9,opacity:0.2});assert.equal(n.getAttribute('opacity'),'0.2');assert.match(n.getAttribute('transform'),/^translate\(9 /);
  f.set('n',null);assert.equal(n.getAttribute('opacity'),'1');assert.match(n.getAttribute('transform'),/^translate\(10 /);
  f.clip('a').stop();assert.match(n.getAttribute('transform'),/^translate\(7 /);f.destroy();
  const svg=root();svg.children[0].setAttribute('cx','42');const g=createRuntime(svg,{clips:[{id:'x',duration:1,tracks:[]}]},{autoTick:false});g.set('n',{cx:2});g.set('n',null);assert.equal(svg.children[0].getAttribute('cx'),'42');g.destroy();
});

test('deterministic play/pause/seek/end; no callbacks on seek, unsubscribing and callback controls',()=>{
  const {f,n}=runtime();const c=f.clip('a'),seen=[];
  const off=f.timeline.at(1,d=>seen.push(d.time),{clip:'a'});let ends=0;f.on('clip.end',()=>ends++);
  c.play();f.tick(0.5);assert.equal(c.time,0.5);assert.equal(n.getAttribute('cx'),'5');c.pause();f.tick(1);assert.equal(c.time,0.5);
  c.seek(1.5);assert.deepEqual(seen,[]);c.play();f.tick(0.5);assert.equal(ends,1);assert.equal(c.playing,false);assert.equal(c.time,2);f.tick(3);assert.equal(ends,1);
  c.seek(0.5).play();f.tick(0.5);assert.deepEqual(seen,[1]);off();c.seek(0).play();f.tick(2);assert.deepEqual(seen,[1]);
  f.timeline.at(0.25,()=>c.pause(),{clip:'a'});c.seek(0).play();f.tick(2);assert.equal(c.time,0.25);assert.equal(c.playing,false);f.destroy();
});

test('loops process duration before zero; skipped frames cross every lap; backward seek rearms; mutation stops catchup',()=>{
  const {f}=runtime(project({clips:[{id:'a',duration:1,looping:true,tracks:[]}]}));const c=f.clip('a'),seen=[];
  for(const t of [0,0.5,1])f.timeline.at(t,d=>seen.push(`${d.cycle}:${d.time}`),{clip:'a'});
  c.play();f.tick(2.5);assert.deepEqual(seen,['0:0','0:0.5','0:1','1:0','1:0.5','1:1','2:0','2:0.5']);assert.equal(c.time,0.5);
  c.seek(0.25);assert.equal(seen.length,8);f.tick(0.25);assert.equal(seen.at(-1),'0:0.5');
  const off=f.timeline.at(1,()=>c.seek(0.25),{clip:'a'});f.tick(0.5);assert.equal(c.time,0.25);off();
  f.tick(10001);assert.equal(c.playing,false);assert.match(f.diagnostics.at(-1).message,/catch-up/);f.destroy();
});

test('cross-clip callbacks advance remaining time independent of storage order',()=>{
  for(const order of [['a','b'],['b','a']]) {
    const {f}=runtime(project({clips:order.map(id=>({id,duration:2,tracks:[]}))}));
    f.timeline.at(0.5,()=>f.clip('b').play(),{clip:'a'});f.clip('a').play();f.tick(1);assert.equal(f.clip('b').time,0.5);f.destroy();
  }
});

test('global timeline independent of clip loops, global seek silent and clips synchronize on timeline seek',()=>{
  const {f}=runtime(project({clips:[{id:'a',duration:1,loop:true,tracks:[]}]}));const seen=[];
  f.timeline.at(1.5,d=>seen.push(d.time));f.clip('a').play();f.tick(2.5);assert.equal(f.timeline.time,2.5);assert.deepEqual(seen,[1.5]);
  f.timeline.pause();f.tick(1);assert.equal(f.timeline.time,2.5);f.timeline.seek(0.25);assert.equal(f.clip('a').time,0.25);f.timeline.play();f.tick(1.5);assert.deepEqual(seen,[1.5,1.5]);f.destroy();
  const {f:g}=runtime({audioCues:[],clips:[]});let done=0;g.timeline.at(1,()=>done++);g.timeline.play();g.tick(2);assert.equal(done,1);g.destroy();
});

test('machine schema: initial entry, event target guards, priority, actions and structural equality',()=>{
  const {f,n}=runtime(project({variables:{v:{a:1,b:2},count:0},stateMachines:[{id:'m',initial:'closed',states:[{id:'closed',onEnter:[{type:'setVariable',name:'count',value:1}]},{id:'open',onEnter:[{type:'playClip',clipId:'a'}],overrides:[{nodeId:'n',property:'opacity',value:number(0.5)}]}],transitions:[{from:'closed',to:'open',event:{name:'OPEN',target:'n'},guard:{variable:'v',equals:{b:2,a:1}},actions:[{type:'setProperty',nodeId:'n',property:'opacity',value:number(0.2)}]}]}]}));
  assert.equal(f.variable('count').get(),1);f.machine('m').send({name:'OPEN',target:'other'});assert.equal(f.machine('m').state,'closed');f.machine('m').send({name:'OPEN',target:'n'});assert.equal(f.machine('m').state,'open');assert.equal(f.clip('a').playing,true);assert.equal(n.getAttribute('opacity'),'0.2');f.set('n',null);assert.equal(n.getAttribute('opacity'),'0.5');f.destroy();
});

test('broadcast and nested event delivery strictly FIFO; machines run in document order; subscriber cycles bounded',()=>{
  const states=[{id:'s'}];const {f}=runtime(project({variables:{v:0},stateMachines:[
    {id:'first',initial:'s',states,transitions:[{from:'s',to:'s',event:{name:'GO'},actions:[{type:'setVariable',name:'v',value:1}]}]},
    {id:'second',initial:'s',states:[...states,{id:'done'}],transitions:[{from:'s',to:'done',event:{name:'GO'},guard:{variable:'v',equals:1}}]}
  ]}));
  const seen=[];f.on('GO',()=>{seen.push('GO');f.emit('NEXT');});f.on('NEXT',()=>seen.push('NEXT'));f.emit('GO');assert.equal(f.machine('second').state,'done');assert.deepEqual(seen,['GO','NEXT']);
  f.on('LOOP',()=>f.emit('LOOP'));f.emit('LOOP');assert.match(f.diagnostics.at(-1).message,/Event cycle limit/);f.destroy();
});

test('host controls are not captured or dispatched as scene pointer events',()=>{
  const {f,svg}=runtime();const control=new Element('','g');control.setAttribute('data-splinely-host-control','');control.parentNode=svg;
  const rect=new Element('','rect');rect.parentNode=control;let events=0;f.on('pointerdown',()=>events++);
  svg.fire('pointerdown',{target:rect,pointerId:1});svg.fire('click',{target:rect});
  assert.equal(svg.captured,undefined);assert.equal(rect.captured,undefined);assert.equal(events,0);
  f.destroy();
});

test('click-only figures do not capture the pointer and suppress hover boundaries',()=>{
  const {f,svg,n}=runtime();let clicks=0;f.on('n','click',()=>clicks++);
  svg.fire('pointerdown',{target:n,pointerId:3});
  assert.equal(n.captured,undefined);
  svg.fire('click',{target:n});assert.equal(clicks,1);f.destroy();
});

test('DOM interactions, SVG coordinate drag/capture and keyboard subscriptions; destroy cleanup',()=>{
  const {f,svg,n}=runtime();const seen=[];f.on('n','click',d=>seen.push(d.target));const off=f.on('n','keydown',d=>seen.push(d.key));
  svg.fire('click',{target:n});svg.fire('keydown',{target:n,key:'Enter'});off();svg.fire('keydown',{target:n,key:'Escape'});assert.deepEqual(seen,['n','Enter']);
  let drag;f.on('n','drag',d=>drag=d);svg.fire('pointerdown',{target:n,pointerId:7,clientX:4,clientY:6});assert.equal(n.captured,7);svg.fire('pointermove',{target:svg,pointerId:7,clientX:10,clientY:14});assert.equal(drag.dx,3);assert.equal(drag.dy,4);svg.fire('pointerup',{target:svg,pointerId:7});assert.equal(n.captured,null);
  f.destroy();assert.equal(svg.splinely,undefined);assert.ok([...svg.listeners.values()].every(s=>s.size===0));assert.throws(()=>f.clip('a'),/destroyed/);
});

test('multiple instances own IDs, transports, global alias and preexisting globals',()=>{
  const env={};const a=runtime(project(),env),b=runtime(project(),env);assert.equal(env.splinely,undefined);a.f.set('n',{cx:99});assert.equal(b.n.getAttribute('cx'),'5');a.f.clip('a').play();a.f.tick(1);assert.equal(b.f.clip('a').time,0);b.f.destroy();assert.equal(env.splinely,a.f);a.f.destroy();assert.equal(env.splinely,undefined);
  const previous={host:true};const c=runtime(project(),{splinely:previous});assert.equal(c.svg.ownerDocument.defaultView.splinely,previous);c.f.destroy();assert.equal(c.svg.ownerDocument.defaultView.splinely,previous);
});

function buffer(duration=3,sampleRate=100,channels=1){const data=Array.from({length:channels},()=>new Float32Array(duration*sampleRate).fill(0.5));return {duration,sampleRate,numberOfChannels:channels,length:duration*sampleRate,getChannelData:ch=>data[ch]};}
class AudioContext {
  constructor(){this.currentTime=10;this.destination={};this.starts=[];this.sources=[];AudioContext.latest=this;}
  async resume(){this.resumed=true;}async close(){this.closed=true;}async decodeAudioData(){return buffer();}
  createBuffer(ch,length,rate){const b=buffer(length/rate,rate,ch);for(let i=0;i<ch;i++)b.getChannelData(i).fill(0);return b;}
  createBufferSource(){const ctx=this;const source={connect(){},disconnect(){this.disconnected=true;},start(...args){ctx.starts.push({args,source:this});},stop(){this.stopped=true;}};this.sources.push(source);return source;}
  createGain(){return {gain:{value:1},connect(){},disconnect(){}};}
}
const asset={id:'voice',dataBase64:'AAAA',mimeType:'audio/wav'};

test('embedded audio unlock, absolute cue schedule, pause/resume offsets, seek overlap, and no scrubbing sounds',async()=>{
  const {f}=runtime(project({audioAssets:[asset],audioCues:[{id:'cue',assetId:'voice',time:0.5,offset:0.25,duration:2,volume:0.7}]}),{AudioContext});
  assert.throws(()=>f.audio.play('voice'),/locked/);await f.audio.unlock();const ctx=AudioContext.latest;f.clip('a').play();assert.deepEqual(ctx.starts[0].args,[10.5,0.25,2]);assert.equal(ctx.starts[0].source.loop,false);
  f.tick(1);assert.equal(ctx.starts.length,1);f.timeline.pause();assert.equal(ctx.sources[0].stopped,true);f.timeline.play();assert.deepEqual(ctx.starts.at(-1).args,[10,0.75,1.5]);
  f.timeline.seek(1.5);assert.deepEqual(ctx.starts.at(-1).args,[10,1.25,1]);f.timeline.pause();const count=ctx.starts.length;f.timeline.seek(0.75);assert.equal(ctx.starts.length,count);f.timeline.play();assert.deepEqual(ctx.starts.at(-1).args,[10,0.5,1.75]);
  const independent=f.audio.play('voice',{offset:1});f.audio.seek(2);assert.equal(independent.stopped,true);f.destroy();assert.equal(ctx.closed,true);
});

test('native looping audio pads silence and remains playing across skipped render laps without duplicates',async()=>{
  const {f}=runtime(project({audioAssets:[asset],clips:[{id:'a',duration:1,loop:true,tracks:[],audioCues:[{asset:'voice',time:0.2,offset:0,duration:0.3}]}]}),{AudioContext});
  await f.audio.unlock();f.clip('a').play();const ctx=AudioContext.latest,s=ctx.sources[0];assert.equal(s.loop,true);assert.equal(s.buffer.duration,1);assert.equal(s.buffer.getChannelData(0)[0],0);assert.equal(s.buffer.getChannelData(0)[20],0.5);assert.equal(s.buffer.getChannelData(0)[50],0);assert.deepEqual(ctx.starts[0].args,[10,0]);
  f.tick(3.5);assert.equal(ctx.starts.length,1);assert.notEqual(s.stopped,true);f.clip('a').seek(0.4);assert.equal(s.stopped,true);assert.deepEqual(ctx.starts.at(-1).args,[10,0.4]);f.destroy();
});

test('audio errors reject, report and allow retry without network',async()=>{
  const {f}=runtime(project({audioAssets:[{id:'bad',url:'https://example.invalid/x',data:'AAAA'}]}),{AudioContext});await assert.rejects(f.audio.unlock(),/embedded base64/);assert.match(f.diagnostics.at(-1).message,/embedded base64/);f.destroy();
  const {f:g}=runtime();await assert.rejects(g.audio.unlock(),/Web Audio unavailable/);g.destroy();
});

test('speech is gone: no API, legacy speech data and actions never reach the browser',()=>{
  const spoken=[];const env={speechSynthesis:{speak:u=>spoken.push(u),cancel(){}},SpeechSynthesisUtterance:class{constructor(text){this.text=text;}}};
  const {f}=runtime(project({speechCues:[{time:0.5,text:'hello',lang:'en'}]}),env);
  assert.equal(f.speech,undefined);assert.equal('speech' in f.snapshot(),false);
  f.timeline.play();f.tick(1);assert.deepEqual(spoken,[]);f.destroy();
});

test('malformed data, callback faults, script properties and unknown IDs are handled',()=>{
  assert.throws(()=>runtime({schemaVersion:2}),/schemaVersion/);assert.throws(()=>runtime({clips:[{id:'a',duration:0}]}),/at least 0\.001 seconds/);
  assert.throws(()=>runtime({clips:[{id:'a',duration:1},{id:'a',duration:2}]}),/duplicate/);
  assert.throws(()=>runtime({clips:[{id:'a',duration:1,tracks:[{target:'n',property:'cx',keyframes:[frame(NaN,1)]}]}]}),/finite/);
  assert.throws(()=>easing({type:'cubicBezier',x1:2,y1:0,x2:1,y2:1},0.5),/Unsupported/);
  const {f,n}=runtime();assert.throws(()=>f.clip('missing'),/Unknown/);assert.throws(()=>f.machine('missing'),/Unknown/);assert.throws(()=>f.variable('missing'),/Unknown/);assert.throws(()=>f.clip('a').seek(NaN),/finite/);assert.throws(()=>f.tick(-1),/nonnegative/);
  f.set('n',{onclick:'bad',fill:'url(https://invalid)'});assert.equal(n.getAttribute('onclick'),null);assert.notEqual(n.getAttribute('fill'),'url(https://invalid)');
  f.on('test',()=>{throw new Error('handler fault');});f.emit('test');assert.match(f.diagnostics.at(-1).message,/handler fault/);f.timeline.at(0.1,()=>{throw new Error('callback fault');});f.timeline.play();f.tick(1);assert.ok(f.diagnostics.some(e=>/callback fault/.test(e.message)));f.destroy();
});

test('initial entries execute exactly once before queued initial emits are delivered',()=>{
  const {f}=runtime(project({variables:{first:false,initial:false,destination:false},stateMachines:[
    {id:'bootSender',initial:'s',states:[{id:'s',onEnter:[{type:'emit',event:{name:'BOOT'}}]}]},
    {id:'b',initial:'initial',states:[{id:'initial',onEnter:[{type:'setVariable',name:'initial',value:true}]},{id:'done',onEnter:[{type:'setVariable',name:'destination',value:true}]}],transitions:[{from:'initial',to:'done',event:{name:'BOOT'},guard:{variable:'initial',equals:true}}]}
  ]}));
  assert.equal(f.machine('b').state,'done');assert.equal(f.variable('initial').get(),true);assert.equal(f.variable('destination').get(),true);f.destroy();
});

test('nested SVG events do not leak and active/lost pointer capture is cleaned',()=>{
  const {f,svg,n}=runtime();const other=root(['n']);other.parentNode=svg;const nested=other.children[0];let count=0;f.on('n','click',()=>count++);f.on('keydown',()=>count++);
  svg.fire('click',{target:nested});svg.fire('keydown',{target:nested,key:'Enter'});svg.fire('click',{target:other});assert.equal(count,0);
  svg.fire('pointerdown',{target:n,pointerId:1});let ends=0;f.on('n','dragend',()=>ends++);svg.fire('lostpointercapture',{target:n,pointerId:1});svg.fire('pointermove',{target:n,pointerId:1});assert.equal(ends,1);
  svg.fire('pointerdown',{target:n,pointerId:2});f.destroy();assert.equal(n.captured,null);
});

test('explicit timeline playback survives inactive clip pause/stop; scalar variables retain types',()=>{
  const {f}=runtime(project({variables:{n:1,b:true,z:null}}));f.timeline.play();f.tick(0.5);f.clip('a').stop();f.tick(0.5);assert.equal(f.timeline.time,1);assert.equal(f.timeline.playing,true);
  assert.throws(()=>f.variable('n').set('x'),/type mismatch/);assert.throws(()=>f.variable('z').set(0),/type mismatch/);f.variable('b').set(false);assert.equal(f.variable('b').get(),false);f.destroy();
});

test('invalid native loop offsets fail once without allocating padding',async()=>{
  class CountingContext extends AudioContext {createBuffer(...args){this.allocations=(this.allocations || 0)+1;return super.createBuffer(...args);}}
  const {f}=runtime(project({audioAssets:[asset],clips:[{id:'a',duration:1,loop:true,tracks:[],audioCues:[{asset:'voice',time:0.2,offset:10}]}]}),{AudioContext:CountingContext});
  await f.audio.unlock();f.clip('a').play();f.tick(1);f.tick(1);assert.equal(AudioContext.latest.allocations,undefined);assert.equal(f.diagnostics.filter(e=>/Invalid loop/.test(e.message)).length,1);f.destroy();
});

test('audio clock drives production ticks independently of RAF and timer cleanup is scoped',async()=>{
  let wall=0,interval,frame,cleared=false;const env={AudioContext,performance:{now:()=>wall},requestAnimationFrame:fn=>{frame=fn;return 1;},cancelAnimationFrame(){},setInterval:fn=>{interval=fn;return 1;},clearInterval(){cleared=true;}};
  const {f}=runtime(project({audioAssets:[asset]}),env,['n'],{autoTick:true});f.timeline.play();wall=500;frame();assert.equal(f.timeline.time,0.5);await f.audio.unlock();const ctx=AudioContext.latest;ctx.state='running';interval();ctx.currentTime+=0.5;interval();assert.equal(f.timeline.time,1);frame();assert.equal(f.timeline.time,1);f.destroy();assert.equal(cleared,true);
});

test('autoplay clips use ordered priorities, deterministic partitioned time and never mutate source data',()=>{
  const source=project({clips:[
    {id:'a',autoplay:true,looping:true,duration:1,tracks:[{target:'n',property:'cx',keyframes:[frame(0,0),frame(1,10)]}]},
    {id:'b',autoplay:true,priority:2,duration:2,tracks:[{target:'n',property:'cx',keyframes:[frame(0,50),frame(2,70)]}]}
  ]});const before=JSON.stringify(source);const {f,n}=runtime(source),other=runtime(source);
  assert.equal(n.getAttribute('cx'),'50');f.tick(1.5);for(let i=0;i<6;i++)other.f.tick(0.25);assert.equal(n.getAttribute('cx'),'65');assert.equal(other.n.getAttribute('cx'),'65');assert.equal(f.clip('a').time,other.f.clip('a').time);assert.equal(JSON.stringify(source),before);
  f.clip('b').stop();assert.equal(n.getAttribute('cx'),'5');f.clip('a').seek(1);assert.equal(f.clip('a').time,0);assert.equal(n.getAttribute('cx'),'0');f.destroy();other.f.destroy();
});

test('metadata alternatives are accepted, nested metadata excluded and malformed machines/cues fail before mount',()=>{
  const svg=root(['data']);svg.children[0].dataSplinelyProject=true;svg.children[0].textContent=JSON.stringify(project({nodes:[],clips:[]}));const f=createRuntime(svg,null,{autoTick:false});f.destroy();
  const bad=root();assert.throws(()=>createRuntime(bad,{machines:[{id:'m',initial:'bad',states:[]}]},{autoTick:false}),/initial state/);assert.equal(bad.splinely,undefined);
  assert.throws(()=>createRuntime(bad,{audioCues:[{time:-1}]},{autoTick:false}),/nonnegative/);
});

test('SVG metadata auto-mount runs as a plain browser script, JSON stays data, and malformed export retains fallback',()=>{
  const env={console:{error(){}}},svg=root(['n','splinely-project'],env);svg.children[1].tagName='metadata';svg.children[1].textContent=JSON.stringify(project({name:'</script> & ]]>',customJs:'throw new Error("must not execute")'}));env.document={currentScript:{ownerSVGElement:svg}};env.window=env;
  vm.runInNewContext(fs.readFileSync(require.resolve('../src/splinely.js'),'utf8'),env);assert.ok(svg.splinely);assert.equal(env.splinely,svg.splinely);assert.equal(svg.children[0].getAttribute('cx'),'5');svg.splinely.destroy();
  svg.children[1].textContent='{bad';svg.children[0].setAttribute('cx','100');vm.runInNewContext(fs.readFileSync(require.resolve('../src/splinely.js'),'utf8'),env);assert.equal(svg.splinely,undefined);assert.equal(svg.children[0].getAttribute('cx'),'100');
});

test('hidden canonical descriptors support overrides without a DOM node',()=>{
  const p=project({nodes:[{id:'n',visible:false,kind:{type:'rect',width:10,height:10},transform:{x:0,y:0,rotation:0,scaleX:1,scaleY:1},style:{opacity:1}}],clips:[]});
  const {f}=runtime(p,{},[]);
  assert.doesNotThrow(()=>f.set('n',{x:4}));
  f.tick(.5);
  assert.equal(f.diagnostics.length,0);
  f.destroy();
});
test('hidden canonical descriptor cannot be made visible by raw properties',()=>{
  const p=project({nodes:[{id:'n',visible:false,kind:{type:'rect',width:10,height:10},transform:{x:0,y:0,rotation:0,scaleX:1,scaleY:1},style:{opacity:1}}],clips:[]});
  const {f,n}=runtime(p);
  assert.equal(n.getAttribute('visibility'),'hidden');
  f.set('n',{visibility:'visible',x:3});
  assert.equal(n.getAttribute('visibility'),'hidden');
  assert.equal(f.diagnostics.length,0);
  f.destroy();
  assert.equal(n.getAttribute('visibility'),'hidden'); // destroy intentionally leaves the last frame.
});

test('gradient fills keep their exported url() while other properties animate',()=>{
  const gradient={radial:false,angle:0,stops:[{offset:0,color:{r:1,g:0,b:0,a:1}},{offset:1,color:{r:0,g:0,b:1,a:1}}]};
  const node={id:'n',kind:{type:'circle',cx:0,cy:0,radius:4},transform:{x:0,y:0,rotation:0,scaleX:1,scaleY:1},style:{fill:{r:0,g:1,b:0,a:1},opacity:1,stroke:null,strokeWidth:1},gradient};
  const p=project({nodes:[node],clips:[{id:'a',duration:2,autoplay:true,tracks:[
    {nodeId:'n',property:'x',keyframes:[frame(0,number(0)),frame(2,number(20))]},
    {nodeId:'n',property:'fill',keyframes:[frame(0,{type:'color',value:{r:0,g:0,b:0,a:1}}),frame(2,{type:'color',value:{r:1,g:1,b:1,a:1}})]}]}]});
  const {f,n}=runtime(p);
  n.setAttribute('fill','url(#n-gradient)');
  f.clip('a').seek(1);
  assert.equal(n.getAttribute('fill'),'url(#n-gradient)');
  assert.match(n.getAttribute('transform'),/^translate\(10 /);
  f.destroy();
});

test('animated gradients update the referenced gradient element; path data morphs',()=>{
  const g=(angle,r)=>({type:'gradient',value:{radial:false,angle,stops:[{offset:0,color:{r,g:0,b:0,a:1}},{offset:1,color:{r:0,g:0,b:1,a:0.5}}]}});
  const node={id:'n',kind:{type:'path',d:'M0 0 L10 0 L10 10 Z',bounds:[0,0,10,10]},transform:{x:0,y:0,rotation:0,scaleX:1,scaleY:1},style:{fill:{r:0,g:0,b:0,a:1},opacity:1,stroke:null,strokeWidth:1},gradient:g(0,1).value};
  const p=project({nodes:[node],clips:[{id:'a',duration:2,autoplay:true,tracks:[
    {nodeId:'n',property:'gradient',keyframes:[frame(0,g(0,1)),frame(2,g(90,0))]},
    {nodeId:'n',property:'d',keyframes:[frame(0,{type:'path',value:'M0 0 L10 0 L10 10 Z'}),frame(2,{type:'path',value:'M10 10 L30 10 L30 30 Z'})]}]}]});
  const svg=root(['n','n-gradient']);
  const [n,grad]=svg.children;
  grad.tagName='linearGradient';
  const stops=[new Element('s0','stop'),new Element('s1','stop')];
  grad.children=stops;
  n.setAttribute('fill','url(#n-gradient)');
  const f=createRuntime(svg,p,{autoTick:false});
  f.clip('a').seek(1);
  assert.equal(n.getAttribute('d'),'M 5 5 L 20 5 L 20 20 Z');
  assert.equal(grad.getAttribute('x2'),'0.853553'); // 45° halfway between 0° and 90°
  assert.equal(stops[0].getAttribute('stop-color'),'rgb(128,0,0)');
  assert.equal(stops[1].getAttribute('stop-opacity'),'0.5');
  assert.equal(n.getAttribute('fill'),'url(#n-gradient)');
  f.destroy();
});

test('weighted clips blend compatible path data and fall back to the heaviest otherwise',()=>{
  const node={id:'n',kind:{type:'path',d:'M0 0 L10 0',bounds:[0,0,10,0]},transform:{x:0,y:0,rotation:0,scaleX:1,scaleY:1},style:{fill:{r:0,g:0,b:0,a:1},opacity:1,stroke:null,strokeWidth:1}};
  const track=d=>({nodeId:'n',property:'d',keyframes:[frame(0,{type:'path',value:d})]});
  const p=project({nodes:[node],clips:[
    {id:'a',duration:1,autoplay:true,blendMode:'weighted',weight:0.5,tracks:[track('M0 0 L20 0')]},
    {id:'b',duration:1,autoplay:true,blendMode:'weighted',weight:0.5,tracks:[track('M0 10 L20 10')]}]});
  const {f,n}=runtime(p);
  f.clip('a').seek(0);
  assert.equal(n.getAttribute('d'),'M 0 5 L 20 5');
  f.destroy();
});

test('two machines crossfade together while click, hover and a pointer follower all work at once',()=>{
  const circle=(id,r)=>({id,kind:{type:'circle',cx:100,cy:100,radius:r},transform:{x:0,y:0,rotation:0,scaleX:1,scaleY:1},style:{opacity:1}});
  const p={schemaVersion:1,nodes:[circle('face',50),circle('pupil',5)],clips:[],
    stateMachines:[
      {id:'mood',initial:'calm',states:[{id:'calm',overrides:[{nodeId:'face',property:'opacity',value:number(1)}]},{id:'happy',overrides:[{nodeId:'face',property:'opacity',value:number(0.5)}]}],
        transitions:[{from:'calm',to:'happy',event:{name:'click',target:'face'},blend:1}]},
      {id:'hover',initial:'out',states:[{id:'out'},{id:'in',overrides:[{nodeId:'face',property:'scaleX',value:number(2)}]}],
        transitions:[{from:'out',to:'in',event:{name:'pointerenter',target:'face'},blend:0.5},{from:'in',to:'out',event:{name:'pointerleave',target:'face'},blend:0.5}]}],
    followers:[{id:'look',target:'pupil',max:10,strength:0.5,smoothing:0,when:{machine:'hover',states:['in']}}]};
  const {f,svg}=runtime(p,{},['face','pupil']);
  const [face,pupil]=svg.children;
  const fire=(type,target,x=216,y=200)=>{for(const fn of svg.listeners.get(type)||[])fn({type,target,clientX:x,clientY:y});};
  fire('click',face);fire('pointerenter',face);fire('pointermove',face);
  f.tick(0.25);
  const opacity=Number(face.getAttribute('opacity')),scale=Number(/scale\(([\d.]+)/.exec(face.getAttribute('transform'))[1]);
  assert.ok(opacity<1 && opacity>0.5,String(opacity));assert.ok(scale>1 && scale<2,String(scale));
  assert.match(pupil.getAttribute('transform'),/^translate\(4 0\)/);
  f.tick(1);
  assert.equal(face.getAttribute('opacity'),'0.5');assert.match(face.getAttribute('transform'),/scale\(2 1\)/);
  assert.deepEqual(f.snapshot().states,{mood:'happy',hover:'in'});
  fire('pointerleave',face);f.tick(0.25);
  assert.match(pupil.getAttribute('transform'),/^translate\(0 0\)/,'follower rests once its state is left');
  assert.deepEqual(f.diagnostics,[]);f.destroy();
});

test('followers smooth towards the pointer and rotate within limits',()=>{
  const p={schemaVersion:1,clips:[],nodes:[{id:'head',kind:{type:'rect',x:90,y:90,width:20,height:20},transform:{x:0,y:0,rotation:0,scaleX:1,scaleY:1}}],
    followers:[{id:'turn',target:'head',mode:'rotate',max:30,smoothing:0.5}]};
  const {f,svg}=runtime(p,{},['head']);const head=svg.children[0];
  for(const fn of svg.listeners.get('pointermove')||[])fn({type:'pointermove',target:head,clientX:200,clientY:400});
  f.tick(0.1);const early=Number(/rotate\(([-\d.]+)/.exec(head.getAttribute('transform'))[1]);
  f.tick(5);const late=Number(/rotate\(([-\d.]+)/.exec(head.getAttribute('transform'))[1]);
  assert.ok(early>0 && early<late,`${early} < ${late}`);near(late,30,1e-6);
  assert.throws(()=>normalize({followers:[{id:'x',target:'missing',max:1}]}),/Unknown follower target/);
  f.destroy();
});

test('timer transitions fire after their time and exit actions run before the next state',()=>{
  const p={schemaVersion:1,clips:[],nodes:[{id:'eye',kind:{type:'circle',cx:10,cy:10,radius:5},transform:{x:0,y:0,rotation:0,scaleX:1,scaleY:1}}],
    stateMachines:[{id:'blink',initial:'open',states:[{id:'open',onExit:[{type:'emit',event:{name:'closing'}}]},{id:'shut',overrides:[{nodeId:'eye',property:'scaleY',value:number(0.1)}]}],
      transitions:[{from:'open',to:'shut',after:2},{from:'shut',to:'open',after:0.15}]}]};
  const {f,svg}=runtime(p,{},['eye']);const seen=[];f.on('closing',()=>seen.push(f.machine('blink').state));
  f.tick(1.9);assert.equal(f.machine('blink').state,'open');
  f.tick(0.2);assert.equal(f.machine('blink').state,'shut');assert.equal(seen.length,1,'exit action ran once (its event is queued, like every emit)');
  assert.match(svg.children[0].getAttribute('transform'),/scale\(1 0\.1\)/);
  f.emit('after');assert.equal(f.machine('blink').state,'shut','events named after do not trigger timers');
  f.tick(0.2);assert.equal(f.machine('blink').state,'open');
  assert.deepEqual(f.diagnostics,[]);f.destroy();
});

test('reduced motion: no looping autoplay, instant state changes, resting followers',()=>{
  const env={matchMedia:q=>({matches:q==='(prefers-reduced-motion: reduce)'})};
  const p=project({clips:[{id:'a',duration:2,looping:true,autoplay:true,tracks:[{target:'n',property:'cx',keyframes:[frame(0,0),frame(2,20)]}]}],
    stateMachines:[{id:'m',initial:'a',states:[{id:'a'},{id:'b',overrides:[{nodeId:'n',property:'opacity',value:number(0.5)}]}],transitions:[{from:'a',to:'b',event:{name:'go'},blend:2}]}]});
  const {f,n}=runtime(p,env);
  assert.equal(f.snapshot().reducedMotion,true);assert.equal(f.clip('a').playing,false);
  f.emit('go');assert.equal(n.getAttribute('opacity'),'0.5','switches instantly');f.destroy();
  const {f:g}=runtime(p,env,['n'],{reducedMotion:false});assert.equal(g.clip('a').playing,true);g.destroy();
});

test('motion corners keep incoming tangents and animated paths accept scientific notation',()=>{
  for(const [d,rotation,expected] of [['M0 0 L1 0 L1 1',30,30],['M0 0 L1 1 L0 2',0,45]]) {
    const p=motionScene(d);p.nodes[1].transform={rotation};
    const {f,n}=runtime(p);const angle=Number(n.getAttribute('transform').match(/rotate\(([^)]+)\)/)[1]);
    near(angle,expected);assert.deepEqual(f.diagnostics,[]);f.destroy();
  }
  const p=motionScene('M0 0 L1e2 0');p.nodes[0].transform={};
  p.clips=[{id:'morph',duration:1,tracks:[{nodeId:'route',property:'d',keyframes:[frame(0,{type:'path',value:'M0 0 L1e2 0'}),frame(1,{type:'path',value:'M0 0 L2e2 0'})]}]}];
  const {f,n}=runtime(p);f.clip('morph').seek(0.5);
  near(Number(n.getAttribute('transform').slice(10).split(' ')[0]),75);f.destroy();
  for(const d of ['M0,,0 L100 0','M,0 0 L100 0','M0 0, L100 0','M0 0 L100 0,'])assert.throws(()=>normalize(motionScene(d)));
});

function motionScene(d='M0 0 L100 0 L100 300') {
  return {schemaVersion:1,nodes:[
    {id:'n',kind:{type:'circle',cx:0,cy:0,radius:3},transform:{x:4},motionPath:{path:'route',progress:0.5,orient:true}},
    {id:'route',visible:false,kind:{type:'path',d,bounds:[0,0,100,300]},transform:{}}
  ]};
}
test('motion paths use scene arc length, transformed sources and tangent orientation',()=>{
  const p=motionScene(),{f,n}=runtime(p);
  assert.equal(n.getAttribute('transform'),'translate(104 100) rotate(90) scale(1 1)');
  f.set('n',{motionProgress:0});assert.equal(n.getAttribute('transform'),'translate(4 0) rotate(0) scale(1 1)');
  f.set('n',{motionProgress:1});assert.equal(n.getAttribute('transform'),'translate(104 300) rotate(90) scale(1 1)');
  assert.throws(()=>f.set('n',{motionProgress:1.1}),/weight/i);
  assert.throws(()=>f.set('route',{motionProgress:0.5}),/binding/);f.destroy();
  p.nodes[1].transform={x:10,y:20,rotation:90,scaleX:2,scaleY:2};
  const r=runtime(p);const nums=r.n.getAttribute('transform').match(/[-+]?\d*\.?\d+(?:e[-+]?\d+)?/g).map(Number);
  near(nums[0],-186);near(nums[1],220);near(nums[2],180);r.f.destroy();
});
test('motion path curves, relative commands, close and disconnected subpaths match Rust samples',()=>{
  for(const [d,x,y] of [['m0 0 q50 100 100 0',50,50],['M0 0 C0 100 100 100 100 0',50,75],['M0 0 L0 0 L10 0 M100 0 L130 0',110,0],['M0 0 L10 0 Z',10,0]]) {
    const p=motionScene(d);p.nodes[0].transform={};p.nodes[0].motionPath.orient=false;
    const {f,n}=runtime(p);const xy=n.getAttribute('transform').slice(10).split(')')[0].split(' ').map(Number);
    near(xy[0],x);near(xy[1],y);assert.deepEqual(f.diagnostics,[]);f.destroy();
  }
});
test('motion progress weighted clips and state crossfades match native runtime',()=>{
  const p=motionScene('M0 0 L100 0');p.nodes[0].transform={};p.nodes[0].motionPath.progress=0;
  p.clips=[{id:'travel',duration:1,autoplay:true,blendMode:'weighted',weight:0.5,tracks:[{nodeId:'n',property:'motionProgress',keyframes:[frame(0,number(1))]}]}];
  p.stateMachines=[{id:'mood',initial:'idle',states:[{id:'idle'},{id:'active',overrides:[{nodeId:'n',property:'motionProgress',value:number(0.4)}]}],transitions:[{from:'idle',to:'active',event:{name:'go'},blend:1}]}];
  const {f,n}=runtime(p),x=()=>Number(n.getAttribute('transform').slice(10).split(' ')[0]);
  near(x(),50);f.emit('go');f.tick(0.5);near(x(),60);f.tick(0.5);near(x(),70);
  f.set('n',{motionProgress:0.9});near(x(),90);assert.deepEqual(f.diagnostics,[]);f.destroy();
});
test('motion progress participates in clips, states, explicit changes and validation',()=>{
  const p=motionScene('M0 0 L100 0');p.nodes[0].transform={};p.nodes[0].motionPath.orient=false;
  p.clips=[{id:'travel',duration:1,tracks:[{nodeId:'n',property:'motionProgress',keyframes:[frame(0,number(0)),frame(1,number(1))]}]}];
  p.stateMachines=[{id:'m',initial:'idle',states:[{id:'idle',overrides:[{nodeId:'n',property:'motionProgress',value:number(0.2)}]}]}];
  const {f,n}=runtime(p);assert.match(n.getAttribute('transform'),/^translate\(20 /);
  f.clip('travel').seek(0.25);assert.match(n.getAttribute('transform'),/^translate\(25 /);
  f.set('n',{motionProgress:0.8});assert.match(n.getAttribute('transform'),/^translate\(80 /);
  f.set('route',{x:10});assert.match(n.getAttribute('transform'),/^translate\(90 /);assert.deepEqual(f.diagnostics,[]);f.destroy();
  for(const progress of [-0.1,1.1,NaN]){const q=motionScene();q.nodes[0].motionPath.progress=progress;assert.throws(()=>normalize(q));}
  for(const path of ['n','missing']){const q=motionScene();q.nodes[0].motionPath.path=path;assert.throws(()=>normalize(q));}
  for(const d of ['M0 0','M0 0 L0 0','M0 0 H100'])assert.throws(()=>normalize(motionScene(d)));
  const q=motionScene();q.nodes[1].motionPath={path:'n'};assert.throws(()=>normalize(q));
});

// Hostile-input bounds (audit JS-1..JS-3). Limits match splinely_document (RUNTIME_API.md, "Limits").
const linePath=(segments,step=1)=>'M0 0'+Array.from({length:segments},(_,i)=>` L${(i+1)*step} ${i%2}`).join('');
test('motion path chord budget: 100000 per path and summed over distinct sources, their d keyframes and overrides',()=>{
  // 64 chords per segment: 1562 segments = 99968 chords fit, 1563 do not.
  assert.doesNotThrow(()=>normalize(motionScene(linePath(1562))));
  assert.throws(()=>normalize(motionScene(linePath(1563))),/^Error: Motion path too complex$/);
  const two=(d1,d2,extra={})=>({...motionScene(d1),...extra,nodes:[...motionScene(d1).nodes,{id:'m',kind:{type:'circle',cx:0,cy:0,radius:1},motionPath:{path:'route2'}},
    {id:'route2',visible:false,kind:{type:'path',d:d2,bounds:[0,0,1,1]},transform:{}}]});
  assert.doesNotThrow(()=>normalize(two(linePath(700),linePath(800))));
  assert.throws(()=>normalize(two(linePath(800),linePath(800))),/Motion paths too complex: more than 100000 chords/);
  // Many bindings to one source count it once.
  const shared=motionScene(linePath(1500));for(let i=0;i<50;i++)shared.nodes.push({id:'b'+i,kind:{type:'circle',cx:0,cy:0,radius:1},motionPath:{path:'route',progress:i/50}});
  assert.doesNotThrow(()=>normalize(shared));
  const keyed=motionScene(linePath(800));keyed.clips=[{id:'c',duration:1,tracks:[{nodeId:'route',property:'d',keyframes:[frame(0,linePath(700)),frame(1,linePath(800))]}]}];
  assert.throws(()=>normalize(keyed),/Motion paths too complex/);
  const over=motionScene(linePath(800));over.stateMachines=[{id:'m',initial:'a',states:[{id:'a',overrides:[{nodeId:'route',property:'d',value:linePath(800)}]}]}];
  assert.throws(()=>normalize(over),/Motion paths too complex/);
  const overMap=motionScene(linePath(800));overMap.stateMachines=[{id:'m',initial:'a',states:[{id:'a',overrides:{route:{d:linePath(800)}}}]}];
  assert.throws(()=>normalize(overMap),/Motion paths too complex/);
});
test('motion chord budget counts sampled segments: zero-length ones, tiny scales and per frame',()=>{
  // Zero-length segments still cost 64 samples each (as in splinely_document).
  assert.throws(()=>normalize(motionScene('M0 0 L1 0'+' L1 0'.repeat(1562))),/^Error: Motion path too complex$/);
  // A tiny base scale made animated d values unsampleable and count 0; they now count per segment.
  const tiny=()=>{const p=motionScene('M0 0 L1e7 0');p.nodes[1].transform={scaleX:1e-17,scaleY:1e-17};
    p.clips=[{id:'grow',duration:1,autoplay:true,tracks:[{nodeId:'route',property:'d',keyframes:[frame(0,linePath(800))]}]}];return p;};
  assert.doesNotThrow(()=>normalize(tiny()));
  const both=tiny();both.clips[0].tracks[0].keyframes.push(frame(1,linePath(800)));
  assert.throws(()=>normalize(both),/Motion paths too complex: more than 100000 chords/);
  // Values set at runtime are held to the document total per frame.
  const p=motionScene(linePath(700));p.nodes.push({id:'m',kind:{type:'circle',cx:0,cy:0,radius:1},motionPath:{path:'route2'}},
    {id:'route2',visible:false,kind:{type:'path',d:linePath(700),bounds:[0,0,1,1]},transform:{}});
  const {f}=runtime(p,{},['n','route','m','route2']);assert.deepEqual(f.diagnostics,[]);
  f.set('route2',{d:linePath(1000)});
  assert.ok(f.diagnostics.some(d=>/Motion paths too complex: more than 100000 chords/.test(d.message || d)),JSON.stringify(f.diagnostics));
  f.destroy();
});
test('long motion paths: tables are built once, progress is a binary search and samples match the chord walk',()=>{
  const reference=require('./fixtures/splinely-reference.js');
  const d='M0 0'+Array.from({length:700},(_,i)=>` Q${i*3+1} ${(i%7)*2} ${i*3+3} ${i%5} L${i*3+3.5} ${(i%3)-1}`).join('');
  const p=motionScene(d);p.nodes[0].transform={};
  for(let i=0;i<20;i++)p.nodes.push({id:'b'+i,kind:{type:'circle',cx:0,cy:0,radius:1},motionPath:{path:'route',orient:true}});
  p.clips=[{id:'go',duration:2,looping:true,autoplay:true,tracks:p.nodes.filter(n=>n.motionPath).map((n,i)=>({nodeId:n.id,property:'motionProgress',keyframes:[frame(0,number(0)),frame(2,number(1-i/40))]}))}];
  const ids=p.nodes.map(n=>n.id),a=runtime(JSON.parse(JSON.stringify(p)),{},ids),svg=root(ids),b=reference.createRuntime(svg,JSON.parse(JSON.stringify(p)),{autoTick:false});
  let spent=0;
  for(let k=0;k<30;k++){const t0=Date.now();a.f.tick(1/30);spent+=Date.now()-t0;b.tick(1/30);
    for(let i=0;i<ids.length;i++){const x=a.svg.children[i].getAttribute('transform'),y=svg.children[i].getAttribute('transform');if(x===y)continue;
      const num=t=>t.match(/-?\d[\d.]*(?:e[-+]?\d+)?/g).map(Number),nx=num(x),ny=num(y);nx.forEach((v,j)=>near(v,ny[j],1e-9));}}
  assert.ok(spent<1500,'30 frames of 21 bindings to an 89600-chord path took '+spent+' ms');assert.deepEqual(a.f.diagnostics,[]);a.f.destroy();b.destroy();
});
test('clip durations below 0.001 s are rejected; tiny looping clips advance whole laps instead of a frame per lap',()=>{
  assert.throws(()=>normalize({clips:[{id:'a',duration:0.0009}]}),/at least 0\.001 seconds/);
  const p=project({clips:[{id:'a',duration:0.001,looping:true,autoplay:true,tracks:[{target:'n',property:'cx',keyframes:[frame(0,0),frame(0.001,1)]}]}]});
  const {f,n}=runtime(p);let writes=0;const set=n.setAttribute;n.setAttribute=function(k,v){writes++;return set.call(this,k,v);};
  for(let i=0;i<30;i++)f.tick(1/30);
  assert.ok(writes<=30,'one frame per tick, not one per lap: '+writes+' writes');near(f.clip('a').time,(30/30)%0.001,1e-9);
  // A long catch-up (e.g. after a hidden tab) keeps the clip playing: no 10000-segment pause.
  f.tick(3600);assert.equal(f.clip('a').playing,true);assert.deepEqual(f.diagnostics,[]);
  assert.ok(f.clip('a').time>=0 && f.clip('a').time<0.001);f.destroy();
});
test('idPrefix: element ids carry a per-mount prefix, node ids in the API do not',()=>{
  const {f,n}=runtime(project(),{},['m1-n'],{idPrefix:'m1-'});
  f.clip('a').seek(1);assert.equal(n.getAttribute('cx'),'10');f.set('n',{opacity:0.5});assert.equal(n.getAttribute('opacity'),'0.5');
  assert.deepEqual(f.diagnostics,[]);f.destroy();
});
test('clip.end replaying many tiny clips: catch-up work per tick is capped, clip handles are reused',()=>{
  const clips=Array.from({length:300},(_,c)=>({id:'c'+c,duration:0.001+c*1e-6,autoplay:true,tracks:[{target:'n',property:'cx',keyframes:[frame(0,0),frame(0.001,10)]}]}));
  const act=clips.map(c=>({type:'playClip',clipId:c.id}));
  const p=project({clips,stateMachines:[{id:'m',initial:'a',states:[{id:'a',onEnter:act},{id:'b',onEnter:act}],
    transitions:[{from:'a',to:'b',event:{name:'clip.end'}},{from:'b',to:'a',event:{name:'clip.end'}},{from:'a',to:'b',after:0},{from:'b',to:'a',after:0}]}]});
  const {f}=runtime(p);assert.equal(f.clip('c1'),f.clip('c1'));
  // Before: about 4 s per tick (segments x actions), with no diagnostic.
  const start=Date.now();for(let i=0;i<5;i++)f.tick(1/30);
  assert.ok(Date.now()-start<1500,(Date.now()-start)+' ms');
  assert.match(f.diagnostics.at(-1).message,/catch-up limit exceeded \(work 20000\)/);f.destroy();
});
test('after:0 timers restarting a tiny clip or an emit ping-pong cost a bounded number of frames and are stopped',()=>{
  const act=[{type:'playClip',clipId:'a'}];
  const p=project({clips:[{id:'a',duration:0.001,looping:true,autoplay:true,tracks:[{target:'n',property:'cx',keyframes:[frame(0,0),frame(0.001,1)]}]}],
    stateMachines:[{id:'m',initial:'x',states:[{id:'x',onEnter:act},{id:'y',onEnter:act}],transitions:[{from:'x',to:'y',after:0},{from:'y',to:'x',after:0}]}]});
  const {f,n}=runtime(p);let writes=0;const set=n.setAttribute;n.setAttribute=function(k,v){writes++;return set.call(this,k,v);};
  for(let i=0;i<30;i++)f.tick(1/30);assert.ok(writes<=90,writes+' writes');assert.deepEqual(f.diagnostics,[]);f.destroy();
  const emit=[{type:'emit',event:{name:'go'}}];
  const q=project({clips:[],stateMachines:[{id:'m',initial:'a',states:[{id:'a',onEnter:emit},{id:'b',onEnter:emit}],
    transitions:[{from:'a',to:'b',event:{name:'go'}},{from:'b',to:'a',event:{name:'go'}},{from:'a',to:'b',after:0},{from:'b',to:'a',after:0}]}]});
  const r=runtime(q);let changes=0;r.f.on('state.change',()=>changes++);
  for(let i=0;i<5;i++)r.f.tick(1/30);
  assert.ok(changes>3*100,'the limit is hit on consecutive ticks: '+changes);
  assert.match(r.f.diagnostics.at(-1).message,/State machine m stopped: event cycle limit exceeded on 3 consecutive ticks/);
  changes=0;for(let i=0;i<10;i++)r.f.tick(1/30);r.f.emit('go');
  assert.equal(changes,0,'a stopped machine ignores timers and events');r.f.destroy();
  // One-off cycles (a host emit starting the ping-pong without timers) do not stop anything.
  q.stateMachines[0].transitions=q.stateMachines[0].transitions.filter(t=>t.after==null);
  const s=runtime(q);for(let i=0;i<4;i++){s.f.emit('go');s.f.tick(1/30);s.f.tick(1/30);}
  assert.ok(s.f.diagnostics.some(e=>/Event cycle limit/.test(e.message)));
  assert.ok(!s.f.diagnostics.some(e=>/stopped/.test(e.message)));s.f.destroy();
});
test('runtime auto-start honours data-splinely-max-fps and data-splinely-native-animations on the root svg',()=>{
  const {createEnv}=require('./fake-waapi.cjs');
  const start=attrs=>{
    const {env,svg}=createEnv(`<svg ${attrs}><circle id="n" cx="0" cy="0" r="1"/></svg>`,{io:true});
    const meta={id:'splinely-project',tagName:'metadata',parentNode:svg,getAttribute:()=>null,textContent:JSON.stringify({schemaVersion:1,nodes:[{id:'n',kind:{type:'circle',cx:0,cy:0,radius:1}}],
      clips:[{id:'a',duration:1,looping:true,autoplay:true,tracks:[{nodeId:'n',property:'x',keyframes:[frame(0,number(0)),frame(1,number(10))]}]}]})};
    const all=svg.querySelectorAll;svg.querySelectorAll=sel=>sel==='[id]'?all(sel):[meta];
    env.document.currentScript={ownerSVGElement:svg};env.window=env;env.matchMedia=()=>({matches:false});env.console={error(e){throw e;}};
    vm.runInNewContext(fs.readFileSync(require.resolve('../src/splinely.js'),'utf8'),env);assert.ok(svg.splinely);
    const el=svg.all[0];let ticks=0,last=el.getAttribute('transform');
    for(let i=0;i<60;i++){env.advanceTo(env.time+1000/60);const v=el.getAttribute('transform');if(v!==last){ticks++;last=v;}}
    svg.splinely.destroy();return {ticks,animate:env.animateCalls};
  };
  const plain=start(''),capped=start('data-splinely-max-fps="20" data-splinely-native-animations="false"'),bad=start('data-splinely-max-fps="0" data-splinely-native-animations="no"');
  assert.ok(plain.animate>0 && bad.animate>0,'native animations stay the default');
  assert.equal(capped.animate,0);assert.ok(capped.ticks>=18 && capped.ticks<=22,'20 fps: '+capped.ticks);
});
test('throttled runtimes share the frame grid: several maxFps 30 runtimes tick in the same frames',()=>{
  const {createEnv}=require('./fake-waapi.cjs');const SVG='<svg><circle id="n" cx="0" cy="0" r="1"/></svg>';
  const {env,svg}=createEnv(SVG,{io:true,waapi:false}),svgs=[svg];
  const proj=()=>({schemaVersion:1,nodes:[{id:'n',kind:{type:'circle',cx:0,cy:0,radius:1}}],clips:[{id:'a',duration:1,looping:true,autoplay:true,tracks:[{nodeId:'n',property:'x',keyframes:[frame(0,number(0)),frame(1,number(10))]}]}]});
  const apis=[createRuntime(svg,proj(),{maxFps:30,reducedMotion:false})];
  for(let k=1;k<4;k++){env.advanceTo(env.time+1000/60*k);const s=env.addSvg(SVG);svgs.push(s);apis.push(createRuntime(s,proj(),{maxFps:30,reducedMotion:false}));}
  env.advanceTo(env.time+100);
  const el=s=>s.all[0],last=svgs.map(s=>el(s).getAttribute('transform'));let any=0,each=0;
  for(let i=0;i<120;i++){env.advanceTo(env.time+1000/60);let changed=false;svgs.forEach((s,k)=>{const v=el(s).getAttribute('transform');if(v!==last[k]){changed=true;each++;last[k]=v;}});if(changed)any++;}
  assert.ok(each>=4*55 && each<=4*65,'each runtime ticks at ~30 fps: '+each);
  assert.ok(any<=65,'all runtimes tick in the same ~60 of 120 frames: '+any);
  for(const a of apis)a.destroy();
});
