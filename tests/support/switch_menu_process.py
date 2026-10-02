"""Interactive switch proofs through a real PTY, using only the standard library."""
import os
import pty
import re
import select
import signal
import subprocess
import sys
import termios
import time

if not __debug__:
    sys.exit('switch menu proofs require Python assertions')

signal.alarm(20)
binary, home, scenario = sys.argv[1:]
os.environ['ASK_HOME'] = home
# No proof contacts the project's published model list.
os.environ['ASK_MODEL_LIST_URL'] = ''
os.environ['TERM'] = 'dumb' if scenario == 'dumb' else 'xterm-256color'
master, slave = pty.openpty()
termios.tcsetwinsize(slave, (6, 24) if scenario == 'viewport' else
                    (2, 24) if scenario == 'tiny' else (24, 24))
before = termios.tcgetattr(slave)
child = subprocess.Popen([binary, 'switch'], stdin=slave, stdout=subprocess.PIPE,
                         stderr=slave, close_fds=True,
                         preexec_fn=lambda: signal.signal(signal.SIGINT, signal.SIG_DFL))


def terminal_state_restored(before, actual):
    if actual == before:
        return True
    if sys.platform != 'darwin':
        return False
    expected = before.copy()
    expected[3] |= termios.PENDIN
    return actual == expected


def wait_for(predicate, transcript, description, timeout=5):
    deadline = time.monotonic() + timeout
    while not predicate(transcript):
        remaining = deadline - time.monotonic()
        assert remaining > 0, (description, 'timed out', transcript)
        ready, _, _ = select.select([master], [], [], min(0.05, remaining))
        if ready:
            transcript += os.read(master, 4096)
        returncode = child.poll()
        assert predicate(transcript) or returncode is None, (
            description, "child exited", returncode, transcript)
    return transcript


def raw_mode(_transcript):
    return not termios.tcgetattr(slave)[3] & termios.ICANON


def sampled(transcript, chunks, span=0.25):
    """Writes `chunks` spread over `span` seconds, checking the terminal
    attributes between writes and output reads: while the menu waits, the
    terminal never echoes input or buffers lines."""
    start = time.monotonic()
    pending = list(chunks)
    samples = 0
    while pending or time.monotonic() - start < span or samples < 50:
        local = termios.tcgetattr(slave)[3]
        assert not local & (termios.ECHO | termios.ICANON), (samples, local, transcript)
        assert child.poll() is None, ('sampling attributes', 'child exited', transcript)
        samples += 1
        written = len(chunks) - len(pending)
        if pending and written < (time.monotonic() - start) / span * len(chunks):
            os.write(master, pending.pop(0))
        if select.select([master], [], [], 0.001)[0]:
            transcript += os.read(master, 4096)
    return transcript


def finish(transcript, timeout=10):
    stdout = b''
    deadline = time.monotonic() + timeout
    while child.poll() is None:
        remaining = deadline - time.monotonic()
        assert remaining > 0, ('waiting for switch exit', 'timed out', transcript, stdout)
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
    transcript = b''
    if scenario == 'dumb':
        transcript = wait_for(lambda output: b'select a thread' in output, transcript,
                              'waiting for dumb terminal prompt')
        os.write(master, b'2\n')
        out, transcript = finish(transcript)
        assert child.returncode == 0, (child.returncode, transcript)
        assert out == b'', out
        actual = termios.tcgetattr(slave)
        assert terminal_state_restored(before, actual), (before, actual)
        sys.exit(0)
    if scenario == 'tiny':
        transcript = wait_for(
            lambda output: b'interactive selection requires terminal height' in output,
            transcript, 'waiting for terminal height diagnostic')
        out, transcript = finish(transcript)
        assert child.returncode == 1, (child.returncode, transcript, out)
        assert b'interactive selection requires terminal height of at least 3 rows' in transcript, transcript
        actual = termios.tcgetattr(slave)
        assert terminal_state_restored(before, actual), (before, actual)
        sys.exit(0)
    transcript = wait_for(raw_mode, transcript, 'waiting for switch raw mode')
    if scenario == 'viewport':
        while True:
            ready, _, _ = select.select([master], [], [], 0.05)
            if not ready:
                break
            transcript += os.read(master, 4096)
        assert len(re.findall(rb'[> ] \d+\. thread ', transcript)) <= 4, transcript
        os.write(master, b'\x1b[A')
        redraw = wait_for(lambda output: b'> 10. thread 1 ' in output, b'',
                          'waiting for wrapped viewport selection')
        assert b'> 10. thread 1 ' in redraw, redraw
        assert len(re.findall(rb'[> ] \d+\. thread ', redraw)) <= 4, redraw
        rendered = re.sub(rb'\x1b\[[0-9;?]*[ -/]*[@-~]', b'', transcript + redraw)
        for line in rendered.splitlines():
            if b'thread ' in line or b'arrow keys' in line:
                width = sum(1 if ord(character) < 128 else 2
                            for character in line.decode())
                assert width <= 23, (width, line, rendered)
        os.write(master, b'\r')
        out, transcript = finish(transcript)
        assert child.returncode == 0, (child.returncode, transcript)
        assert b'thread 1' in out, out
    elif scenario in ('select', 'held'):
        if scenario == 'held':
            transcript = sampled(transcript, [b'\x1b[B', b'\x1b[A'] * 4)
        os.write(master, b'\x1b[B\r')
        out, transcript = finish(transcript)
        assert child.returncode == 0, (child.returncode, transcript)
        assert b'thread 2' in out and b'You:\n' in out and b'Assistant:\n' in out, out
    elif scenario == 'escape':
        os.write(master, b'\x1b')
        out, transcript = finish(transcript)
        assert child.returncode == 1, (child.returncode, transcript)
        assert out == b'', out
    else:
        os.write(master, b'\x03')
        out, transcript = finish(transcript)
        assert child.returncode == -signal.SIGINT, (child.returncode, transcript)
        assert out == b'', out
    actual = termios.tcgetattr(slave)
    assert terminal_state_restored(before, actual), (before, actual)
finally:
    os.close(slave)
    os.close(master)
    if child.poll() is None:
        child.kill()
        child.wait()
