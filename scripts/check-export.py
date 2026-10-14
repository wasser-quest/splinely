#!/usr/bin/env python3
"""Structural offline check, not a browser/security certification."""
import base64
import io
import json
from pathlib import Path
import re
import sys
import wave
import xml.etree.ElementTree as ET

project_path, svg_path = map(Path, sys.argv[1:])
project = json.loads(project_path.read_text(encoding='utf-8'))
root = ET.parse(svg_path).getroot()
ns = '{http://www.w3.org/2000/svg}'
assert root.tag == ns + 'svg'
assert root.attrib['data-forma-version'] == '1'
metadata = root.find(f"{ns}metadata[@id='forma-project']")
assert metadata is not None
assert json.loads(metadata.text) == project, 'Project metadata changed'
scripts = root.findall(ns + 'script')
assert scripts and any('createRuntime' in (s.text or '') for s in scripts)
for element in root.iter():
    for key, value in element.attrib.items():
        if key.split('}')[-1] in ('href', 'src'):
            assert value.startswith(('#', 'data:')), f'External resource: {value}'
        assert not re.search(r'url\(\s*[\"\']?(?!#|data:)', value), 'External CSS URL'
# Catch network-loading primitives in this particular bundled example.
source = '\n'.join(s.text or '' for s in scripts) + project.get('customJs', '')
assert not re.search(r'\b(?:fetch|XMLHttpRequest|WebSocket|EventSource|importScripts)\s*\(', source)
# createElementNS uses this identifier; it does not request a network resource.
assert not re.search(r'https?://', source.replace('http://www.w3.org/2000/svg', ''))
assert {n['id'] for n in project['nodes']} <= {e.get('id') for e in root.iter()}
assert len(project['audioAssets']) == 1
asset = project['audioAssets'][0]
assert asset['mimeType'] == 'audio/wav'
with wave.open(io.BytesIO(base64.b64decode(asset['dataBase64'], validate=True)), 'rb') as wav:
    assert (wav.getnchannels(), wav.getsampwidth(), wav.getframerate(), wav.getnframes()) == (1, 2, 8000, 3200)
    assert len(wav.readframes(3200)) == 6400
assert project['audioCues'][0]['time'] == 1.5
assert project['audioCues'][0]['assetId'] == asset['id']
assert not project['speechCues'], 'Tone demo must not claim speech'
print(f'PASS: XML, project roundtrip, embedded 0.4 s WAV, no external resource references: {svg_path}')
