#!/usr/bin/env python3
"""Real Firefox host error-channel check; audio failure is deliberately mocked."""
import json
from pathlib import Path
import subprocess
import tempfile
import time
from firefox_harness import Firefox

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / 'target/debug/splinely-cli'
subprocess.run(['cargo', 'build', '-p', 'splinely-cli'], cwd=ROOT, check=True)
with tempfile.TemporaryDirectory(prefix='splinely-host-errors-') as directory:
    folder = Path(directory)
    project = json.loads((ROOT/'examples/demo.splinely.json').read_text())
    project['customJs'] = "splinely.on('error', ({error}) => {window.hostError={isError:error instanceof Error,message:error.message};}); throw new Error('authored failure');"
    source, svg = folder/'project.json', folder/'project.svg'
    source.write_text(json.dumps(project))
    subprocess.run([str(CLI), 'export', str(source), '--trusted-js', '--output', str(svg)], check=True)
    with Firefox() as browser:
        browser.command('WebDriver:Navigate', {'url':svg.as_uri()})
        observed = browser.js("return {payload:window.hostError,status:document.documentElement.splinely.snapshot()};")
        assert observed['payload']['isError']
        assert 'authored failure' in observed['payload']['message']
        assert len([x for x in observed['status']['diagnostics'] if 'authored failure' in x]) == 1

        project['customJs'] = "window.AudioContext=class {constructor(){this.state='suspended';}resume(){return Promise.reject(new Error('mock unlock failure'));}};"
        source.write_text(json.dumps(project))
        subprocess.run([str(CLI), 'export', str(source), '--trusted-js', '--output', str(svg)], check=True)
        browser.command('WebDriver:Navigate', {'url':svg.as_uri()+'?second'})
        browser.command('WebDriver:PerformActions', {'actions':[{'type':'pointer','id':'mouse','parameters':{'pointerType':'mouse'},'actions':[
            {'type':'pointerMove','duration':0,'origin':'viewport','x':35,'y':25},
            {'type':'pointerDown','button':0}, {'type':'pointerUp','button':0}]}]})
        time.sleep(.15)
        observed = browser.js("return {status:document.documentElement.splinely.snapshot(),text:document.querySelector('[data-splinely-host-control] text').textContent};")
        assert observed['text'] == 'Retry audio start', observed
        assert len([x for x in observed['status']['diagnostics'] if 'mock unlock failure' in x]) == 1, observed
        print('PASS: real Firefox Error payload and single audio-failure diagnostic (mocked failure, no audibility claim)')
