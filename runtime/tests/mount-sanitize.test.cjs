'use strict';
// splinely-mount.js sanitizer (audit JS-4): only what the Splinely exporter writes reaches the host page.
// Minimal namespace-aware fake DOM; scripts/inline-mount-smoke.cjs repeats the check in Chromium.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const SVG='http://www.w3.org/2000/svg',XHTML='http://www.w3.org/1999/xhtml',META='urn:splinely:metadata';
class Node {constructor(type){this.nodeType=type;this.parentNode=null;this.children=[];}
  get firstChild(){return this.children[0]||null;}
  get nextSibling(){const p=this.parentNode;return p?p.children[p.children.indexOf(this)+1]||null:null;}
  removeChild(c){this.children.splice(this.children.indexOf(c),1);c.parentNode=null;return c;}
  appendChild(c){c.parentNode=this;this.children.push(c);return c;}}
class El extends Node {constructor(ns,name,attrs={}){super(1);this.namespaceURI=ns;this.localName=name.split(':').pop();this.tagName=name;this.map=new Map(Object.entries(attrs));}
  get attributes(){return [...this.map].map(([name,value])=>({name,value}));}
  getAttribute(k){return this.map.has(k)?this.map.get(k):null;}removeAttribute(k){this.map.delete(k);}}
const el=(ns,name,attrs,...kids)=>{const e=new El(ns,name,attrs);for(const k of kids)e.appendChild(typeof k==='string'?Object.assign(new Node(3),{data:k}):k);return e;};
globalThis.SplinelyRuntime=require('../src/splinely.js');
const {sanitize}=require('../src/splinely-mount.js');
const names=e=>e.children.filter(c=>c.nodeType===1).map(c=>c.tagName);

test('exporter markup is kept unchanged',()=>{
  const exported=()=>el(SVG,'svg',{xmlns:SVG,width:'64',height:'64',viewBox:'0 0 64 64','data-splinely-version':'1',role:'img','aria-labelledby':'t'},
    el(SVG,'metadata',{},el(META,'splinely:generator',{'xmlns:splinely':META,name:'Splinely',version:'0.5.5',schemaVersion:'1'})),
    el(SVG,'title',{id:'t'},'Simo'),el(SVG,'desc',{id:'d'},'x'),
    el(SVG,'g',{id:'body','data-splinely-group':'body','aria-label':'Body'},
      el(SVG,'defs',{},el(SVG,'linearGradient',{id:'a-gradient',x1:'0',y1:'0.5',x2:'1',y2:'0.5'},el(SVG,'stop',{offset:'0','stop-color':'rgb(10% 20% 30%)','stop-opacity':'1'}))),
      el(SVG,'path',{id:'a','data-splinely-node':'a',transform:'translate(0 0) rotate(0) scale(1 1)',fill:'url(#a-gradient)','fill-opacity':'1',stroke:'none','stroke-opacity':'1','stroke-width':'1',opacity:'1',d:'M0 0 L1 1 Z','fill-rule':'nonzero'}),
      el(SVG,'circle',{id:'c','data-splinely-node':'c',cx:'1',cy:'2',r:'3',fill:'rgb(1% 2% 3%)','aria-hidden':'true'}),
      el(SVG,'rect',{id:'r',x:'0',y:'0',width:'1',height:'1',rx:'0',role:'button',tabindex:'0','aria-label':'Go'}),
      el(SVG,'text',{id:'x',x:'0',y:'0','font-size':'12'},'Hallo <b>')),
    el(SVG,'metadata',{id:'splinely-project','data-encoding':'sply-base64'},'U1BMWQ=='));
  const dump=e=>e.nodeType===1?[e.namespaceURI,e.tagName,[...e.map],e.children.map(dump)]:e.data;
  const before=exported(),after=sanitize(exported());
  assert.deepEqual(dump(after),dump(before));
});

test('scripts, styles, foreign content, links, animation and external references are removed',()=>{
  const root=el(SVG,'svg',{onload:'x()',style:'position:fixed',class:'modal'},
    el(SVG,'script',{},'x()'),el(SVG,'style',{},'body{display:none}'),
    el(SVG,'foreignObject',{},el(XHTML,'div',{},'x')),el(XHTML,'iframe',{}),el(XHTML,'script',{}),
    el(SVG,'a',{href:'javascript:alert(1)'},el(SVG,'circle',{r:'1'})),
    el(SVG,'animate',{attributeName:'href',to:'javascript:x'}),el(SVG,'set',{attributeName:'fill'}),el(SVG,'image',{href:'https://e/i.png'}),
    el(SVG,'use',{href:'https://e/s.svg#a'}),el(SVG,'use',{'xlink:href':'#r'}),el(SVG,'use',{href:'#r x'}),
    el(SVG,'rect',{id:'r',onclick:'alert(1)',ONMOUSEOVER:'x',fill:'url(https://e/x)',stroke:'url( "#ok" )',mask:'url(#m) url(data:x)','clip-path':'u\\72l(x)',filter:'url(#f)'}),
    el(SVG,'circle',{fill:'URL(#g)',stroke:'rgb(1 2 3)','aria-label':'a \\ b','data-x':'url(x)'}),
    el(META,'splinely:generator',{}));
  sanitize(root);
  assert.deepEqual([...root.map.keys()],[]);
  assert.deepEqual(names(root),['use','use','use','rect','circle'],'only allowlisted SVG elements stay');
  const [u1,u2,u3,rect,circle]=root.children;
  assert.equal(u1.getAttribute('href'),null);assert.equal(u2.getAttribute('xlink:href'),'#r');assert.equal(u3.getAttribute('href'),null);
  assert.deepEqual([...rect.map],[['id','r'],['stroke','url( "#ok" )']]);
  assert.deepEqual([...circle.map],[['fill','URL(#g)'],['stroke','rgb(1 2 3)'],['aria-label','a \\ b'],['data-x','url(x)']]);
});

test('mount() sanitizes element sources too',()=>{
  const doc={importNode:n=>n};globalThis.document=doc;
  try {
    const svg=el(SVG,'svg',{},el(SVG,'style',{},'*{}'),el(SVG,'circle',{id:'n',onclick:'x'}));
    const container={replaceChildren(c){this.child=c;}};
    const g=require('../src/splinely-mount.js').mount(container,svg,{autoTick:false});
    assert.equal(container.child,svg);assert.deepEqual(names(svg),['circle']);assert.equal(svg.children[0].getAttribute('onclick'),null);
    assert.ok(g.error,'no project metadata: the static drawing stays');
  } finally {delete globalThis.document;}
});
