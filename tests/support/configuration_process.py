"""Deterministic configuration proofs; only the Python standard library is used."""
import os
import pathlib
import pty
import subprocess
import sys
import signal

if not __debug__:
    sys.exit('configuration proofs require Python assertions')

signal.alarm(20)

binary, home, scenario = sys.argv[1:]
home = pathlib.Path(home)
os.environ['ASK_HOME'] = str(home)
candidate = (home / 'candidate.toml').read_bytes()
destination = home / 'config.toml'


def start(*args, **kwargs):
    return subprocess.Popen([binary, *args], stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, **kwargs)


def finish(child, code):
    out, err = child.communicate(timeout=10)
    assert child.returncode == code, (child.returncode, err)
    assert not out, out
    return err


def reading():
    fifo = home / 'source.fifo'
    os.mkfifo(fifo)
    child = start('configure', 'apply', str(fifo))
    # Opening the producer rendezvous proves ask has reached candidate reading.
    producer = open(fifo, 'wb', buffering=0)
    return child, producer


if scenario == 'terminal':
    master, slave = pty.openpty()
    try:
        for action in ('check', 'apply'):
            assert b'usage:' in finish(start('configure', action, stdin=slave), 2)
    finally:
        os.close(slave)
        os.close(master)
elif scenario in ('content', 'identity', 'appeared'):
    if scenario != 'appeared':
        destination.write_bytes(candidate)
    child, producer = reading()
    changed = candidate if scenario == 'identity' else b'external edit\n'
    other = home / 'other'
    other.write_bytes(changed)
    other.replace(destination)
    producer.write(candidate)
    producer.close()
    assert b'changed' in finish(child, 1)
    assert destination.read_bytes() == changed
elif scenario == 'exclusion':
    child, producer = reading()
    assert b'lock' in finish(start('configure', 'apply', '-', stdin=subprocess.DEVNULL), 1)
    answers = b'local\nhttp://localhost/v1\nm\nKEY\n\n\ny\n'
    init = start('init', stdin=subprocess.PIPE)
    init.stdin.write(answers)
    init.stdin.close()
    init.stdin = None
    assert b'lock' in finish(init, 1)
    producer.write(candidate)
    producer.close()
    finish(child, 0)
    assert destination.read_bytes() == candidate
elif scenario == 'init_interaction':
    init = start('init', stdin=subprocess.PIPE)
    # The flushed confirmation prompt is the dialogue rendezvous.
    init.stdin.write(b'local\nhttp://localhost/v1\nm\nKEY\n\n\n')
    init.stdin.flush()
    transcript = b''
    while not transcript.endswith(b'Write this configuration? [y/N]: '):
        byte = init.stderr.read(1)
        assert byte
        transcript += byte
    apply = start('configure', 'apply', str(home / 'candidate.toml'))
    finish(apply, 0)
    init.stdin.write(b'y\n')
    init.stdin.close()
    init.stdin = None
    assert b'already exists' in finish(init, 1)
    assert destination.read_bytes() == candidate
elif scenario == 'utf8':
    for action in ('check', 'apply'):
        destination.write_bytes(candidate)
        source = home / 'bad.toml'
        source.write_bytes(b'\xffSECRET_SENTINEL')
        assert b'SECRET_SENTINEL' not in finish(start('configure', action, str(source)), 1)
        assert destination.read_bytes() == candidate
        child = start('configure', action, '-', stdin=subprocess.PIPE)
        child.stdin.write(b'\xffSECRET_SENTINEL')
        child.stdin.close()
        child.stdin = None
        assert b'SECRET_SENTINEL' not in finish(child, 1)
else:
    raise AssertionError(scenario)
assert not list(home.glob('.ask-config-*.tmp'))
