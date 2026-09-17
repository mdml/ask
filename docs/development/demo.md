# Terminal demo

The README terminal demo is a repeatable presentation fixture, not provider-speed evidence. `scripts/demo.py` runs the exact supplied, already-built executable as `ask` in a Python standard-library pseudo-terminal against a loopback-only fake OpenAI-compatible provider. The fixture supplies the captioned answers; `ask` supplies all command output, history, statistics, and terminal presentation. Recorded event times reflect the individual run and can vary.

The recorder writes asciicast v2 terminal bytes with measured event times to `docs/assets/demo/demo.cast` and derives `docs/assets/demo/demo.txt` from the same byte stream by removing ANSI control sequences and trailing whitespace. It renders `docs/assets/demo/demo.gif` only when an approved renderer is supplied. Generated artifacts contain no credentials, machine-specific paths, or hostnames.

The recorder gives Bash ownership of the pseudo-terminal and drives typing and output draining in bounded event loops, including while Bash exits. Shell startup and each command have separate 10 second deadlines, with no retries or skipped commands. The configured stage allowances total 76 seconds across those deadlines, four 1.5 second presentation pauses, the shell exit wait, and process cleanup. Fixture setup, shell process creation, provider shutdown, and the wait after a forced kill are not timed by those allowances. The smoke test's 91 second outer watchdog protects the whole recorder process and uses process-group termination only as a last resort. An outer interruption reports the active phase and total elapsed time; an interrupted command also reports its elapsed time, input progress, and the sanitized tail of terminal output. A shell exit timeout reports its elapsed time and the sanitized tail drained after `exit`.

## Record and validate

Use the final presentation build when it is available:

```sh
python3 scripts/demo-test.py <path-to-final-ask>
python3 scripts/demo.py <path-to-final-ask>
```

A recording made from another build is provisional and must be replaced by running the commands above against the final presentation build.

## Render

Rendering uses `agg` 1.9.0 from release source commit `26ca84c02523973198fca28533369edcfc7ed929`, published 2026-05-29T18:13:31Z. The reviewed Linux musl asset has SHA-256 `ddcbf6ca044c8ac3a434dcb9ee89fb9e3be87209982b7c2adb55f782e8f0f390`; no upstream attestation was available. `scripts/demo.py` verifies that hash immediately before invoking the renderer with a cleared environment inside a Bubblewrap network namespace. The renderer receives only local inputs.

```sh
python3 scripts/demo.py <path-to-final-ask> --agg <path-to-approved-agg>
```

The render uses DejaVu Sans Mono Book 2.37 from Debian package `fonts-dejavu-core` version `2.37-8`, SHA-256 `c805f9436dbc268644c1d9584f01a601a653e028e08fd74b9b949f6cf8304d88`, at 16 px. The recorder finds the font in the system font directory. The cast header records the supplied `ask` binary's SHA-256. The [dependency review](../reviews/agg-1.9.0.md) records the accepted limitations.

The renderer binary is local tooling and must never be added to the repository. Do not replace it or download another asset without a new security review.
