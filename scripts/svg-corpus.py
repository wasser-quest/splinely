#!/usr/bin/env python3
"""Real installed SVG corpus, no copied artwork. Requires Mint-X/Adwaita packages.
Missing fixtures fail explicitly. Records SHA256/provenance, tests all-or-nothing CLI import.
Run: python3 scripts/svg-corpus.py [output-directory] [forma-cli-path]
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

out = Path(sys.argv[1] if len(sys.argv) > 1 else '/tmp/forma-svg-corpus')
cli = str(Path(sys.argv[2] if len(sys.argv) > 2 else 'target/debug/forma-cli').resolve())
out.mkdir(parents=True, exist_ok=True)
cases = [
 ('/usr/share/icons/Mint-X/status/scalable/network-cellular-signal-good-symbolic.svg', True, 'Mint-X GPL-3+, linuxmint/mint-x-icons'),
 ('/usr/share/icons/Mint-X/actions/scalable/window-minimize-symbolic.svg', True, 'Mint-X GPL-3+, linuxmint/mint-x-icons'),
 ('/usr/share/icons/Mint-X/actions/scalable/open-menu-symbolic.svg', True, 'Mint-X GPL-3+, linuxmint/mint-x-icons'),
 ('/usr/share/icons/Adwaita/symbolic/categories/applications-utilities-symbolic.svg', True, 'GNOME Project; LGPLv3 or CC-BY-SA-3.0-US'),
 ('/usr/share/icons/Adwaita/scalable/devices/audio-headphones.svg', False, 'GNOME Project; LGPLv3 or CC-BY-SA-3.0-US'),
 ('/usr/share/icons/Mint-X/actions/96/address-book-new.svg', False, 'Mint-X GPL-3+, linuxmint/mint-x-icons'),
 ('/usr/share/icons/Mint-Y/actions/symbolic/system-reboot-symbolic.svg', False, 'Mint-Y icon-files CC-BY-SA-4; linuxmint/mint-y-icons'),
 ('/usr/share/icons/Mint-X/apps/96/tomahawk.svg', False, 'Mint-X GPL-3+, linuxmint/mint-x-icons'),
]
results = []
for i, (source, expected, license_note) in enumerate(cases):
 p = Path(source)
 assert p.is_file(), f'Missing corpus fixture: {source}'
 data = p.read_bytes()
 project = out / f'{i}.json'
 project.write_text('unchanged sentinel')
 run = subprocess.run([cli, 'import-svg', source, '--output', str(project)], text=True, capture_output=True)
 success = run.returncode == 0
 assert success == expected, (source, expected, run.stderr)
 record = {'source': source, 'resolvedPath': str(p.resolve()), 'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data), 'license': license_note, 'imported': success}
 if success:
  document = json.loads(project.read_text())
  assert document['nodes'] and all(n['kind']['type'] == 'path' for n in document['nodes'])
  if i == 0:
   assert len(document['nodes']) == 4 and document['width'] == 16 and document['height'] == 16
  subprocess.run([cli, 'export', str(project), '--output', str(out / f'{i}.svg')], check=True, capture_output=True)
  record['nodes'] = len(document['nodes'])
  record['export'] = str(out / f'{i}.svg')
 else:
  assert project.read_text() == 'unchanged sentinel', 'Rejected import overwrote destination'
  record['error'] = json.loads(run.stderr)['error']['message']
 results.append(record)
(out / 'manifest.json').write_text(json.dumps(results, indent=2))
print(f'PASS: {len(results)} real SVG fixtures; {sum(r["imported"] for r in results)} imported, remainder rejected without overwrite; manifest {out / "manifest.json"}')
