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
# The published model list is disabled unless the scenario supplies a fake's URL.
os.environ['ASK_MODEL_LIST_URL'] = rest[1] if len(rest) > 1 else ''
# Proxy settings never reach ask, so requests to a loopback fake go direct.
for variable in ('HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'NO_PROXY'):
    os.environ.pop(variable, None)
    os.environ.pop(variable.lower(), None)
os.environ['NO_PROXY'] = '127.0.0.1,localhost'
os.environ['TERM'] = 'xterm-256color'
# Only the scenario's own credential variable may reach init.
for variable in ('OPENAI_API_KEY', 'ANTHROPIC_API_KEY', 'GEMINI_API_KEY', 'OPENROUTER_API_KEY',
                 'GROQ_API_KEY', 'CEREBRAS_API_KEY', 'XAI_API_KEY', 'KEY'):
    os.environ.pop(variable, None)
environment_key = os.environ.get('LOCAL_API_KEY', '').encode()
PASTED = b'pasted-secret-never-print'
# A long key pasted in chunks; the alphabet appears nowhere else in init's output.
LONG_PASTE = b'pasted-' + b'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789' * 4
LOCAL = b'\x1b[B' * 7 + b'\r'
CUSTOM = b'\x1b[B' * 8 + b'\r'
DOWN = b'\x1b[B'
config_path = os.path.join(home, 'config.toml')
# An existing configuration makes init change it; cancellation must leave it as is.
original = open(config_path, 'rb').read() if os.path.exists(config_path) else None
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


def assert_restored():
    actual = termios.tcgetattr(slave)
    assert terminal_state_restored(before, actual), (before, actual)


def sampled(transcript, chunks, span=0.25):
    """Writes `chunks` spread over `span` seconds, checking the terminal
    attributes between writes and output reads: while a menu or the hidden
    prompt waits, the terminal never echoes input or buffers lines."""
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


def interrupted(transcript):
    os.write(master, b'\x03')
    out, transcript = finish(transcript)
    assert child.returncode == -signal.SIGINT, (child.returncode, transcript)
    assert out == b'', out
    assert not os.path.exists(os.path.join(home, 'config.toml'))


def custom_endpoint(transcript, variable=b'LOCAL_API_KEY'):
    """Selects the custom endpoint and answers its three questions."""
    os.write(master, CUSTOM)
    transcript = wait_for_prompt(transcript, b'Provider name')
    assert_restored()
    os.write(master, b'local\n' + base_url.encode() + b'\n' + variable + b'\n')
    return transcript


def local_server(transcript, downs):
    """Selects the local model server entry, then the server `downs` rows down."""
    os.write(master, LOCAL)
    transcript = at_raw_prompt(transcript, b'Select a local model server')
    for _ in range(downs):
        os.write(master, b'\x1b[B')
        transcript = wait_for_raw(transcript)
    return transcript


def type_filter(transcript, text):
    """Types `text` into the model filter one key at a time."""
    typed = b''
    for key in text:
        typed += bytes([key])
        transcript = wait_for_raw(transcript)
        sent = len(transcript)
        os.write(master, bytes([key]))
        transcript = transcript[:sent] + wait_for_prompt(transcript[sent:], b'Filter: ' + typed)
    return wait_for_raw(transcript)


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


def send_key(transcript, key, redraw):
    """Sends one key in raw mode and waits for the redraw it causes."""
    transcript = wait_for_raw(transcript)
    sent = len(transcript)
    os.write(master, key)
    return transcript[:sent] + wait_for_prompt(transcript[sent:], redraw)


def cancelled(transcript):
    out, transcript = finish(transcript)
    assert child.returncode == 1, (child.returncode, transcript)
    assert out == b'', out
    assert b'configuration cancelled; nothing was written' in transcript, transcript
    if original is None:
        assert not os.path.exists(config_path)
    else:
        assert open(config_path, 'rb').read() == original


def press(transcript, key, redraw):
    """Sends one key in raw mode and waits for the redraw it causes."""
    transcript = wait_for_raw(transcript)
    sent = len(transcript)
    os.write(master, key)
    return transcript[:sent] + wait_for_prompt(transcript[sent:], redraw)


def pick(transcript, title, position):
    """Waits for the menu `title`, moves down to `position` one key at a time,
    and chooses it."""
    transcript = at_raw_prompt(transcript, title)
    for number in range(2, position + 1):
        transcript = press(transcript, DOWN, b'> %d. ' % number)
    transcript = wait_for_raw(transcript)
    os.write(master, b'\r')
    return transcript


def escape_at(transcript, title):
    transcript = at_raw_prompt(transcript, title)
    os.write(master, b'\x1b')
    cancelled(transcript)


def confirm_edit(transcript):
    """Confirms the previewed change and returns the replaced file."""
    transcript = write_after(transcript, b'Write this configuration', b'y\n')
    out, transcript = finish(transcript)
    assert child.returncode == 0, (child.returncode, transcript)
    assert out == b'', out
    assert b'warning: comments and formatting' not in transcript, transcript
    config = open(config_path, 'rb').read()
    for secret in (PASTED, environment_key):
        assert not secret or secret not in transcript + out + config, secret
    return config, transcript


def edit_scenario(transcript):
    """Drives one change to the existing configuration."""
    if scenario == 'edit-provider':
        transcript = pick(transcript, b'Choose a change', 1)
        transcript = pick(transcript, b'Select a provider', 1)
        transcript = write_after(transcript, b'Provider name', b'openai\n')
        transcript = write_after(transcript, b'already used', b'work\n')
        transcript = at_raw_prompt(transcript, b'API key (hidden; Enter skips): ')
        os.write(master, b'\r')
        transcript = write_after(transcript, b'Model identifier', b'typed-model\n')
        transcript = write_after(transcript, b'Replacement system prompt', b'\n')
        transcript = write_after(transcript, b'Profile name [work]', b'\n')
        config, transcript = confirm_edit(transcript)
        assert b'[providers.work]\nkind = "openai"' in config, config
        assert b'[profiles.work]\nprovider = "work"\nmodel = "typed-model"' in config, config
    elif scenario == 'edit-profile':
        transcript = pick(transcript, b'Choose a change', 2)
        transcript = pick(transcript, b'Select a provider', 1)
        transcript = pick(transcript, b'Select a model', 1)
        transcript = write_after(transcript, b'Replacement system prompt', b'\n')
        transcript = write_after(transcript, b'Profile name [local]', b'default\n')
        transcript = write_after(transcript, b'already used', b'work\n')
        config, transcript = confirm_edit(transcript)
        assert b'[profiles.work]\nprovider = "local"\nmodel = "fake-model"' in config, config
        assert b'Verified: the provider answered a minimal request.' in transcript
    elif scenario == 'edit-model':
        transcript = pick(transcript, b'Choose a change', 3)
        transcript = pick(transcript, b'Select a profile', 1)
        transcript = pick(transcript, b'Select a model', 2)
        transcript = wait_for_prompt(transcript, b'new threads only')
        config, transcript = confirm_edit(transcript)
        assert b'model = "other-model"' in config, config
        assert b'Verified: the provider answered a minimal request.' in transcript
    elif scenario == 'edit-default':
        transcript = pick(transcript, b'Choose a change', 4)
        transcript = pick(transcript, b'Select the default profile', 2)
        config, _ = confirm_edit(transcript)
        assert config.startswith(b'default_profile = "spare"\n'), config
    elif scenario == 'edit-escape-action':
        escape_at(transcript, b'Choose a change')
    elif scenario == 'edit-escape-model':
        transcript = pick(transcript, b'Choose a change', 3)
        transcript = pick(transcript, b'Select a profile', 1)
        escape_at(transcript, b'Select a model')
    elif scenario == 'edit-escape-hidden':
        transcript = pick(transcript, b'Choose a change', 3)
        transcript = pick(transcript, b'Select a profile', 2)
        escape_at(transcript, b'API key (hidden; Enter skips): ')
    else:
        action, title = EDIT_MENUS[scenario]
        transcript = pick(transcript, b'Choose a change', action)
        escape_at(transcript, title)


EDIT_MENUS = {
    'edit-escape-preset': (1, b'Select a provider'),
    'edit-escape-provider': (2, b'Select a provider'),
    'edit-escape-profile': (3, b'Select a profile'),
    'edit-escape-default': (4, b'Select the default profile'),
}


try:
    transcript = wait_for_raw(b'')
    if scenario.startswith('edit-'):
        edit_scenario(transcript)
    elif scenario == 'select':
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
        assert b'No key entered; continuing without a key' in transcript, transcript
    elif scenario == 'filter':
        assert environment_key
        transcript = custom_endpoint(transcript)
        transcript = at_raw_prompt(transcript, b'Select a model')
        os.write(master, b'OTHERx')
        transcript = wait_for_prompt(transcript, b'Filter: OTHERx')
        # Each key waits for the redraw it causes and for raw mode: a key that
        # arrives between reads is handled by the terminal's line discipline.
        for key, redraw in ((b'\x7f', b'Filter: OTHER\r\n'), (b'\x1b[B', b'> 2. other-mini')):
            transcript = wait_for_raw(transcript)
            sent = len(transcript)
            os.write(master, key)
            transcript = transcript[:sent] + wait_for_prompt(transcript[sent:], redraw)
        transcript = wait_for_raw(transcript)
        os.write(master, b'\r')
        transcript = wait_for_prompt(transcript, b'Model: other-mini')
        config, transcript = finish_written(transcript)
        assert b'model = "other-mini"' in config, config
        assert b'Using LOCAL_API_KEY from the environment (value not shown).' in transcript
        assert b'Verified: the provider answered a minimal request.' in transcript
    elif scenario == 'burst':
        assert environment_key
        transcript = sampled(transcript, [DOWN, b'\x1b[A'] * 4)
        transcript = custom_endpoint(transcript)
        transcript = at_raw_prompt(transcript, b'Select a model')
        os.write(master, b'OTHERx')
        transcript = wait_for_prompt(transcript, b'Filter: OTHERx')
        transcript = sampled(transcript, [b'y', b'\x7f'] * 4)
        # Backspace, Down, and Enter in one write, without waiting for redraws.
        os.write(master, b'\x7f\x1b[B\r')
        transcript = wait_for_prompt(transcript, b'Model: other-mini')
        assert_restored()
        config, transcript = finish_written(transcript)
        assert b'model = "other-mini"' in config, config
    elif scenario == 'paste':
        assert not environment_key
        transcript = custom_endpoint(transcript)
        prompt = b'API key (hidden; Enter skips): '
        transcript = at_raw_prompt(transcript, prompt)
        chunks = [LONG_PASTE[start:start + 16] for start in range(0, len(LONG_PASTE), 16)]
        transcript = sampled(transcript, chunks)
        os.write(master, b'\r')
        transcript = wait_for_prompt(transcript, b'Requesting the model list')
        assert_restored()
        after = transcript[transcript.rindex(prompt) + len(prompt):]
        assert after.startswith(b'\r\nRequesting the model list'), after
        assert not any(chunk in transcript for chunk in chunks), transcript
        transcript = at_raw_prompt(transcript, b'Select a model')
        transcript = sampled(transcript, [b'\x1b[A', DOWN] * 4)
        os.write(master, b'\x1b[A\r')
        transcript = write_after(transcript, b'Model identifier', b'typed-model\n')
        config, transcript = finish_written(transcript)
        assert LONG_PASTE not in transcript + config
        assert b'model = "typed-model"' in config, config
    elif scenario == 'hidden-ctrl-d':
        transcript = custom_endpoint(transcript)
        transcript = at_raw_prompt(transcript, b'API key (hidden; Enter skips): ')
        os.write(master, b'partial\x04')
        cancelled(transcript)
    elif scenario == 'hidden-ctrl-c':
        transcript = custom_endpoint(transcript)
        interrupted(at_raw_prompt(transcript, b'API key (hidden; Enter skips): '))
    elif scenario == 'hangup':
        # End of input at a raw terminal is a hangup. The device is gone
        # afterwards, so its attributes can no longer be read back.
        transcript = custom_endpoint(transcript)
        transcript = at_raw_prompt(transcript, b'API key (hidden; Enter skips): ')
        os.close(master)
        assert child.wait(timeout=5) == 1, child.returncode
        assert child.stdout.read() == b''
        assert not os.path.exists(config_path)
        sys.exit(0)
    elif scenario == 'model-ctrl-c':
        assert environment_key
        transcript = custom_endpoint(transcript)
        interrupted(at_raw_prompt(transcript, b'Select a model'))
    elif scenario == 'local':
        assert environment_key
        transcript = local_server(transcript, 1)
        os.write(master, b'\r')
        transcript = write_after(transcript, b'Endpoint base URL [http://localhost:1234/v1]',
                                 base_url.encode() + b'\n')
        transcript = at_raw_prompt(transcript, b'Select a model')
        transcript = type_filter(transcript, b'mini')
        os.write(master, b'\r')
        transcript = wait_for_prompt(transcript, b'Model: other-mini')
        config, transcript = finish_written(transcript)
        assert b'[providers.lmstudio]' in config, config
        assert b'timeout_ms = 120000' in config, config
        assert b'api_key_env' not in config, config
        assert b'model = "other-mini"' in config, config
        assert b'Sending a minimal request to ' + base_url.encode() + b' to verify it.' in transcript, transcript
        assert b'Verified: the provider answered a minimal request.' in transcript
    elif scenario == 'local-escape':
        transcript = local_server(transcript, 0)
        os.write(master, b'\x1b')
        cancelled(transcript)
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
    elif scenario == 'published':
        assert not environment_key and os.environ['ASK_MODEL_LIST_URL']
        os.write(master, b'\r')
        transcript = at_raw_prompt(transcript, b'API key (hidden; Enter skips): ')
        os.write(master, b'\r')
        transcript = at_raw_prompt(transcript, b'Select a model')
        assert b'; no credentials are sent.' in transcript, transcript
        assert b'Published model list generated 2026-10-01' in transcript, transcript
        typed = b''
        for key in (b'o', b't', b'h', b'e', b'r'):
            typed += key
            transcript = send_key(transcript, key, b'Filter: ' + typed + b'\r\n')
        transcript = send_key(transcript, b'\x1b[B', b'> 2. other-mini')
        transcript = wait_for_raw(transcript)
        os.write(master, b'\r')
        transcript = wait_for_prompt(transcript, b'Model: other-mini')
        config, transcript = finish_written(transcript)
        assert b'kind = "openai"' in config and b'model = "other-mini"' in config, config
        assert b'[2Jevil' not in transcript, transcript
        assert b'live check' not in transcript and b'Verified' not in transcript, transcript
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
        interrupted(transcript)
    assert_restored()
finally:
    os.close(slave)
    if scenario != 'hangup':
        os.close(master)
    if child.poll() is None:
        child.kill()
        child.wait()
