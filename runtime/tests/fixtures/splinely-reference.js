/* Frozen reference copy of runtime/src/splinely.js taken before the performance work (W4). perf-golden.test.cjs compares the live runtime against it; refresh only for intentional behaviour changes. */
/* Splinely runtime v1. Dependency-free; embed this file unchanged in an SVG script. */
(function (host) {
  'use strict';
  const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
  const finite = (v, label) => { if (typeof v !== 'number' || !Number.isFinite(v)) throw new Error(label + ' must be finite'); return v; };
  const weightValue = v => { finite(v,'Clip weight'); if(v<0 || v>1)throw new Error('Clip weight must be in [0,1]'); return v; };
  const fadeDuration = v => { finite(v,'Fade duration'); if(v<0)throw new Error('Fade duration must be nonnegative'); return v; };
  const list = v => Array.isArray(v) ? v : Object.entries(v || {}).map(([id, item]) => ({ ...item, id: item.id || id }));
  const unwrap = v => v && ['number','color','gradient','path'].includes(v.type) ? v.value : v;
  const own = (o, k) => Object.prototype.hasOwnProperty.call(o, k);
  function equal(a,b) {
    if(a===b)return true;if(!a || !b || typeof a!=='object' || typeof b!=='object' || Array.isArray(a)!==Array.isArray(b))return false;
    const keys=Object.keys(a);return keys.length===Object.keys(b).length && keys.every(k=>own(b,k) && equal(a[k],b[k]));
  }
  function color(v) {
    if (v && typeof v === 'object' && !Array.isArray(v) && ['r','g','b'].every(k => Number.isFinite(v[k])) && (v.a==null || Number.isFinite(v.a))) return [v.r, v.g, v.b, v.a == null ? 1 : v.a];
    if (typeof v !== 'string' || !/^#([a-f\d]{3,4}|[a-f\d]{6}|[a-f\d]{8})$/i.test(v)) return null;
    let hex = v.slice(1); if (hex.length < 5) hex = [...hex].map(c => c + c).join('');
    return [0,2,4,6].map((i) => i < hex.length ? parseInt(hex.slice(i,i+2),16) / 255 : 1);
  }
  // Structured values as shape + numbers: gradients (angle, stops) and path
  // data (letters, numbers) interpolate and blend number by number.
  const pathToken = /[a-zA-Z]|[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?/g;
  const pathNumber = v => String(Math.round(v*1000)/1000+0);
  function vectorOf(v) {
    v = unwrap(v);
    if (typeof v === 'number') return {kind:'n', shape:'n', nums:[v]};
    const c = color(v); if (c) return {kind:'c', shape:'c', nums:c};
    if (v && typeof v === 'object' && Array.isArray(v.stops)) {
      const nums=[Number(v.angle)||0];
      for (const s of v.stops) nums.push(Number(s.offset)||0, ...(color(s.color) || [0,0,0,1]));
      return {kind:'g', shape:'g'+(v.radial?'r':'l')+v.stops.length, nums, radial:!!v.radial};
    }
    if (typeof v === 'string' && /^\s*[Mm]/.test(v)) {
      const tokens = v.match(pathToken) || [];
      return {kind:'p', shape:tokens.map(t=>/^[a-zA-Z]$/.test(t)?t:'#').join(''), nums:tokens.filter(t=>!/^[a-zA-Z]$/.test(t)).map(Number), template:tokens};
    }
    return null;
  }
  const smoothstep=t=>{t=Math.max(0,Math.min(1,t));return t*t*(3-2*t);};
  // Crossfade mix: numbers, colours, gradients and matching paths blend; anything else switches halfway.
  function mixValues(a,b,t) {
    const va=vectorOf(a),vb=vectorOf(b);
    if(va && vb && va.shape===vb.shape)return fromVector(va,va.nums.map((x,i)=>x+(vb.nums[i]-x)*t));
    return t<0.5?a:b;
  }
  function fromVector(vec, n) {
    if (vec.kind === 'n') return n[0];
    if (vec.kind === 'c') return {r:n[0], g:n[1], b:n[2], a:n[3]};
    if (vec.kind === 'g') {
      const stops=[]; for (let i=1;i<n.length;i+=5) stops.push({offset:clamp(n[i],0,1), color:{r:n[i+1], g:n[i+2], b:n[i+3], a:n[i+4]}});
      return {radial:vec.radial, angle:n[0], stops};
    }
    let k=0; return vec.template.map(t => /^[a-zA-Z]$/.test(t) ? t : pathNumber(n[k++])).join(' ');
  }
  function interpolate(a, b, t) {
    a = unwrap(a); b = unwrap(b);
    if (typeof a === 'number' && typeof b === 'number') return t <= 0 ? a : t >= 1 ? b : a*(1-t)+b*t;
    const ca = color(a), cb = color(b);
    if (ca && cb) {
      const c = ca.map((v,i) => t <= 0 ? v : t >= 1 ? cb[i] : v*(1-t)+cb[i]*t);
      return {r:c[0],g:c[1],b:c[2],a:c[3]};
    }
    if (Array.isArray(a) && Array.isArray(b) && a.length === b.length && [...a,...b].every(Number.isFinite)) return a.map((v,i) => t <= 0 ? v : t >= 1 ? b[i] : v*(1-t)+b[i]*t);
    const va = vectorOf(a), vb = vectorOf(b);
    if (va && vb && va.shape === vb.shape) return fromVector(va, va.nums.map((v,i) => t <= 0 ? v : t >= 1 ? vb.nums[i] : v*(1-t)+vb.nums[i]*t));
    return t < 1 ? a : b;
  }
  function easing(spec, t) {
    const type = Array.isArray(spec) ? 'cubicBezier' : typeof spec === 'string' ? spec : (spec || {}).type || 'linear';
    if (type === 'step' || type === 'discrete') return t < 1 ? 0 : 1;
    if (type === 'linear') return t;
    const points = Array.isArray(spec) ? spec : type === 'cubicBezier' ? [spec.x1,spec.y1,spec.x2,spec.y2] : null;
    if (!points || points.length !== 4 || !points.every(Number.isFinite) || points[0] < 0 || points[0] > 1 || points[2] < 0 || points[2] > 1) throw new Error('Unsupported easing');
    if (t <= 0 || t >= 1) return t;
    const curve = (s,a,b) => 3*(1-s)*(1-s)*s*a + 3*(1-s)*s*s*b + s*s*s;
    let lo=0, hi=1;
    for (let i=0;i<60;i++) { const mid=(lo+hi)/2; if (curve(mid,points[0],points[2]) < t) lo=mid; else hi=mid; }
    return curve((lo+hi)/2,points[1],points[3]);
  }
  function evaluateTrack(track, time) {
    const frames = track.keyframes || [];
    if (!frames.length) return undefined;
    if (time < frames[0].time) return unwrap(frames[0].value);
    // Last duplicate at an exact timestamp wins; normalization preserves stored order.
    let i = 0; while (i+1 < frames.length && frames[i+1].time <= time) i++;
    if (i+1 === frames.length) return unwrap(frames[i].value);
    const a=frames[i], b=frames[i+1];
    return interpolate(a.value,b.value,easing(a.easing,(time-a.time)/(b.time-a.time)));
  }
  const boneProperties = new Set(['x','y','rotation']);
  function bonePatch(props) {
    const result={};
    for(const [key,value] of Object.entries(props)) {
      if(!boneProperties.has(key)) throw new Error('Unsupported bone property: '+key);
      if(value && typeof value==='object' && (value.type!=='number' || !own(value,'value'))) throw new Error('Bone '+key+' must be numeric');
      result[key]=finite(unwrap(value),'Bone '+key);
    }
    return result;
  }
  function solveBones(bones) {
    const byId=new Map(bones.map(b=>[b.id,b])), poses=new Map(), visiting=new Set();
    function solve(id,depth) {
      if(depth>128) throw new Error('Bone depth limit exceeded (128)');
      if(visiting.has(id)) throw new Error('Bone cycle: '+id);
      const b=byId.get(id); if(!b) throw new Error('Missing bone parent: '+id);
      visiting.add(id);
      // Traverse even cached ancestors so the limit is independent of input order.
      const parent=b.parentId==null?null:solve(b.parentId,depth+1);
      visiting.delete(id);
      if(poses.has(id)) return poses.get(id);
      let x=finite(b.x,'Bone x'), y=finite(b.y,'Bone y'), rotation=finite(b.rotation,'Bone rotation');
      const length=finite(b.length,'Bone length'); if(length<=0) throw new Error('Bone length must be positive');
      if(parent) {
        const angle=(parent.rotation%360)*Math.PI/180, c=Math.cos(angle), s=Math.sin(angle);
        const localX=x, localY=y;
        x=parent.tipX+c*localX-s*localY; y=parent.tipY+s*localX+c*localY;
        rotation+=parent.rotation;
      }
      const angle=(rotation%360)*Math.PI/180;
      const pose={id:b.id,x:finite(x,'Bone world x'),y:finite(y,'Bone world y'),rotation:finite(rotation,'Bone world rotation'),length,tipX:finite(x+length*Math.cos(angle),'Bone tip x'),tipY:finite(y+length*Math.sin(angle),'Bone tip y')};
      poses.set(id,pose); return pose;
    }
    return bones.map(b=>solve(b.id,1));
  }
  // Same deterministic 64-chord arc-length approximation as splinely_document.
  function motionSample(d, transform, progress) {
    if(typeof d!=='string' || d.replace(pathToken,'').replace(/[\s,]/g,''))throw new Error('Invalid motion path');
    if(/(?:^|[a-zA-Z,])\s*,|,\s*(?:[a-zA-Z]|$)/.test(d))throw new Error('Invalid motion path separators');
    if(!/^\s*[Mm]/.test(d))throw new Error('Motion path must start with M');
    const tokens=d.match(pathToken)||[];let i=0,cmd='',current=[0,0],start=[0,0],total=0;const chords=[];
    const tr=transform||{},angle=(tr.rotation||0)*Math.PI/180,c=Math.cos(angle),s=Math.sin(angle);
    const map=p=>[c*(tr.scaleX==null?1:tr.scaleX)*p[0]-s*(tr.scaleY==null?1:tr.scaleY)*p[1]+(tr.x||0),s*(tr.scaleX==null?1:tr.scaleX)*p[0]+c*(tr.scaleY==null?1:tr.scaleY)*p[1]+(tr.y||0)];
    const segment=(points)=>{let a=map(points[0]);for(let j=1;j<=64;j++){const t=j/64,u=1-t;const b=map([0,1].map(k=>u*u*u*points[0][k]+3*u*u*t*points[1][k]+3*u*t*t*points[2][k]+t*t*t*points[3][k]));const len=Math.hypot(b[0]-a[0],b[1]-a[1]);if(len>1e-12){total+=len;chords.push([a,b,len]);if(chords.length>1000000)throw new Error('Motion path too complex');}a=b;}};
    const line=(a,b)=>segment([a,[a[0]+(b[0]-a[0])/3,a[1]+(b[1]-a[1])/3],[a[0]+2*(b[0]-a[0])/3,a[1]+2*(b[1]-a[1])/3],b]);
    while(i<tokens.length){
      if(/^[a-z]$/i.test(tokens[i]))cmd=tokens[i++];
      if(!/^[MmLlQqCcZz]$/.test(cmd))throw new Error('Motion paths support M/L/Q/C/Z only');
      const upper=cmd.toUpperCase(),relative=cmd!==upper;
      if(upper==='Z'){if(Math.hypot(current[0]-start[0],current[1]-start[1])>1e-9)line(current,start);current=start;cmd='';continue;}
      const count={M:2,L:2,Q:4,C:6}[upper];const nums=tokens.slice(i,i+count).map(Number);
      if(nums.length!==count || !nums.every(Number.isFinite))throw new Error('Invalid motion path coordinates');i+=count;
      const points=[];for(let j=0;j<count;j+=2)points.push([nums[j]+(relative?current[0]:0),nums[j+1]+(relative?current[1]:0)]);
      const end=points[points.length-1];
      if(upper==='M'){current=end;start=end;cmd=relative?'l':'L';continue;}
      if(upper==='L')line(current,end);
      else if(upper==='Q'){const q=points[0];segment([current,[current[0]+2*(q[0]-current[0])/3,current[1]+2*(q[1]-current[1])/3],[end[0]+2*(q[0]-end[0])/3,end[1]+2*(q[1]-end[1])/3],end]);}
      else segment([current,...points]);current=end;
    }
    if(!Number.isFinite(total) || total<=1e-12)throw new Error('Motion path must have finite nonzero length');
    let distance=clamp(progress,0,1)*total;
    for(let j=0;j<chords.length;j++){const [a,b,len]=chords[j];if(distance<=len+total*1e-12 || j===chords.length-1){const t=clamp(distance/len,0,1);return [a[0]+(b[0]-a[0])*t,a[1]+(b[1]-a[1])*t,Math.atan2(b[1]-a[1],b[0]-a[0])*180/Math.PI];}distance-=len;}
  }

  const blendParameter = v => {finite(v,'Blend-space parameter');if(Math.abs(v)>1e7)throw new Error('Blend-space parameter out of range');return v;};
  function spaceWeights(spaces, get) {
    const result=new Map();
    for(const s of spaces) {
      const p=s.parameters.map(k=>blendParameter(get(k)));
      const distances=s.samples.map(v=>(v.position[0]-p[0])**2+(v.position[1]-p[1])**2);
      const nearest=Math.min(...distances), exact=distances.indexOf(0);
      const raw=distances.map((d,i)=>exact<0?nearest/d:(i===exact?1:0)), sum=raw.reduce((a,b)=>a+b,0);
      s.samples.forEach((v,i)=>result.set(v.clipId,raw[i]/sum));
    }
    return result;
  }
  function normalize(project) {
    if (!project || typeof project !== 'object' || Array.isArray(project)) throw new Error('Project must be an object');
    if (project.schemaVersion != null && project.schemaVersion !== 1) throw new Error('Unsupported schemaVersion: ' + project.schemaVersion);
    const unique = (items, label) => {
      const ids=new Set(); for (const x of items) { if (typeof x.id !== 'string' || !x.id || ids.has(x.id)) throw new Error('Missing or duplicate ' + label + ' id: ' + x.id); ids.add(x.id); } return items;
    };
    const clips = unique(list(project.clips).map(c => {
      if(c.id==='@timeline')throw new Error('Reserved clip id: @timeline');
      const duration=finite(c.duration,'Clip duration'); if (duration <= 0) throw new Error('Clip duration must be positive');
      const blendMode=c.blendMode===undefined?'override':c.blendMode;
      if(!['override','weighted','additive'].includes(blendMode))throw new Error('Unsupported blendMode: '+blendMode);
      const weight=weightValue(c.weight===undefined?1:c.weight);
      const trackKeys=new Set();
      return {...c,duration,blendMode,weight,loop: !!(c.looping == null ? c.loop : c.looping),tracks:(c.tracks || []).map(t => {
        if(typeof (t.nodeId || t.target)!=='string' || typeof t.property!=='string')throw new Error('Track requires target and property');
        const trackKey=JSON.stringify([t.nodeId || t.target,t.property]);
        if(trackKeys.has(trackKey))throw new Error('Clip '+c.id+': duplicate property track: '+(t.nodeId || t.target)+'.'+t.property);
        trackKeys.add(trackKey);
        if(!t.keyframes || !t.keyframes.length)throw new Error('Track requires keyframes');
        const keyframes=t.keyframes.map(k => {
          if (finite(k.time,'Keyframe time') < 0 || k.time > duration) throw new Error('Keyframe outside clip');
          const value=unwrap(k.value);if(value===undefined)throw new Error('Keyframe requires value');
          if(typeof value==='number' || (k.value && k.value.type==='number'))finite(value,'Keyframe value');
          if(blendMode==='additive') {
            const v=vectorOf(value), color=['fill','stroke'].includes(t.property);
            if(!v || v.kind!==(color?'c':'n') || !['x','y','rotation','scaleX','scaleY','opacity','fill','stroke','width','height','radius','fontSize','motionProgress'].includes(t.property))throw new Error('Additive clips support matching numeric and RGBA properties only');
            if(v.nums.some(n=>!Number.isFinite(n) || Math.abs(n)>1e7 || (color && (n<0 || n>1))))throw new Error('Additive keyframe out of range');
            if(['opacity','motionProgress'].includes(t.property))weightValue(value);
            if(['width','height','radius','fontSize'].includes(t.property) && value<0)throw new Error('Additive geometry must be nonnegative');
          }
          if(blendMode==='weighted' && !vectorOf(value))throw new Error('Weighted keyframe must be numeric, RGBA, gradient or path data');
          if(Array.isArray(value) && value.some(v=>typeof v!=='number' || !Number.isFinite(v)))throw new Error('Keyframe array must contain finite numbers');
          easing(k.easing,0.5);return {...k};
        }).sort((a,b)=>a.time-b.time);
        return {...t,target:t.nodeId || t.target,property:t.property,keyframes};
      })};
    }), 'clip');
    const ownedClips=new Set();
    if(project.blendSpaces!==undefined && !Array.isArray(project.blendSpaces))throw new Error('blendSpaces must be an array');
    const blendSpaces=unique((project.blendSpaces || []).map(s=>{
      if(!s || typeof s!=='object' || Object.keys(s).some(k=>!['id','parameters','samples'].includes(k)) || typeof s.id!=='string' || !/^[A-Za-z_][A-Za-z0-9_.-]*$/.test(s.id))throw new Error('Invalid blend space fields or ID');
      if(!Array.isArray(s.parameters) || s.parameters.length!==2 || s.parameters.some(k=>typeof k!=='string' || !k) || s.parameters[0]===s.parameters[1])throw new Error('Blend space needs two distinct parameter names');
      s.parameters.forEach(k=>blendParameter((project.variables || {})[k]));
      if(!Array.isArray(s.samples) || !s.samples.length || s.samples.length>256)throw new Error('Blend space needs 1..256 samples');
      s.samples.forEach((v,i)=>{
        if(!v || typeof v!=='object' || Object.keys(v).some(k=>!['clipId','position'].includes(k)))throw new Error('Invalid blend-space sample fields');
        if(!clips.some(c=>c.id===v.clipId && c.blendMode==='weighted') || ownedClips.has(v.clipId))throw new Error('Blend-space sample needs a unique existing weighted clip');
        ownedClips.add(v.clipId);
        if(!Array.isArray(v.position) || v.position.length!==2)throw new Error('Blend-space sample needs a 2D position');
        v.position.forEach(blendParameter);
        if(s.samples.slice(0,i).some(a=>(a.position[0]-v.position[0])**2+(a.position[1]-v.position[1])**2<=1e-12))throw new Error('Blend-space samples must be more than 0.000001 apart');
      });
      return {id:s.id,parameters:[...s.parameters],samples:s.samples.map(v=>({clipId:v.clipId,position:[...v.position]}))};
    }),'blend space');
    const machines=unique(list(project.stateMachines || project.machines).map(m=>{
      const states=unique(list(m.states),'state'), ids=new Set(states.map(s=>s.id));
      if(!ids.has(m.initial))throw new Error('Unknown initial state: '+m.initial);
      for(const t of [...(m.transitions || []),...states.flatMap(s=>s.transitions || [])])if(!ids.has(t.to || t.target))throw new Error('Unknown state: '+(t.to || t.target));
      return {...m,states};
    }),'machine');
    for(const source of [project,...clips]) {
      for(const cue of source.audioCues || []) {
        if(finite(cue.time,'Cue time')<0)throw new Error('Cue time must be nonnegative');
        for(const k of ['offset','volume','duration'])if(cue[k]!=null && (finite(cue[k],'Cue '+k)<0 || (k==='duration' && cue[k]===0)))throw new Error('Invalid cue '+k);
      }
    }
    const nodes=unique(list(project.nodes),'node'), bones=unique(list(project.bones),'bone'), assets=unique(list(project.audioAssets || project.assets),'asset');
    unique([...nodes,...bones,...clips,...machines,...assets,...(project.audioCues || []),...clips.flatMap(c=>c.audioCues || [])].filter(x=>x.id!=null),'global');
    const boneIds=new Set(bones.map(b=>b.id));
    for(const b of bones) if(b.parentId!=null && (typeof b.parentId!=='string' || !boneIds.has(b.parentId))) throw new Error('Missing bone parent: '+b.parentId);
    solveBones(bones);
    for(const n of nodes) if(n.boneId!=null && !boneIds.has(n.boneId)) throw new Error('Missing node bone: '+n.boneId);
    for(const n of nodes) if(n.motionPath!=null) {
      const b=n.motionPath,source=nodes.find(p=>p.id===b.path);
      if(!source || source===n || !source.kind || source.kind.type!=='path')throw new Error('Unknown or invalid motion path source');
      if(n.boneId!=null || source.boneId!=null || source.motionPath!=null)throw new Error('Motion path bindings cannot be chained or bone-bound');
      weightValue(b.progress===undefined?0:b.progress);
      if(b.orient!==undefined && typeof b.orient!=='boolean')throw new Error('Motion orientation must be boolean');
      motionSample(source.kind.d,source.transform,b.progress||0);
    }
    const validatePatch=(id,props)=>{if(boneIds.has(id)) bonePatch(props);if(own(props,'motionProgress')){if(!nodes.some(n=>n.id===id && n.motionPath))throw new Error('motionProgress requires a motionPath binding');weightValue(unwrap(props.motionProgress));}};
    for(const c of clips) for(const t of c.tracks) {
      if(c.blendMode==='additive' && !boneIds.has(t.target)) {
        const n=nodes.find(n=>n.id===t.target);
        if(!n)throw new Error('Unknown additive target: '+t.target);
        const kind={width:'rect',height:'rect',radius:'circle',fontSize:'text'}[t.property];
        if(kind && n.kind && n.kind.type!==kind)throw new Error('Additive property unsupported for node kind');
      }
      for(const k of t.keyframes) validatePatch(t.target,{[t.property]:k.value});
    }
    for(const m of machines) for(const s of m.states) {
      if(Array.isArray(s.overrides)) for(const o of s.overrides) validatePatch(o.nodeId || o.target,{[o.property]:o.value});
      else for(const [id,props] of Object.entries(s.overrides || {})) validatePatch(id,props);
    }
    const validateActions=items=>{for(const a of items || []){if((a.type==='setVariable' || a.type==='variable.set') && blendSpaces.some(s=>s.parameters.includes(a.name || a.variable)))blendParameter(a.value);if(a.type==='setProperty' && a.property==='motionProgress')validatePatch(a.nodeId || a.target,{[a.property]:a.value});if(a.type==='blendClip') {
      const c=clips.find(c=>c.id===a.clipId);
      if(!c || c.blendMode==='override')throw new Error('blendClip requires an existing weighted or additive clip: '+a.clipId);
      weightValue(a.weight);fadeDuration(a.duration===undefined?0:a.duration);
    }}};
    for(const m of machines) {
      for(const s of m.states) {for(const key of ['onEnter','entry','exit','onExit'])validateActions(s[key]);for(const t of s.transitions || [])validateActions(t.actions);}
      for(const t of m.transitions || [])validateActions(t.actions);
    }
    for(const b of project.interactions || [])validateActions(b.actions);
    for(const m of machines)for(const t of [...(m.transitions || []),...m.states.flatMap(s=>s.transitions || [])])
      {if(t.blend!=null && (finite(t.blend,'Transition blend')<0 || t.blend>60))throw new Error('Transition blend must be 0..60 seconds');
      if(t.after!=null && (finite(t.after,'Timer after')<0 || t.after>3600))throw new Error('Timer after must be 0..3600 seconds');}
    const nodeIds=new Set(nodes.map(n=>n.id));
    const followers=unique(list(project.followers),'follower').map(f=>{
      if(!nodeIds.has(f.target) && !boneIds.has(f.target))throw new Error('Unknown follower target: '+f.target);
      const mode=f.mode===undefined?'offset':f.mode;if(!['offset','rotate'].includes(mode))throw new Error('Unsupported follower mode: '+mode);
      const strength=f.strength===undefined?1:finite(f.strength,'Follower strength');if(strength<0 || strength>1)throw new Error('Follower strength must be 0..1');
      const max=finite(f.max,'Follower max');if(max<0 || (mode==='rotate' && max>180))throw new Error('Invalid follower max');
      const smoothing=f.smoothing===undefined?0:finite(f.smoothing,'Follower smoothing');if(smoothing<0 || smoothing>10)throw new Error('Follower smoothing must be 0..10');
      if(f.when!=null){const m=machines.find(m=>m.id===f.when.machine);if(!m || !Array.isArray(f.when.states) || !f.when.states.every(s=>m.states.some(st=>st.id===s)))throw new Error('Unknown follower condition: '+f.id);}
      return {...f,mode,strength,max,smoothing,restAngle:f.restAngle==null?0:finite(f.restAngle,'Follower restAngle')};
    });
    unique([...nodes,...bones,...clips,...machines,...followers,...blendSpaces,...assets,...(project.audioCues || []),...clips.flatMap(c=>c.audioCues || [])].filter(x=>x.id!=null),'global');
    return {...project,nodes,bones,clips,machines,assets,followers,blendSpaces};
  }
  function owningSvg(el) {
    for(let n=el;n;n=n.parentNode)if((n.tagName || '').toLowerCase()==='svg')return n;
    return el && el.ownerSVGElement;
  }
  function readProject(svg) {
    const candidates=svg.querySelectorAll('[id="splinely-project"], [data-splinely-project]');
    const block=[...candidates].find(el=>(!owningSvg(el) || owningSvg(el)===svg) && (!el.namespaceURI || el.namespaceURI==='http://www.w3.org/2000/svg'));
    if (!block) throw new Error('Missing splinely-project metadata');
    try { return JSON.parse(block.textContent); } catch (e) { throw new Error('Invalid splinely-project JSON: ' + e.message); }
  }
  function createRuntime(svg, project, options) {
    options = options || {};
    if (!svg || typeof svg.querySelectorAll !== 'function') throw new Error('An SVG root is required');
    project = normalize(project == null ? readProject(svg) : project);
    if (svg.splinely) throw new Error('SVG already has a Splinely runtime');
    const env=options.window || (svg.ownerDocument && svg.ownerDocument.defaultView) || host;
    const diagnostics=[], subscriptions=new Map(), cleanup=[], explicit=new Map(), nodes=new Map(), clips=new Map(), machines=new Map();
    const bones=new Map(project.bones.map(b=>[b.id,b]));
    let bonePoses=[];
    const variables=new Map(Object.entries(project.variables || {}).map(([k,v]) => [k,v]));
    let dead=false, raf=null, last=null, api, processing=false, ticking=false;
    let syncClock=()=>{};
    const queue=[];
    const report = error => { const e=error instanceof Error ? error : new Error(String(error)); diagnostics.push(e); if (diagnostics.length>100) diagnostics.shift(); dispatch('error',{error:e}); return e; };
    function dispatch(name, detail, target) {
      const keys = target == null ? [name] : [target+'\u0000'+name,name];
      for (const key of keys) for (const fn of [...(subscriptions.get(key) || [])]) {
        try { const result=fn(detail,api); if (result && typeof result.catch === 'function') result.catch(e => { if (name !== 'error') report(e); }); }
        catch(e) { if (name !== 'error') report(e); }
      }
    }
    const ensure = () => { if (dead) throw new Error('Splinely runtime destroyed'); };
    function on(a,b,c) {
      ensure(); const key=typeof b === 'function' ? a : a+'\u0000'+b, fn=typeof b === 'function' ? b : c;
      if (typeof fn !== 'function') throw new Error('Event handler must be a function');
      if (!subscriptions.has(key)) subscriptions.set(key,new Set()); subscriptions.get(key).add(fn);
      return () => { const set=subscriptions.get(key); if(set) {set.delete(fn); if(!set.size) subscriptions.delete(key);} };
    }
    const elements=new Map(); if(svg.id) elements.set(svg.id,svg);
    for(const el of svg.querySelectorAll('[id]'))if((!owningSvg(el) || owningSvg(el)===svg) && (!el.namespaceURI || el.namespaceURI==='http://www.w3.org/2000/svg') && !['svg','script','metadata'].includes((el.tagName || '').toLowerCase()))elements.set(el.id,el);
    for (const node of project.nodes) {
      const el=elements.get(node.id); if (!el && node.visible!==false) {report('Missing SVG node: '+node.id); continue;}
      const core=!!node.kind;
      const base=core ? {x:0,y:0,rotation:0,scaleX:1,scaleY:1,opacity:1,fill:{r:80/255,g:140/255,b:240/255,a:1},...node.transform,...node.style} : {...node.properties};
      if(core) { for(const k of ['width','height','radius','fontSize','d']) if(node.kind[k]!=null) base[k]=node.kind[k]; if(node.gradient) base.gradient=node.gradient; if(node.motionPath) base.motionProgress=node.motionPath.progress===undefined?0:node.motionPath.progress; }
      nodes.set(node.id,{el,base,core,visible:node.visible!==false,boneId:node.boneId,gradient:!!(core && node.gradient),original:new Map(),lastKeys:new Set()});
    }
    // Adapter allows exported elements without a corresponding node descriptor.
    function getNode(id) {
      if(!nodes.has(id) && elements.has(id)) nodes.set(id,{el:elements.get(id),base:{},core:false,original:new Map(),lastKeys:new Set()});
      if(!nodes.has(id)) throw new Error('Unknown node: '+id); return nodes.get(id);
    }
    function attr(node,name,value) {
      if(dead) return;
      if(!node.original.has(name)) node.original.set(name,node.el.getAttribute(name));
      if(value == null) node.el.removeAttribute(name); else node.el.setAttribute(name,String(value));
    }
    const aliases={radius:'r',fontSize:'font-size',strokeWidth:'stroke-width',scaleX:'scaleX',scaleY:'scaleY'};
    const attrName=k => Object.prototype.hasOwnProperty.call(aliases,k)?aliases[k]:k;
    const supported=new Set('x y cx cy r radius rx ry width height fill stroke opacity fill-opacity stroke-opacity strokeWidth stroke-width fontSize font-size rotation scaleX scaleY transform d points visibility'.split(' '));
    // Update the exported <linearGradient>/<radialGradient> referenced by fill="url(#…)".
    function applyGradient(node,g) {
      if(!node.el || !g || !Array.isArray(g.stops)) return;
      const ref=/url\(#([^)]+)\)/.exec(node.el.getAttribute('fill') || '');
      const el=ref && elements.get(ref[1]);
      if(!el) return;
      if(!g.radial) {
        const a=(Number(g.angle)||0)*Math.PI/180, r=v=>Math.round(v*1e6)/1e6+0;
        el.setAttribute('x1',r(0.5-Math.cos(a)/2)); el.setAttribute('y1',r(0.5-Math.sin(a)/2));
        el.setAttribute('x2',r(0.5+Math.cos(a)/2)); el.setAttribute('y2',r(0.5+Math.sin(a)/2));
      }
      const stops=[...(el.children || [])].filter(c=>(c.tagName || '').toLowerCase()==='stop');
      stops.forEach((stop,i)=>{
        const s=g.stops[i], c=s && color(s.color); if(!c) return;
        stop.setAttribute('offset',clamp(Number(s.offset)||0,0,1));
        stop.setAttribute('stop-color',`rgb(${c.slice(0,3).map(v=>Math.round(clamp(v,0,1)*255)).join(',')})`);
        stop.setAttribute('stop-opacity',clamp(c[3],0,1));
      });
    }
    // Followers: current smoothed value per follower, [dx,dy] or [degrees,0].
    const follow=new Map(); let pointer=null;
    // Honour the viewer's "reduce motion" setting unless the host decides: no
    // looping autoplay, instant state changes, resting followers.
    const reducedMotion=options.reducedMotion!=null?!!options.reducedMotion:(()=>{try{return !!(env.matchMedia && env.matchMedia('(prefers-reduced-motion: reduce)').matches);}catch(_){return false;}})();
    function followerOrigin(f) {
      if(Array.isArray(f.origin))return f.origin;
      const n=project.nodes.find(n=>n.id===f.target);
      if(n && n.kind) {
        const k=n.kind,t={x:0,y:0,rotation:0,scaleX:1,scaleY:1,...n.transform};
        const box=k.type==='rect'?[k.x,k.y,k.width,k.height]:k.type==='circle'?[k.cx-k.radius,k.cy-k.radius,2*k.radius,2*k.radius]:k.type==='path'?k.bounds:[k.x,k.y-k.fontSize,String(k.text).length*k.fontSize*0.6,k.fontSize*1.2];
        const a=t.rotation*Math.PI/180,c=Math.cos(a),s=Math.sin(a),xs=[],ys=[];
        for(const [px,py] of [[box[0],box[1]],[box[0]+box[2],box[1]],[box[0],box[1]+box[3]],[box[0]+box[2],box[1]+box[3]]]){const sx=px*t.scaleX,sy=py*t.scaleY;xs.push(t.x+sx*c-sy*s);ys.push(t.y+sx*s+sy*c);}
        return [(Math.min(...xs)+Math.max(...xs))/2,(Math.min(...ys)+Math.max(...ys))/2];
      }
      const pose=bonePoses.find(p=>p.id===f.target);return pose?[pose.x,pose.y]:[0,0];
    }
    function followerGoal(f) {
      if(!pointer || reducedMotion)return [0,0];
      if(f.when){const m=machines.get(f.when.machine);if(!m || !f.when.states.includes(m.state))return [0,0];}
      const [ox,oy]=followerOrigin(f),dx=pointer[0]-ox,dy=pointer[1]-oy;
      if(f.mode==='rotate'){
        if(dx===0 && dy===0)return [0,0];
        let angle=(Math.atan2(dy,dx)*180/Math.PI-f.restAngle)%360;if(angle>180)angle-=360;if(angle<-180)angle+=360;
        return [clamp(angle*f.strength,-f.max,f.max),0];
      }
      let x=dx*f.strength,y=dy*f.strength;const length=Math.hypot(x,y);
      if(length>f.max && length>0){x*=f.max/length;y*=f.max/length;}
      return [x,y];
    }
    function updateFollowers(dt) {
      let moved=false;
      for(const f of project.followers) {
        const goal=followerGoal(f),cur=follow.get(f.id) || [0,0];
        const keep=f.smoothing>0?Math.exp(-4*dt/f.smoothing):0;
        const next=[goal[0]+(cur[0]-goal[0])*keep,goal[1]+(cur[1]-goal[1])*keep].map(v=>Math.abs(v)<1e-6?0:v);
        if(next[0]!==cur[0] || next[1]!==cur[1])moved=true;
        follow.set(f.id,next);
      }
      return moved;
    }
    function fireTimers() {
      for(const m of machines.values()) {
        const state=m.states.get(m.state);
        const t=[...(state.transitions || []),...(m.transitions || [])].find(t=>t.after!=null && (t.from==null || t.from===m.state) && t.after<=m.elapsed+1e-9 && guard(t.guard));
        if(t)enqueue({id:m.id,timer:t,event:'after'});
      }
    }
    function advanceBlends(dt) {
      let changed=false;
      for(const m of machines.values())m.elapsed=(m.elapsed || 0)+dt;
      fireTimers();
      for(const m of machines.values())if(m.fade){m.fade.elapsed=Math.min(m.fade.duration,m.fade.elapsed+dt);if(m.fade.elapsed>=m.fade.duration)m.fade=null;changed=true;}
      return updateFollowers(dt) || changed;
    }
    function render() {
      if(dead) return;
      const values=new Map([...nodes].map(([id,n]) => [id,{...n.base}]));
      const boneValues=new Map([...bones].map(([id,b])=>[id,{...b}]));
      const merge=(id,props) => { try {
        if(bones.has(id)) Object.assign(boneValues.get(id),bonePatch(props));
        else {getNode(id); if(!values.has(id)) values.set(id,{}); Object.assign(values.get(id),props);}
      } catch(e){report(e);} };
      const entries=state=>{
        const overrides=state && state.overrides, out=[];
        if(Array.isArray(overrides)) for(const o of overrides) out.push([o.nodeId || o.target,o.property,o.value]);
        else for(const [id,p] of Object.entries(overrides || {})) for(const [k,v] of Object.entries(p)) out.push([id,k,v]);
        return out;
      };
      const current=(id,property)=>{const v=bones.has(id)?boneValues.get(id):values.get(id);return v && v[property];};
      for(const m of machines.values()) {
        const now=entries(m.states.get(m.state));
        if(!m.fade) {for(const [id,k,v] of now) merge(id,{[k]:v});continue;}
        // Crossfade every property either state sets; a property only one state
        // sets fades between that value and the value underneath.
        const before=entries(m.states.get(m.fade.from)), t=smoothstep(m.fade.elapsed/m.fade.duration), touched=new Map();
        for(const [id,k] of [...before,...now])touched.set(id+'\u0000'+k,[id,k]);
        for(const [id,k] of touched.values()) {
          const under=current(id,k), pick=list=>{const hit=list.filter(([i,p])=>i===id && p===k).pop();return hit?hit[2]:under;};
          const a=pick(before), b=pick(now);
          merge(id,{[k]:a===undefined?b:b===undefined?a:mixValues(a,b,t)});
        }
      }
      const spaceFactors=spaceWeights(project.blendSpaces,k=>variables.get(k));
      const restValue=(id,property)=>{
        if(bones.has(id))return bones.get(id)[property];
        const n=getNode(id);let v=n.base[property];
        if(v===undefined && !n.core && n.el){const name=attrName(property);v=n.original.has(name)?n.original.get(name):n.el.getAttribute(name);}
        if(v==null || v==='none')return property==='stroke'?{r:0,g:0,b:0,a:0}:['scaleX','scaleY','opacity'].includes(property)?1:0;
        return ['fill','stroke'].includes(property)?v:Number(unwrap(v));
      };
      const additive=(id,property,sample,weight)=>{
        if(weight===0)return;
        const reference=restValue(id,property), under=current(id,property);
        const a=vectorOf(under==null?reference:under), b=vectorOf(sample), r=vectorOf(reference);
        if(!a || !b || !r || a.shape!==b.shape || a.shape!==r.shape)throw new Error('Additive reference and sample must have matching numeric/RGBA types');
        const nums=a.nums.map((v,i)=>v+weight*(b.nums[i]-r.nums[i]));
        if(a.kind==='c' || ['opacity','motionProgress'].includes(property))nums.forEach((v,i)=>nums[i]=clamp(v,0,1));
        else if(['width','height','radius','fontSize'].includes(property))nums[0]=Math.max(0,nums[0]);
        nums.forEach(v=>finite(v,'Additive result'));
        merge(id,{[property]:fromVector(a,nums)});
      };
      let group=new Map();
      const flush=()=>{
        for(const [id,props] of group)for(const [property,samples] of props) {
          const total=samples.reduce((s,v)=>s+v.weight,0);if(total===0)continue;
          try {
            let base=(bones.has(id)?boneValues.get(id):values.get(id) || getNode(id).base)[property];
            const residual=Math.max(0,1-total), divisor=Math.max(1,total);
            // Adapter baselines come from original DOM, never a previously animated frame.
            if(residual>0 && base===undefined && !bones.has(id)) {
              const node=getNode(id), name=attrName(property);
              if(!node.core && node.el) {
                base=node.original.has(name)?node.original.get(name):node.el.getAttribute(name);
                if(typeof samples[0].value==='number' && base!=null)base=Number(base);
                if(property==='stroke' && base==='none')base=null;
              }
            }
            let value;
            if(typeof samples[0].value==='number') {
              const fallback=['scaleX','scaleY','opacity'].includes(property)?1:0;
              const baseline=residual===0?0:finite(base==null?fallback:unwrap(base),'Weighted baseline');
              value=baseline*residual+samples.reduce((s,v)=>s+v.value*(v.weight/divisor),0);
              finite(value,'Weighted result');
            } else if(['gradient','d'].includes(property)) {
              // Structured values blend number by number when shapes match;
              // otherwise the heaviest contribution wins.
              const vectors=samples.map(v=>vectorOf(v.value)), baseVector=vectorOf(base);
              const shape=vectors[0] && vectors[0].shape;
              if(vectors.every(v=>v && v.shape===shape) && (residual===0 || (baseVector && baseVector.shape===shape))) {
                const nums=vectors[0].nums.map((_,i)=>(residual===0?0:baseVector.nums[i]*residual)+vectors.reduce((s,v,k)=>s+v.nums[i]*(samples[k].weight/divisor),0));
                value=fromVector(vectors[0],nums);
              } else {
                const heaviest=samples.reduce((a,b)=>b.weight>a.weight?b:a);
                value=heaviest.weight>=residual?heaviest.value:base;
              }
            } else {
              const baseline=residual===0?[0,0,0,0]:color(unwrap(base)) || (property==='stroke' && base==null?[0,0,0,0]:null);
              if(!baseline)throw new Error('Weighted baseline must be RGBA');
              const channels=baseline.map((b,i)=>b*residual+samples.reduce((s,v)=>s+color(v.value)[i]*(v.weight/divisor),0));
              value={r:channels[0],g:channels[1],b:channels[2],a:channels[3]};
            }
            merge(id,{[property]:value});
          } catch(e){report(e);}
        }
        group=new Map();
      };
      for(const c of [...clips.values()].sort((a,b)=>a.priority-b.priority || a.index-b.index)) if(c.active) {
        if(c.blendMode!=='weighted')flush();
        // A clip contributes at most once per property (last duplicate track wins).
        const sampled=new Map();
        for(const t of c.tracks) {
          const value=evaluateTrack(t,c.time);if(value===undefined)continue;
          if(c.blendMode==='additive'){try{additive(t.target,t.property,value,c.weight);}catch(e){report(e);}}
          else if(c.blendMode!=='weighted')merge(t.target,{[t.property]:value});
          else {if(!sampled.has(t.target))sampled.set(t.target,new Map());sampled.get(t.target).set(t.property,value);}
        }
        if(c.blendMode==='weighted' && c.weight>0)for(const [id,props] of sampled) {
          if(!group.has(id))group.set(id,new Map());
          for(const [property,value] of props) {
            const target=group.get(id);if(!target.has(property))target.set(property,[]);
            const weight=c.weight*(spaceFactors.has(c.id)?spaceFactors.get(c.id):1);
            if(weight>0)target.get(property).push({value,weight});
          }
        }
      }
      flush();
      for(const [id,p] of explicit) merge(id,p);
      for(const f of project.followers) {
        const [a,b]=follow.get(f.id) || [0,0];if(a===0 && b===0)continue;
        const v=bones.has(f.target)?boneValues.get(f.target):(values.get(f.target) || {});
        const num=k=>{const x=unwrap(v[k]);return typeof x==='number'?x:0;};
        if(f.mode==='rotate')merge(f.target,{rotation:num('rotation')+a});
        else merge(f.target,{x:num('x')+a,y:num('y')+b});
      }
      for(const n of project.nodes) if(n.motionPath) {
        try {
          const binding=n.motionPath,source=values.get(binding.path),props=values.get(n.id);
          const [x,y,angle]=motionSample(unwrap(source.d),Object.fromEntries(['x','y','rotation','scaleX','scaleY'].map(k=>[k,unwrap(source[k])])),unwrap(props.motionProgress));
          props.x=(unwrap(props.x)||0)+x;props.y=(unwrap(props.y)||0)+y;
          if(binding.orient)props.rotation=(unwrap(props.rotation)||0)+angle;
        } catch(e){report(e);}
      }
      let poses;
      try {bonePoses=solveBones([...boneValues.values()]); poses=new Map(bonePoses.map(p=>[p.id,p]));}
      catch(e) {report(e);return;} // Keep the last valid frame on FK overflow.
      for(const [id,props] of values) {
        if(dead) return;
        const binding=getNode(id).boneId;
        if(binding!=null) {
          try {
            const pose=poses.get(binding), angle=(pose.rotation%360)*Math.PI/180, c=Math.cos(angle), s=Math.sin(angle);
            const x=finite(unwrap(props.x==null?0:props.x),'Transform x'), y=finite(unwrap(props.y==null?0:props.y),'Transform y');
            props.x=finite(pose.x+c*x-s*y,'Attached x'); props.y=finite(pose.y+s*x+c*y,'Attached y');
            props.rotation=finite(pose.rotation+finite(unwrap(props.rotation==null?0:props.rotation),'Transform rotation'),'Attached rotation');
          } catch(e) {report(e);continue;}
        }
        const node=getNode(id), keys=new Set();
        if(!node.el) continue; // Hidden descriptors remain valid animation/override targets.
        if(node.visible===false) {attr(node,'visibility','hidden'); keys.add('visibility');}
        const composed=node.core || binding!=null;
        if(composed) {
          try {
            const component=(key,fallback)=>finite(props[key]==null?fallback:unwrap(props[key]),'Transform '+key);
            attr(node,'transform',`translate(${component('x',0)} ${component('y',0)}) rotate(${component('rotation',0)}) scale(${component('scaleX',1)} ${component('scaleY',1)})`);
          } catch(error){report(error);} keys.add('transform');
        }
        for(const [key,raw] of Object.entries(props)) {
          if(dead) return;
          if(composed && ['x','y','rotation','scaleX','scaleY'].includes(key)) continue;
          if(composed && key==='transform') {report('Raw transform is unsupported on composed nodes');continue;}
          // Never turn schema properties into scripts, URLs, or markup.
          if(key==='motionProgress') continue;
          if(key==='gradient') {applyGradient(node,unwrap(raw));continue;}
          if(!supported.has(key)) {report('Unsupported property: '+key);continue;}
          // A gradient fill is static SVG; colour values must not replace its url().
          if(node.gradient && (key==='fill' || key==='fill-opacity')) continue;
          if(node.visible===false && key==='visibility') continue;
          const name=attrName(key), value=unwrap(raw), col=color(value);
          if((key==='fill'||key==='stroke') && col) {
            attr(node,name,`rgb(${Math.round(clamp(col[0],0,1)*255)},${Math.round(clamp(col[1],0,1)*255)},${Math.round(clamp(col[2],0,1)*255)})`);
            attr(node,name+'-opacity',clamp(col[3],0,1)); keys.add(name+'-opacity');
          } else {
            if(Array.isArray(value) && !value.every(v=>typeof v==='number' && Number.isFinite(v))){report('Property arrays must contain finite numbers');continue;}
            const serialized=Array.isArray(value)?value.join(' '):(node.core && key==='stroke' && value==null?'none':value);
            if(typeof serialized === 'string' && (/url\s*\(/i.test(serialized) || serialized.includes('\\'))){report('URL properties are unsupported');continue;}
            if(typeof serialized==='number' && !Number.isFinite(serialized)){report('Property must be finite');continue;}
            if(serialized && typeof serialized==='object'){report('Unsupported property value');continue;}
            attr(node,name,serialized);
          }
          keys.add(name);
        }
        for(const key of node.lastKeys) if(!keys.has(key)) attr(node,key,node.original.get(key));
        node.lastKeys=keys;
      }
    }
    function set(id,props) {ensure();if(props && own(props,'motionProgress')) {if(!project.nodes.some(n=>n.id===id && n.motionPath))throw new Error('motionProgress requires a motionPath binding');weightValue(unwrap(props.motionProgress));}if(!bones.has(id))getNode(id); if(props===null) explicit.delete(id); else {if(!props || typeof props!=='object' || Array.isArray(props)) throw new Error('Properties must be an object or null');explicit.set(id,{...explicit.get(id),...(bones.has(id)?bonePatch(props):props)});}render();}
    function variable(id) {ensure();if(!variables.has(id)) throw new Error('Unknown variable: '+id);return {get:()=>variables.get(id),set:v=>{ensure();const old=variables.get(id);if((old===null?v!==null:typeof old!==typeof v) || Array.isArray(old)!==Array.isArray(v))throw new Error('Variable type mismatch: '+id);if(v===undefined || typeof v==='function' || (typeof v==='number' && !Number.isFinite(v)))throw new Error('Variable must be a JSON value');if(project.blendSpaces.some(s=>s.parameters.includes(id)))blendParameter(v);variables.set(id,v);render();emit('variable.change',{id,value:v});}};}
    function actions(items,detail) {
      for(const a of items || []) {
        try {
          switch(a.type) {
            case 'setProperty': set(a.nodeId,{[a.property]:a.value});break;
            case 'set': set(a.target,a.properties);break;
            case 'playClip': case 'clip.play': clip(a.clipId || a.clip).play();break;
            case 'blendClip': clip(a.clipId).fadeTo(a.weight,a.duration===undefined?0:a.duration);break;
            case 'stopClip': case 'clip.stop': clip(a.clipId || a.clip).stop();break;
            case 'clip.pause': clip(a.clipId || a.clip).pause();break;
            case 'clip.seek': clip(a.clipId || a.clip).seek(a.time);break;
            case 'setVariable': case 'variable.set': variable(a.name || a.variable).set(a.value);break;
            case 'emit': emit(typeof a.event === 'object'?a.event.name:a.event,detail,typeof a.event==='object'?a.event.target:undefined);break;
            case 'machine.send': send(a.machine,a.event,detail);break;
            case 'playAudio': case 'audio.play': audio.play(a.assetId || a.asset);break;
            default: throw new Error('Unsupported action: '+a.type);
          }
        } catch(e) {report(e);}
      }
    }
    function eventMatches(spec,event,target) { return typeof spec==='string' ? spec===event : spec && spec.name===event && (spec.target==null || spec.target===target); }
    function guard(g) {if(!g)return true; if(typeof g !== 'object' || !own(g,'variable')) {report('Unsupported guard');return false;} return variables.has(g.variable) && equal(variables.get(g.variable),g.equals);}
    function send(id,event,detail,target) {ensure();if(!machines.has(id)) throw new Error('Unknown machine: '+id);enqueue({id,event,detail,target});}
    function enqueue(item) {if(!processing){syncClock();ensure();}queue.push(item);drain();}
    function drain() {
      if(processing)return; processing=true;
      try {
        let count=0;
        while(queue.length) {
          if(++count>256) {queue.length=0;report('Event cycle limit exceeded (256)');break;}
          const e=queue.shift();
          if(e.broadcast) {dispatch(e.event,e.detail,e.target);queue.unshift(...[...machines.keys()].map(id=>({...e,id,broadcast:false})));continue;}
          const m=machines.get(e.id); if(!m)continue;
          const state=m.states.get(m.state);
          const transitions=[...(state.transitions || []),...(m.transitions || [])];
          // Timer transitions fire from advanceBlends, never on events.
          const t=e.timer?(transitions.includes(e.timer) && guard(e.timer.guard)?e.timer:null):transitions.find(t=>t.after==null && (t.from==null || t.from===m.state || t.from==='*') && eventMatches(t.event,e.event,e.target) && guard(t.guard));
          if(!t)continue;
          const next=t.to || t.target; if(!m.states.has(next)) {report('Unknown state: '+next);continue;}
          actions(state.exit || state.onExit,e.detail); const previous=m.state; m.state=next;
          const blend=t.blend==null?0:t.blend;m.fade=blend>0 && !reducedMotion?{from:previous,elapsed:0,duration:blend}:null;m.elapsed=0;
          actions(t.actions,e.detail); actions(m.states.get(next).onEnter || m.states.get(next).entry,e.detail);
          render();dispatch('state.change',{machine:m.id,from:previous,to:next,event:e.timer?'after':e.event});
        }
      } finally {processing=false;}
    }
    function emit(event,detail,target) {ensure();enqueue({broadcast:true,event,detail,target});}
    for(const m of project.machines) {
      const states=new Map(m.states.map(s=>[s.id,s])); if(!states.has(m.initial)) throw new Error('Unknown initial state: '+m.initial);
      machines.set(m.id,{...m,states,state:m.initial,elapsed:0});
    }
    function machine(id) {ensure();const m=machines.get(id);if(!m) throw new Error('Unknown machine: '+id);return {send:(event,detail)=>send(id,typeof event==='object'?event.name:event,detail,typeof event==='object'?event.target:undefined),get state(){return m.state;}};}
    const callbacks=new Set();
    for(const [index,c] of project.clips.entries()) clips.set(c.id,{...c,index,priority:c.priority || 0,time:0,playing:false,active:false,cycle:0,revision:0,ramp:null,sources:new Map()});
    const first=project.clips[0] && project.clips[0].id;
    const transport={id:'@timeline',duration:Infinity,time:0,playing:false,manual:false,loop:false,cycle:0,revision:0,sources:new Map(),audioCues:project.audioCues || []};
    const cueList=c => c.audioCues || [];
    function transportPlay(){if(!transport.playing){transport.playing=true;transport.revision++;if(transport.time===0)crossing(transport,0,0,true);schedule(transport);}}
    function transportPause(){transport.playing=false;transport.revision++;stopSources(transport);}
    function getClip(id) {ensure();const c=clips.get(id);if(!c)throw new Error('Unknown clip: '+id);return c;}
    function stopSources(c) {for(const source of c.sources.values()) source.stop(); c.sources.clear();}
    function crossing(c,from,to,zero) {
      const rev=c.revision;
      const items=[...callbacks].filter(cb=>cb.clip===c.id);
      items.sort((a,b)=>a.time-b.time);
      for(const item of items) if((item.time>from && item.time<=to) || (zero && item.time===0)) {
        if(c.revision!==rev || dead) return false;
        if(callbacks.has(item.ref)) try{const result=item.fn({clip:c.id,time:item.time,cycle:c.cycle},api);if(result && typeof result.catch==='function')result.catch(report);}catch(e){report(e);}
      }
      return c.revision===rev && !dead;
    }
    const failedLoopCues=new Set();
    function schedule(c) {
      if(!c.playing || !audio.ready)return;
      const cues=cueList(c);
      for(let lap=0;lap<=(c.loop && !audio.context.createBuffer?1:0);lap++) for(let i=0;i<cues.length;i++) {
        const cue=cues[i], key=(c.loop?'loop':c.cycle+lap)+':'+i;
        if(c.sources.has(key))continue;
        if(c.loop && audio.context.createBuffer) {
          const failure=c.id+':'+i;if(failedLoopCues.has(failure))continue;
          try {const buffer=audio.loopBuffer(c,i,cue);c.sources.set(key,audio.start(cue.assetId || cue.asset,{buffer,loop:true,offset:c.time%c.duration,volume:cue.volume}));}catch(e){failedLoopCues.add(failure);report(e);}
          continue;
        }
        const start=finite(cue.time,'Audio cue time')+(lap?lap*c.duration:0);
        const asset=cue.assetId || cue.asset;
        const buffer=audio.buffers.get(asset);if(!buffer){report('Unknown or unavailable audio asset: '+asset);continue;}
        const offset=cue.offset || 0, length=Math.min(cue.duration==null?buffer.duration-offset:cue.duration,c.duration-cue.time);
        const elapsed=Math.max(0,c.time-start);
        if(start<0 || cue.time>=c.duration || length<=elapsed)continue;
        try {
          const handle=audio.start(asset,{offset:offset+elapsed,volume:cue.volume,duration:length-elapsed,when:audio.context.currentTime+Math.max(0,start-c.time)});
          c.sources.set(key,handle);
        } catch(e){report(e);}
      }
    }
    function clip(id) {
      const c=getClip(id);
      return {
        get time(){return c.time;},get duration(){return c.duration;},get playing(){return c.playing;},get loop(){return c.loop;},
        get weight(){ensure();syncClock();return c.weight;},
        setWeight(weight){ensure();weightValue(weight);if(c.blendMode==='override')throw new Error('Weight controls require a weighted or additive clip');syncClock();c.ramp=null;c.weight=weight;render();return this;},
        fadeTo(weight,seconds){ensure();weightValue(weight);fadeDuration(seconds);if(c.blendMode==='override')throw new Error('Weight controls require a weighted or additive clip');syncClock();c.ramp=seconds===0?null:{from:c.weight,to:weight,duration:seconds,elapsed:0};if(seconds===0)c.weight=weight;render();return this;},
        set loop(v){ensure();c.loop=!!v;if(c.loop)c.time%=c.duration;c.revision++;stopSources(c);render();schedule(c);},
        play(){ensure();syncClock();if(c.playing)return this;if(c.time>=c.duration){c.time=0;c.cycle=0;}c.active=true;c.playing=true;c.revision++;const revision=c.revision;transportPlay();if(dead || c.revision!==revision || !c.playing)return this;render();if(c.time===0)crossing(c,0,0,true);schedule(c);return this;},
        pause(){ensure();syncClock();c.playing=false;c.revision++;stopSources(c);if(!transport.manual && ![...clips.values()].some(c=>c.playing))transportPause();return this;},
        seek(time){ensure();syncClock();finite(time,'Seek time');c.time=c.loop?Math.max(0,time)%c.duration:clamp(time,0,c.duration);c.cycle=0;c.active=true;c.revision++;stopSources(c);render();schedule(c);return this;},
        stop(){this.pause();c.ramp=null;c.time=0;c.cycle=0;c.active=false;render();return this;}
      };
    }
    function tick(delta) {
      ensure();if(finite(delta,'Tick delta')<0)throw new Error('Tick delta must be nonnegative');
      if(ticking)throw new Error('Reentrant tick is unsupported');
      ticking=true;
      try {
      if(advanceBlends(delta))render();
      let remaining=delta,segments=0;
      do {
        if(++segments>10000){report('Transport catch-up limit exceeded (10000)');for(const c of clips.values())if(c.playing)clip(c.id).pause();transportPause();break;}
        const active=[transport,...clips.values()].filter(c=>c.playing);
        const fading=[...clips.values()].filter(c=>c.ramp);
        if(!active.length && !fading.length)break;
        let step=remaining;
        for(const c of fading)step=Math.min(step,c.ramp.duration-c.ramp.elapsed);
        for(const c of active) {
          step=Math.min(step,c.duration-c.time);
          for(const cb of callbacks)if(cb.clip===c.id && cb.time>c.time)step=Math.min(step,cb.time-c.time);
        }
        const snapshots=active.map(c=>({c,from:c.time,revision:c.revision}));
        for(const c of active)c.time+=step;
        for(const c of fading) {
          const r=c.ramp;r.elapsed=Math.min(r.duration,r.elapsed+step);
          c.weight=r.elapsed===r.duration?r.to:r.from*(1-r.elapsed/r.duration)+r.to*(r.elapsed/r.duration);
          if(r.elapsed===r.duration)c.ramp=null;
        }
        remaining=Math.max(0,remaining-step);render();
        for(const {c,from,revision} of snapshots) {
          if(dead)return;if(c.revision!==revision || !c.playing)continue;
          if(!crossing(c,from,c.time,false))continue;
          if(c.time===c.duration) {
            if(!c.loop){c.playing=false;c.revision++;stopSources(c);emit('clip.end',{clip:c.id},c.id);}
            else {c.time=0;c.cycle++;for(const [key,h] of c.sources)if(Number(key.split(':')[0])<c.cycle){h.stop();c.sources.delete(key);}render();crossing(c,0,0,true);}
          }
        }
      } while(remaining>0 && !dead);
      if(dead)return;for(const c of [transport,...clips.values()])schedule(c);render();
      } finally {ticking=false;}
    }
    function at(time,fn,opts) {
      ensure();finite(time,'Callback time');if(typeof fn!=='function')throw new Error('Timeline callback must be a function');
      const id=opts && opts.clip || transport.id,c=id===transport.id?transport:getClip(id);if(time<0 || time>c.duration)throw new Error('Callback outside clip');
      const cb={clip:id,time,fn};cb.ref=cb;callbacks.add(cb);return()=>callbacks.delete(cb);
    }
    let context=null, unlocking=null,loopBytes=0;const buffers=new Map(),independent=new Set(),loopBuffers=new Map();
    const audio={
      ready:false,buffers,get context(){return context;},
      loopBuffer(c,index,cue){
        const key=c.id+':'+index;if(loopBuffers.has(key))return loopBuffers.get(key);
        const original=buffers.get(cue.assetId || cue.asset);if(!original)throw new Error('Unknown loop audio asset');
        const frames=Math.ceil(c.duration*original.sampleRate),bytes=frames*original.numberOfChannels*4;
        if(loopBytes+bytes>128*1024*1024)throw new Error('Loop audio padding exceeds 128 MiB limit');
        const start=Math.round(cue.time*original.sampleRate),offset=Math.round((cue.offset || 0)*original.sampleRate);
        const count=Math.min(frames-start,original.length-offset,Math.round((cue.duration==null?c.duration:cue.duration)*original.sampleRate));
        if(start<0 || offset<0 || count<=0)throw new Error('Invalid loop audio cue range');
        const buffer=context.createBuffer(original.numberOfChannels,frames,original.sampleRate);
        for(let ch=0;ch<original.numberOfChannels;ch++)buffer.getChannelData(ch).set(original.getChannelData(ch).subarray(offset,offset+count),start);
        loopBytes+=bytes;loopBuffers.set(key,buffer);return buffer;
      },
      async unlock(){
        ensure();if(unlocking)return unlocking;
        unlocking=(async()=>{
          const C=env.AudioContext || env.webkitAudioContext;if(!C)throw new Error('Web Audio unavailable');if(!context)context=new C();
          await context.resume();
          for(const a of project.assets) if(!buffers.has(a.id)) {
            const data=a.dataBase64 || a.data;if(a.url || typeof data!=='string' || !/^[a-z\d+/]*={0,2}$/i.test(data) || data.length%4!==0)throw new Error('Asset must contain embedded base64: '+a.id);
            if(data.length>64*1024*1024)throw new Error('Audio asset exceeds 64 MiB encoded limit: '+a.id);
            const raw=env.atob ? env.atob(data) : (typeof Buffer!=='undefined'?Buffer.from(data,'base64').toString('binary'):null);if(raw===null)throw new Error('Base64 decoder unavailable');
            const bytes=Uint8Array.from(raw,c=>c.charCodeAt(0));const buffer=await context.decodeAudioData(bytes.buffer);
            if(dead)throw new Error('Splinely runtime destroyed');buffers.set(a.id,buffer);
          }
          if(dead)throw new Error('Splinely runtime destroyed');audio.ready=true;for(const c of [transport,...clips.values()])schedule(c);return api;
        })();
        try{return await unlocking;}catch(e){report(e);throw e;}finally{unlocking=null;}
      },
      start(id,opts){
        ensure();if(!audio.ready)throw new Error('Audio is locked; await audio.unlock()');opts=opts || {};const buffer=opts.buffer || buffers.get(id);if(!buffer)throw new Error('Unknown audio asset: '+id);
        const offset=finite(opts.offset==null?0:opts.offset,'Audio offset'),volume=finite(opts.volume==null?1:opts.volume,'Audio volume');
        const duration=finite(opts.duration==null?buffer.duration-offset:opts.duration,'Audio duration');
        if(offset<0 || offset>=buffer.duration || duration<=0 || volume<0)throw new Error('Invalid audio range or volume');
        const source=context.createBufferSource(),gain=context.createGain();source.buffer=buffer;source.loop=!!opts.loop;gain.gain.value=volume;source.connect(gain);gain.connect(context.destination);
        let stopped=false;const handle={stop(){if(stopped)return;stopped=true;try{source.stop();}catch(_){}source.disconnect();gain.disconnect();independent.delete(handle);},get stopped(){return stopped;}};
        source.onended=()=>{stopped=true;source.disconnect();gain.disconnect();independent.delete(handle);};
        try {if(opts.loop)source.start(opts.when==null?context.currentTime:opts.when,offset);
          else source.start(opts.when==null?context.currentTime:opts.when,offset,Math.min(duration,buffer.duration-offset));}
        catch(e){handle.stop();throw e;}return handle;
      },
      play(id,opts){const handle=audio.start(id,opts);independent.add(handle);return handle;},
      pause(){for(const h of [...independent])h.stop();},seek(time){audio.pause();if(time!=null)api.timeline.seek(time);}
    };
    const pointerStarts=new Map();
    function wantsPointerCapture(target) {
      if(options.pointerCapture!=null)return options.pointerCapture===true;
      const dragEvents=['dragstart','drag','dragend'];
      return dragEvents.some(name=>subscriptions.has(name)||subscriptions.has(target+'\u0000'+name)) ||
        (project.interactions||[]).some(b=>b.target===target && dragEvents.includes(b.event)) ||
        (project.stateMachines||[]).some(m=>(m.transitions||[]).some(t=>t.event && dragEvents.includes(t.event.name) && (!t.event.target||t.event.target===target)));
    }
    const eventTypes=['click','dblclick','pointerenter','pointerleave','pointerdown','pointermove','pointerup','pointercancel','lostpointercapture','wheel','keydown','keyup','focus','blur'];
    function coordinates(event) {
      let x=event.clientX || 0,y=event.clientY || 0;
      try {const matrix=svg.getScreenCTM();if(matrix){const p=svg.createSVGPoint();p.x=x;p.y=y;const q=p.matrixTransform(matrix.inverse());x=q.x;y=q.y;}}catch(e){report(e);}
      return {x,y};
    }
    for(const type of eventTypes) {
      const listener=event=>{
        if(dead)return;
        for(let control=event.target;control && control!==svg;control=control.parentNode)if(control.getAttribute && control.getAttribute('data-splinely-host-control')!=null)return;
        if(event.target && owningSvg(event.target) && owningSvg(event.target)!==svg)return;
        let el=event.target;while(el && el!==svg && elements.get(el.id)!==el)el=el.parentNode;
        const target=el && el.id || svg.id,point=coordinates(event),detail={...point,event,target,key:event.key,pointerId:event.pointerId,buttons:event.buttons};
        if(type==='pointermove' || type==='pointerdown' || type==='pointerenter')pointer=[point.x,point.y];
        if(type==='pointerleave' && event.target===svg)pointer=null;
        if(project.followers.length && /^pointer/.test(type)){updateFollowers(0);render();}
        // Keep keyboard handling scoped to focus within this SVG. Focusability remains exporter's choice.
        emit(type,detail,target);
        if(dead)return;
        for(const binding of project.interactions || [])if(binding.target===target && binding.event===type)actions(binding.actions,detail);
        if(dead)return;
        if(type==='pointerdown') {pointerStarts.set(event.pointerId,{...point,target,el});try{if(el && el.setPointerCapture && wantsPointerCapture(target))el.setPointerCapture(event.pointerId);}catch(_){}emit('dragstart',{...detail,dx:0,dy:0},target);}
        if(type==='pointermove' && pointerStarts.has(event.pointerId)){const start=pointerStarts.get(event.pointerId);emit('drag',{...detail,dx:point.x-start.x,dy:point.y-start.y},start.target);}
        if((type==='pointerup'||type==='pointercancel'||type==='lostpointercapture') && pointerStarts.has(event.pointerId)){const start=pointerStarts.get(event.pointerId);pointerStarts.delete(event.pointerId);try{if(start.el.releasePointerCapture)start.el.releasePointerCapture(event.pointerId);}catch(_){}emit('dragend',{...detail,dx:point.x-start.x,dy:point.y-start.y,cancelled:type!=='pointerup'},start.target);}
      };
      svg.addEventListener(type,listener,true);cleanup.push(()=>svg.removeEventListener(type,listener,true));
    }
    function snapshot() {
      // Observational only: don't advance clocks, invoke callbacks or expose handles.
      return {version:1,destroyed:dead,time:transport.time,playing:transport.playing,
        clips:[...clips.values()].map(c=>({id:c.id,time:c.time,duration:c.duration,active:c.active,playing:c.playing,weight:c.weight,fading:!!c.ramp})),
        states:Object.fromEntries([...machines].map(([id,m])=>[id,m.state])),
        audio:{ready:audio.ready,state:context?context.state:'locked',embeddedAssets:project.assets.length},
        reducedMotion,
        diagnostics:diagnostics.map(e=>e.message)};
    }
    api={version:1,snapshot,reportError(error){ensure();return report(error);},diagnostics,on,emit,bones(){ensure();return bonePoses.map(p=>({...p}));},onDestroy(fn){ensure();if(typeof fn!=='function')throw new Error('Disposer must be a function');cleanup.push(fn);},get destroyed(){return dead;}, clip,machine,set,variable,timeline:{at,get time(){return transport.time;},get playing(){return transport.playing;},play(){ensure();syncClock();transport.manual=true;transportPlay();return this;},pause(){ensure();transport.manual=false;for(const c of clips.values())clip(c.id).pause();transportPause();return this;},seek(time){ensure();syncClock();finite(time,'Timeline seek time');if(time<0)throw new Error('Timeline seek must be nonnegative');transport.time=time;transport.revision++;stopSources(transport);for(const c of clips.values())if(c.active)clip(c.id).seek(c.loop?time%c.duration:Math.min(time,c.duration));schedule(transport);return this;}},audio,tick,
      destroy(){if(dead)return;dead=true;if(raf!=null && env.cancelAnimationFrame)env.cancelAnimationFrame(raf);for(const fn of cleanup)try{fn();}catch(error){report(error);}for(const c of [transport,...clips.values()])stopSources(c);audio.pause();loopBuffers.clear();buffers.clear();if(context && context.close)Promise.resolve(context.close()).catch(()=>{});subscriptions.clear();callbacks.clear();for(const [id,start] of pointerStarts)try{if(start.el.releasePointerCapture)start.el.releasePointerCapture(id);}catch(_){}pointerStarts.clear();if(svg.splinely===api)delete svg.splinely;unregister(env,api);}
    };
    svg.splinely=api;register(env,api);
    try {
    processing=true;
    try {for(const m of machines.values())actions(m.states.get(m.state).onEnter || m.states.get(m.state).entry);}
    finally {processing=false;drain();}
    // Preservation-only customJs is expected, not an error. Trusted hosts decide execution separately.
    render();for(const c of project.clips)if(c.autoplay && !(reducedMotion && (c.looping || c.loop)))clip(c.id).play();
    } catch(error) {api.destroy();throw error;}
    if(options.autoTick!==false && !dead){
      const now=()=>env.performance && env.performance.now?env.performance.now():Date.now();last=now();let clock=env;
      const advance=()=>{if(dead)return;const nextClock=audio.ready && context.state==='running'?context:env;
        const current=nextClock===env?now():context.currentTime*1000;
        if(clock!==nextClock){clock=nextClock;last=current;return;}
        const delta=Math.max(0,(current-last)/1000);last=current;try{tick(delta);}catch(e){report(e);}};
      syncClock=()=>{if(!ticking && !processing && !dead)advance();};
      if(env.requestAnimationFrame){const frame=()=>{advance();if(!dead)raf=env.requestAnimationFrame(frame);};raf=env.requestAnimationFrame(frame);}
      // Replenish/audio-clock transport even when rendering is suspended. Native padded loops do not depend on either timer.
      if(env.setInterval){const timer=env.setInterval(()=>{if(audio.ready || [...clips.values()].some(c=>c.ramp))advance();},25);cleanup.push(()=>env.clearInterval(timer));}
    }
    return api;
  }
  const registryKey=typeof Symbol==='function'?Symbol.for('splinely.runtime.instances.v1'):'__splinelyRuntimeInstancesV1';
  function register(env,api){if(!env)return;if(!env[registryKey])env[registryKey]={instances:new Set(),alias:null};const r=env[registryKey];r.instances.add(api);syncAlias(env,r);}
  function unregister(env,api){const r=env && env[registryKey];if(r){r.instances.delete(api);syncAlias(env,r);}}
  function syncAlias(env,r){if(r.alias && env.splinely===r.alias)delete env.splinely;r.alias=null;if(r.instances.size===1 && env.splinely==null){r.alias=[...r.instances][0];env.splinely=r.alias;}}
  const exported={createRuntime,readProject,normalize,evaluateTrack,interpolate,easing};
  if(typeof module==='object' && module.exports)module.exports=exported;
  else {
    if(!host.SplinelyRuntime)host.SplinelyRuntime=exported;
    const script=host.document && host.document.currentScript;
    const svg=script && (script.ownerSVGElement || (script.parentNode && script.parentNode.closest && script.parentNode.closest('svg')));
    if(svg && !svg.splinely)try{createRuntime(svg);}catch(e){if(host.console)host.console.error('Splinely initialization failed; static SVG retained:',e);}
  }
})(typeof window!=='undefined'?window:globalThis);
