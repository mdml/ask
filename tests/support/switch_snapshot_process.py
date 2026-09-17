"""Prove interactive switch renders the selected snapshot if current changes."""
import os
import pty
import select
import signal
import sqlite3
import subprocess
import sys
import termios
import time

signal.alarm(20)
binary, home = sys.argv[1:]
database = os.path.join(home, 'data', 'ask.sqlite3')
with sqlite3.connect(database) as connection:
    connection.execute(
        'CREATE TRIGGER redirect_current AFTER UPDATE ON current_thread '
        'BEGIN UPDATE current_thread SET thread_id = 1; END'
    )

os.environ['ASK_HOME'] = home
os.environ['TERM'] = 'xterm-256color'
master, slave = pty.openpty()
child = subprocess.Popen(
    [binary, 'switch'], stdin=slave, stdout=subprocess.PIPE, stderr=slave,
    close_fds=True,
)


def wait_for_raw(transcript, timeout=5):
    deadline = time.monotonic() + timeout
    while termios.tcgetattr(slave)[3] & termios.ICANON:
        returncode = child.poll()
        assert returncode is None, ('waiting for switch raw mode', 'child exited',
                                    returncode, transcript)
        remaining = deadline - time.monotonic()
        assert remaining > 0, ('waiting for switch raw mode', 'timed out', transcript)
        ready, _, _ = select.select([master], [], [], min(0.05, remaining))
        if ready:
            transcript += os.read(master, 4096)
    return transcript


try:
    transcript = wait_for_raw(b'')
    os.write(master, b'\x1b[B\r')
    out, _ = child.communicate(timeout=10)
    assert child.returncode == 0, (child.returncode, transcript)
    assert b'thread 2' in out and b'You:\ntwo' in out, out
    with sqlite3.connect(database) as connection:
        assert connection.execute(
            'SELECT thread_id FROM current_thread'
        ).fetchone() == (1,)
finally:
    os.close(slave)
    os.close(master)
    if child.poll() is None:
        child.kill()
        child.wait()
