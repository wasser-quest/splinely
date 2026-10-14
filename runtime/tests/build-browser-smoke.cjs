// Generates one offline XML SVG with the actual runtime, not a fetched script.
// node runtime/tests/build-browser-smoke.cjs [output.svg]
'use strict';
const fs=require('node:fs');
const path=require('node:path');
const runtime=fs.readFileSync(path.join(__dirname,'../src/forma.js'),'utf8');
const xml=s=>s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
const cdata=s=>'<![CDATA['+s.replace(/]]>/g,']]]]><![CDATA[>')+']]>';
const project={schemaVersion:1,nodes:[{id:'dot',kind:{type:'circle',cx:50,cy:60,radius:20},transform:{x:0,y:0,rotation:0,scaleX:1,scaleY:1},style:{opacity:1,fill:{r:0,g:0.5,b:1,a:1}}}],clips:[{id:'move',duration:1,looping:true,tracks:[{nodeId:'dot',property:'x',keyframes:[{time:0,value:{type:'number',value:0}},{time:1,value:{type:'number',value:100}}]}]}],variables:{count:0},stateMachines:[{id:'button',initial:'idle',states:[{id:'idle'},{id:'clicked',onEnter:[{type:'setVariable',name:'count',value:1}]}],transitions:[{from:'idle',to:'clicked',event:{name:'click',target:'dot'}}]}],name:'JSON is data: </script> & ]]>'};
project.bones=[{id:'childBone',parentId:'rootBone',x:0,y:0,rotation:-90,length:10},{id:'rootBone',x:200,y:60,rotation:90,length:20}];
project.nodes.push({id:'rigLimb',boneId:'childBone',locked:true,originalKind:{type:'rect',width:12,height:4},kind:{type:'rect',width:12,height:4},transform:{x:2,y:0,rotation:0,scaleX:1,scaleY:1},style:{fill:'#7850ba'}});
project.clips.push({id:'rigPose',duration:2,tracks:[{nodeId:'rootBone',property:'rotation',keyframes:[{time:0,value:{type:'number',value:350}},{time:2,value:{type:'number',value:10}}]}]});
const harness=`(function(){
  const root=document.currentScript.ownerSVGElement, f=root.forma, status=root.querySelector('#status');
  const check=(ok,label)=>{if(!ok)throw new Error(label);};
  try {
    check(!!f,'auto mount');const dot=root.querySelector('#dot');
    const seen=[];f.timeline.at(0.5,d=>seen.push(d.cycle),{clip:'move'});f.clip('move').play();f.tick(2.5);
    check(seen.join(',')==='0,1,2','loop crossings');check(dot.getAttribute('transform').startsWith('translate(50 '),'core transform');
    f.clip('move').pause();f.clip('move').seek(0.25);check(seen.length===3,'silent seek');
    dot.dispatchEvent(new MouseEvent('click',{bubbles:true}));check(f.machine('button').state==='clicked','DOM to machine');check(f.variable('count').get()===1,'entry action');
    const limb=root.querySelector('#rigLimb');
    const near=(a,b)=>Math.abs(a-b)<1e-9;
    const checkTransform=(x,y,rotation)=>{const v=limb.getAttribute('transform').match(/[-+]?\\d*\\.?\\d+(?:e[-+]?\\d+)?/gi).map(Number);check(near(v[0],x)&&near(v[1],y)&&near(v[2],rotation),'bone attachment');};
    checkTransform(202,80,0);check(limb.getAttribute('x')==='0','local geometry');
    f.clip('rigPose').seek(1);check(f.bones().find(b=>b.id==='rootBone').rotation===180,'numeric rotation interpolation');
    f.clip('rigPose').stop();f.set('rootBone',{rotation:0});checkTransform(220,58,-90);
    f.set('rootBone',null);checkTransform(202,80,0);
    const other=root.querySelector('#other');const second=FormaRuntime.createRuntime(other,null,{autoTick:false});
    check(!window.forma,'no ambiguous global');second.set('dot',{opacity:0.25});check(dot.getAttribute('opacity')==='1','scoped IDs');
    second.destroy();check(window.forma===f,'alias restored');
    f.destroy();check(!root.forma,'destroy');
    status.textContent='PASS: direct offline SVG runtime';status.setAttribute('fill','#087f23');
  }catch(e){status.textContent='FAIL: '+e.message;status.setAttribute('fill','#c00000');}
})();`;
const svg=`<svg xmlns="http://www.w3.org/2000/svg" width="620" height="200" viewBox="0 0 620 200" data-forma-version="1">
<rect width="620" height="200" fill="white"/>
<circle id="dot" data-forma-node="dot" cx="50" cy="60" r="20" fill="#0080ff" tabindex="0"/>
<rect id="rigLimb" x="0" y="0" width="12" height="4" fill="#7850ba" transform="translate(202 80)"/>
<text id="status" x="20" y="155" font-size="24">STATIC FALLBACK: scripts disabled</text>
<metadata id="forma-project">${xml(JSON.stringify(project))}</metadata>
<svg id="other" x="300" width="200" height="100"><circle id="dot" cx="30" cy="60" r="20" fill="orange"/><metadata id="forma-project">${xml(JSON.stringify({...project,nodes:[project.nodes[0]],bones:[],clips:[],stateMachines:[]}))}</metadata></svg>
<script>${cdata(runtime)}</script><script>${cdata(harness)}</script>
</svg>`;
const output=path.resolve(process.argv[2] || path.join(__dirname,'.generated/browser-smoke.svg'));
fs.mkdirSync(path.dirname(output),{recursive:true});fs.writeFileSync(output,svg);console.log(output);
