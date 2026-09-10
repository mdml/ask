"""Query terminal proofs through a PTY, using only the Python standard library."""
import fcntl
import os
import pty
import signal
import subprocess
import sys
import termios

if not __debug__:
    sys.exit('query proofs require Python assertions')

signal.alarm(20)
binary, home, scenario, *args = sys.argv[1:]
os.environ['ASK_HOME'] = home
master, slave = pty.openpty()


def controlling_terminal():
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)


try:
    child = subprocess.Popen([binary, *args], stdin=slave,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             preexec_fn=controlling_terminal if scenario == 'cancel' else None)
    if scenario != 'words':
        assert child.stderr.read(5) == b'ask> '
        if scenario == 'cancel':
            assert termios.tcgetattr(slave)[3] & termios.ISIG
            os.write(master, b'partial question')
            os.write(master, b'\x03')
        else:
            payload = {'multiline': b'first line\nsecond line\n',
                       'empty': b'', 'whitespace': b' \t\n'}[scenario]
            os.write(master, payload + b'\x04')
    out, err = child.communicate(timeout=10)
    if scenario == 'cancel':
        assert child.returncode == -signal.SIGINT, (child.returncode, err)
        assert not out, out
    elif scenario in ('empty', 'whitespace'):
        assert child.returncode == 2, (child.returncode, err)
        assert not out, out
        assert err.startswith(b'ask: '), err
        assert len(err.splitlines()) == 1, err
    else:
        assert child.returncode == 0, (child.returncode, err)
        assert out == b'**4**\n', out
        assert err.startswith(b'ask: fake-model'), err
        assert len(err.splitlines()) == 1, err
finally:
    os.close(slave)
    os.close(master)
    if child.poll() is None:
        child.kill()
        child.wait()
