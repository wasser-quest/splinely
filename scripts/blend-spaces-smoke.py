#!/usr/bin/env python3
"""Real Firefox check of the CLI's offline export and shared Rust/Node fixture."""
import json
from pathlib import Path
import subprocess
import tempfile
from firefox_harness import Firefox

ROOT = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="splinely-blending-") as directory:
    svg = Path(directory, "blending.svg")
    subprocess.run([str(ROOT / "target/debug/splinely-cli"), "export",
                    str(ROOT / "runtime/tests/fixtures/blend-spaces.json"),
                    "--output", str(svg)], check=True, capture_output=True)
    with Firefox() as browser:
        browser.command("WebDriver:Navigate", {"url": svg.as_uri()})
        result = browser.js(r"""
          const root=document.documentElement, f=root.splinely, n=document.getElementById('n');
          if(!f)throw new Error('export did not auto-mount');
          f.timeline.pause();
          const pose=()=>n.getAttribute('transform').match(/[-+]?\d*\.?\d+(?:e[-+]?\d+)?/gi).map(Number);
          const near=(a,b)=>{if(Math.abs(Number(a)-b)>1e-9)throw new Error(a+' != '+b);};
          near(pose()[0],42);near(pose()[1],32);near(pose()[2],15);near(pose()[3],1.25);
          near(n.getAttribute('fill-opacity'),.75);near(f.bones()[0].x,5);
          f.variable('speed').set(0);near(pose()[0],32);
          f.variable('speed').set(2);near(pose()[0],52);
          f.variable('speed').set(1);f.clip('offset').fadeTo(0,1);f.tick(.5);
          near(pose()[0],41);near(f.bones()[0].x,4);
          f.tick(.5);near(pose()[0],40);
          let rejected=false;try{f.variable('speed').set(null);}catch(e){rejected=true;}
          if(!rejected)throw new Error('invalid parameter accepted');
          f.set('n',{x:99});near(pose()[0],99);f.set('n',null);near(pose()[0],40);
          if(f.diagnostics.length)throw new Error(JSON.stringify(f.diagnostics));
          return {pass:true,x:pose()[0],fill:n.getAttribute('fill'),diagnostics:f.diagnostics.length};
        """)
        print(json.dumps(result, sort_keys=True))
