"""Build a local, visible animated native/Rust coat comparison from coat_gate JSON.

Usage: coat_view.py replay.json output.html
No network requests, game uploads or external libraries.
"""
import json
import sys
from pathlib import Path

data=json.loads(Path(sys.argv[1]).read_text())
template=Path(__file__).with_name('coat_view.html').read_text(encoding='utf-8')
Path(sys.argv[2]).write_text(template.replace('/*CAPTURE_DATA*/',json.dumps(data)),encoding='utf-8')
print(Path(sys.argv[2]).resolve())
