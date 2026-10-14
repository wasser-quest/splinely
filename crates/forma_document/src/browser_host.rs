//! Explicit browser host capabilities, kept separate from safe document data.
use crate::{DocumentError, Project};

#[derive(Clone, Copy, Debug, Default)]
pub struct BrowserExportOptions {
    /// Arbitrary code is not sandboxed. Only enable for reviewed projects.
    pub trusted_js: bool,
    /// Browser speech voices and offline availability are system-dependent.
    pub speech: bool,
    /// Add a user-gesture start control for embedded audio or enabled speech.
    pub audio_controls: bool,
}

impl Project {
    pub fn to_svg_with_options(
        &self,
        runtime: &str,
        options: BrowserExportOptions,
    ) -> Result<String, DocumentError> {
        let mut script = runtime.to_owned();
        script.push_str(&format!(
            "\n;(function(){{\nconst speech={}, controls={};\n",
            options.speech, options.audio_controls
        ));
        script.push_str(r#"
const svg=document.currentScript && document.currentScript.ownerSVGElement;
if(!svg || !svg.forma)return;
const forma=svg.forma;
const data=JSON.parse(svg.querySelector('[id="forma-project"]').textContent);
if(speech && forma.speech.enable)forma.speech.enable();
const report=error=>{if(forma.destroyed)return;const e=new Error('Forma host: '+String(error));if(forma.reportError)forma.reportError(e);else{forma.diagnostics.push(e);if(forma.diagnostics.length>100)forma.diagnostics.shift();forma.emit('error',{error:e});}console.error('Forma host:',error);};
if(controls && ((data.audioAssets || []).length || speech)) {
  const resume=(data.clips || []).filter(c=>forma.clip(c.id).playing).map(c=>c.id);
  forma.timeline.pause();forma.timeline.seek(0);
  const ns='http://www.w3.org/2000/svg', button=document.createElementNS(ns,'g');
  button.setAttribute('data-forma-host-control','');button.setAttribute('role','button');button.setAttribute('tabindex','0');
  button.setAttribute('aria-label','Start Forma audio and animation');button.style.cursor='pointer';
  const rect=document.createElementNS(ns,'rect');rect.setAttribute('x','8');rect.setAttribute('y','8');
  rect.setAttribute('width','176');rect.setAttribute('height','36');rect.setAttribute('rx','6');rect.setAttribute('fill','#202a44');
  const text=document.createElementNS(ns,'text');text.setAttribute('x','20');text.setAttribute('y','32');
  text.setAttribute('fill','white');text.setAttribute('font-size','16');text.style.pointerEvents='none';text.textContent='Start audio / animation';
  button.append(rect,text);svg.appendChild(button);
  let pending=false;
  const start=async event=>{
    if(event.type==='keydown' && event.key!=='Enter' && event.key!==' ')return;
    event.preventDefault();event.stopPropagation();if(pending)return;pending=true;
    let unlockingAudio=false;
    try {
      if((data.audioAssets || []).length){unlockingAudio=true;await forma.audio.unlock();unlockingAudio=false;}
      if(forma.destroyed)return;
      forma.timeline.seek(0);for(const id of resume)forma.clip(id).play();forma.timeline.play();button.remove();
    }catch(error){pending=false;text.textContent='Retry audio start';if(!unlockingAudio)report(error);}
  };
  button.addEventListener('click',start);button.addEventListener('keydown',start);
  forma.onDestroy(()=>{button.removeEventListener('click',start);button.removeEventListener('keydown',start);button.remove();});
}
"#);
        if options.trusted_js {
            script.push_str(
                r#"
if(data.customJs) {
  try {new Function('forma','"use strict";\n'+data.customJs)(forma);}
  catch(error){report(error);}
}
"#,
            );
        }
        script.push_str("\n})();\n");
        self.to_svg(&script)
    }
}
