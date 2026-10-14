'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const {createRuntime,evaluateTrack,easing,normalize}=require('../src/forma.js');
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
  svg.querySelectorAll=selector=>selector==='[id]'?children:children.filter(e=>e.id==='forma-project' || e.dataFormaProject);
  svg.querySelector=selector=>children.find(e=>selector==='[id="forma-project"]'?e.id==='forma-project':!!e.dataFormaProject)||null;
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
  const {f,svg}=runtime();const control=new Element('','g');control.setAttribute('data-forma-host-control','');control.parentNode=svg;
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
  f.destroy();assert.equal(svg.forma,undefined);assert.ok([...svg.listeners.values()].every(s=>s.size===0));assert.throws(()=>f.clip('a'),/destroyed/);
});

test('multiple instances own IDs, transports, global alias and preexisting globals',()=>{
  const env={};const a=runtime(project(),env),b=runtime(project(),env);assert.equal(env.forma,undefined);a.f.set('n',{cx:99});assert.equal(b.n.getAttribute('cx'),'5');a.f.clip('a').play();a.f.tick(1);assert.equal(b.f.clip('a').time,0);b.f.destroy();assert.equal(env.forma,a.f);a.f.destroy();assert.equal(env.forma,undefined);
  const previous={host:true};const c=runtime(project(),{forma:previous});assert.equal(c.svg.ownerDocument.defaultView.forma,previous);c.f.destroy();assert.equal(c.svg.ownerDocument.defaultView.forma,previous);
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

test('speech disabled by default with honest diagnostic; explicitly enabled cue crossing never replays on seek',()=>{
  const spoken=[];let cancelled=0;const env={speechSynthesis:{speak:u=>spoken.push(u),cancel:()=>cancelled++},SpeechSynthesisUtterance:class{constructor(text){this.text=text;}}};
  const {f}=runtime(project(),env);assert.equal(f.speech.say('hi'),null);assert.match(f.diagnostics.at(-1).message,/opt-in/);f.destroy();
  const {f:g}=runtime(project({speechCues:[{time:1,text:'hello',lang:'en'}]}),env,['n'],{speech:true});g.timeline.play();g.tick(2);assert.equal(spoken.length,1);g.timeline.seek(0.5);assert.equal(cancelled,1);assert.equal(spoken.length,1);g.tick(0.5);assert.equal(spoken.length,2);g.destroy();
});

test('speech picks the most natural voice for the language and splits long text into sentences',()=>{
  const voices=[
    {name:'espeak-ng German',lang:'de',localService:true,default:true},
    {name:'Google Deutsch',lang:'de-DE',localService:false},
    {name:'Microsoft Katja Online (Natural) - German (Germany)',lang:'de-DE',localService:false},
    {name:'Samantha',lang:'en-US',localService:true},
  ];
  const spoken=[];const env={speechSynthesis:{speak:u=>spoken.push(u),cancel(){},getVoices:()=>voices},SpeechSynthesisUtterance:class{constructor(text){this.text=text;}}};
  const {f}=runtime(project(),env,['n'],{speech:true});
  f.speech.say('Hallo.',{lang:'de-DE'});assert.equal(spoken[0].voice.name,voices[2].name);
  assert.deepEqual(f.speech.voices('de-DE').map(v=>v.name),[voices[2].name,voices[1].name,voices[0].name]);
  f.speech.say('Hi',{lang:'en-US',voice:'Samantha'});assert.equal(spoken[1].voice.name,'Samantha');
  spoken.length=0;const long=Array.from({length:6},(_,i)=>`Satz Nummer ${i} ist absichtlich recht lang, damit der Text geteilt wird.`).join(' ');
  f.speech.say(long,{lang:'de'});assert.ok(spoken.length>1,'split');assert.ok(spoken.every(u=>u.text.length<=200));assert.equal(spoken.map(u=>u.text).join(' '),long);
  // Without any voice list the browser default is used.
  const plain=[];const {f:g}=runtime(project(),{speechSynthesis:{speak:u=>plain.push(u),cancel(){}},SpeechSynthesisUtterance:class{constructor(text){this.text=text;}}},['n'],{speech:true});
  g.speech.say('Hallo',{lang:'de'});assert.equal(plain[0].voice,undefined);f.destroy();g.destroy();
});

test('malformed data, callback faults, script properties and unknown IDs are handled',()=>{
  assert.throws(()=>runtime({schemaVersion:2}),/schemaVersion/);assert.throws(()=>runtime({clips:[{id:'a',duration:0}]}),/positive/);
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
  const svg=root(['data']);svg.children[0].dataFormaProject=true;svg.children[0].textContent=JSON.stringify(project({nodes:[],clips:[]}));const f=createRuntime(svg,null,{autoTick:false});f.destroy();
  const bad=root();assert.throws(()=>createRuntime(bad,{machines:[{id:'m',initial:'bad',states:[]}]},{autoTick:false}),/initial state/);assert.equal(bad.forma,undefined);
  assert.throws(()=>createRuntime(bad,{audioCues:[{time:-1}]},{autoTick:false}),/nonnegative/);
});

test('SVG metadata auto-mount runs as a plain browser script, JSON stays data, and malformed export retains fallback',()=>{
  const env={console:{error(){}}},svg=root(['n','forma-project'],env);svg.children[1].tagName='metadata';svg.children[1].textContent=JSON.stringify(project({name:'</script> & ]]>',customJs:'throw new Error("must not execute")'}));env.document={currentScript:{ownerSVGElement:svg}};env.window=env;
  vm.runInNewContext(fs.readFileSync(require.resolve('../src/forma.js'),'utf8'),env);assert.ok(svg.forma);assert.equal(env.forma,svg.forma);assert.equal(svg.children[0].getAttribute('cx'),'5');svg.forma.destroy();
  svg.children[1].textContent='{bad';svg.children[0].setAttribute('cx','100');vm.runInNewContext(fs.readFileSync(require.resolve('../src/forma.js'),'utf8'),env);assert.equal(svg.forma,undefined);assert.equal(svg.children[0].getAttribute('cx'),'100');
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
