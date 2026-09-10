"""Linux syscall-boundary proof of real publication errors, without product hooks.

Pause the child at link/rename entry, deny directory writes for that syscall,
then restore permissions at exit so the product can clean up its temporary file.
"""
import ctypes
import os
import pathlib
import platform
import signal

signal.alarm(20)
import sys

if not __debug__:
    sys.exit('configuration proofs require Python assertions')

binary, home, mode = sys.argv[1:]
home = pathlib.Path(home)
os.environ['ASK_HOME'] = str(home)
source = home / 'candidate.toml'
destination = home / 'config.toml'
original = b'original\n'
if mode == 'replace':
    destination.write_bytes(original)
libc = ctypes.CDLL(None, use_errno=True)
libc.ptrace.restype = ctypes.c_long


def trace(request, pid, address=0, data=0):
    result = libc.ptrace(request, pid, ctypes.c_void_p(address), ctypes.c_void_p(data))
    if result == -1:
        raise OSError(ctypes.get_errno(), 'ptrace')
    return result


out_read, out_write = os.pipe()
err_read, err_write = os.pipe()
pid = os.fork()
if pid == 0:
    os.close(out_read)
    os.close(err_read)
    os.dup2(out_write, 1)
    os.dup2(err_write, 2)
    trace(0, 0)  # PTRACE_TRACEME
    os.execv(binary, [binary, 'configure', 'apply', str(source)])
os.close(out_write)
os.close(err_write)
_, status = os.waitpid(pid, 0)  # exec trap
assert os.WIFSTOPPED(status), os.read(err_read, 65536)
trace(0x4200, pid, 0, 0x100001)  # PTRACE_SETOPTIONS / TRACESYSGOOD
calls = {'x86_64': {82, 86, 264, 265, 316}, 'aarch64': {37, 38, 276}}
publication_calls = calls[platform.machine()]
denied = False
observed = False
try:
    while True:
        trace(24, pid)  # PTRACE_SYSCALL
        _, status = os.waitpid(pid, 0)
        if os.WIFEXITED(status):
            assert os.WEXITSTATUS(status) == 1, status
            break
        assert os.WIFSTOPPED(status), status
        assert os.WSTOPSIG(status) == signal.SIGTRAP | 0x80, status
        info = ctypes.create_string_buffer(88)
        trace(0x420e, pid, len(info), ctypes.addressof(info))  # GET_SYSCALL_INFO
        operation = info.raw[0]
        number = int.from_bytes(info.raw[24:32], sys.byteorder)
        if operation == 1 and number in publication_calls:
            assert list(home.glob('.ask-config-*.tmp'))
            if mode == 'appeared':
                destination.write_bytes(original)
            else:
                home.chmod(0o500)
                denied = True
            observed = True
        elif operation == 2 and denied:
            home.chmod(0o700)
            denied = False
finally:
    home.chmod(0o700)
assert observed, 'publication syscall was not reached'
assert not os.read(out_read, 65536)
message = os.read(err_read, 65536)
assert b'cannot write' in message or b'already exists' in message
assert not list(home.glob('.ask-config-*.tmp'))
if mode in ('replace', 'appeared'):
    assert destination.read_bytes() == original
else:
    assert not destination.exists()
