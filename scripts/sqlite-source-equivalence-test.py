#!/usr/bin/env python3
"""Regression for optimized verification; supply the documented downloaded inputs."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

script = Path(__file__).with_name('sqlite-source-equivalence.py').resolve()
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
print('PASS: altered archive rejected without output at optimization levels 0, 1 and 2')
