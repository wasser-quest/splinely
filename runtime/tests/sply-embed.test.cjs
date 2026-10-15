'use strict';
// SPLY embedding in exported SVGs: <metadata id="splinely-project" data-encoding="sply-base64"> vs legacy JSON text,
// plus the guard that the decoder injected into runtime/src/splinely.js is exactly runtime/src/sply_decode.js.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const lib=require('../src/splinely.js');
const {crc32c}=require('../src/sply_decode.js');
const {inject,DECODER,RUNTIME}=require('../../scripts/sync-runtime-decoder.cjs');
const ROOT=path.join(__dirname,'..','..'),EX=path.join(ROOT,'examples');

class Element {
  constructor(id,tag,attrs){this.id=id;this.tagName=tag;this.attrs=new Map(Object.entries(attrs||{}));this.listeners=new Map();this.parentNode=null;this.children=[];}
  getAttribute(k){return this.attrs.has(k)?this.attrs.get(k):null;}
  setAttribute(k,v){this.attrs.set(k,String(v));}
  removeAttribute(k){this.attrs.delete(k);}
  addEventListener(){}removeEventListener(){}
}
// Fake DOM of an exported example (elements/attributes only) plus a <metadata id="splinely-project"> block.
function domFromSvg(file,meta){
  let text=fs.readFileSync(file,'utf8');
  text=text.replace(/<!--[\s\S]*?-->/g,'').replace(/<metadata[\s\S]*?<\/metadata>/g,'').replace(/<script[\s\S]*?<\/script>/g,'').replace(/<title[\s\S]*?<\/title>/g,'').replace(/<desc[\s\S]*?<\/desc>/g,'');
  const svg=new Element('root','svg'),all=[],stack=[svg];let first=true;
  svg.ownerDocument={defaultView:{}};
  for(const m of text.matchAll(/<(\/?)([A-Za-z][\w:-]*)([^>]*?)(\/?)>/g)){
    if(m[1]){if(stack.length>1)stack.pop();continue;}
    if(first){first=false;continue;}
    const attrs={};for(const a of m[3].matchAll(/([\w:-]+)="([^"]*)"/g))attrs[a[1]]=a[2].replace(/&quot;/g,'"').replace(/&amp;/g,'&');
    const el=new Element(attrs.id,m[2],attrs);el.parentNode=stack[stack.length-1];el.ownerSVGElement=svg;all.push(el);
    if(!m[4])stack.push(el);
  }
  const block=new Element('splinely-project','metadata',{id:'splinely-project',...(meta.encoding?{'data-encoding':meta.encoding}:{})});
  block.textContent=meta.text;block.parentNode=svg;block.ownerSVGElement=svg;
  const withId=all.filter(e=>e.id);
  svg.querySelectorAll=sel=>sel==='[id]'?withId:sel.includes('splinely-project')?(meta?[block]:[]):[];
  svg.querySelector=()=>null;
  return {svg,all:withId};
}
// Minimal test-only SPLY encoder (docs/sply-binary-format.md): JSON value -> .sply bytes. Integers use tags 3/4,
// everything else tag 5. (Rust-produced files are cross-checked by the shared fixture manifest in sply-decode.test.cjs.)
function encodeSply(root){
  const strings=[],index=new Map(),body=[];
  const intern=s=>{if(!index.has(s)){index.set(s,strings.length);strings.push(Buffer.from(s,'utf8'));}return index.get(s);};
  const varint=v=>{const out=[];do{let c=Number(v&0x7fn);v>>=7n;if(v>0n)c|=0x80;out.push(c);}while(v>0n);return Buffer.from(out);};
  const f64=v=>{const b=Buffer.alloc(8);b.writeDoubleLE(v);return b;};
  const val=x=>{
    if(x===null)return body.push(Buffer.from([0]));
    if(x===false)return body.push(Buffer.from([1]));
    if(x===true)return body.push(Buffer.from([2]));
    if(typeof x==='number'){
      if(Number.isInteger(x)&&!Object.is(x,-0)){const b=BigInt(x);return b>=0n?body.push(Buffer.from([3]),varint(b)):body.push(Buffer.from([4]),varint(-b-1n));}
      return body.push(Buffer.from([5]),f64(x));
    }
    if(typeof x==='string')return body.push(Buffer.from([6]),varint(BigInt(intern(x))));
    if(Array.isArray(x)){body.push(Buffer.from([7]),varint(BigInt(x.length)));return x.forEach(val);}
    const ks=Object.keys(x);body.push(Buffer.from([8]),varint(BigInt(ks.length)));
    for(const k of ks){body.push(varint(BigInt(intern(k))));val(x[k]);}
  };
  val(root);
  const offs=[0];for(const b of strings)offs.push(offs[offs.length-1]+b.length);
  const u32=v=>{const b=Buffer.alloc(4);b.writeUInt32LE(v>>>0);return b;};
  const sdata=Buffer.concat(strings),bodyBuf=Buffer.concat(body);
  const tail=Buffer.concat([...offs.map(u32),sdata,bodyBuf]);
  const h=Buffer.alloc(32);
  Buffer.from([0x89,0x53,0x50,0x4c,0x59,0x0d,0x0a,0x1a]).copy(h);
  h.writeUInt16LE(1,8);h.writeUInt16LE(0,10);h.writeUInt32LE(0,12);
  h.writeUInt32LE(strings.length,16);h.writeUInt32LE(sdata.length,20);h.writeUInt32LE(bodyBuf.length,24);h.writeUInt32LE(crc32c(tail),28);
  return Buffer.concat([h,tail]);
}
const examples=fs.readdirSync(EX).filter(f=>f.endsWith('.splinely.json')&&fs.existsSync(path.join(EX,f.replace('.splinely.json','.svg')))).map(f=>f.slice(0,-14)).sort();
const jsonText=n=>fs.readFileSync(path.join(EX,n+'.splinely.json'),'utf8');
const splyBytes=n=>encodeSply(JSON.parse(jsonText(n)));
const jsonMeta=n=>({text:jsonText(n)});
const binMeta=n=>({encoding:'sply-base64',text:splyBytes(n).toString('base64')});
const TIMES=[0,0.25,0.5,1,1.7,2.3,3.9,8];
function observe(n,meta){
  const dom=domFromSvg(path.join(EX,n+'.svg'),meta);
  const api=lib.createRuntime(dom.svg,undefined,{autoTick:false,reducedMotion:false});
  const frames=[];
  api.timeline.play();
  for(const t of TIMES){api.tick(t===0?0:0.2);frames.push({snap:api.snapshot(),attrs:dom.all.map(e=>[e.id,[...e.attrs.entries()]]),bones:api.bones()});}
  const source=api.sourceProject;api.destroy&&api.destroy();
  return {frames,source};
}

test('the example set covers the committed examples',()=>{assert.ok(examples.length>=6,examples.join());});

for(const name of examples){
  test(`binary and JSON metadata produce identical runtime state: ${name}`,()=>{
    const a=observe(name,jsonMeta(name)),b=observe(name,binMeta(name));
    assert.deepEqual(b.frames,a.frames);
    assert.deepEqual(b.source,a.source,'decoded project equals parsed JSON project');
  });
}

test('SPLY metadata is decoded once, without JSON.parse, and from raw blobs',()=>{
  const name='demo',bytes=splyBytes(name);
  const dom0=domFromSvg(path.join(EX,name+'.svg'),binMeta(name));
  const realParse=JSON.parse;let parses=0;
  JSON.parse=function(...a){parses++;return realParse.apply(this,a);};
  let api;
  try{api=lib.createRuntime(dom0.svg,undefined,{autoTick:false});}
  finally{JSON.parse=realParse;}
  assert.equal(parses,0,'binary metadata must not be JSON-parsed');
  const ref=lib.createRuntime(domFromSvg(path.join(EX,name+'.svg'),jsonMeta(name)).svg,undefined,{autoTick:false}).snapshot();
  assert.deepEqual(api.snapshot(),ref);
  for(const blob of [new Uint8Array(bytes),new Uint8Array(bytes).buffer,bytes,new DataView(new Uint8Array(bytes).buffer)]){
    const dom=domFromSvg(path.join(EX,name+'.svg'),jsonMeta(name));
    const r=lib.createRuntime(dom.svg,blob,{autoTick:false});
    assert.deepEqual(r.snapshot(),ref);
    assert.deepEqual(r.sourceProject,api.sourceProject);
  }
  assert.equal(globalThis.SplyDecode,undefined,'decoder must not leak onto the global object');
});

test('metadata errors are typed',()=>{
  const dom=meta=>domFromSvg(path.join(EX,'demo.svg'),meta).svg;
  const expect=(meta,code,re)=>assert.throws(()=>lib.readProject(dom(meta)),e=>e instanceof Error&&e.name==='SplinelyProjectError'&&e.code===code&&re.test(e.message),code);
  expect({text:'{nope'},'invalid-json',/Invalid splinely-project JSON/);
  expect({encoding:'sply-base64',text:'not base64!'},'invalid-sply',/Invalid splinely-project SPLY: sply:/);
  const good=binMeta('demo');
  const bytes=Buffer.from(good.text,'base64');bytes[bytes.length-3]^=0xff; // corrupt body: CRC mismatch
  expect({encoding:'sply-base64',text:bytes.toString('base64')},'invalid-sply',/sply:/);
  expect({encoding:'sply-base64',text:Buffer.from('{"nodes":[]}').toString('base64')},'invalid-sply',/sply:/);
  expect({encoding:'zip',text:'x'},'unsupported-encoding',/data-encoding: zip/);
  const empty=domFromSvg(path.join(EX,'demo.svg'),jsonMeta('demo'));empty.svg.querySelectorAll=()=>[];
  assert.throws(()=>lib.readProject(empty.svg),e=>e.name==='SplinelyProjectError'&&e.code==='missing-metadata'&&/Missing splinely-project metadata/.test(e.message));
  assert.throws(()=>lib.createRuntime(dom(jsonMeta('demo')),new Uint8Array([1,2,3]),{autoTick:false}),e=>e.code==='invalid-sply');
});

test('metadata lookup: direct SVG-namespace child of the root, exactly one candidate (decoys ignored)',()=>{
  const SVG_NS='http://www.w3.org/2000/svg';
  const base=()=>domFromSvg(path.join(EX,'demo.svg'),jsonMeta('demo'));
  const withBlocks=blocks=>{const d=base();d.svg.querySelectorAll=sel=>sel==='[id]'?d.all:blocks;return d.svg;};
  const real=()=>{const e=new Element('splinely-project','metadata',{id:'splinely-project'});e.textContent=jsonMeta('demo').text;return e;};
  const attach=(el,svg)=>{el.parentNode=svg;el.ownerSVGElement=svg;return el;};
  const run=blocks=>{const svg=withBlocks([]);svg.querySelectorAll=sel=>sel==='[id]'?[]:blocks.map(b=>attach(b,svg));return lib.readProject(svg);};
  assert.ok(run([real()]).nodes.length>0);
  // decoys: nested in <g>/<svg>/<foreignObject> (not a direct child) or another namespace
  const nested=real();
  const svg=withBlocks([]);svg.querySelectorAll=sel=>sel==='[id]'?[]:[nested,attach(real(),svg)];nested.parentNode=new Element('g','g');nested.ownerSVGElement=svg;
  assert.ok(lib.readProject(svg).nodes.length>0,'a nested decoy before the real block is ignored');
  const foreign=real();foreign.namespaceURI='http://www.w3.org/1999/xhtml';
  const svg2=withBlocks([]);svg2.querySelectorAll=sel=>sel==='[id]'?[]:[attach(foreign,svg2)];
  assert.throws(()=>lib.readProject(svg2),e=>e.code==='missing-metadata');
  const bogus=real();bogus.textContent='{decoy';bogus.namespaceURI=SVG_NS;bogus.parentNode=new Element('svg','svg');
  const svg3=withBlocks([]);svg3.querySelectorAll=sel=>sel==='[id]'?[]:[bogus,attach(real(),svg3)];
  assert.ok(lib.readProject(svg3).nodes.length>0,'a nested-svg decoy before the real block is ignored');
  // two real candidates: ambiguous, never "first wins"
  assert.throws(()=>run([real(),real()]),e=>e.name==='SplinelyProjectError'&&e.code==='ambiguous-metadata'&&/Multiple splinely-project/.test(e.message));
  const attr=real();attr.id='other';attr.attrs.set('data-splinely-project','');attr.dataSplinelyProject=true;
  assert.throws(()=>run([real(),attr]),e=>e.code==='ambiguous-metadata');
});

test('base64 whitespace/line wrapping is accepted in metadata',()=>{
  const m=binMeta('demo');m.text=m.text.replace(/(.{76})/g,'$1\n');
  assert.ok(lib.readProject(domFromSvg(path.join(EX,'demo.svg'),m).svg).nodes.length>0);
});

test('splinely.js carries an exact copy of runtime/src/sply_decode.js between the markers',()=>{
  const runtime=fs.readFileSync(RUNTIME,'utf8');
  assert.equal(runtime.split('// <sply-decode>').length,2);
  assert.equal(runtime.split('// </sply-decode>').length,2);
  assert.equal(inject(runtime,fs.readFileSync(DECODER,'utf8')),runtime,'stale: run `node scripts/sync-runtime-decoder.cjs`');
  assert.notEqual(inject(runtime,fs.readFileSync(DECODER,'utf8')+'\n// drift\n'),runtime,'the guard must detect decoder drift');
  assert.throws(()=>inject('no markers',fs.readFileSync(DECODER,'utf8')),/marker pair/);
});

test('size comparison: desert-stickfight SVG metadata, JSON text vs base64 SPLY',()=>{
  const json=fs.readFileSync(path.join(EX,'desert-stickfight.splinely.json'),'utf8');
  const compact=JSON.stringify(JSON.parse(json));
  const bin=splyBytes('desert-stickfight');
  const b64=bin.toString('base64');
  console.log(`desert-stickfight metadata bytes: compact JSON=${compact.length} base64 SPLY=${b64.length} (binary file ${bin.length})`);
  assert.ok(b64.length<compact.length);
});
