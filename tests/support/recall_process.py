"""Recall terminal proofs through a PTY, using only the Python standard library.

Scenarios:
  cancel      SIGINT at the multiline prompt; no output.
  blank       EOF on a whitespace-only submission; usage error.
  switch:ID   at the prompt, another process runs `ask switch ID`; then a
              submission that must succeed.
  expired     a submission that must fail with no current thread.
"""
import os
import pty
import signal
import subprocess
import sys

if not __debug__:
    sys.exit('recall proofs require Python assertions')

NO_CURRENT = b'ask: no current thread; start one with `ask new`\n'

signal.alarm(20)
binary, home, scenario, *args = sys.argv[1:]
os.environ['ASK_HOME'] = home
master, slave = pty.openpty()
child = None
try:
    # SIG_DFL for SIGINT in the child, even if the runner inherited SIG_IGN.
    child = subprocess.Popen([binary, *args], stdin=slave,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             preexec_fn=lambda: signal.signal(signal.SIGINT, signal.SIG_DFL))
    prompt = child.stderr.read(5)
    assert prompt == b'ask> ', prompt + child.stderr.read()
    if scenario == 'cancel':
        os.write(master, b'partial question')
        child.send_signal(signal.SIGINT)
    elif scenario == 'blank':
        os.write(master, b' \n\x04')
    else:
        if scenario.startswith('switch:'):
            switched = subprocess.run([binary, 'switch', scenario.split(':', 1)[1]],
                                      stdin=subprocess.DEVNULL, capture_output=True)
            assert switched.returncode == 0, switched
        os.write(master, b'follow up\n\x04')
    out, err = child.communicate(timeout=10)
    if scenario == 'cancel':
        assert child.returncode == -signal.SIGINT, (child.returncode, err)
        assert not out, out
    elif scenario == 'blank':
        assert child.returncode == 2, (child.returncode, err)
        assert not out, out
    elif scenario == 'expired':
        assert (child.returncode, out, err) == (1, b'', NO_CURRENT), (child.returncode, out, err)
    else:
        assert child.returncode == 0, (child.returncode, err)
        assert out, out
finally:
    os.close(slave)
    os.close(master)
    if child is not None and child.poll() is None:
        child.kill()
        child.wait()
