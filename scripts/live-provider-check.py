#!/usr/bin/env python3
"""Opt-in live-provider compatibility checks; reports only, never merges or mutates code."""
import argparse
import datetime
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_QUERY = "Reply with exactly: live-check-ok"
DEFAULT_REPLY = "Reply with exactly: live-check-reply-ok"
QUERY_ANSWER_MARKER = "live-check-ok"
REPLY_ANSWER_MARKER = "live-check-reply-ok"
LIVE_CHECK_SYSTEM_PROMPT = "Answer briefly."
REQUIRED_MAX_OUTPUT_TOKENS = 128
REVISION = re.compile(r"^[0-9a-f]{40}$")
USAGE = re.compile(r"(\d+)\s+in\s*/\s*(\d+)\s+out")
RELEVANT_PATHS = (
    "Cargo.toml",
    "Cargo.lock",
    "scripts/live-provider-check.py",
    "src",
    "tests",
)
INVOKE_ENV_KEYS = (
    "HOME",
    "PATH",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TMPDIR",
    "TEMP",
    "TMP",
    "USER",
    "LOGNAME",
    "SHELL",
    "TERM",
    "PYTHONDONTWRITEBYTECODE",
)


def require(condition, message="verification failed"):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def strict_int(value, label):
    require(type(value) is int and not isinstance(value, bool), f"{label} must be an integer")
    return value


def strict_bool(value, label, default=False):
    if value is None:
        return default
    require(type(value) is bool, f"{label} must be a boolean")
    return value


def toml_basic_string(value):
    require(isinstance(value, str), "toml string value must be a string")
    return json.dumps(value, ensure_ascii=False).replace(chr(127), r"\u007f")


def toml_table_key(name):
    require(isinstance(name, str) and name, "toml table key must be a non-empty string")
    if re.fullmatch(r"[A-Za-z0-9_-]+", name):
        return name
    return toml_basic_string(name)


def toml_key_value(key, value):
    if value is None:
        return None
    if isinstance(value, bool):
        rendered = "true" if value else "false"
    elif isinstance(value, int) and not isinstance(value, bool):
        rendered = str(value)
    elif isinstance(value, str):
        rendered = toml_basic_string(value)
    else:
        raise ValueError(f"unsupported toml value for {key}")
    return f"{toml_table_key(key)} = {rendered}"


def render_ask_home_toml(snapshot):
    provider = snapshot["provider"]
    profile = snapshot["profile"]
    lines = [f"default_profile = {toml_basic_string(profile)}", ""]
    provider_table = f"[providers.{toml_table_key(provider)}]"
    profile_table = f"[profiles.{toml_table_key(profile)}]"
    provider_fields = [
        toml_key_value("kind", snapshot.get("kind")),
        toml_key_value("base_url", snapshot.get("base_url")),
        toml_key_value("api_key_env", snapshot.get("api_key_env")),
    ]
    profile_fields = [
        toml_key_value("provider", provider),
        toml_key_value("model", snapshot.get("model")),
        toml_key_value("max_output_tokens", snapshot.get("max_output_tokens")),
        toml_key_value("system_prompt", LIVE_CHECK_SYSTEM_PROMPT),
    ]
    lines.append(provider_table)
    lines.extend(field for field in provider_fields if field is not None)
    lines.append("")
    lines.append(profile_table)
    lines.extend(field for field in profile_fields if field is not None)
    return "\n".join(lines) + "\n"


def load_config(path):
    config = tomllib.loads(path.read_text())
    require(config.get("version") == 1, "unsupported check configuration version")
    run = config.get("run") or {}
    policy = config.get("policy") or {}
    targets = config.get("targets") or []
    require(isinstance(run, dict) and isinstance(policy, dict) and isinstance(targets, list))
    require(targets, "at least one target is required")
    ask_binary = run.get("ask_binary")
    source_revision = run.get("source_revision")
    wrapper = run.get("credential_wrapper")
    require(isinstance(ask_binary, str) and Path(ask_binary).is_absolute(), "run.ask_binary must be an absolute path")
    require(isinstance(source_revision, str) and REVISION.fullmatch(source_revision), "run.source_revision must be a 40-character commit")
    require(isinstance(wrapper, str) and Path(wrapper).is_absolute(), "run.credential_wrapper must be an absolute path")
    host_id = run.get("host_id")
    require(isinstance(host_id, str) and host_id.strip(), "run.host_id is required")
    max_requests = strict_int(policy.get("max_requests", 8), "policy.max_requests")
    max_answer_chars = strict_int(policy.get("max_answer_chars", 64), "policy.max_answer_chars")
    query_prompt = policy.get("query_prompt", DEFAULT_QUERY)
    reply_prompt = policy.get("reply_prompt", DEFAULT_REPLY)
    require(1 <= max_requests <= 64)
    require(8 <= max_answer_chars <= 4096)
    require(query_prompt == DEFAULT_QUERY, "policy.query_prompt must use the minimal live-check prompt")
    require(reply_prompt == DEFAULT_REPLY, "policy.reply_prompt must use the minimal live-check prompt")
    parsed_targets = []
    seen_target_ids = set()
    for index, target in enumerate(targets, start=1):
        require(isinstance(target, dict), f"targets[{index}] must be a table")
        target_id = target.get("id")
        ask_home = target.get("ask_home")
        require(isinstance(target_id, str) and re.fullmatch(r"[a-z0-9-]+", target_id), f"targets[{index}].id is invalid")
        require(target_id not in seen_target_ids, f"duplicate target id {target_id}")
        seen_target_ids.add(target_id)
        require(isinstance(ask_home, str) and Path(ask_home).is_absolute(), f"targets[{index}].ask_home must be absolute")
        parsed_targets.append({"id": target_id, "ask_home": Path(ask_home)})
    state_path = run.get("state_path")
    if state_path is None:
        state_path = path.with_name(path.name + ".state.json")
    else:
        require(isinstance(state_path, str) and Path(state_path).is_absolute(), "run.state_path must be absolute")
        state_path = Path(state_path)
    lock_path = run.get("lock_path")
    if lock_path is None:
        lock_path = state_path.with_name(state_path.name + ".lock")
    else:
        require(isinstance(lock_path, str) and Path(lock_path).is_absolute(), "run.lock_path must be absolute")
        lock_path = Path(lock_path)
    return {
        "path": path,
        "run": {
            "ask_binary": Path(ask_binary),
            "source_revision": source_revision,
            "credential_wrapper": Path(wrapper),
            "host_id": host_id.strip(),
            "budget_available": strict_bool(run.get("budget_available"), "run.budget_available"),
            "state_path": state_path,
            "lock_path": lock_path,
        },
        "policy": {
            "max_requests": max_requests,
            "max_answer_chars": max_answer_chars,
            "query_prompt": query_prompt,
            "reply_prompt": reply_prompt,
        },
        "targets": parsed_targets,
    }


def git_output(args, cwd=None):
    result = subprocess.run(
        ["git", *args],
        cwd=ROOT if cwd is None else cwd,
        capture_output=True,
        text=True,
    )
    require(result.returncode == 0, "git command failed")
    return result.stdout.strip()


def git_output_optional(args, cwd=None):
    result = subprocess.run(
        ["git", *args],
        cwd=ROOT if cwd is None else cwd,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return None
    return result.stdout.strip()


def relevant_tree_paths():
    paths = []
    for entry in RELEVANT_PATHS:
        path = ROOT / entry
        if path.is_dir():
            for child in sorted(path.rglob("*")):
                if child.is_file() and not child.name.endswith(".md"):
                    paths.append(child.relative_to(ROOT).as_posix())
        elif path.is_file():
            paths.append(entry)
    return paths


def ask_home_target_snapshot(ask_home):
    config_path = ask_home / "config.toml"
    require(config_path.is_file(), "missing ask configuration")
    document = tomllib.loads(config_path.read_text())
    default_profile = document.get("default_profile")
    profiles = document.get("profiles") or {}
    providers = document.get("providers") or {}
    require(isinstance(default_profile, str) and default_profile in profiles, "default profile is missing")
    profile = profiles[default_profile]
    provider_name = profile.get("provider")
    require(isinstance(provider_name, str) and provider_name in providers, "profile provider is missing")
    provider = providers[provider_name]
    max_output_tokens = profile.get("max_output_tokens")
    strict_int(max_output_tokens, "profile max_output_tokens")
    require(
        max_output_tokens == REQUIRED_MAX_OUTPUT_TOKENS,
        f"profile max_output_tokens must be {REQUIRED_MAX_OUTPUT_TOKENS}",
    )
    return {
        "profile": default_profile,
        "provider": provider_name,
        "kind": provider.get("kind"),
        "base_url": provider.get("base_url"),
        "api_key_env": provider.get("api_key_env"),
        "model": profile.get("model"),
        "system_prompt": profile.get("system_prompt"),
        "max_output_tokens": max_output_tokens,
    }


def snapshot_fingerprint(snapshot):
    payload = json.dumps(snapshot, sort_keys=True, separators=(",", ":")).encode()
    return digest(payload)


def validate_targets(config):
    reasons = []
    snapshots = {}
    for target in config["targets"]:
        ask_home = target["ask_home"]
        if not ask_home.is_dir():
            reasons.append(f"target {target['id']} ask_home missing")
            continue
        try:
            snapshots[target["id"]] = ask_home_target_snapshot(ask_home)
        except ValueError as error:
            reasons.append(f"target {target['id']} {error}")
    return reasons, snapshots


def config_fingerprint(config, target_snapshots):
    targets = []
    for target in config["targets"]:
        targets.append(
            {
                "ask_home": snapshot_fingerprint(target_snapshots[target["id"]]),
                "id": target["id"],
            }
        )
    payload = json.dumps(
        {
            "policy": config["policy"],
            "targets": targets,
            "version": 1,
        },
        sort_keys=True,
        separators=(",", ":"),
    ).encode()
    return digest(payload)


def source_fingerprint(config, target_snapshots):
    parts = [config_fingerprint(config, target_snapshots)]
    for rel in relevant_tree_paths():
        path = ROOT / rel
        if path.is_file():
            parts.append(f"{rel}\0{digest(path.read_bytes())}")
    return digest("\0".join(parts).encode())


def head_revision():
    return git_output(["rev-parse", "HEAD"])


def doc_only_change(revision):
    diff = git_output_optional(["diff", "--name-only", revision, "HEAD"])
    if diff is None:
        return False
    if not diff:
        return True
    return all(path.startswith("docs/") or path.endswith(".md") for path in diff.splitlines())


def relevant_source_dirty():
    status = git_output_optional(["status", "--porcelain", "--", *RELEVANT_PATHS])
    return bool(status and status.strip())


def load_state(path):
    if not path.is_file():
        return {"attempts": []}
    try:
        state = json.loads(path.read_text())
    except json.JSONDecodeError as error:
        raise ValueError("state file is not valid JSON") from error
    require(isinstance(state, dict) and isinstance(state.get("attempts"), list), "invalid state file")
    return state


def durable_replace(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_suffix(path.suffix + ".tmp")
    temp.write_text(text)
    with open(temp, "r+b") as handle:
        os.fsync(handle.fileno())
    os.replace(temp, path)
    with open(path, "r+b") as handle:
        os.fsync(handle.fileno())
    directory = os.open(path.parent, os.O_RDONLY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def save_state(path, state):
    durable_replace(path, json.dumps(state, indent=2, sort_keys=True) + "\n")


def sanitize_answer(text, limit):
    cleaned = "".join(ch if ch.isprintable() and ch not in "\r\n\t" else " " for ch in text)
    cleaned = re.sub(r"\s+", " ", cleaned).strip()
    if len(cleaned) > limit:
        cleaned = cleaned[: limit - 3] + "..."
    return cleaned


def parse_usage(stderr):
    match = USAGE.search(stderr)
    if not match:
        return None
    return {"input_tokens": int(match.group(1)), "output_tokens": int(match.group(2))}


def binary_manifest_status(binary, expected_revision):
    manifest = binary.parent / "manifest.json"
    if not manifest.is_file():
        return {"verified": False, "reason": "missing adjacent manifest"}
    try:
        info = json.loads(manifest.read_text())
    except json.JSONDecodeError:
        return {"verified": False, "reason": "invalid manifest"}
    source_sha = info.get("source_sha")
    if not isinstance(source_sha, str) or not REVISION.fullmatch(source_sha):
        return {"verified": False, "reason": "manifest missing source_sha"}
    binary_sha = info.get("binary_sha256")
    if not isinstance(binary_sha, str) or len(binary_sha) != 64:
        return {"verified": False, "reason": "manifest missing binary_sha256"}
    if source_sha != expected_revision:
        return {"verified": False, "reason": "manifest source_sha mismatch"}
    if not binary.is_file():
        return {"verified": False, "reason": "missing ask binary"}
    if binary_sha != digest(binary.read_bytes()):
        return {"verified": False, "reason": "manifest binary_sha256 mismatch"}
    return {"verified": True, "manifest_source_sha": source_sha, "manifest_binary_sha256": binary_sha}


class AttemptLock:
    def __init__(self, path):
        self.path = path
        self.handle = None

    def acquire(self):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.handle = open(self.path, "a+")
        try:
            fcntl.flock(self.handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            self.handle.close()
            self.handle = None
            raise ValueError("another live check is running") from error

    def release(self):
        if self.handle is None:
            return
        fcntl.flock(self.handle.fileno(), fcntl.LOCK_UN)
        self.handle.close()
        self.handle = None

    def __enter__(self):
        self.acquire()
        return self

    def __exit__(self, exc_type, exc, tb):
        self.release()
        return False


def invoke_env(ask_home):
    env = {}
    for key in INVOKE_ENV_KEYS:
        if key in os.environ:
            env[key] = os.environ[key]
    for key, value in os.environ.items():
        if key.startswith("LIVE_CHECK_"):
            env[key] = value
    env["ASK_HOME"] = str(ask_home)
    return env


def invoke(wrapper, ask_binary, ask_home, args, timeout=120):
    command = [str(wrapper), str(ask_binary), *args]
    result = subprocess.run(
        command,
        env=invoke_env(ask_home),
        capture_output=True,
        text=True,
        timeout=timeout,
        shell=False,
    )
    return result


def invoke_provider_request(wrapper, ask_binary, ask_home, args, timeout=120):
    try:
        return invoke(wrapper, ask_binary, ask_home, args, timeout=timeout), None
    except subprocess.TimeoutExpired:
        return None, "timeout"
    except OSError:
        return None, "os_error"


def invocation_failure_reason(kind):
    if kind == "timeout":
        return "provider request timed out"
    return "provider request failed"


def persist_attempted_requests(state_path, fingerprint, request_count, report):
    report["request_count"] = request_count
    update_attempt(state_path, fingerprint, {"request_count": request_count})


def write_ask_home_from_snapshot(base_dir, snapshot):
    home = Path(tempfile.mkdtemp(prefix="live-check-", dir=base_dir))
    home.mkdir(parents=True, exist_ok=True)
    (home / "config.toml").write_text(render_ask_home_toml(snapshot))
    return home


def preflight_target(wrapper, ask_binary, ask_home):
    return invoke(wrapper, ask_binary, ask_home, ["doctor"])


def command_succeeded(result, max_answer_chars, expected_marker):
    if result.returncode != 0:
        return False
    return result.stdout.strip() == expected_marker


def readiness(config, host_env, budget_flag, force):
    run = config["run"]
    reasons = []
    if not run["credential_wrapper"].is_file():
        reasons.append("missing credential wrapper")
    elif not os.access(run["credential_wrapper"], os.X_OK):
        reasons.append("credential wrapper is not executable")
    if not run["ask_binary"].is_file():
        reasons.append("missing ask binary")
    elif not os.access(run["ask_binary"], os.X_OK):
        reasons.append("ask binary is not executable")
    if host_env != run["host_id"]:
        reasons.append("host mismatch")
    if not (budget_flag or run["budget_available"]):
        reasons.append("budget unavailable")
    exported = [name for name in os.environ if name.endswith("_API_KEY") or name.endswith("_KEY")]
    if exported:
        reasons.append("provider credentials already exported")

    target_reasons, snapshots = validate_targets(config)
    reasons.extend(target_reasons)
    if target_reasons:
        return None, None, reasons, {"verified": False, "reason": "target validation failed"}, None

    fingerprint = source_fingerprint(config, snapshots)
    revision = head_revision()
    manifest = binary_manifest_status(run["ask_binary"], run["source_revision"])
    if not manifest.get("verified"):
        reasons.append("binary manifest not verified")
    elif relevant_source_dirty():
        reasons.append("relevant source tree is dirty")
    if revision != run["source_revision"]:
        if doc_only_change(run["source_revision"]):
            reasons.append("documentation-only change since expected revision")
        else:
            reasons.append("checkout revision differs from configured source revision")
    state = load_state(run["state_path"])
    for attempt in reversed(state["attempts"]):
        if attempt.get("fingerprint") != fingerprint:
            continue
        status = attempt.get("status")
        if status == "pass":
            reasons.append("unchanged relevant fingerprint already passed")
            break
        if status in {"fail", "started"} and not force:
            reasons.append("prior failed attempt for this fingerprint")
            break
    return fingerprint, revision, reasons, manifest, snapshots


def record_attempt(state_path, record):
    state = load_state(state_path)
    state["attempts"].append(record)
    save_state(state_path, state)


def update_attempt(state_path, fingerprint, updates):
    state = load_state(state_path)
    for attempt in reversed(state["attempts"]):
        if attempt.get("fingerprint") == fingerprint and attempt.get("status") == "started":
            attempt.update(updates)
            save_state(state_path, state)
            return
    raise ValueError("started attempt not found")


def run_check(config, host_env, budget_flag, force):
    lock = AttemptLock(config["run"]["lock_path"])
    try:
        lock.acquire()
    except ValueError as error:
        report = {
            "status": "notrun",
            "reason": str(error),
            "request_count": 0,
            "targets": [],
        }
        print(json.dumps(report, indent=2, sort_keys=True))
        return 2

    temp_root = tempfile.mkdtemp(prefix="live-check-run-")
    materialized = []
    try:
        fingerprint, revision, reasons, manifest, snapshots = readiness(
            config, host_env, budget_flag, force
        )

        run = config["run"]
        policy = config["policy"]
        report = {
            "status": "notrun",
            "request_count": 0,
            "binary_source": manifest,
            "targets": [],
        }
        if fingerprint is not None:
            report["fingerprint"] = fingerprint
            report["source_revision"] = revision
            report["expected_revision"] = run["source_revision"]
            report["binary_sha256"] = (
                digest(run["ask_binary"].read_bytes()) if run["ask_binary"].is_file() else None
            )
        if reasons:
            report["reason"] = "; ".join(reasons)
            print(json.dumps(report, indent=2, sort_keys=True))
            return 0 if any("documentation-only change" in reason for reason in reasons) else 2

        preflight = []
        runtime_targets = []
        for target in config["targets"]:
            snapshot = snapshots[target["id"]]
            runtime_home = write_ask_home_from_snapshot(temp_root, snapshot)
            materialized.append(runtime_home)
            doctor = preflight_target(run["credential_wrapper"], run["ask_binary"], runtime_home)
            preflight.append(
                {
                    "id": target["id"],
                    "doctor_exit": doctor.returncode,
                    "ready": doctor.returncode == 0,
                }
            )
            runtime_targets.append({"id": target["id"], "ask_home": runtime_home})

        if not all(entry["ready"] for entry in preflight):
            report["reason"] = "doctor preflight failed"
            report["preflight"] = preflight
            print(json.dumps(report, indent=2, sort_keys=True))
            return 2

        record_attempt(
            run["state_path"],
            {
                "fingerprint": fingerprint,
                "source_revision": revision,
                "expected_revision": run["source_revision"],
                "started_at": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat(),
                "status": "started",
                "request_count": 0,
            },
        )

        requests = 0
        overall = "pass"
        wrapper = run["credential_wrapper"]
        binary = run["ask_binary"]
        for target in runtime_targets:
            entry = {"id": target["id"], "query": "skipped", "reply": "skipped"}
            if requests >= policy["max_requests"]:
                entry["reason"] = "request cap reached"
                overall = "fail"
                report["targets"].append(entry)
                continue
            requests += 1
            persist_attempted_requests(run["state_path"], fingerprint, requests, report)
            query, query_failure = invoke_provider_request(
                wrapper, binary, target["ask_home"], ["new", policy["query_prompt"]]
            )
            if query_failure:
                entry["query"] = "fail"
                entry["reply"] = "skipped"
                entry["reason"] = invocation_failure_reason(query_failure)
                overall = "fail"
                report["targets"].append(entry)
                continue
            if not command_succeeded(query, policy["max_answer_chars"], QUERY_ANSWER_MARKER):
                entry["query"] = "fail"
                overall = "fail"
                report["targets"].append(entry)
                continue
            entry["query"] = "pass"
            entry["answer_chars"] = len(sanitize_answer(query.stdout, policy["max_answer_chars"]))
            entry["query_usage"] = parse_usage(query.stderr)
            if requests >= policy["max_requests"]:
                entry["reply"] = "skipped"
                entry["reason"] = "request cap reached"
                overall = "fail"
                report["targets"].append(entry)
                continue
            requests += 1
            persist_attempted_requests(run["state_path"], fingerprint, requests, report)
            reply, reply_failure = invoke_provider_request(
                wrapper, binary, target["ask_home"], ["reply", policy["reply_prompt"]]
            )
            if reply_failure:
                entry["reply"] = "fail"
                entry["reason"] = invocation_failure_reason(reply_failure)
                overall = "fail"
                report["targets"].append(entry)
                continue
            if not command_succeeded(reply, policy["max_answer_chars"], REPLY_ANSWER_MARKER):
                entry["reply"] = "fail"
                overall = "fail"
            else:
                entry["reply"] = "pass"
                entry["reply_usage"] = parse_usage(reply.stderr)
            report["targets"].append(entry)

        report["status"] = overall
        update_attempt(
            run["state_path"],
            fingerprint,
            {
                "status": overall,
                "finished_at": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat(),
                "request_count": requests,
            },
        )
        print(json.dumps(report, indent=2, sort_keys=True))
        return 0 if overall == "pass" else 1
    finally:
        for home in materialized:
            shutil.rmtree(home, ignore_errors=True)
        shutil.rmtree(temp_root, ignore_errors=True)
        lock.release()


def build_parser():
    parser = argparse.ArgumentParser(description="Run opt-in live-provider compatibility checks.")
    sub = parser.add_subparsers(dest="command", required=True)
    run = sub.add_parser("run", help="Execute configured live checks when readiness preconditions pass.")
    run.add_argument("--config", type=Path, required=True)
    run.add_argument("--host", dest="host_env", default=os.environ.get("LIVE_CHECK_HOST", ""))
    run.add_argument("--budget-available", action="store_true")
    run.add_argument("--force", action="store_true", help="Retry a fingerprint that previously failed.")
    fingerprint = sub.add_parser("fingerprint", help="Print the current relevant source fingerprint.")
    fingerprint.add_argument("--config", type=Path, required=True)
    return parser


def main(argv=None):
    if os.environ.get("PYTHONOPTIMIZE"):
        print("live-provider-check.py refuses PYTHONOPTIMIZE", file=sys.stderr)
        return 2
    parser = build_parser()
    args = parser.parse_args(argv)
    config = load_config(args.config.resolve())
    if args.command == "fingerprint":
        _, snapshots = validate_targets(config)
        require(not _, "target validation failed")
        print(source_fingerprint(config, snapshots))
        return 0
    return run_check(config, args.host_env.strip(), args.budget_available, args.force)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, subprocess.TimeoutExpired, OSError):
        print("live-provider-check failed; raw diagnostics withheld", file=sys.stderr)
        raise SystemExit(1)
