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
binary, home, scenario, *rest = sys.argv[1:]
base_url = rest[0] if rest else 'http://localhost/v1'
os.environ['ASK_HOME'] = home
os.environ['TERM'] = 'xterm-256color'
# Only the scenario's own credential variable may reach init.
for variable in ('OPENAI_API_KEY', 'ANTHROPIC_API_KEY', 'GEMINI_API_KEY', 'OPENROUTER_API_KEY',
                 'GROQ_API_KEY', 'CEREBRAS_API_KEY', 'XAI_API_KEY', 'KEY'):
    os.environ.pop(variable, None)
environment_key = os.environ.get('LOCAL_API_KEY', '').encode()
PASTED = b'pasted-secret-never-print'
CUSTOM = b'\x1b[B' * 7 + b'\r'
master, slave = pty.openpty()
before = termios.tcgetattr(slave)
child = subprocess.Popen([binary, 'init'], stdin=slave, stdout=subprocess.PIPE,
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


def wait_for_raw(transcript, timeout=5):
    """Waits until the terminal is in raw mode."""
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


def custom_endpoint(transcript, variable=b'LOCAL_API_KEY'):
    """Selects the custom endpoint and answers its three questions."""
    os.write(master, CUSTOM)
    transcript = wait_for_prompt(transcript, b'Provider name')
    os.write(master, b'local\n' + base_url.encode() + b'\n' + variable + b'\n')
    return transcript


def at_raw_prompt(transcript, prompt):
    """Waits for `prompt`, then for the raw mode that reads its keys."""
    return wait_for_raw(wait_for_prompt(transcript, prompt))


def write_after(transcript, prompt, keys):
    transcript = wait_for_prompt(transcript, prompt)
    os.write(master, keys)
    return transcript


def finish_written(transcript):
    """Declines another provider, confirms, and returns the written file."""
    transcript = write_after(transcript, b'Replacement system prompt', b'\n')
    transcript = write_after(transcript, b'Profile name', b'\n')
    transcript = write_after(transcript, b'Add another provider', b'n\n')
    transcript = write_after(transcript, b'Write this configuration', b'y\n')
    out, transcript = finish(transcript)
    assert child.returncode == 0, (child.returncode, transcript)
    assert out == b'', out
    config = open(os.path.join(home, 'config.toml'), 'rb').read()
    for secret in (PASTED, environment_key):
        assert not secret or secret not in transcript + out + config, secret
    return config, transcript


def cancelled(transcript):
    out, transcript = finish(transcript)
    assert child.returncode == 1, (child.returncode, transcript)
    assert out == b'', out
    assert b'configuration cancelled; nothing was written' in transcript, transcript
    assert not os.path.exists(os.path.join(home, 'config.toml'))


try:
    transcript = wait_for_raw(b'')
    if scenario == 'select':
        redraws = int(os.environ.get('ASK_PTY_STRESS_REDRAWS', '0'))
        for _ in range(redraws):
            os.write(master, b'\x1b[B\x1b[A')
            while select.select([master], [], [], 0)[0]:
                transcript += os.read(master, 4096)
        transcript = custom_endpoint(transcript, b'KEY')
        transcript = at_raw_prompt(transcript, b'API key (hidden; Enter skips): ')
        os.write(master, b'\r')
        transcript = write_after(transcript, b'Model identifier', b'm\n')
        config, transcript = finish_written(transcript)
        assert b'kind = "openai-compatible"' in config, config
        assert b'No key entered; skipping the model list' in transcript, transcript
    elif scenario == 'filter':
        assert environment_key
        transcript = custom_endpoint(transcript)
        transcript = at_raw_prompt(transcript, b'Select a model')
        os.write(master, b'OTHERx')
        transcript = wait_for_prompt(transcript, b'Filter: OTHERx')
        os.write(master, b'\x7f\x1b[B\r')
        transcript = wait_for_prompt(transcript, b'Model: other-mini')
        config, transcript = finish_written(transcript)
        assert b'model = "other-mini"' in config, config
        assert b'Using LOCAL_API_KEY from the environment (value not shown).' in transcript
        assert b'Verified: the provider answered a minimal request.' in transcript
    elif scenario == 'hidden':
        assert not environment_key
        transcript = custom_endpoint(transcript)
        transcript = at_raw_prompt(transcript, b'API key (hidden; Enter skips): ')
        os.write(master, PASTED + b'\r')
        transcript = at_raw_prompt(transcript, b'Select a model')
        os.write(master, b'\x1b[A\r')
        transcript = write_after(transcript, b'Model identifier', b'typed-model\n')
        config, transcript = finish_written(transcript)
        assert b'model = "typed-model"' in config, config
        assert b'Verified: the provider answered a minimal request.' in transcript
    elif scenario == 'model-escape':
        assert environment_key
        transcript = custom_endpoint(transcript)
        transcript = at_raw_prompt(transcript, b'Select a model')
        os.write(master, b'\x1b')
        cancelled(transcript)
    elif scenario == 'hidden-escape':
        transcript = custom_endpoint(transcript)
        transcript = at_raw_prompt(transcript, b'API key (hidden; Enter skips): ')
        os.write(master, b'\x1b')
        cancelled(transcript)
    elif scenario == 'escape':
        os.write(master, b'\x1b')
        cancelled(transcript)
    else:
        os.write(master, b'\x03')
        out, transcript = finish(transcript)
        assert child.returncode == -signal.SIGINT, (child.returncode, transcript)
        assert out == b'', out
        assert not os.path.exists(os.path.join(home, 'config.toml'))
    actual = termios.tcgetattr(slave)
    assert terminal_state_restored(before, actual), (before, actual)
finally:
    os.close(slave)
    os.close(master)
    if child.poll() is None:
        child.kill()
        child.wait()
