#!/usr/bin/env python3
"""Run one explicitly selected local P4 Intake component under the shared GPU lock."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import tempfile

from run_a10_edit import (
    P0_INPUT_SET, P0_PROFILE, PROFILE, TAG, Rejected, load_frozen, read_json, sha,
    verify_provider, write_once,
)

TEST = (
    "plan22_goal_contract_tests::existing_test_tests::changed_tests::"
    "middle_edit_tests::p4_live_edit::live_intake::live_ollama_4k_intake_starts_budget_bound_run"
)
SOURCE_BYTES = 35
TIMEOUT_SECONDS = 480
ALTERNATE_EDIT_TAGS = ("bonsai-2:27b", "ornith-1.5:9b")
EDIT_BRANCH_MODEL_ENV = "A3_PLAN22_P4_EDIT_BRANCH_MODEL"


def positive_int(value):
    return type(value) is int and value > 0


def validate_native_execution(native, owned, before, after):
    if (owned["exit_code"] != 0 or owned["timed_out"] or owned["output_limit"]
            or owned["interrupted"] or owned["supervisor_error"] is not None):
        raise Rejected("native_attempt_failed")
    loaded = [model for model in after["selected_model_loaded"]
              if model.get("context_length") == PROFILE[0] and model.get("digest") == before["digest"]]
    if (len(loaded) != 1 or not before["server_version"]
            or after["server_version"] != before["server_version"]):
        raise Rejected("effective_context_or_server_unverified")
    if native.is_symlink() or not (native / "native-start.json").is_file():
        raise Rejected("native_test_not_observed")


def validate_attempt(native, owned, before, after):
    validate_native_execution(native, owned, before, after)
    start = read_json(native / "native-start.json", 8_192)
    if start != {"version": 1, "phase": "intake", "tag": TAG}:
        raise Rejected("native_test_identity_unverified")
    final_path = native / "native-final.json"
    if not final_path.is_file():
        raise Rejected("native_completion_missing")
    final = read_json(final_path, 8_192)
    if final["version"] != 1 or final["success"] is not True or final["goal_done"] is not False:
        raise Rejected("native_completion_missing")
    probe = read_json(native / "probe-result.json", 8_192)
    if probe["verified"] is not True or probe["invocations"] != 1 or probe["failure"] is not None:
        raise Rejected("capability_probe_unverified")
    intent = read_json(native / "intake-intent.json", 8_192)
    wire = intent["wire"]
    reserved_prompt = intent["charged_prompt_tokens"]
    reserved_output = intent["charged_output_tokens"]
    if (intent["capacity_error"] is not None or intent["source_bytes"] != SOURCE_BYTES
            or (wire["num_ctx"], wire["num_predict"]) != PROFILE or wire["think"] is not False
            or not positive_int(wire["body_bytes"]) or not positive_int(reserved_prompt)
            or reserved_output != PROFILE[1] or reserved_prompt + reserved_output > PROFILE[0]):
        raise Rejected("intake_wire_unverified")
    result = read_json(native / "intake-result.json", 8_192)
    usage = result["charged_usage"]
    observed_prompt = result["reported_prompt_tokens"]
    observed_output = result["reported_output_tokens"]
    charges = (usage["calls"], usage["repairs"], usage["unobserved_calls"], usage["unknown_calls"])
    if (result["observed_review"] is not True or result["failure"] is not None
            or charges != (1, 0, 0, 0)
            or not positive_int(observed_prompt) or observed_prompt > reserved_prompt
            or not positive_int(observed_output) or observed_output > reserved_output
            or usage["prompt_tokens"] != reserved_prompt or usage["output_tokens"] != reserved_output):
        raise Rejected("settled_intake_usage_unverified")
    if read_json(native / "intake-review-check.json", 8_192) != {
        "finding_count": 1, "question_count": 0, "findings": [{"kind": "must", "start": 0, "end": SOURCE_BYTES}],
    }:
        raise Rejected("literal_intake_review_unverified")
    reopened = read_json(native / "intake-reopen.json", 8_192)
    started, remaining = reopened["run_started_millis"], reopened["run_remaining_millis"]
    if (reopened["run_started"] is not True or reopened["run_state"] != "intake"
            or reopened["original_bytes"] != SOURCE_BYTES or reopened["review_revision"] != 1
            or reopened["plan_model_calls"] != 0 or reopened["goal_done"] is not False
            or reopened["protected_worktree_unchanged"] is not True or reopened["usage"] != usage
            or not positive_int(started) or not positive_int(remaining)
            or started + remaining != reopened["original_deadline_millis"]):
        raise Rejected("reopened_intake_run_unverified")


def run_owned_native(
    repository, root, worker_source, runner, baseline, *, test, output_env, timeout,
    check_before=None, model_tag=TAG,
):
    if model_tag not in (TAG, *ALTERNATE_EDIT_TAGS) or (
        model_tag != TAG and output_env != "A3_PLAN22_P4_EDIT_BRANCH_RUN_DIR"
    ):
        raise Rejected("unsupported_native_model")
    native = root / "native"
    worker, digest = runner.freeze_worker(repository, worker_source.resolve(strict=True))
    with runner.GpuLock(Path(tempfile.gettempdir()) / "a3-plan22-ollama-gpu.lock"):
        before = runner.model_metadata(model_tag)
        write_once(root / "runtime-before.json", {
            "worker_sha256": digest, "worker_test": test, "model": before,
            "host": runner.runtime_metadata(repository), "p0_ollama_version": baseline["ollama_version"],
            "same_server_as_p0": before["server_version"] == baseline["ollama_version"],
        })
        if model_tag == TAG:
            verify_provider(before, baseline)
        elif before["tag"] != model_tag or not before["server_version"]:
            raise Rejected("model_identity_unverified")
        if check_before is not None:
            check_before(before)
        env = dict(os.environ)
        env.pop("A3_NATIVE_LIBSQL_UNIT_TEST", None)
        env.pop("A3_NATIVE_LIBSQL_SUCCESS_MARKER", None)
        env.pop(EDIT_BRANCH_MODEL_ENV, None)
        if model_tag != TAG:
            env[EDIT_BRANCH_MODEL_ENV] = model_tag
        env[output_env] = str(native)
        owned, _, _ = runner.run_owned_process(
            [str(worker), test, "--ignored", "--exact", "--nocapture", "--test-threads=1"],
            cwd=repository, timeout=timeout, env=env, capture_limit=16 * 1024,
            terminate_on_output_limit=True,
        )
        write_once(root / "owned-worker.json", owned)
        after = runner.model_metadata(model_tag)
        write_once(root / "runtime-after.json", after)
        if model_tag == TAG:
            verify_provider(after, baseline)
        elif any(before[key] != after[key] for key in (
            "tag", "digest", "template_sha256", "parameters_sha256", "system_sha256",
            "weights_blob_sha256", "server_version",
        )):
            raise Rejected("model_identity_changed")
    return native, owned, before, after


def run_once(repository, root, worker_source, runner, baseline):
    execution = run_owned_native(
        repository, root, worker_source, runner, baseline,
        test=TEST, output_env="A3_PLAN22_P4_INTAKE_RUN_DIR", timeout=TIMEOUT_SECONDS,
    )
    validate_attempt(*execution)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--worker", required=True, type=Path)
    parser.add_argument("--run-dir", required=True, type=Path)
    args = parser.parse_args()
    repository = args.repository.resolve(strict=True)
    parent = (repository / "target" / "plan22-p4-runs").resolve()
    root = args.run_dir.resolve()
    if not parent.is_relative_to(repository) or root.parent != parent or root.exists():
        parser.error("use a fresh direct directory under target/plan22-p4-runs")
    parent.mkdir(parents=True, exist_ok=True)
    root.mkdir()
    write_once(root / "launch.json", {
        "schema": "a3.plan22.p4-intake-launch.v1", "launcher_sha256": sha(Path(__file__).read_bytes()),
        "shared_driver_sha256": sha(Path(__file__).with_name("run_a10_edit.py").read_bytes()),
        "p0_input_set_sha256": P0_INPUT_SET, "p0_profile_sha256": P0_PROFILE,
        "worker_test": TEST, "outer_timeout_seconds": TIMEOUT_SECONDS, "tag": TAG,
    })
    runner = None
    failure = None
    try:
        _, runner, baseline, _ = load_frozen(repository)
        run_once(repository, root, args.worker, runner, baseline)
    except Rejected as error:
        failure = str(error)
    except (OSError, ValueError, KeyError, TypeError, KeyboardInterrupt) as error:
        failure = type(error).__name__
    except Exception as error:
        if runner is None or not isinstance(error, runner.RunnerFailure):
            raise
        failure = str(error)
    result = {
        "passed": failure is None, "failure": failure, "full_p4_gate": False,
        "scope": "one local intake to budget-bound run",
    }
    write_once(root / "result.json", result)
    print(json.dumps(result))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
