# Injecting credentials when launching `ask`

`ask` reads the environment-variable name in each provider's `api_key_env`. Store the key in an external credential manager and inject it only when launching `ask`. Keep credential values out of configuration, shell profiles, command arguments, and shell history. Process-scoped environment variables remain accessible to sufficiently privileged processes; they are not a sandbox.

For a one-off hidden prompt in bash or zsh, replace `OPENAI_API_KEY` with the name shown by `ask init`:

```sh
( printf 'API key: ' >&2; IFS= read -rs OPENAI_API_KEY </dev/tty || exit; printf '\n' >&2; export OPENAI_API_KEY; exec ask "what is 2+2" )
```

Paste the key at the prompt. The subshell exports it only for the launched process and its descendants, then exits; the parent shell is unchanged. Reading from `/dev/tty` leaves redirected stdin available for query text.

## Example for existing 1Password CLI users

This optional recipe uses an external tool; `ask` neither requires nor installs it. With an authenticated 1Password CLI, save an environment file containing a secret reference, not the key itself:

```dotenv
OPENAI_API_KEY="op://your-vault/your-item/your-field"
```

Replace the reference with the field that holds your provider key. Launch:

```sh
op run --env-file=ask-secrets.env -- ask "what is 2+2"
```

`op run` resolves the reference and supplies the value to its child process. For recurring use, put that command in a separate launcher script and forward query arguments with `"$@"`; do not name the script `ask` if it would call itself. Other credential managers can use their equivalent process-environment injection command. See [1Password's environment injection documentation](https://developer.1password.com/docs/cli/secrets-environment-variables/) for authentication and reference setup.
