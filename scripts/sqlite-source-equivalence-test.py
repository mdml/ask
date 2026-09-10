#!/usr/bin/env python3
"""Regression for optimized verification; supply the documented downloaded inputs."""
import os
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

script = Path(__file__).with_name('sqlite-source-equivalence.py').resolve()
valid_output = None
for optimization in ['0', '1', '2']:
    env = dict(os.environ, PYTHONOPTIMIZE=optimization)
    result = subprocess.run([sys.executable, str(script), sys.argv[1]],
                            env=env, capture_output=True, timeout=30)
    if result.returncode or result.stderr or not result.stdout:
        sys.exit('FAIL: valid inputs rejected with optimization=' + optimization)
    evidence = json.loads(result.stdout)
    if valid_output is not None and evidence != valid_output:
        sys.exit('FAIL: optimization changed valid evidence')
    valid_output = evidence
with tempfile.TemporaryDirectory() as temporary:
    inputs = Path(temporary) / 'inputs'
    shutil.copytree(sys.argv[1], inputs)
    archive = inputs / 'sqlite-rusqlite.crate'
    contents = bytearray(archive.read_bytes())
    contents[-1] ^= 1
    archive.write_bytes(contents)
    for optimization in ['0', '1', '2']:
        env = dict(os.environ, PYTHONOPTIMIZE=optimization)
        result = subprocess.run([sys.executable, str(script), str(inputs)],
                                env=env, capture_output=True, timeout=30)
        if result.returncode == 0 or result.stdout:
            sys.exit('FAIL: altered archive accepted with optimization=' + optimization)
        if result.stderr != b'SQLite source correspondence failed; raw diagnostics withheld for hygiene\n':
            sys.exit('FAIL: unexpected diagnostic')
print('PASS: valid inputs accepted identically and altered archive rejected at optimization levels 0, 1 and 2')
