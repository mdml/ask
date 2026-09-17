"""Interactive init menu proofs through a real PTY, using only the standard library."""
import os
import pty
import select
import signal
import subprocess
import sys
import termios
import time

if not __debug__:
    sys.exit('init menu proofs require Python assertions')

signal.alarm(20)
binary, home, scenario = sys.argv[1:]
os.environ['ASK_HOME'] = home
os.environ['TERM'] = 'xterm-256color'
master, slave = pty.openpty()
before = termios.tcgetattr(slave)
child = subprocess.Popen([binary, 'init'], stdin=slave, stdout=subprocess.PIPE,
                         stderr=slave, close_fds=True,
                         preexec_fn=lambda: signal.signal(signal.SIGINT, signal.SIG_DFL))


def wait_for_raw(transcript, timeout=5):
    deadline = time.monotonic() + timeout
    while termios.tcgetattr(slave)[3] & termios.ICANON:
        returncode = child.poll()
        assert returncode is None, ('waiting for init raw mode', 'child exited',
                                    returncode, transcript)
        remaining = deadline - time.monotonic()
        assert remaining > 0, ('waiting for init raw mode', 'timed out', transcript)
        ready, _, _ = select.select([master], [], [], min(0.05, remaining))
        if ready:
            transcript += os.read(master, 4096)
    return transcript


def wait_for_prompt(transcript, prompt, timeout=5):
    deadline = time.monotonic() + timeout
    while prompt not in transcript:
        returncode = child.poll()
        assert returncode is None, ('waiting for init prompt', 'child exited',
                                    returncode, transcript)
        remaining = deadline - time.monotonic()
        assert remaining > 0, ('waiting for init prompt', 'timed out', transcript)
        ready, _, _ = select.select([master], [], [], min(0.05, remaining))
        if ready:
            transcript += os.read(master, 4096)
    return transcript


def finish(transcript, timeout=10):
    stdout = b''
    deadline = time.monotonic() + timeout
    while child.poll() is None:
        remaining = deadline - time.monotonic()
        assert remaining > 0, ('waiting for init exit', 'timed out', transcript, stdout)
        ready, _, _ = select.select([master, child.stdout], [], [], min(0.05, remaining))
        for source in ready:
            if source == master:
                try:
                    transcript += os.read(master, 4096)
                except OSError:
                    pass
            else:
                stdout += child.stdout.read1(4096)
    stdout += child.stdout.read()
    while select.select([master], [], [], 0)[0]:
        try:
            transcript += os.read(master, 4096)
        except OSError:
            break
    return stdout, transcript


try:
    transcript = wait_for_raw(b'')
    if scenario == 'select':
        redraws = int(os.environ.get('ASK_PTY_STRESS_REDRAWS', '0'))
        for _ in range(redraws):
            os.write(master, b'\x1b[B\x1b[A')
            while select.select([master], [], [], 0)[0]:
                transcript += os.read(master, 4096)
        os.write(master, b'\x1b[B\x1b[B\x1b[B\x1b[B\r')
        transcript = wait_for_prompt(transcript, b'Provider name')
        os.write(master, b'local\nhttp://localhost/v1\nKEY\nm\n\n\ny\n')
        out, transcript = finish(transcript)
        assert child.returncode == 0, (child.returncode, transcript)
        assert out == b'', out
        config = open(os.path.join(home, 'config.toml'), 'rb').read()
        assert b'kind = "openai-compatible"' in config, config
    elif scenario == 'escape':
        os.write(master, b'\x1b')
        out, transcript = finish(transcript)
        assert child.returncode == 1, (child.returncode, transcript)
        assert out == b'', out
        assert not os.path.exists(os.path.join(home, 'config.toml'))
    else:
        os.write(master, b'\x03')
        out, transcript = finish(transcript)
        assert child.returncode == -signal.SIGINT, (child.returncode, transcript)
        assert out == b'', out
        assert not os.path.exists(os.path.join(home, 'config.toml'))
    assert termios.tcgetattr(slave) == before, (before, termios.tcgetattr(slave))
finally:
    os.close(slave)
    os.close(master)
    if child.poll() is None:
        child.kill()
        child.wait()
