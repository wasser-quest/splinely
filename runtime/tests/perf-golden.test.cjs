'use strict';
// Golden comparison of the optimised runtime (runtime/src/splinely.js) against a frozen copy of the
// implementation taken before the hot-path work (fixtures/splinely-reference.js). Both run the same
// scripted sequence of ticks, seeks, state changes, overrides, variable changes and pointer events on
// identical fake DOMs; after every step the full attribute maps, snapshot(), bones() and event logs must match.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const optimized=require('../src/splinely.js');
const reference=require('./fixtures/splinely-reference.js');
const ROOT=path.join(__dirname,'..','..');

class Element {
  constructor(id,tag,attrs){this.id=id;this.tagName=tag;this.attrs=new Map(Object.entries(attrs||{}));this.listeners=new Map();this.parentNode=null;this.children=[];this.writes=0;}
  getAttribute(k){return this.attrs.has(k)?this.attrs.get(k):null;}
  setAttribute(k,v){this.writes++;this.attrs.set(k,String(v));}
  removeAttribute(k){this.writes++;this.attrs.delete(k);}
  addEventListener(k,fn){if(!this.listeners.has(k))this.listeners.set(k,new Set());this.listeners.get(k).add(fn);}
  removeEventListener(k,fn){this.listeners.get(k)?.delete(fn);}
  fire(type,event={}){for(const fn of [...(this.listeners.get(type)||[])])fn({target:this,type,...event});}
  setPointerCapture(){}releasePointerCapture(){}
}
function makeDom(specs){
  const svg=new Element('root','svg'),all=[];
  svg.ownerDocument={defaultView:{}};
  const stack=[svg];
  for(const s of specs){
    if(s.close){stack.pop();continue;}
    const el=new Element(s.id,s.tag,s.attrs);const parent=stack[stack.length-1];
    el.parentNode=parent;el.ownerSVGElement=svg;parent.children.push(el);all.push(el);
    if(!s.selfClose)stack.push(el);
  }
  const withId=all.filter(e=>e.id);
  svg.querySelectorAll=sel=>sel==='[id]'?withId:[];
  svg.querySelector=()=>null;
  svg.getScreenCTM=()=>({inverse:()=>({})});
  svg.createSVGPoint=()=>({x:0,y:0,matrixTransform(){return {x:this.x/2,y:this.y/2};}});
  return {svg,all:withId};
}
// Minimal SVG reader for the checked-in examples: elements, attributes and nesting (gradient stops).
function domFromSvg(file){
  let text=fs.readFileSync(file,'utf8');
  text=text.replace(/<!--[\s\S]*?-->/g,'').replace(/<metadata[\s\S]*?<\/metadata>/g,'').replace(/<script[\s\S]*?<\/script>/g,'').replace(/<title[\s\S]*?<\/title>/g,'').replace(/<desc[\s\S]*?<\/desc>/g,'');
  const specs=[];let first=true;
  for(const m of text.matchAll(/<(\/?)([A-Za-z][\w:-]*)([^>]*?)(\/?)>/g)){
    if(m[1]){specs.push({close:true});continue;}
    if(first){first=false;continue;} // the root <svg> is synthesised
    const attrs={};for(const a of m[3].matchAll(/([\w:-]+)="([^"]*)"/g))attrs[a[1]]=a[2].replace(/&quot;/g,'"').replace(/&amp;/g,'&');
    specs.push({id:attrs.id,tag:m[2],attrs,selfClose:!!m[4]});
  }
  // drop the trailing </svg>
  if(specs.length && specs[specs.length-1].close)specs.pop();
  return makeDom(specs);
}
const noop=()=>{};
function boot(lib,project,dom){
  const log=[];
  const api=lib.createRuntime(dom.svg,JSON.parse(JSON.stringify(project)),{autoTick:false,reducedMotion:false});
  api.on('state.change',d=>log.push(['state.change',d.machine,d.from,d.to,d.event]));
  api.on('clip.end',d=>log.push(['clip.end',d.clip]));
  api.on('error',d=>log.push(['error',d.error.message]));
  api.on('variable.change',d=>log.push(['variable',d.id,d.value]));
  return {api,log,dom};
}
function dump(r){
  return {
    attrs:r.dom.all.map(e=>[e.id,[...e.attrs.entries()]]),
    snapshot:r.api.snapshot(),
    bones:r.api.bones(),
    log:r.log.slice(),
  };
}
function rng(seed){let s=seed>>>0||1;return ()=>{s=(s*1664525+1013904223)>>>0;return s/4294967296;};}
const arr=v=>Array.isArray(v)?v:Object.entries(v||{}).map(([id,x])=>({...x,id:x.id||id}));
function makeOps(project,seed,count){
  const rand=rng(seed),ops=[];
  const clips=arr(project.clips),machines=arr(project.stateMachines||project.machines),nodes=arr(project.nodes);
  const vars=Object.entries(project.variables||{});
  const pick=a=>a[Math.floor(rand()*a.length)];
  const events=new Set();
  for(const m of machines)for(const t of [...(m.transitions||[]),...arr(m.states).flatMap(s=>s.transitions||[])]){const e=t.event;if(e)events.add(typeof e==='string'?e:e.name);}
  const dts=[0,1/60,1/60,1/30,0.05,0.2,0.75,2.6];
  for(let i=0;i<count;i++){
    const r=rand();
    if(r<0.45)ops.push({op:'tick',dt:pick(dts)});
    else if(r<0.55&&clips.length)ops.push({op:'play',clip:pick(clips).id});
    else if(r<0.60&&clips.length)ops.push({op:'pause',clip:pick(clips).id});
    else if(r<0.68&&clips.length){const c=pick(clips);ops.push({op:'seek',clip:c.id,time:Math.round(rand()*c.duration*1.1*1000)/1000});}
    else if(r<0.72&&clips.length)ops.push({op:'stop',clip:pick(clips).id});
    else if(r<0.76&&clips.length)ops.push({op:'weight',clip:pick(clips).id,weight:Math.round(rand()*10)/10,fade:rand()<0.5?0:Math.round(rand()*100)/100});
    else if(r<0.80&&clips.length)ops.push({op:'loop',clip:pick(clips).id,value:rand()<0.5});
    else if(r<0.84)ops.push({op:'timeline',kind:pick(['seek','play','pause']),time:Math.round(rand()*3*100)/100});
    else if(r<0.89&&nodes.length)ops.push({op:'set',node:pick(nodes).id,props:rand()<0.3?null:pick([{opacity:0.5},{opacity:Math.round(rand()*10)/10},{x:Math.round(rand()*100)},{rotation:Math.round(rand()*360)},{fill:{r:rand(),g:0.5,b:0.1,a:1}}])});
    else if(r<0.93&&vars.length){const [k,v]=pick(vars);ops.push({op:'variable',name:k,value:typeof v==='number'?Math.round(rand()*100)/10:v});}
    else if(r<0.98&&events.size)ops.push({op:'send',machine:pick(machines).id,event:pick([...events])});
    else ops.push({op:'pointer',type:pick(['pointermove','pointerdown','pointermove','pointerup','pointerleave']),x:Math.round(rand()*400),y:Math.round(rand()*300)});
  }
  return ops;
}
function apply(r,o){
  const f=r.api;
  try{
    switch(o.op){
      case 'tick':f.tick(o.dt);break;
      case 'play':f.clip(o.clip).play();break;
      case 'pause':f.clip(o.clip).pause();break;
      case 'seek':f.clip(o.clip).seek(o.time);break;
      case 'stop':f.clip(o.clip).stop();break;
      case 'weight':if(o.fade)f.clip(o.clip).fadeTo(o.weight,o.fade);else f.clip(o.clip).setWeight(o.weight);break;
      case 'loop':f.clip(o.clip).loop=o.value;break;
      case 'timeline':if(o.kind==='seek')f.timeline.seek(o.time);else f.timeline[o.kind]();break;
      case 'set':f.set(o.node,o.props);break;
      case 'variable':f.variable(o.name).set(o.value);break;
      case 'send':f.machine(o.machine).send(o.event);break;
      case 'pointer':r.dom.svg.fire(o.type,{clientX:o.x,clientY:o.y,pointerId:1,target:r.dom.svg});break;
    }
    return null;
  }catch(e){return e.message;}
}
function runGolden(project,dom,seed,count,label){
  const a=boot(reference,project,dom()),b=boot(optimized,project,dom());
  assert.deepEqual(dump(b),dump(a),label+': initial frame');
  const ops=makeOps(project,seed,count);let step=0;
  for(const o of ops){
    const ea=apply(a,o),eb=apply(b,o);step++;
    assert.equal(eb,ea,`${label}: step ${step} ${JSON.stringify(o)} threw differently`);
    assert.deepEqual(dump(b),dump(a),`${label}: step ${step} ${JSON.stringify(o)}`);
  }
  const wa=a.dom.all.reduce((s,e)=>s+e.writes,0)+a.dom.svg.writes,wb=b.dom.all.reduce((s,e)=>s+e.writes,0)+b.dom.svg.writes;
  assert.ok(wb<=wa,`${label}: optimised runtime wrote more attributes (${wb} > ${wa})`);
  a.api.destroy();b.api.destroy();
  return {wa,wb};
}

// The frozen reference runtime predates the opt-in accessibility layer (docs/accessibility.md). Examples that now carry
// a11y data are compared without it (the layer adds attributes/listeners by design); the layer itself is covered by
// runtime/tests/accessibility.test.cjs, and a project without a11y data stays identical to the reference here.
function withoutA11y(project){
  const copy=JSON.parse(JSON.stringify(project));
  delete copy.description;
  for(const n of copy.nodes||[])delete n.a11y;
  for(const m of copy.stateMachines||copy.machines||[])for(const st of m.states||[])delete st.announce;
  return copy;
}
const examples=['anime-hello','brown-owl','cactus-runner','demo','desert-stickfight','random-figures','rig','stickfight','water-drop'];
for(const name of examples){
  test(`golden: examples/${name} matches the reference implementation (DOM, snapshot, bones, events)`,()=>{
    const project=withoutA11y(JSON.parse(fs.readFileSync(path.join(ROOT,'examples',name+'.splinely.json'),'utf8')));
    const svg=path.join(ROOT,'examples',name+'.svg');
    const count=name==='desert-stickfight'?120:name==='stickfight'?160:300;
    const {wa,wb}=runGolden(project,()=>domFromSvg(svg),0xC0FFEE^name.length,count,name);
    assert.ok(wb<wa||wa===0,`${name}: expected fewer DOM writes (${wb} vs ${wa})`);
  });
}
test('golden: blend-space fixture matches the reference implementation',()=>{
  const project=JSON.parse(fs.readFileSync(path.join(__dirname,'fixtures','blend-spaces.json'),'utf8'));
  runGolden(project,()=>makeDom([{id:'n',tag:'rect',attrs:{}}]),77,300,'blend-spaces');
});

// A project that touches every renderer branch: adapters, hidden nodes, bone-attached nodes, gradients,
// weighted/additive blending, blend spaces, state fades, timers, followers, motion paths, interactions.
const frame=(time,value,easing)=>easing===undefined?{time,value}:{time,value,easing};
const num=value=>({type:'number',value});
const featureProject={
  schemaVersion:1,
  variables:{speed:1,turn:0,flag:true},
  nodes:[
    {id:'c1',kind:{type:'circle',cx:30,cy:40,radius:4},transform:{x:2,y:3,rotation:10,scaleX:1,scaleY:2},style:{fill:{r:1,g:0,b:0,a:0.4},opacity:1,stroke:null,strokeWidth:2}},
    {id:'r1',kind:{type:'rect',x:0,y:0,width:20,height:10},transform:{x:5,y:5},style:{fill:{r:0.1,g:0.2,b:0.3,a:1},opacity:1},gradient:{radial:false,angle:30,stops:[{offset:0,color:'#ff0000'},{offset:1,color:'#0000ff'}]}},
    {id:'a1',properties:{cx:5,opacity:1}},
    {id:'h1',visible:false,properties:{x:1}},
    {id:'b1',kind:{type:'rect',x:0,y:0,width:6,height:6},boneId:'child',transform:{x:1,y:0}},
    {id:'route',visible:false,kind:{type:'path',d:'M0 0 L100 0 L100 100',bounds:[0,0,100,100]},transform:{x:10,y:0}},
    {id:'rider',kind:{type:'circle',cx:0,cy:0,radius:3},motionPath:{path:'route',orient:true}},
    {id:'t1',kind:{type:'text',x:2,y:20,text:'hi',fontSize:12}},
  ],
  bones:[{id:'root',x:10,y:10,rotation:0,length:20},{id:'child',parentId:'root',x:20,y:0,rotation:15,length:10}],
  clips:[
    {id:'main',duration:2,looping:true,tracks:[
      {nodeId:'c1',property:'x',keyframes:[frame(0,num(0)),frame(1,num(20),{type:'cubicBezier',x1:0.42,y1:0,x2:0.58,y2:1}),frame(1,num(25)),frame(2,num(5),'step')]},
      {nodeId:'c1',property:'fill',keyframes:[frame(0,{r:1,g:0,b:0,a:1}),frame(0.5,'#00ff0080',[0.25,0.1,0.25,1]),frame(2,{r:0,g:0,b:1,a:1})]},
      {nodeId:'c1',property:'rotation',keyframes:[frame(0,10),frame(2,370,{type:'step'})]},
      {nodeId:'c1',property:'radius',keyframes:[frame(0,4),frame(1,9),frame(2,4)]},
      {nodeId:'a1',property:'cx',keyframes:[frame(0,0),frame(2,40)]},
      {nodeId:'a1',property:'stroke',keyframes:[frame(0,'#102030'),frame(2,'#a0b0c0')]},
      {nodeId:'h1',property:'x',keyframes:[frame(0,1),frame(2,9)]},
      {nodeId:'root',property:'rotation',keyframes:[frame(0,0),frame(2,90)]},
      {nodeId:'b1',property:'y',keyframes:[frame(0,0),frame(2,6)]},
      {nodeId:'free',property:'opacity',keyframes:[frame(0,1),frame(2,0.2)]},
      {nodeId:'r1',property:'gradient',keyframes:[frame(0,{type:'gradient',value:{radial:false,angle:0,stops:[{offset:0,color:'#ff0000'},{offset:1,color:'#0000ff'}]}}),frame(2,{type:'gradient',value:{radial:false,angle:90,stops:[{offset:0.2,color:'#00ff00'},{offset:0.8,color:'#ffff00'}]}})]},
      {nodeId:'t1',property:'fontSize',keyframes:[frame(0,12),frame(2,24)]},
      {nodeId:'rider',property:'motionProgress',keyframes:[frame(0,num(0)),frame(2,num(1))]},
      {nodeId:'route',property:'x',keyframes:[frame(0,10),frame(2,30)]},
    ]},
    {id:'high',duration:1,priority:5,tracks:[{nodeId:'c1',property:'x',keyframes:[frame(0,100),frame(1,150)]},{nodeId:'a1',property:'opacity',keyframes:[frame(0,1),frame(1,0.1)]}]},
    {id:'w1',duration:2,blendMode:'weighted',weight:0.5,tracks:[{nodeId:'c1',property:'opacity',keyframes:[frame(0,0.2),frame(2,0.8)]},{nodeId:'a1',property:'cx',keyframes:[frame(0,10),frame(2,50)]},{nodeId:'c1',property:'fill',keyframes:[frame(0,'#ff0000'),frame(2,'#00ff00')]}]},
    {id:'w2',duration:2,blendMode:'weighted',weight:0.5,tracks:[{nodeId:'c1',property:'opacity',keyframes:[frame(0,1),frame(2,0.1)]},{nodeId:'a1',property:'cx',keyframes:[frame(0,0),frame(2,5)]}]},
    {id:'add',duration:2,blendMode:'additive',weight:0.7,tracks:[{nodeId:'c1',property:'x',keyframes:[frame(0,0),frame(2,10)]},{nodeId:'root',property:'rotation',keyframes:[frame(0,0),frame(2,20)]}]},
    {id:'bad',duration:1,tracks:[{nodeId:'a1',property:'bogus',keyframes:[frame(0,1),frame(1,2)]},{nodeId:'a1',property:'d',keyframes:[frame(0,'M0 0 L1 1'),frame(1,'M0 0 L5 5')]}]},
    {id:'unknown',duration:1,tracks:[{nodeId:'nope',property:'x',keyframes:[frame(0,1),frame(1,2)]}]},
  ],
  blendSpaces:[{id:'loco',parameters:['speed','turn'],samples:[{clipId:'w1',position:[0,0]},{clipId:'w2',position:[1,1]}]}],
  stateMachines:[{id:'m',initial:'idle',states:[
    {id:'idle',overrides:[{nodeId:'a1',property:'opacity',value:0.5},{nodeId:'c1',property:'fill',value:{r:0,g:1,b:0,a:1}},{nodeId:'root',property:'x',value:12}],transitions:[{event:'go',to:'run',blend:0.5},{after:1.5,to:'run',guard:{variable:'flag',equals:true}}]},
    {id:'run',overrides:{c1:{fill:{r:0,g:0,b:1,a:1},opacity:0.9},a1:{cx:77}},transitions:[{event:'go',to:'idle',blend:0.3},{event:'snap',to:'idle',actions:[{type:'setProperty',nodeId:'t1',property:'opacity',value:0.3}]}]},
  ]}],
  followers:[{id:'f1',target:'c1',mode:'offset',max:20,smoothing:0.3},{id:'f2',target:'child',mode:'rotate',max:30,smoothing:0.2,when:{machine:'m',states:['run']}}],
  interactions:[{target:'c1',event:'click',actions:[{type:'playClip',clipId:'main'}]}],
};
const featureDom=()=>makeDom([
  {id:'c1',tag:'circle',attrs:{cx:'30',cy:'40'}},{id:'g1',tag:'linearGradient',attrs:{}},{id:'s1',tag:'stop',attrs:{offset:'0'},selfClose:true},{id:'s2',tag:'stop',attrs:{offset:'1'},selfClose:true},{close:true},
  {id:'r1',tag:'rect',attrs:{fill:'url(#g1)'},selfClose:true},
  {id:'a1',tag:'circle',attrs:{r:'3',stroke:'#000',opacity:'0.9'},selfClose:true},
  {id:'b1',tag:'rect',attrs:{},selfClose:true},{id:'rider',tag:'circle',attrs:{},selfClose:true},{id:'t1',tag:'text',attrs:{},selfClose:true},
  {id:'free',tag:'g',attrs:{opacity:'0.8'},selfClose:true},
]);
// Gradient children must be nested under the gradient element.
function featureDomFixed(){const d=featureDom();return d;}
test('golden: feature project (adapters, bones, gradients, weighted/additive, blend spaces, fades, timers, followers, motion paths, diagnostics)',()=>{
  for(const seed of [1,2,3,4,5,6])runGolden(featureProject,featureDomFixed,seed*7919,400,'feature#'+seed);
});

test('golden: random keyframe tracks (easing kinds, duplicate times, colours, strings, paths) seek identically',()=>{
  const rand=rng(42);
  for(let round=0;round<12;round++){
    const n=1+Math.floor(rand()*14),times=[];
    for(let i=0;i<n;i++)times.push(Math.round(rand()*20)/10);
    times.sort((x,y)=>x-y);
    const easings=[undefined,'linear','step',{type:'step'},[0.3,0.1,0.6,1],{type:'cubicBezier',x1:0.9,y1:-0.5,x2:0.1,y2:1.5}];
    const kind=round%3;
    const val=i=>kind===0?num(Math.round(rand()*1000)/10-30):kind===1?(rand()<0.5?'#'+Math.floor(rand()*0xffffff).toString(16).padStart(6,'0'):{r:rand(),g:rand(),b:rand(),a:rand()}):'M0 0 L'+Math.round(rand()*50)+' '+Math.round(rand()*50);
    const prop=kind===0?'cx':kind===1?'fill':'d';
    const frames=times.map((t,i)=>frame(t,val(i),easings[Math.floor(rand()*easings.length)]));
    const project={schemaVersion:1,nodes:[{id:'n',properties:{cx:1}}],clips:[{id:'a',duration:2,tracks:[{target:'n',property:prop,keyframes:frames}]}]};
    const a=boot(reference,project,makeDom([{id:'n',tag:'path',attrs:{},selfClose:true}])),b=boot(optimized,project,makeDom([{id:'n',tag:'path',attrs:{},selfClose:true}]));
    for(const t of [0,...times,...times.map(x=>x+0.001),...times.map(x=>x-0.001),1/3,2,0.99999999,...Array.from({length:40},()=>rand()*2)]){
      const tt=Math.min(2,Math.max(0,t));
      a.api.clip('a').seek(tt);b.api.clip('a').seek(tt);
      assert.deepEqual(dump(b),dump(a),`round ${round} t=${tt}`);
    }
  }
});

test('idle runtime skips render and DOM writes; unchanged attributes are not rewritten',()=>{
  const project=JSON.parse(fs.readFileSync(path.join(ROOT,'examples','stickfight.splinely.json'),'utf8'));
  const r=boot(optimized,project,domFromSvg(path.join(ROOT,'examples','stickfight.svg')));
  const writes=()=>r.dom.all.reduce((s,e)=>s+e.writes,0);
  const f=r.api;
  for(const c of project.clips)f.clip(c.id).play();
  f.tick(0.1);
  for(const c of project.clips)f.clip(c.id).stop();
  const settled=writes();
  for(let i=0;i<20;i++)f.tick(1/60);
  assert.equal(writes(),settled,'idle ticks must not touch the DOM');
  // Re-applying identical values is also free; a real change writes only what differs.
  f.set(project.nodes[0].id,{opacity:0.5});const afterSet=writes();
  assert.ok(afterSet-settled<=3,`set of one property wrote ${afterSet-settled} attributes`);
  f.set(project.nodes[0].id,{opacity:0.5});assert.equal(writes(),afterSet);
});

test('idle ticks still reproduce render diagnostics exactly like the reference',()=>{
  const project={schemaVersion:1,nodes:[{id:'a',properties:{cx:1}}],clips:[{id:'c',duration:1,tracks:[{nodeId:'a',property:'bogus',keyframes:[frame(0,1),frame(1,2)]},{nodeId:'ghost',property:'x',keyframes:[frame(0,1),frame(1,2)]}]}]};
  const mk=lib=>boot(lib,project,makeDom([{id:'a',tag:'circle',attrs:{},selfClose:true}]));
  const a=mk(reference),b=mk(optimized);
  for(const r of [a,b]){r.api.clip('c').play();for(let i=0;i<5;i++)r.api.tick(0.1);r.api.clip('c').pause();for(let i=0;i<5;i++)r.api.tick(0.1);r.api.set('a',{cx:3});r.api.set('a',null);}
  assert.deepEqual(dump(b),dump(a));
  assert.ok(a.api.snapshot().diagnostics.length>5);
});
