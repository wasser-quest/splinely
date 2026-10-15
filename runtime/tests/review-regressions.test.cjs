'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {createRuntime,evaluateTrack}=require('../src/splinely.js');
function mount(extra={},env={},opts={}) {
 const attrs=new Map(); const n={id:'n',tagName:'circle',getAttribute:k=>attrs.get(k)||null,setAttribute:(k,v)=>attrs.set(k,String(v)),removeAttribute:k=>attrs.delete(k)};
 const svg={id:'root',ownerDocument:{defaultView:env},querySelectorAll:()=>[n],addEventListener(){},removeEventListener(){}};
 n.ownerSVGElement=svg;
 const project={nodes:[{id:'n',kind:{type:'circle',radius:5},transform:{x:0,y:0,rotation:0,scaleX:1,scaleY:1},style:{stroke:null,fill:'#fff'}}],clips:[{id:'a',duration:2,tracks:[]}],...extra};
 return {f:createRuntime(svg,project,{autoTick:false,...opts}),attrs};
}
test('preserving customJs is not an error and does not execute it',()=>{
 delete globalThis.__splinelyPreservedScriptRan;
 const {f}=mount({customJs:'globalThis.__splinelyPreservedScriptRan=true;'});
 assert.equal(globalThis.__splinelyPreservedScriptRan,undefined);assert.deepEqual(f.diagnostics,[]);f.destroy();
});
test('snapshot is observational, detached and available after destroy',()=>{
 const {f}=mount();f.clip('a').play();f.tick(0.5);
 const s=f.snapshot();assert.equal(s.time,0.5);assert.equal(s.clips[0].time,0.5);assert.equal(s.audio.state,'locked');
 s.clips[0].time=999;s.diagnostics.push('fake');s.audio.ready=true;
 assert.equal(f.clip('a').time,0.5);assert.equal(f.diagnostics.length,0);assert.equal(f.audio.ready,false);
 f.destroy();assert.equal(f.snapshot().destroyed,true);
});
test('host errors share Error payloads and bounded diagnostic history',()=>{
 const {f}=mount();let last;f.on('error',payload=>{last=payload;});
 for(let i=0;i<105;i++)f.reportError('host '+i);
 assert.equal(f.diagnostics.length,100);assert.ok(last.error instanceof Error);
 assert.equal(last.error.message,'host 104');assert.equal(f.diagnostics[0].message,'host 5');
 assert.deepEqual(f.snapshot().diagnostics,f.diagnostics.map(e=>e.message));
 f.destroy();assert.throws(()=>f.reportError('late'),/destroyed/);
});
test('resource attributes, CSS escapes and prototype aliases are rejected',()=>{
 const {f,attrs}=mount();f.set('n',{fill:'u\\72l(http://example.invalid/a)',srcset:'http://example.invalid/a 1x',constructor:1});
 assert.ok(!attrs.has('srcset'));assert.ok(!String(attrs.get('fill')).includes('example.invalid'));assert.ok(f.diagnostics.length>=3);
 f.set('n',{fill:{r:1,g:0,b:0,a:'bogus'}});assert.notEqual(attrs.get('fill-opacity'),'NaN');f.destroy();
});
test('reserved clock id and destroy disposers',()=>{
 assert.throws(()=>mount({clips:[{id:'@timeline',duration:1}]}),/Reserved clip/);
 const {f}=mount();let count=0;f.onDestroy(()=>count++);f.destroy();f.destroy();assert.equal(count,1);assert.equal(f.destroyed,true);
});
test('finite extreme interpolation retains endpoints and midpoint',()=>{
 const track={keyframes:[{time:0,value:1e308},{time:1,value:-1e308}]};
 assert.equal(evaluateTrack(track,0),1e308);assert.equal(evaluateTrack(track,0.5),0);assert.equal(evaluateTrack(track,1),-1e308);
});
test('no-stroke, tagged transforms, nonfinite and URL array guards',()=>{
 const {f,attrs}=mount();assert.equal(attrs.get('stroke'),'none');f.set('n',{x:{type:'number',value:7}});assert.match(attrs.get('transform'),/^translate\(7 /);
 f.set('n',{fill:['url(https://example.invalid/paint.svg#x)'],x:Infinity});assert.ok(!String(attrs.get('fill')).includes('url('));assert.match(attrs.get('transform'),/^translate\(7 /);assert.ok(f.diagnostics.length>=2);f.destroy();
});
test('reentrant tick is rejected without duplicate or reordered crossings',()=>{
 const {f}=mount(), seen=[];
 f.timeline.at(0.5,()=>f.tick(0.5));f.timeline.at(0.5,()=>seen.push(0.5),{clip:'a'});f.timeline.at(0.75,()=>seen.push(0.75),{clip:'a'});
 f.clip('a').play();f.tick(0.5);assert.deepEqual(seen,[0.5]);assert.match(f.diagnostics[0].message,/Reentrant tick/);f.tick(0.25);assert.deepEqual(seen,[0.5,0.75]);f.destroy();
});
test('zero global callback can cancel local play crossing',()=>{
 const {f}=mount(), seen=[];f.timeline.at(0,()=>f.clip('a').stop());f.timeline.at(0,()=>seen.push('local'),{clip:'a'});f.clip('a').play();assert.deepEqual(seen,[]);assert.equal(f.clip('a').playing,false);f.destroy();
});
test('timeline async rejection reaches diagnostics',async()=>{
 const {f}=mount();f.timeline.at(0.5,async()=>{throw Error('async fault');});f.timeline.play();f.tick(0.5);await Promise.resolve();assert.equal(f.diagnostics[0].message,'async fault');f.destroy();
});
test('play and resume do not include idle elapsed wall time',()=>{
 let wall=0, frame;const env={performance:{now:()=>wall},requestAnimationFrame:fn=>(frame=fn,1),cancelAnimationFrame(){}};
 const {f}=mount({},env,{autoTick:true});wall=900;f.timeline.play();wall=1000;frame();assert.equal(f.timeline.time,0.1);
 wall=1100;f.timeline.pause();assert.equal(f.timeline.time,0.2);wall=1900;f.timeline.play();wall=2000;frame();assert.ok(Math.abs(f.timeline.time-0.3)<1e-12);f.destroy();
});
