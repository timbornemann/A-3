#!/usr/bin/env python3
"""Register and run one of seven isolated A01 candidate measurements."""

from __future__ import annotations

import argparse
from decimal import Decimal
import hashlib
import importlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
RUNS = ROOT / "target" / "plan22-p5-runs"
NATIVE_SOURCE = ROOT / "crates/a3-storage-libsql/src/plan22_p5_live_a01.rs"
NATIVE_TEST = (
    "plan22_goal_contract_tests::existing_test_tests::changed_tests::p5_live_a01::"
    "live_ollama_a01_completes_budget_bound_source_run"
)
PREFIX = "a3.plan22.p5.a01."
CASE, VARIANT = "A01", "development"
P0_INPUT_SET = "7ba991a39bf2437b4fdbf353b9d879df9de2e8564cef7efbc08bf3598da66f52"
P0_FREEZE = "9e6e563d62ae157d396f224acaf84b2a6f332daae382c8a845e560b1056819bb"
P0_FILES = ("baseline.json", "cases.py", "oracle.py", "order_trace.py",
            "run_baseline.py", "suite.json", "supervisor.py")
ORNITH, QWEN, FOUR_K = "ornith-1.5:9b", "qwen38-27b-iq3s:latest", "qwen38-4k:latest"
PROFILES = {ORNITH: (8192, 2048), QWEN: (8192, 2048), FOUR_K: (4096, 1024)}
SLOTS = (
    ("ornith-01", ORNITH), ("qwen27-01", QWEN), ("qwen4k-01", FOUR_K),
    ("qwen27-02", QWEN), ("ornith-02", ORNITH), ("ornith-03", ORNITH), ("qwen27-03", QWEN),
)
ARGV = ["python", "-B", "-m", "unittest", "discover", "-s", "tests"]
TASK_TIMEOUT, PROBE_TIMEOUT, SHUTDOWN_GRACE = 360, 120, 30
OUTER_TIMEOUT = TASK_TIMEOUT + PROBE_TIMEOUT + SHUTDOWN_GRACE
MAX_FILE_BYTES = 1 << 20
HEX = re.compile(r"[0-9a-f]{64}")
NUMERIC_PARAMETER = re.compile(r"[ \t]*([a-z][a-z0-9_]*)[ \t]+([+-]?[0-9]{1,32}(?:[.][0-9]{1,32})?)[ \t]*")
MODEL_HASHES = ("digest", "template_sha256", "parameters_sha256", "system_sha256",
                "model_info_sha256", "weights_blob_sha256")
WIRE_KEYS = {
    "phase", "kind", "wire_blake3", "request_bytes", "num_ctx", "num_predict",
    "think", "finish", "prompt_tokens", "output_tokens",
}
PASS_ARTIFACTS = {
    "request.json", "launch.json", "native-start.json", "native-final.json",
    "runtime-before.json", "runtime-after.json", "owned-worker.json",
    "owned-oracle.json", "oracle.json",
}


class Rejected(Exception):
    """Content-free supervisor failure."""


def require(condition, reason):
    if not condition:
        raise Rejected(reason)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def linked(path):
    info = path.lstat()
    return stat.S_ISLNK(info.st_mode) or bool(
        getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT
    )


def small(path, limit=MAX_FILE_BYTES):
    require(not linked(path) and path.is_file(), "nonregular_input")
    with path.open("rb") as handle:
        data = handle.read(limit + 1)
    require(len(data) <= limit, "input_too_large")
    return data


def unique_object(items):
    result = dict(items)
    require(len(result) == len(items), "duplicate_json_key")
    return result


def parse(raw):
    result = json.loads(raw, object_pairs_hook=unique_object)
    require(isinstance(result, dict), "invalid_json_record")
    return result


def read_json(path):
    return parse(small(path))


def write_once(path, data):
    with path.open("xb") as output:
        output.write(data)
        output.flush()
        os.fsync(output.fileno())


def json_once(path, data):
    write_once(path, (json.dumps(data, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))


def clean_environment():
    return {key: value for key, value in os.environ.items() if key.upper() in {
        "PATH", "PATHEXT", "SYSTEMROOT", "WINDIR", "COMSPEC", "TEMP", "TMP", "TMPDIR",
        "HOME", "USERPROFILE", "LOCALAPPDATA", "LANG", "LC_ALL", "LC_CTYPE",
    }}


def process_ok(result):
    return (result["exit_code"] == 0 and not result["timed_out"] and not result["interrupted"]
            and not result["output_limit"]
            and all(not result[key]["read_failed"] and not result[key]["truncated"]
                    for key in ("stdout", "stderr")))


def p0_inputs(series_dir, create=False):
    frozen_path = ROOT / "fixtures/local-agent-eval-v1/p0-freeze.json"
    freeze_raw = small(frozen_path)
    require(sha(freeze_raw) == P0_FREEZE, "p0_freeze_changed")
    freeze = parse(freeze_raw)
    require(freeze["input_set_sha256"] == P0_INPUT_SET, "p0_input_set_changed")
    archive = ROOT / freeze["input_snapshot"] / "fixtures/local-agent-eval-v1"
    copied = series_dir / "p0-inputs"
    if create:
        copied.mkdir()
    require(not linked(copied) and copied.is_dir(), "invalid_p0_copy")
    hashes = freeze["inputs_sha256"]
    for name in P0_FILES:
        destination = copied / name
        if create:
            data = small(archive / name)
            require(sha(data) == hashes[f"fixtures/local-agent-eval-v1/{name}"], "p0_input_changed")
            write_once(destination, data)
        require(sha(small(destination)) == hashes[f"fixtures/local-agent-eval-v1/{name}"],
                "p0_copy_changed")
    sys.dont_write_bytecode = True
    sys.path.insert(0, str(copied))
    cases = importlib.import_module("cases")
    runner = importlib.import_module("run_baseline")
    require(Path(cases.__file__).resolve() == copied / "cases.py"
            and Path(runner.__file__).resolve() == copied / "run_baseline.py", "wrong_frozen_import")
    return cases, runner, runner.load_series_freeze(), copied


def source_manifest(runner):
    result, stdout, _ = runner.run_owned_process(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--",
         "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo", "crates",
         "vendor/libsql-0.9.29", "fixtures/plan22-p5"],
        cwd=ROOT, timeout=30, env=clean_environment(), capture_limit=1 << 20,
        terminate_on_output_limit=True,
    )
    require(process_ok(result), "source_inventory_failed")
    names = sorted({name.decode("utf-8") for name in stdout.split(b"\0") if name})
    require(len(names) <= 4096, "source_inventory_too_large")
    manifest = {}
    for name in names:
        path = ROOT / name
        require(path.resolve().is_relative_to(ROOT) and not linked(path) and path.is_file(),
                "invalid_source_file")
        manifest[name] = runner.file_sha256(path)
    require(NATIVE_SOURCE.relative_to(ROOT).as_posix() in manifest, "native_source_missing")
    return manifest


def require_worker_test(runner, binary):
    result, stdout, _ = runner.run_owned_process(
        [binary, NATIVE_TEST, "--list", "--ignored", "--exact"], cwd=ROOT,
        timeout=30, env=clean_environment(), capture_limit=16 * 1024,
        terminate_on_output_limit=True,
    )
    lines = stdout.decode("utf-8").splitlines()
    require(process_ok(result) and lines.count(f"{NATIVE_TEST}: test") == 1,
            "missing_exact_native_worker_test")


def register(series_dir, worker, server_version):
    require(NATIVE_SOURCE.is_file() and not linked(NATIVE_SOURCE), "p5_native_worker_pending")
    require(re.fullmatch(r"[0-9]+[.][0-9]+[.][0-9]+", server_version) is not None,
            "invalid_server_version")
    worker = worker.resolve(strict=True)
    require(worker.parent == ROOT / "target/debug/deps"
            and re.fullmatch(r"a3_storage_libsql-[0-9a-f]+(?:[.]exe)?", worker.name) is not None,
            "invalid_storage_worker_path")
    series_dir.mkdir(exist_ok=False)
    _, runner, baseline, _ = p0_inputs(series_dir, create=True)
    manifest = source_manifest(runner)
    frozen, worker_hash = runner.freeze_worker(ROOT, worker)
    require_worker_test(runner, frozen)
    try:
        with runner.GpuLock(Path(tempfile.gettempdir()) / "a3-plan22-ollama-gpu.lock"):
            pins = {model: model_pin(runner, runner.model_metadata(model), baseline, model, server_version)
                    for model in PROFILES}
    except runner.RunnerFailure as error:
        raise Rejected(str(error)) from error
    registration = series_dir / "runtime-registration.json"
    json_once(registration, {"schema": PREFIX + "runtime-registration.v1",
                             "server_version": server_version, "models": pins})
    json_once(series_dir / "source-manifest.json", manifest)
    json_once(series_dir / "series.json", {
        "schema": PREFIX + "series.v3",
        "case_id": CASE, "variant": VARIANT, "server_version": server_version,
        "intake_mode": "bound_source",
        "candidate_unpaired": True, "p0_server_version": baseline["ollama_version"],
        "p0_input_set_sha256": P0_INPUT_SET, "p0_freeze_sha256": P0_FREEZE,
        "source_manifest_sha256": sha(small(series_dir / "source-manifest.json")),
        "runtime_registration_sha256": sha(small(registration)),
        "worker": frozen.relative_to(ROOT).as_posix(), "worker_sha256": worker_hash,
        "worker_test": NATIVE_TEST, "native_retries": 0,
        "slots": [{"attempt_id": slot, "model": model,
                   "context_limit": PROFILES[model][0], "output_limit": PROFILES[model][1]}
                  for slot, model in SLOTS],
    })


def registered(series_dir):
    series = read_json(series_dir / "series.json")
    require(series.get("schema") in (PREFIX + "series.v1", PREFIX + "series.v2", PREFIX + "series.v3")
            and (series.get("schema") != PREFIX + "series.v3" or series.get("intake_mode") == "bound_source")
            and series.get("case_id") == CASE and series.get("variant") == VARIANT
            and series.get("candidate_unpaired") is True
            and series.get("p0_input_set_sha256") == P0_INPUT_SET
            and series.get("p0_freeze_sha256") == P0_FREEZE
            and series.get("worker_test") == NATIVE_TEST and series.get("native_retries") == 0
            and isinstance(series.get("worker_sha256"), str) and HEX.fullmatch(series["worker_sha256"])
            and series.get("slots") == [{"attempt_id": slot, "model": model,
                                         "context_limit": PROFILES[model][0], "output_limit": PROFILES[model][1]}
                                        for slot, model in SLOTS], "invalid_registered_series")
    return series


def runtime_pins(series_dir, series):
    require(series["schema"] in (PREFIX + "series.v2", PREFIX + "series.v3"), "runtime_registration_required")
    raw = small(series_dir / "runtime-registration.json")
    require(sha(raw) == series.get("runtime_registration_sha256"), "runtime_registration_changed")
    registration = parse(raw)
    models = registration.get("models")
    require(registration.get("schema") == PREFIX + "runtime-registration.v1"
            and registration.get("server_version") == series["server_version"]
            and isinstance(models, dict) and set(models) == set(PROFILES), "invalid_runtime_registration")
    for tag, pin in models.items():
        require(isinstance(pin, dict) and pin.get("tag") == tag
                and (pin.get("context_limit"), pin.get("output_limit")) == PROFILES[tag]
                and pin.get("parameter_match") in ("frozen_raw", "canonical_numeric_only")
                and all(isinstance(pin.get(key), str) and HEX.fullmatch(pin[key]) for key in MODEL_HASHES),
                "invalid_model_registration")
    return models


def inventory(root, initial, *, allow_git=True):
    """A01 contains exactly these regular source, manifest and test files."""
    require(not linked(root) and root.is_dir(), "worktree_missing")
    paths = set(initial)
    files = {}
    root_entries = {"pyproject.toml", "subject.py", "tests"}
    if allow_git:
        root_entries.add(".git")
    for directory, expected in ((root, root_entries),
                                (root / "tests", {"__init__.py", "test_behavior.py"})):
        require(not linked(directory) and directory.is_dir(), "worktree_reparse")
        with os.scandir(directory) as entries:
            for entry in entries:
                require(entry.name in expected, "unapproved_path")
                path = Path(entry.path)
                require(not linked(path), "worktree_reparse")
                relative = path.relative_to(root).as_posix()
                if relative in paths:
                    files[relative] = small(path)
                else:
                    require(path.is_dir(), "unapproved_path_type")
    require(set(files) == paths, "required_file_missing")
    hashes = {name: sha(raw) for name, raw in files.items()}
    require(all(hashes[name] == initial[name] for name in paths - {"subject.py"}),
            "protected_bytes_changed")
    return files, hashes


def copy_files(root, files):
    root.mkdir(exist_ok=False)
    for name, content in files.items():
        destination = root / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        write_once(destination, content)


def frozen_case(copied):
    pinned, = [item for item in read_json(copied / "suite.json")["cases"]
               if (item["case_id"], item["variant"]) == (CASE, VARIANT)]
    return pinned


def prepare(root, cases, copied, slot, worker_sha256):
    definition = cases.materialize(CASE, VARIANT, root)
    pinned = frozen_case(copied)
    initial = pinned["initial_file_sha256"]
    expected = {key: value for key, value in pinned.items()
                if key not in ("initial_file_sha256", "intervention_file_sha256")}
    require(definition == expected and definition["allowed_write_paths"] == ["subject.py"]
            and definition["allowed_processes"] == [{"argv": ARGV, "cwd": ".", "timeout_seconds": 30}]
            and not definition["allowed_network_requests"] and not definition["process_effect_paths"],
            "frozen_a01_contract_changed")
    files, hashes = inventory(root / "worktree", initial)
    require(hashes == initial, "frozen_initial_inventory_changed")
    copy_files(root / "initial-tree", files)
    launch = {
        "schema": PREFIX + "launch.v1", **slot, "case_id": CASE, "variant": VARIANT,
        "task_timeout_seconds": TASK_TIMEOUT, "probe_timeout_seconds": PROBE_TIMEOUT,
        "outer_timeout_seconds": OUTER_TIMEOUT, "request_sha256": sha(small(root / "request.json")),
        "original_sha256": sha(definition["objective"].encode("utf-8")), "worker_sha256": worker_sha256,
    }
    json_once(root / "launch.json", launch)
    identity = {"attempt_id": slot["attempt_id"], "launch_sha256": sha(small(root / "launch.json")),
                "request_sha256": launch["request_sha256"]}
    return initial, identity


def numeric_parameters(raw):
    require(isinstance(raw, str) and raw.isascii() and len(raw) <= 16 * 1024,
            "p5_parameters_not_strictly_numeric")
    parsed = {}
    for line in raw.replace("\r\n", "\n").split("\n"):
        if not line.strip(" \t"):
            continue
        match = NUMERIC_PARAMETER.fullmatch(line)
        require(match is not None and match[1] not in parsed, "p5_parameters_not_strictly_numeric")
        parsed[match[1]] = Decimal(match[2])
    return parsed


def model_pin(runner, observed, baseline, model, server):
    expected = baseline["models"][model]
    hashes = {key: observed.get(key) for key in MODEL_HASHES}
    require(observed.get("tag") == model and observed.get("server_version") == server
            and PROFILES[model] == (expected["context_limit"], expected["output_limit"])
            and all(isinstance(value, str) and HEX.fullmatch(value) for value in hashes.values())
            and all(hashes[key] == expected[key] for key in ("digest", "template_sha256")),
            "p5_model_identity_mismatch")
    match = "frozen_raw"
    if hashes["parameters_sha256"] != expected["parameters_sha256"]:
        require(server != baseline["ollama_version"], "p5_parameter_hash_changed_without_server_drift")
        # The frozen helper reports selected numeric keys only. Inspect the complete raw list here.
        show = runner._metadata_json("/api/show", {"model": model, "verbose": False})
        require(isinstance(show, dict), "p5_parameter_observation_changed")
        current = show.get("parameters")
        frozen = expected["tag_parameters"]
        require(isinstance(current, str) and sha(current.encode()) == hashes["parameters_sha256"]
                and sha(frozen.encode()) == expected["parameters_sha256"]
                and sha(str(show.get("template") or "").encode()) == hashes["template_sha256"],
                "p5_parameter_observation_changed")
        require(numeric_parameters(current) == numeric_parameters(frozen), "p5_numeric_parameters_changed")
        match = "canonical_numeric_only"
    return {"tag": model, "context_limit": PROFILES[model][0], "output_limit": PROFILES[model][1],
            **hashes, "parameter_match": match}


def loaded_context(observed, pin):
    loaded = observed.get("selected_model_loaded")
    return isinstance(loaded, list) and len(loaded) == 1 and isinstance(loaded[0], dict) and all(
        loaded[0].get(field) == value for field, value in
        (("name", pin["tag"]), ("digest", pin["digest"]), ("context_length", pin["context_limit"]))
    )


def positive_int(value):
    return type(value) is int and value > 0


def native_identity(receipt, identity, name):
    return receipt.get("schema") == PREFIX + name + ".v1" and all(
        receipt.get(key) == value for key, value in identity.items()
    )


def valid_dispatches(dispatches, context, output):
    expected_initial = ["plan", "edit"]
    if not isinstance(dispatches, list) or len(dispatches) not in (2, 3):
        return False
    body_limit = context - output - (context + 15) // 16 - 128
    initial = []
    stopped = []
    repair_seen = False
    seen_wire = set()
    for index, wire in enumerate(dispatches):
        if not isinstance(wire, dict) or set(wire) != WIRE_KEYS:
            return False
        if (wire["phase"] not in ("plan", "edit")
                or wire["kind"] not in ("initial", "structural_repair")
                or not isinstance(wire["wire_blake3"], str) or not HEX.fullmatch(wire["wire_blake3"])
                or not positive_int(wire["request_bytes"]) or wire["request_bytes"] > body_limit
                or wire["num_ctx"] != context or wire["num_predict"] != output or wire["think"] is not False
                or wire["finish"] not in ("stop", "output_limit")
                or not positive_int(wire["prompt_tokens"]) or not positive_int(wire["output_tokens"])
                or wire["prompt_tokens"] > wire["request_bytes"] + (context + 15) // 16 + 128
                or wire["output_tokens"] > output or wire["wire_blake3"] in seen_wire):
            return False
        seen_wire.add(wire["wire_blake3"])
        if wire["kind"] == "initial":
            initial.append(wire["phase"])
        else:
            if (repair_seen or index == 0 or dispatches[index - 1]["kind"] != "initial"
                    or dispatches[index - 1]["phase"] != wire["phase"]):
                return False
            repair_seen = True
        if wire["finish"] == "stop" and wire["phase"] not in stopped:
            stopped.append(wire["phase"])
    return initial == expected_initial and stopped == expected_initial


def valid_native(receipt, context, output):
    test = receipt.get("product_test")
    reopen = receipt.get("reopen")
    intake = receipt.get("bound_intake")
    return (receipt.get("run_state") == "Done" and receipt.get("formal_completed") is True
            and receipt.get("source_controller") is True and receipt.get("capability_probe_verified") is True
            and receipt.get("effect_scope_violations") == []
            and isinstance(intake, dict) and intake.get("original_bytes") == 325
            and intake.get("source_complete") is True and type(intake.get("provider_calls")) is int
            and intake["provider_calls"] == 0 and positive_int(intake.get("captured_at_millis"))
            and intake.get("absolute_deadline_millis") == intake["captured_at_millis"] + TASK_TIMEOUT * 1_000
            and valid_dispatches(receipt.get("phase_dispatches"), context, output)
            and isinstance(test, dict) and test.get("argv") == ARGV and test.get("cwd") == "."
            and positive_int(test.get("timeout_seconds")) and test["timeout_seconds"] <= 30
            and type(test.get("test_count")) is int and test["test_count"] == 1
            and test.get("outcome") == "passed" and test.get("snapshot_current") is True
            and test.get("typed_test_evidence") is True and test.get("static_source_proof") is True
            and isinstance(test.get("source_sha256"), str) and HEX.fullmatch(test["source_sha256"])
            and isinstance(reopen, dict) and reopen.get("run_state") == "Done"
            and reopen.get("current_evidence_bound") is True)


def run_oracle(runner, copied, root, initial, source_proof):
    try:
        files, hashes = inventory(root / "worktree", initial)
    except Rejected as error:
        return {"correct": False, "failure": str(error), "scope_violation": str(error) != "required_file_missing"}
    except OSError:
        return {"correct": None, "failure": "worktree_unreadable", "scope_violation": False}
    copy_files(root / "final-tree", files)
    if source_proof != hashes["subject.py"]:
        return {"correct": None, "failure": "current_static_source_proof_missing",
                "scope_violation": False, "final_file_sha256": hashes}
    copy_files(root / "oracle-worktree", files)
    result, stdout, _ = runner.run_owned_process(
        [sys.executable, "-B", copied / "oracle.py", "--case", CASE, "--variant", VARIANT,
         "--worktree", root / "oracle-worktree"],
        cwd=copied, timeout=120, env=clean_environment(), capture_limit=256 * 1024,
        terminate_on_output_limit=True,
    )
    json_once(root / "owned-oracle.json", result)
    if not process_ok(result):
        return {"correct": None, "failure": "oracle_process_failed", "scope_violation": False}
    verdict = parse(stdout)
    checks = verdict.get("checks")
    require(verdict.get("schema") == "a3.local-agent-eval.v1" and verdict.get("case_id") == CASE
            and verdict.get("variant") == VARIANT and isinstance(checks, list) and checks
            and all(isinstance(check, dict) and type(check.get("passed")) is bool for check in checks)
            and verdict.get("manual_review_required") is False and verdict.get("environment_failure") is None
            and verdict.get("product_verification_imported") is False
            and verdict.get("passed") is all(check["passed"] for check in checks), "invalid_frozen_oracle")
    json_once(root / "oracle.json", verdict)
    return {"correct": verdict["passed"] and hashes["subject.py"] != initial["subject.py"],
            "failure": None, "scope_violation": False, "final_file_sha256": hashes}


def classify(owned, start, final, identity, independent, metadata_ok, slot):
    started = native_identity(start, identity, "native-start")
    ended = native_identity(final, identity, "native-final")
    done = ended and final.get("run_state") == "Done" and final.get("formal_completed") is True
    scope = independent.get("scope_violation") is True or (
        ended and isinstance(final.get("effect_scope_violations"), list)
        and bool(final["effect_scope_violations"])
    )
    correct = independent.get("correct")
    false_done = done and (correct is False or scope)
    dispatches = final.get("phase_dispatches")
    actual_generation = metadata_ok and started and ended and isinstance(dispatches, list) and any(
        isinstance(wire, dict) and set(wire) == WIRE_KEYS
        and wire["num_ctx"] == slot["context_limit"] and wire["num_predict"] == slot["output_limit"]
        and wire["think"] is False and positive_int(wire["prompt_tokens"])
        and positive_int(wire["output_tokens"]) for wire in dispatches
    )
    native_ok = ended and valid_native(final, slot["context_limit"], slot["output_limit"])
    if owned.get("timed_out"):
        status = "timeout"
    elif owned.get("interrupted"):
        status = "interrupted"
    elif not owned or not started or not ended or not metadata_ok:
        status = "harness_failure"
    elif scope or false_done:
        status = "false_done" if false_done else "scope_violation"
    elif not done and actual_generation:
        status = "product_incomplete"
    elif not done or not process_ok(owned):
        status = "harness_failure"
    elif not native_ok:
        status = "false_done_unverified"
    elif correct is None:
        status = "oracle_unavailable"
    else:
        status = "passed"
    return {"status": status, "formal_done": done, "independently_correct": correct,
            "false_done": false_done,
            "false_done_pending": done and not false_done and (correct is None or not native_ok),
            "security_violation": scope, "native_proof": native_ok,
            "actual_generation_observed": actual_generation}


def optional_receipt(root, name):
    try:
        return read_json(root / name)
    except (OSError, ValueError, Rejected):
        return {}


def run_slot(series_dir, slot_id):
    series = registered(series_dir)
    require(series["schema"] == PREFIX + "series.v3", "bound_intake_registration_required")
    pins = runtime_pins(series_dir, series)
    cases, runner, baseline, copied = p0_inputs(series_dir)
    manifest = series_dir / "source-manifest.json"
    require(sha(small(manifest)) == series["source_manifest_sha256"]
            and read_json(manifest) == source_manifest(runner), "candidate_sources_changed")
    worker = (ROOT / series["worker"]).resolve(strict=True)
    expected_worker = ROOT / "target/plan22-frozen" / series["worker_sha256"]
    require(worker.parent == expected_worker and worker.name in ("worker", "worker.exe")
            and runner.file_sha256(worker) == series["worker_sha256"], "candidate_worker_changed")
    require_worker_test(runner, worker)
    position = next(i for i, slot in enumerate(series["slots"]) if slot["attempt_id"] == slot_id)
    slot = series["slots"][position]
    root = series_dir / slot_id
    with runner.GpuLock(Path(tempfile.gettempdir()) / "a3-plan22-ollama-gpu.lock"):
        require(all((series_dir / prior["attempt_id"]).is_dir()
                    for prior in series["slots"][:position]), "slot_out_of_order")
        root.mkdir(exist_ok=False)
        initial = identity = None
        owned = {}
        metadata_ok = False
        failure = None
        independent = {"correct": None, "scope_violation": False}
        model = slot["model"]
        server = series["server_version"]
        try:
            initial, identity = prepare(root, cases, copied, slot, series["worker_sha256"])
            runner._git_initialize(root / "worktree")
            before = runner.model_metadata(model)
            json_once(root / "runtime-before.json", {
                "model": before, "host": runner.runtime_metadata(ROOT),
                "worker_sha256": series["worker_sha256"],
            })
            require(model_pin(runner, before, baseline, model, server) == pins[model],
                    "p5_runtime_pin_mismatch")
            env = clean_environment()
            env["A3_PLAN22_P5_RUN_DIR"] = str(root)
            owned, _, _ = runner.run_owned_process(
                [worker, NATIVE_TEST, "--ignored", "--exact", "--nocapture", "--test-threads=1"],
                cwd=ROOT, timeout=OUTER_TIMEOUT, env=env, capture_limit=16 * 1024,
                terminate_on_output_limit=True,
            )
            json_once(root / "owned-worker.json", owned)
            after = runner.model_metadata(model)
            json_once(root / "runtime-after.json", after)
            metadata_ok = (model_pin(runner, after, baseline, model, server) == pins[model]
                           and loaded_context(after, pins[model]))
            require(metadata_ok, "p5_runtime_changed_or_context_unobserved")
        except (Rejected, runner.RunnerFailure) as error:
            failure = str(error)
        except (OSError, ValueError, KeyError, TypeError):
            failure = "attempt_infrastructure_error"
        except KeyboardInterrupt:
            owned["interrupted"] = True
            failure = "operator_interrupted"
        start = optional_receipt(root, "native-start.json")
        final = optional_receipt(root, "native-final.json")
        if initial is not None:
            try:
                test = final.get("product_test")
                source_proof = None
                if (native_identity(start, identity, "native-start")
                        and native_identity(final, identity, "native-final")
                        and final.get("effect_scope_violations") == []
                        and isinstance(test, dict) and test.get("static_source_proof") is True):
                    source_proof = test.get("source_sha256")
                independent = run_oracle(runner, copied, root, initial, source_proof)
            except (OSError, ValueError, TypeError, Rejected, runner.RunnerFailure):
                independent["failure"] = "oracle_infrastructure_error"
            except KeyboardInterrupt:
                independent["failure"] = "oracle_interrupted"
        if identity is None:
            identity = {"attempt_id": slot_id, "launch_sha256": "missing", "request_sha256": "missing"}
        try:
            metadata_ok = (metadata_ok and sha(small(root / "request.json")) == identity["request_sha256"]
                           and sha(small(root / "launch.json")) == identity["launch_sha256"])
        except (OSError, Rejected):
            metadata_ok = False
        summary = classify(owned, start, final, identity, independent, metadata_ok, slot)
        result = {"schema": PREFIX + "result.v1", **slot, **summary, "failure": failure,
                  "oracle": independent,
                  "artifact_sha256": {name: sha(small(root / name)) for name in
                                      sorted(PASS_ARTIFACTS)
                                      if (root / name).is_file() and not linked(root / name)
                                      and (root / name).stat().st_size <= MAX_FILE_BYTES}}
        json_once(root / "result.json", result)
    return result


def status(series_dir):
    series = registered(series_dir)
    if series["schema"] != PREFIX + "series.v1":
        runtime_pins(series_dir, series)
    attempts = []
    pinned_initial = None
    for slot in series["slots"]:
        root = series_dir / slot["attempt_id"]
        result = optional_receipt(root, "result.json")
        summary_keys = ("status", "false_done", "false_done_pending", "security_violation",
                        "actual_generation_observed")
        try:
            valid = (result.get("schema") == PREFIX + "result.v1"
                     and result.get("attempt_id") == slot["attempt_id"]
                     and result.get("model") == slot["model"]
                     and (result.get("status") != "passed" or PASS_ARTIFACTS <= result["artifact_sha256"].keys())
                     and all(Path(name).name == name and sha(small(root / name)) == digest
                             for name, digest in result["artifact_sha256"].items()
                             ) and all(key in result for key in summary_keys))
            if valid and result["status"] == "passed":
                if pinned_initial is None:
                    _, _, _, copied = p0_inputs(series_dir)
                    pinned_initial = frozen_case(copied)["initial_file_sha256"]
                _, initial = inventory(root / "initial-tree", pinned_initial, allow_git=False)
                _, final = inventory(root / "final-tree", pinned_initial, allow_git=False)
                valid = initial == pinned_initial and final == result["oracle"]["final_file_sha256"]
        except (OSError, ValueError, KeyError, TypeError, AttributeError, Rejected):
            valid = False
        attempts.append({**slot, **({key: result[key] for key in summary_keys} if valid else
                                    {"status": "unstarted" if not root.exists() else "incomplete",
                                     "false_done": None, "false_done_pending": None,
                                     "security_violation": None, "actual_generation_observed": False})})
    passed = {tag: sum(row["model"] == tag and row["status"] == "passed" for row in attempts)
              for tag in PROFILES}
    gate = (all(row["actual_generation_observed"] is True and row["false_done"] is False
                and row["false_done_pending"] is False and row["security_violation"] is False
                for row in attempts)
            and passed[ORNITH] >= 2 and passed[QWEN] >= 2 and passed[FOUR_K] == 1)
    return {"passed": gate, "planned": len(SLOTS), "correct_by_profile": passed, "slots": attempts}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="operation", required=True)
    for operation in ("register", "attempt", "status"):
        command = commands.add_parser(operation)
        command.add_argument("--series-dir", type=Path, required=True)
        if operation == "register":
            command.add_argument("--worker", type=Path, required=True)
            command.add_argument("--server-version", required=True)
        if operation == "attempt":
            command.add_argument("--slot", choices=[slot for slot, _ in SLOTS], required=True)
    args = parser.parse_args()
    try:
        directory = args.series_dir.absolute()
        require(directory.parent.resolve() == RUNS and directory.resolve().parent == RUNS
                and re.fullmatch(r"[a-z0-9][a-z0-9-]{0,63}", directory.name),
                "invalid_direct_series_directory")
        if args.operation == "register":
            require(not RUNS.exists() or not linked(RUNS), "linked_series_root")
            RUNS.mkdir(exist_ok=True)
            register(directory, args.worker, args.server_version)
            print(json.dumps({"registered": str(directory), "planned": len(SLOTS)}))
            return 0
        require(directory.is_dir() and not linked(directory), "invalid_series_directory")
        result = status(directory) if args.operation == "status" else run_slot(directory, args.slot)
        print(json.dumps(result, ensure_ascii=False))
        return 0 if result.get("passed", result.get("status") == "passed") else 1
    except (OSError, ValueError, KeyError, TypeError, Rejected) as error:
        print(json.dumps({"status": "supervisor_failure",
                          "failure": str(error) if isinstance(error, Rejected) else "invalid_measurement_io"}))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())

