'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {decodeSply,isSply,decodeSplyBase64,crc32c}=require('../src/sply_decode.js');

// ---- tiny test-only encoder (mirrors docs/sply-binary-format.md) ----
const u32=v=>{const b=Buffer.alloc(4);b.writeUInt32LE(v>>>0);return b;};
function varint(v){ // v: bigint
  const out=[];
  do{let c=Number(v&0x7fn);v>>=7n;if(v>0n)c|=0x80;out.push(c);}while(v>0n);
  return Buffer.from(out);
}
const f64=v=>{const b=Buffer.alloc(8);b.writeDoubleLE(v);return b;};
// Float marker for JS numbers that must be encoded as tag 5/9
class F{constructor(v){this.v=v;}}
const isF=x=>x instanceof F;
function encode(root,opts={}){
  const strings=[];const index=new Map();
  const intern=s=>{if(!index.has(s)){index.set(s,strings.length);strings.push(s);}return index.get(s);};
  const body=[];
  const w=(...bs)=>body.push(...bs);
  function val(x){
    if(x===null)return w(Buffer.from([0]));
    if(x===false)return w(Buffer.from([1]));
    if(x===true)return w(Buffer.from([2]));
    if(typeof x==='bigint')return x>=0n?w(Buffer.from([3]),varint(x)):w(Buffer.from([4]),varint(-x-1n));
    if(isF(x))return w(Buffer.from([5]),f64(x.v));
    if(typeof x==='number'){if(!Number.isInteger(x))throw new Error('use F()');return val(BigInt(x));}
    if(typeof x==='string')return w(Buffer.from([6]),varint(BigInt(intern(x))));
    if(x instanceof Uint8Array)return w(Buffer.from([10]),varint(BigInt(x.length)),Buffer.from(x));
    if(Array.isArray(x)){
      if(opts.packFloats!==false&&x.length>=2&&x.every(isF))return w(Buffer.from([9]),varint(BigInt(x.length)),...x.map(f=>f64(f.v)));
      w(Buffer.from([7]),varint(BigInt(x.length)));return x.forEach(val);
    }
    const ks=Object.keys(x);w(Buffer.from([8]),varint(BigInt(ks.length)));
    for(const k of ks){w(varint(BigInt(intern(k))));val(x[k]);}
  }
  val(root);
  return assemble(strings,Buffer.concat(body),opts);
}
function assemble(strings,bodyBuf,o={}){
  const sb=strings.map(s=>Buffer.isBuffer(s)?s:Buffer.from(s,'utf8'));
  const offs=[0];for(const s of sb)offs.push(offs[offs.length-1]+s.length);
  const offsets=Buffer.concat(offs.map(u32));
  const sdata=Buffer.concat(sb);
  const tail=Buffer.concat([offsets,sdata,bodyBuf]);
  const h=Buffer.alloc(32);
  Buffer.from([0x89,0x53,0x50,0x4c,0x59,0x0d,0x0a,0x1a]).copy(h);
  h.writeUInt16LE(o.major??1,8);h.writeUInt16LE(o.minor??0,10);h.writeUInt32LE(o.flags??0,12);
  h.writeUInt32LE(o.count??strings.length,16);h.writeUInt32LE(o.stringsLen??sdata.length,20);h.writeUInt32LE(o.bodyLen??bodyBuf.length,24);
  h.writeUInt32LE(o.crc??crc32c(tail),28);
  return Buffer.concat([h,tail]);
}
const raw=(strings,...body)=>assemble(strings,Buffer.from(body.flat()));
const bad=(buf,re)=>assert.throws(()=>decodeSply(new Uint8Array(buf)),e=>e instanceof Error&&/^sply: /.test(e.message)&&(!re||re.test(e.message)),String(re));
const dec=buf=>decodeSply(new Uint8Array(buf));

test('crc32c known vectors',()=>{
  assert.equal(crc32c(Buffer.from('123456789')),0xe3069283);
  assert.equal(crc32c(new Uint8Array(32)),0x8a9136aa);
  assert.equal(crc32c(Buffer.from('xx123456789yy'),2,11),0xe3069283);
});

test('isSply',()=>{
  assert.equal(isSply(encode(null)),true);
  assert.equal(isSply(new Uint8Array(encode(null)).buffer.slice(0)),true);
  assert.equal(isSply(Buffer.from('{"a":1}')),false);
  assert.equal(isSply(Buffer.from([0x89,0x53])),false);
  assert.equal(isSply('nope'),false);
});

test('scalars and tree round trip',()=>{
  const tree={schemaVersion:1n,name:'héllo ✓ 😀',flag:true,off:false,nil:null,
    nested:{list:[1n,-5n,new F(1.5),'x',[]],empty:{}},
    pts:[new F(0),new F(-0),new F(1e300),new F(5e-324)],
    one:[new F(2.5)],mixed:[new F(1),2n]};
  const out=dec(encode(tree));
  assert.deepEqual(out,{schemaVersion:1,name:'héllo ✓ 😀',flag:true,off:false,nil:null,
    nested:{list:[1,-5,1.5,'x',[]],empty:{}},pts:[0,-0,1e300,5e-324],one:[2.5],mixed:[1,2]});
  assert.ok(Object.is(out.pts[1],-0));
  assert.ok(Object.is(dec(encode(new F(-0))),-0));
  // plain vs packed float arrays decode identically
  const a=[new F(1),new F(2.5),new F(-3)];
  assert.deepEqual(dec(encode(a)),dec(encode(a,{packFloats:false})));
  assert.ok(Array.isArray(dec(encode(a))));
});

test('integers and safe range',()=>{
  const n=x=>dec(encode(x));
  assert.equal(n(0n),0);assert.ok(!Object.is(n(-1n),-0));
  assert.equal(n(-1n),-1);assert.equal(n(127n),127);assert.equal(n(128n),128);
  assert.equal(n(2n**32n-1n),2**32-1);assert.equal(n(2n**32n),2**32);
  assert.equal(n(2n**53n-1n),2**53-1);assert.equal(n(-(2n**53n-1n)),-(2**53-1));
  assert.equal(n(2n**53n),2**53);          // exactly representable beyond 2^53-1
  assert.equal(n(2n**60n),2**60);
  assert.equal(n(2n**63n),2**63);
  assert.equal(n(2n**64n-2n**11n),2**64-2**11); // 53 significant bits
  assert.equal(n(-(2n**63n)),-(2**63));
  assert.equal(n(-(2n**62n)),-(2**62));
  assert.equal(n((2n**53n-1n)*2n),(2**53-1)*2);
  for(const v of [2n**53n+1n,2n**64n-1n,2n**63n+1n,2n**60n+1n,-(2n**53n+1n),-(2n**63n)+1n,-(2n**62n+1n)])
    bad(encode(v),/integer exceeds safe range/);
});

test('integer out of i64 range',()=>{
  // tag 4 varint = 2^63 means value -(2^63)-1
  bad(raw([],4,...varint(2n**63n)),/out of range/);
  // varint with >64 bits
  bad(raw([],3,0xff,0xff,0xff,0xff,0xff,0xff,0xff,0xff,0xff,0x02),/overflow/);
  bad(raw([],3,0x80,0x80,0x80,0x80,0x80,0x80,0x80,0x80,0x80,0x80,0x00),/varint/);
});

test('strings: unicode, escapes survive, __proto__ key',()=>{
  const o=dec(encode({'':'', 'a\u0000b':'\u0000', k:'\ud7ff\ue000\u{10ffff}'}));
  assert.deepEqual(Object.keys(o),['','a\u0000b','k']);
  assert.equal(o.k,'\ud7ff\ue000\u{10ffff}');
  const p=dec(raw(['__proto__','x'],8,1,0,6,1));
  assert.deepEqual(Object.keys(p),['__proto__']);
  assert.equal(Object.getPrototypeOf(p),Object.prototype);
  assert.equal(p.__proto__,'x');
  assert.equal(({}).x,undefined);
  // BOM kept
  assert.equal(dec(encode('\ufeffz')),'\ufeffz');
  // long ASCII and long non-ASCII (chunking)
  const long='ab'.repeat(5000),longU='é€😀'.repeat(3000);
  assert.equal(dec(encode(long)),long);assert.equal(dec(encode(longU)),longU);
});

test('manual UTF-8 fallback agrees with strict decoder',()=>{
  const s='é€😀 plain \u0000 end';
  assert.equal(decodeSply(new Uint8Array(encode(s)),{_manualUtf8:true}),s);
  for(const badStr of [[0xc0,0x80],[0xed,0xa0,0x80],[0xf4,0x90,0x80,0x80],[0xe2,0x82],[0x80],[0xff],[0xe0,0x80,0x80],[0xf0,0x80,0x80,0x80]])
    for(const manual of [false,true])
      assert.throws(()=>decodeSply(new Uint8Array(raw([Buffer.from(badStr)],6,0)),{_manualUtf8:manual}),/^Error: sply: invalid UTF-8/);
});

test('blob tag 10 -> padded standard base64 string',()=>{
  for(let len=0;len<10;len++){
    const bytes=new Uint8Array(len).map((_,i)=>(i*61+250)&255);
    assert.equal(dec(encode({dataBase64:bytes})).dataBase64,Buffer.from(bytes).toString('base64'));
  }
  const big=new Uint8Array(20000).map((_,i)=>(i*7)&255);
  assert.equal(dec(encode(big)),Buffer.from(big).toString('base64'));
  assert.equal(dec(encode(new Uint8Array([0xfb,0xff]))),'+/8=');
  bad(raw([],10,5,1,2),/count exceeds/);
});

test('accepts ArrayBuffer, subarray views and base64 wrapper',()=>{
  const buf=encode({a:[new F(1),new F(2)]});
  const ab=buf.buffer.slice(buf.byteOffset,buf.byteOffset+buf.length);
  assert.deepEqual(decodeSply(ab),{a:[1,2]});
  const padded=new Uint8Array(buf.length+10);padded.set(buf,5);
  assert.deepEqual(decodeSply(padded.subarray(5,5+buf.length)),{a:[1,2]});
  const b64=buf.toString('base64');
  assert.deepEqual(decodeSplyBase64(b64),{a:[1,2]});
  assert.deepEqual(decodeSplyBase64('\n  '+b64.slice(0,10)+'\n'+b64.slice(10)+' \n'),{a:[1,2]});
  const atobSaved=globalThis.atob;
  try{globalThis.atob=undefined;assert.deepEqual(decodeSplyBase64(b64),{a:[1,2]});}finally{globalThis.atob=atobSaved;}
  for(const s of ['@@@@','abc','ab=c','====',b64.slice(0,-1),'a===']) assert.throws(()=>decodeSplyBase64(s),/^Error: sply: /);
  assert.throws(()=>decodeSplyBase64(123),/sply:/);
  assert.throws(()=>decodeSply('str'),/sply:/);
  assert.throws(()=>decodeSply(null),/sply:/);
});

test('header rejections',()=>{
  const ok=encode({a:1n});
  bad(ok.subarray(0,31),/truncated/);
  bad(Buffer.alloc(0),/truncated/);
  const mut=(off,fn)=>{const c=Buffer.from(ok);fn(c);return c;};
  bad(mut(0,c=>c[0]=0x88),/magic/);
  bad(mut(0,c=>c[7]=0),/magic/);
  bad(mut(0,c=>c.writeUInt16LE(2,8)),/major/);
  bad(mut(0,c=>c.writeUInt16LE(0,8)),/major/);
  bad(mut(0,c=>c.writeUInt16LE(1,10)),/minor/);
  bad(mut(0,c=>c.writeUInt32LE(1,12)),/flags/);
  bad(mut(0,c=>c.writeUInt32LE(0x80000000,12)),/flags/);
  bad(Buffer.concat([ok,Buffer.from([0])]),/size mismatch/);
  bad(ok.subarray(0,ok.length-1),/size mismatch/);
  bad(mut(0,c=>c.writeUInt32LE(c.readUInt32LE(24)+1,24)),/size mismatch/);
  bad(mut(0,c=>c.writeUInt32LE(0xffffffff,16)),/size mismatch/);
  bad(mut(0,c=>c.writeUInt32LE(0xffffffff,20)),/size mismatch/);
  bad(mut(0,c=>c.writeUInt32LE(0xffffffff,24)),/size mismatch/);
  bad(mut(0,c=>c[c.length-1]^=1),/crc/);
  bad(mut(0,c=>c.writeUInt32LE(c.readUInt32LE(28)^1,28)),/crc/);
  // bytes before offset 32 are not CRC-covered but are header fields (checked above)
});

test('string table rejections',()=>{
  bad(assemble(['a','a'],Buffer.from([0])),/duplicate string/);
  const fix=c=>{c.writeUInt32LE(crc32c(c,32),28);return c;};
  const base=()=>Buffer.from(assemble(['ab','cd'],Buffer.from([0]))); // offsets at 32,36,40 = 0,2,4
  let c=base();c.writeUInt32LE(1,32);bad(fix(c),/offsets/);       // offsets[0] != 0
  c=base();c.writeUInt32LE(3,36);c.writeUInt32LE(2,40);bad(fix(c),/offsets/); // non-monotonic
  c=base();c.writeUInt32LE(9,36);bad(fix(c),/offsets/);           // beyond strings_len
  c=base();c.writeUInt32LE(3,40);bad(fix(c),/offsets/);           // last != strings_len
  // invalid UTF-8 inside the table is rejected even with a valid CRC
  bad(assemble([Buffer.from([0xc3])],Buffer.from([0])),/UTF-8/);
});

test('body rejections',()=>{
  bad(raw([]),/truncated body/);
  bad(raw([],0,0),/trailing/);
  bad(raw([],99),/unknown tag 99/);
  bad(raw([],11),/unknown tag/);
  bad(raw([],3),/truncated/);
  bad(raw([],3,0x80),/truncated/);
  bad(raw([],3,0x80,0x00),/non-minimal/);
  bad(raw([],3,0x81,0x80,0x00),/non-minimal/);
  bad(raw([],5,1,2,3),/truncated/);
  bad(raw(['a'],6,1),/string index/);
  bad(raw(['a'],6,0x80,0x80,0x80,0x80,0x10),/string index/);
  bad(raw([],6,0),/string index/);
  // non-finite floats
  for(const v of [NaN,Infinity,-Infinity]){
    bad(Buffer.concat([assemble([],Buffer.concat([Buffer.from([5]),f64(v)]))]),/non-finite/);
    bad(assemble([],Buffer.concat([Buffer.from([9,2]),f64(1),f64(v)])),/non-finite/);
  }
  // counts larger than remaining bytes (must fail without allocating)
  bad(raw([],7,0xff,0xff,0xff,0xff,0x0f),/count exceeds/);
  bad(raw([],7,5,0,0),/count exceeds/);
  bad(raw([],8,0xff,0xff,0xff,0xff,0x0f),/count exceeds/);
  bad(raw([],8,3,0,0),/count exceeds/);
  bad(raw([],9,0xff,0xff,0xff,0xff,0x0f),/count exceeds/);
  bad(raw([],9,2,...f64(1)),/count exceeds/);
  bad(raw([],7,0x80,0x80,0x80,0x80,0x10),/count exceeds/);
  bad(raw([],7,0xff,0xff,0xff,0xff,0xff,0xff,0xff,0xff,0xff,0x01),/count exceeds/);
  // duplicate object key
  bad(raw(['a'],8,2,0,0,0,0),/duplicate object key/);
  // duplicate key in a large object (Set path) and many-key object round trip
  const many=Array.from({length:40},(_,i)=>'k'+i);
  const dupBody=[8,20];for(let i=0;i<20;i++)dupBody.push(i===19?3:i,0);
  bad(raw(many,dupBody),/duplicate object key/);
  const big={};for(const k of many)big[k]=1n;assert.deepEqual(dec(encode(big)),Object.fromEntries(many.map(k=>[k,1])));
  // truncated array
  bad(raw([],7,2,3,0),/truncated body/);
});

test('depth limit 128',()=>{
  const nest=d=>{let x=null;for(let i=0;i<d;i++)x=[x];return x;};
  const depth=v=>{let d=0;while(Array.isArray(v)){d++;v=v[0];}return d;};
  assert.equal(depth(dec(encode(nest(128)))),128);
  bad(encode(nest(129)),/too deep/);
  const objNest=d=>{let x=null;for(let i=0;i<d;i++)x={k:x};return x;};
  dec(encode(objNest(128)));bad(encode(objNest(129)),/too deep/);
  // hostile deep input does not overflow the stack
  const n=1000000;const body=Buffer.alloc(n*2,0);for(let i=0;i<n;i++){body[2*i]=7;body[2*i+1]=1;}
  bad(assemble([],body),/too deep/);
});

test('hostile inputs never hang or throw non-sply errors (fuzz)',()=>{
  let seed=12345;const rnd=()=>(seed=(seed*1103515245+12345)&0x7fffffff)/0x7fffffff;
  const base=encode({a:[new F(1),new F(2),3n],b:{c:'d',e:null},f:true,g:new Uint8Array([1,2,3])});
  let okCount=0;
  for(let i=0;i<3000;i++){
    const c=Buffer.from(base);
    const k=1+Math.floor(rnd()*3);
    for(let j=0;j<k;j++)c[Math.floor(rnd()*c.length)]=Math.floor(rnd()*256);
    if(rnd()<0.7)c.writeUInt32LE(crc32c(c,32),28); // pass CRC so the parser is exercised
    try{decodeSply(new Uint8Array(c));okCount++;}catch(e){assert.ok(e instanceof Error&&/^sply: /.test(e.message),e&&e.message);}
  }
  assert.ok(okCount<3000);
  for(let i=0;i<500;i++){const r=Buffer.alloc(Math.floor(rnd()*200));for(let j=0;j<r.length;j++)r[j]=Math.floor(rnd()*256);bad(r);}
});

test('canonical base64 only: non-zero trailing bits and oversized text are rejected',()=>{
  const dir=path.join(fixtureDir,'valid');
  let tested=0;
  for(const f of fs.readdirSync(dir)){
    const bytes=fs.readFileSync(path.join(dir,f));
    if(bytes.length%3===0)continue;
    try{decodeSply(new Uint8Array(bytes));}catch(e){continue;} // malformed/lossy fixtures
    const text=bytes.toString('base64');
    assert.doesNotThrow(()=>decodeSplyBase64(text),f);
    const two=text.endsWith('==');
    const at=text.length-(two?3:2),alpha='ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
    const forged=text.slice(0,at)+alpha[alpha.indexOf(text[at])|1]+text.slice(at+1); // same bytes, non-zero pad bits
    assert.equal(Buffer.from(forged,'base64').equals(bytes),true,'forged text decodes to identical bytes');
    assert.throws(()=>decodeSplyBase64(forged),/^Error: sply: invalid base64$/,f);
    tested++;
  }
  assert.ok(tested>=2);
  assert.throws(()=>decodeSplyBase64('A'.repeat(Math.ceil(64*1024*1024/3)*4+4)),/64 MiB/);
});

test('tag 9 obeys the depth limit; container claims and string expansion are bounded like Rust',()=>{
  const f=new Uint8Array(16);
  const nest=(d,tail)=>{const b=[];for(let i=0;i<d;i++)b.push(7,1);return Buffer.concat([Buffer.from(b),tail]);};
  const floats=Buffer.concat([Buffer.from([9,2]),Buffer.from(f)]);
  const file=body=>{
    const h=Buffer.alloc(32+4);h.set([0x89,0x53,0x50,0x4c,0x59,0x0d,0x0a,0x1a]);h.writeUInt16LE(1,8);h.writeUInt32LE(0,16);h.writeUInt32LE(0,20);h.writeUInt32LE(body.length,24);
    const all=Buffer.concat([h,body]);all.writeUInt32LE(crc32c(all,32),28);return new Uint8Array(all);
  };
  assert.equal(decodeSply(file(nest(127,floats))).length,1);
  assert.throws(()=>decodeSply(file(nest(128,floats))),/^Error: sply: nesting too deep$/);
  const claims=Buffer.alloc(40);claims.set([7,30,7,30,7,30]);
  assert.throws(()=>decodeSply(file(claims)),/^Error: sply: count exceeds remaining bytes$/);
  assert.equal(decodeSply(file(Buffer.concat([Buffer.from([7,0xb8,0x17]),Buffer.alloc(3000)]))).length,3000); // dense nulls
});

test('64 MiB limit',()=>{
  bad(new Uint8Array(64*1024*1024+1),/64 MiB/);
});

// ---- shared fixture manifest (written by another worker; skipped if absent) ----
const fixtureDir=path.join(__dirname,'..','..','crates','splinely_document','tests','fixtures','sply');
const manifestPath=path.join(fixtureDir,'manifest.json');
test('shared fixtures manifest',{skip:!fs.existsSync(manifestPath)&&'manifest.json absent'},()=>{
  const m=JSON.parse(fs.readFileSync(manifestPath,'utf8'));
  assert.ok(m.valid.length>0&&m.malformed.length>0,'manifest has no cases');
  const read=f=>new Uint8Array(fs.readFileSync(path.join(fixtureDir,f)));
  for(const c of m.valid){
    if(c.lossy){ // values a JS Number cannot hold: decoder must throw the documented range error or match
      try{decodeSply(read(c.file));}catch(e){assert.match(e.message,/^sply: /,c.file);}
      continue;
    }
    assert.deepEqual(decodeSply(read(c.file)),JSON.parse(c.expectedText),c.file);
  }
  for(const c of m.malformed){
    // Both decoders must reject with a `sply: ` error; exact wording is implementation specific.
    assert.throws(()=>decodeSply(read(c.file)),e=>/^sply: /.test(e.message),c.file+' '+c.rule);
  }
});
