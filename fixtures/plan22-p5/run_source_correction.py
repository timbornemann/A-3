#!/usr/bin/env python3
"""Run one owned Qwen 4k correction from a controlled native red unittest."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys

SCHEMA = "a3.plan22.p5.controlled-correction.v1"
TEST = (
    "plan22_goal_contract_tests::existing_test_tests::changed_tests::"
    "manifest_bound_test_tests::source_failure_replan_tests::live_correction::"
    "live_ollama_4k_corrects_the_actual_guarded_red_after_reopen"
)
ENV = "A3_PLAN22_P5_CORRECTION_RUN_DIR"
TASK_SECONDS = 360
OUTER_SECONDS = TASK_SECONDS + 120 + 30
HEX = re.compile(r"[a-f0-9]{64}")


def validate(native, owned, before, after, cases, read_json, read_small, check_execution):
    check_execution(native, owned, before, after)
    start = read_json(native / "native-start.json", 8192)
    probe = read_json(native / "probe-result.json", 8192)
    red = read_json(native / "red-reopened.json", 8192)
    intent = read_json(native / "edit-intent.json", 8192)
    result = read_json(native / "edit-result.json", 8192)
    final = read_json(native / "native-final.json", 8192)
    cold = read_json(native / "cold-usage.json", 8192)
    if start != {"schema": SCHEMA, "tag": "qwen38-4k:latest"}:
        return "native_identity_unverified"
    if probe["verified"] is not True or probe["invocations"] != 1 or probe["failure"] is not None:
        return "capability_probe_unverified"
    if any(record.get("schema") != SCHEMA for record in (red, intent, result, final)):
        return "native_schema_unverified"
    if red["verification_failed"] is not True or red["run_id"] != intent["run_id"]:
        return "red_run_binding_unverified"
    if not all(HEX.fullmatch(intent[key]) for key in ("run_id", "wire_blake3", "job_blake3")):
        return "wire_identity_unverified"
    if (intent["num_ctx"], intent["num_predict"], intent["think"]) != (4096, 1024, False):
        return "profile_unverified"
    if (
        type(intent["request_bytes"]) is not int or intent["request_bytes"] <= 0
        or result["wire_blake3"] != intent["wire_blake3"] or result["finish"] != "stop"
        or result["failure"] is not None
        or (result["calls"], result["repairs"], result["unknown_calls"], result["unobserved_calls"]) != (1, 0, 0, 0)
    ):
        return "settled_edit_unverified"
    for field in ("prompt", "output"):
        observed = result[f"{field}_tokens"]
        charged = result[f"{field}_charged"]
        reserved = intent[f"{field}_reserved"]
        if type(observed) is not int or type(charged) is not int or type(reserved) is not int:
            return "edit_usage_unverified"
        if not 0 < observed <= charged == reserved:
            return "edit_usage_unverified"
    if intent["prompt_reserved"] + intent["output_reserved"] > 4096:
        return "edit_budget_unverified"
    if final != {
        "schema": SCHEMA, "source_done_after_cold_reopen": True,
        "controlled_initial_red": True, "full_a01_claim": False,
    }:
        return "reopened_source_done_missing"
    if cold != {
        "schema": SCHEMA, "run_id": red["run_id"], "calls": 1, "repairs": 0,
        "unknown_calls": 0, "unobserved_calls": 0, "prompt_charged": result["prompt_charged"],
        "output_charged": result["output_charged"], "deadline_millis": 360003,
    }:
        return "cold_budget_or_run_unverified"
    worktree = native / "worktree"
    expected = cases.fixture_files("A01", "development")
    actual = {
        entry.relative_to(worktree).as_posix()
        for entry in worktree.rglob("*")
        if ".git" not in entry.relative_to(worktree).parts and not entry.is_dir()
    }
    if actual != set(expected):
        return "worktree_scope_changed"
    for path, body in expected.items():
        if path != "subject.py" and read_small(worktree / path, 8192) != body:
            return "protected_bytes_changed"
    subject = read_small(worktree / "subject.py", 8192)
    if not subject or subject == expected["subject.py"]:
        return "subject_edit_missing"
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, required=True)
    parser.add_argument("--worker", type=Path, required=True)
    parser.add_argument("--run-dir", type=Path, required=True)
    args = parser.parse_args()
    repository = args.repository.resolve(strict=True)
    base = (repository / "target" / "plan22-p5-correction-runs").resolve()
    root = args.run_dir.resolve()
    if not base.is_relative_to(repository) or root.parent != base or root.exists():
        parser.error("use a fresh direct directory under target/plan22-p5-correction-runs")
    sys.dont_write_bytecode = True
    helper_root = repository / "fixtures" / "plan22-p4"
    sys.path.insert(0, str(helper_root))
    from run_a10_edit import Rejected, TAG, load_frozen, read_json, read_small, sha, write_once
    from run_intake import run_owned_native, validate_native_execution

    base.mkdir(parents=True, exist_ok=True)
    root.mkdir()
    (root / "native").mkdir()
    source_root = repository / "crates" / "a3-storage-libsql" / "src"
    sources = [Path(__file__).resolve(), helper_root / "run_intake.py", helper_root / "run_a10_edit.py",
               source_root / "plan22_source_failure_replan_tests.rs",
               source_root / "plan22_p5_live_source_correction.rs"]
    write_once(root / "launch.json", {
        "schema": SCHEMA, "worker_test": TEST, "tag": TAG, "num_ctx": 4096, "num_predict": 1024,
        "task_timeout_millis": TASK_SECONDS * 1000, "outer_timeout_seconds": OUTER_SECONDS,
        "task_model_calls": 1, "controlled_initial_red": True, "full_a01_claim": False,
        "source_sha256": {path.name: sha(read_small(path, 1 << 20)) for path in sources},
    })
    failure = None
    runner = None
    try:
        cases, runner, baseline, _ = load_frozen(repository)
        native, owned, before, after = run_owned_native(
            repository, root, args.worker, runner, baseline, test=TEST, output_env=ENV,
            timeout=OUTER_SECONDS,
        )
        failure = validate(native, owned, before, after, cases, read_json, read_small, validate_native_execution)
    except Rejected as error:
        failure = str(error)
    except (OSError, ValueError, KeyError, TypeError, KeyboardInterrupt) as error:
        failure = type(error).__name__
    except Exception as error:
        if runner is None or not isinstance(error, runner.RunnerFailure):
            raise
        failure = str(error)
    result = {"schema": SCHEMA, "passed": failure is None, "failure": failure, "full_a01_claim": False}
    write_once(root / "result.json", result)
    print(json.dumps(result))
    return 0 if failure is None else 1


if __name__ == "__main__":
    raise SystemExit(main())
