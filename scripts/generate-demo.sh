#!/usr/bin/env bash
# Rebuild the example through the real CLI; no third-party Python packages.
set -euo pipefail
cd "$(dirname "$0")/.."
output=${1:-examples}
mkdir -p "$output"
output=$(cd "$output" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cargo run --quiet -p forma-cli -- new --demo --output "$work/base.forma.json"
python3 - "$work/base.forma.json" "$work/patch.json" <<'PY'
import base64, io, json, math, struct, sys, wave
project = json.load(open(sys.argv[1], encoding='utf-8'))
# A small synthetic, syllable-shaped tone, NOT recorded or synthesized words.
rate = 8000
samples = []
for i in range(3200):
    t = i / rate
    pulse = t % 0.2
    envelope = math.sin(math.pi * pulse / 0.16) ** 2 if pulse < 0.16 else 0
    sound = math.sin(2 * math.pi * 220 * t) + 0.3 * math.sin(2 * math.pi * 660 * t)
    samples.append(struct.pack('<h', round(6500 * envelope * sound)))
buffer = io.BytesIO()
with wave.open(buffer, 'wb') as wav:
    wav.setnchannels(1)
    wav.setsampwidth(2)
    wav.setframerate(rate)
    wav.writeframes(b''.join(samples))
clip = project['clips'][0]
clip.update(name='Klick-Bewegung', duration=3.0, looping=False, autoplay=False)
keys = clip['tracks'][0]['keyframes']
keys[1]['time'] = 1.5
keys[2]['time'] = 3.0
project['nodes'][2]['kind']['text'] = 'Kreis klicken: Bewegung + Ton bei 1,5 s'
transition = project['stateMachines'][0]['transitions'][0]
transition['actions'].append({'type': 'playClip', 'clipId': 'travel'})
patch = {
    'name': 'Forma: Klick, Keyframes und eingebetteter Ton',
    'nodes': project['nodes'], 'clips': [clip],
    'stateMachines': project['stateMachines'],
    'audioAssets': [{'id': 'tone', 'name': 'Silbenartiger Ton (keine Worte)',
                     'mimeType': 'audio/wav',
                     'dataBase64': base64.b64encode(buffer.getvalue()).decode('ascii')}],
    'audioCues': [{'id': 'tone-cue', 'assetId': 'tone', 'time': 1.5,
                   'offset': 0.0, 'duration': None, 'volume': 0.6}],
    'customJs': """// Trusted example only: unlock embedded audio on a real user gesture.
forma.on('ball', 'click', async () => {
  try {
    await forma.audio.unlock();
    forma.clip('travel').stop();
    forma.timeline.seek(0);
    forma.clip('travel').play();
  } catch (error) {
    console.error('Forma demo audio failed', error);
  }
});
"""
}
with open(sys.argv[2], 'w', encoding='utf-8') as out:
    json.dump(patch, out, ensure_ascii=False, indent=2)
PY
cargo run --quiet -p forma-cli -- apply "$work/base.forma.json" --patch "$work/patch.json" --output "$output/demo.forma.json"
cargo run --quiet -p forma-cli -- validate "$output/demo.forma.json"
# Explicit opt-in for the short, reviewable gesture handler above.
cargo run --quiet -p forma-cli -- export "$output/demo.forma.json" --trusted-js --output "$output/demo.svg"
