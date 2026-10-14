#!/usr/bin/env python3
"""Reproduce an original anime-inspired Forma showcase using the real CLI."""
import base64
import json
import shutil
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / 'target/debug/forma-cli'
OUT = ROOT / 'examples'
SVG = '''<svg xmlns="http://www.w3.org/2000/svg" width="640" height="640" viewBox="0 0 640 640">
<rect id="background" width="640" height="640" fill="#111b32"/>
<circle id="halo" cx="320" cy="295" r="218" fill="#20314b"/>
<circle id="halo-inner" cx="320" cy="295" r="183" fill="#263c55"/>
<path id="star-left" d="M105 190 L110 203 L123 208 L110 213 L105 226 L100 213 L87 208 L100 203 Z" fill="#77ded8"/>
<path id="star-right" d="M528 287 L533 299 L545 304 L533 309 L528 321 L523 309 L511 304 L523 299 Z" fill="#ffdf96"/>
<circle id="sparkle" cx="507" cy="152" r="5" fill="#b5a6ee"/>
<path id="shadow" d="M217 563 C217 539 423 539 423 563 C423 587 217 587 217 563 Z" fill="#101a2d"/>
<path id="hair-back" d="M210 261 C175 145 211 98 305 95 C413 80 470 176 431 289 L451 344 L414 336 L403 365 L377 337 L244 336 L221 359 L219 330 L185 337 Z" fill="#102e39"/>
<path id="hair-back-light" d="M209 219 C198 141 247 108 306 110 C387 94 441 157 431 234 L416 325 L386 306 L240 304 L218 329 Z" fill="#267b83"/>
<path id="legs" d="M268 484 L371 484 L373 551 L337 551 L324 512 L311 551 L273 551 Z" fill="#243451"/>
<path id="shoe-left" d="M272 544 L308 544 L313 567 L259 567 Q252 549 272 544 Z" fill="#eef3ff"/>
<path id="shoe-right" d="M338 544 L373 544 Q394 549 389 567 L333 567 Z" fill="#eef3ff"/>
<path id="shoe-trim" d="M260 563 L313 563 L313 570 L260 570 Z M333 563 L389 563 L389 570 L333 570 Z" fill="#91aaa9"/>
<path id="left-sleeve" d="M259 351 Q228 341 218 369 L204 430 L238 441 L270 390 Z" fill="#9884db"/>
<path id="left-hand" d="M207 425 Q195 433 201 456 Q211 479 226 464 L237 437 Z" fill="#ffd2bc"/>
<path id="hoodie" d="M272 331 Q320 312 369 331 L397 493 Q320 518 244 493 Z" fill="#b4a0f0"/>
<path id="hoodie-shade" d="M368 351 L397 493 Q364 504 342 504 L354 385 Z" fill="#8d7acb"/>
<path id="neck" d="M292 302 L348 302 L348 348 Q321 370 292 348 Z" fill="#f1b79f"/>
<path id="hood" d="M274 329 L295 322 Q320 352 346 322 L370 332 L349 370 L320 352 L293 370 Z" fill="#e3d9ff"/>
<path id="cords" d="M294 371 L299 371 L300 416 L295 416 Z M345 371 L350 371 L345 416 L340 416 Z" fill="#f3eeff"/>
<path id="pocket" d="M282 439 L358 439 L368 469 L273 469 Z" fill="#a18cde"/>
<path id="hoodie-star" d="M320 388 L326 403 L342 404 L330 415 L334 431 L320 423 L306 431 L310 415 L298 404 L314 403 Z" fill="#ffe4a4"/>
<path id="ear-left" d="M231 226 C208 214 211 269 235 275 Z" fill="#f4bea8"/>
<path id="ear-right" d="M409 226 C432 214 429 269 405 275 Z" fill="#f4bea8"/>
<path id="face" d="M229 200 Q230 135 320 138 Q407 139 411 200 L400 277 Q384 326 321 338 Q260 328 242 277 Z" fill="#ffdac7"/>
<path id="face-shade" d="M400 197 L411 200 L400 277 Q384 326 321 338 Q378 311 389 263 Z" fill="#f4c4af"/>
<path id="hair-fringe" d="M215 225 Q204 128 300 115 Q396 102 420 199 L405 238 L383 204 L374 176 L345 226 L323 184 L302 211 L291 165 L264 220 L252 185 L236 239 Z" fill="#2e9399"/>
<path id="hair-shine" d="M237 169 Q258 127 302 128 L284 143 Q257 146 237 180 Z M319 128 Q367 124 394 160 L367 145 L342 144 Z" fill="#65c5c4"/>
<path id="eyebrow-left" d="M257 225 Q277 212 296 225 L294 229 Q275 220 259 229 Z" fill="#173d46"/>
<path id="eyebrow-right" d="M343 225 Q365 212 384 225 L382 229 Q364 220 346 229 Z" fill="#173d46"/>
<path id="eye-white-left" d="M253 247 Q274 225 297 245 Q298 277 276 284 Q253 278 253 247 Z" fill="#fffaf6"/>
<path id="eye-white-right" d="M342 245 Q364 225 386 247 Q387 278 364 284 Q342 277 342 245 Z" fill="#fffaf6"/>
<path id="iris-left" d="M266 241 Q278 235 288 243 L288 268 Q278 289 266 270 Z" fill="#776bc3"/>
<path id="iris-right" d="M351 243 Q363 235 374 241 L374 270 Q362 289 351 268 Z" fill="#776bc3"/>
<path id="iris-shade" d="M266 241 Q278 235 288 243 L288 253 L266 253 Z M351 243 Q363 235 374 241 L374 253 L351 253 Z" fill="#34325f"/>
<path id="pupils" d="M274 246 L281 246 L281 268 L274 268 Z M359 246 L366 246 L366 268 L359 268 Z" fill="#242640"/>
<path id="lashes" d="M249 242 Q270 224 297 240 L298 247 Q276 231 254 249 L245 243 Z M342 240 Q370 224 390 242 L395 243 L386 249 Q364 231 341 247 Z" fill="#172e3f"/>
<circle id="eye-glint-left" cx="274" cy="248" r="5" fill="#ffffff"/>
<circle id="eye-glint-right" cx="360" cy="248" r="5" fill="#ffffff"/>
<circle id="eye-glint-small-left" cx="284" cy="268" r="2.5" fill="#bfe7ff"/>
<circle id="eye-glint-small-right" cx="370" cy="268" r="2.5" fill="#bfe7ff"/>
<path id="blush" d="M246 281 Q254 273 267 281 Q260 290 246 286 Z M375 281 Q387 273 396 281 L394 286 Q381 290 375 281 Z" fill="#f5a7ac"/>
<path id="nose" d="M318 273 L323 281 L317 282 Z" fill="#e6a98f"/>
<path id="smile" d="M301 298 Q320 306 338 298 Q331 319 320 319 Q309 319 301 298 Z" fill="#944d69"/>
<path id="smile-light" d="M311 310 Q320 305 329 310 L326 316 L314 316 Z" fill="#f6a5b8"/>
<path id="hair-pin" d="M390 176 L396 189 L410 190 L399 200 L402 214 L390 207 L378 214 L381 200 L370 190 L384 189 Z" fill="#ffe4a4"/>
<path id="wave-sleeve" d="M-37 16 Q-55 -7 -38 -31 L-11 -59 L12 -42 L-5 8 Z" fill="#b4a0f0"/>
<path id="wave-cuff" d="M-17 -55 L-3 -73 L19 -56 L12 -41 Z" fill="#e3d9ff"/>
<path id="wave-hand" d="M-2 -68 L-7 -92 Q-8 -103 -1 -102 L7 -83 L9 -111 Q11 -120 17 -115 L20 -85 L26 -109 Q31 -118 35 -110 L32 -81 L41 -98 Q48 -104 49 -94 L42 -70 Q53 -73 53 -64 Q43 -45 21 -50 Z" fill="#ffdac7"/>
</svg>'''

def cli(*args):
    subprocess.run([str(CLI), *map(str, args)], cwd=ROOT, check=True)

def key(time, value):
    return {'time': time, 'value': {'type': 'number', 'value': value}, 'easing': {'type': 'linear'}}

OUT.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix='forma-anime-') as temp:
    temp = Path(temp)
    source = temp / 'source.svg'
    source.write_text(SVG)
    imported = temp / 'imported.json'
    cli('import-svg', source, '--output', imported)
    project = json.loads(imported.read_text())
    for node in project['nodes']:
        if node['id'].startswith('wave-'):
            node['boneId'] = 'wave-joint'
    engine = shutil.which('espeak-ng') or shutil.which('espeak')
    if not engine:
        raise SystemExit('Install espeak-ng to reproduce the embedded offline greeting; no cloud fallback is used.')
    wav = temp / 'hallo.wav'
    subprocess.run([engine, '-v', 'de', '-s', '145', '-p', '60', '-w', str(wav), 'Hallo!'], check=True, timeout=20)
    patch = {
        'name': 'Forma · Aoi sagt Hallo', 'nodes': project['nodes'],
        'bones': [{'id': 'wave-joint', 'parentId': None, 'x': 424, 'y': 363, 'rotation': 0, 'length': 85}],
        'clips': [{'id': 'greeting', 'name': 'Hallo – winken', 'duration': 3.2, 'looping': False, 'autoplay': True,
                   'tracks': [{'nodeId': 'wave-joint', 'property': 'rotation',
                               'keyframes': [key(t, v) for t, v in [(0, -12), (.4, 14), (.8, -16), (1.2, 14), (1.6, -16), (2, 12), (2.4, -8), (3.2, 0)]]}]}],
        'speechCues': [],
        'audioAssets': [{'id': 'hello-offline', 'name': 'Hallo! · lokale deutsche Stimme', 'mimeType': 'audio/wav', 'dataBase64': base64.b64encode(wav.read_bytes()).decode('ascii')}],
        'audioCues': [{'id': 'hello', 'assetId': 'hello-offline', 'time': .18, 'offset': 0, 'duration': None, 'volume': 1}],
    }
    patch_file = temp / 'patch.json'
    patch_file.write_text(json.dumps(patch, ensure_ascii=False))
    authored = OUT / 'anime-hello.forma.json'
    cli('apply', imported, '--patch', patch_file, '--output', authored)
    cli('validate', authored)
    cli('export', authored, '--output', OUT / 'anime-hello.svg')
print('Created examples/anime-hello.forma.json and examples/anime-hello.svg')
