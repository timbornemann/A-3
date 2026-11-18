#!/usr/bin/env python3
"""Run one retained local 4k Intake and initial Plan component."""
from __future__ import annotations

import argparse
import json
from pathlib import Path

from run_a10_edit import (
    P0_INPUT_SET, P0_PROFILE, TAG, Rejected, load_frozen, read_json, read_small, sha, write_once,
)
from run_edit_branches import (
    P0_HELPERS, check_intent, check_loaded_context, check_usage, check_warm_context,
)
from run_intake import positive_int, run_owned_native, validate_native_execution

TEST = (
    "plan22_goal_contract_tests::existing_test_tests::changed_tests::middle_edit_tests::"
    "p4_live_edit::live_intake::live_initial_plan::live_ollama_4k_initial_plan_starts_budget_bound_run"
)
TIMEOUT_SECONDS = 600


def validate_attempt(native, owned, before, after):
    validate_native_execution(native, owned, before, after)
    check_loaded_context(after)

    def receipt(name):
        return read_json(native / name, 8_192)

    start = receipt("native-start.json")
    if type(start["version"]) is not int or start != {"version": 1, "phase": "initial_plan", "tag": TAG}:
        raise Rejected("native_test_identity_unverified")
    final = receipt("native-final.json")
    if final["version"] != 1 or final["success"] is not True or final["goal_done"] is not False:
        raise Rejected("native_completion_missing")
    probe = receipt("probe-result.json")
    if probe["verified"] is not True or probe["invocations"] != 1 or probe["failure"] is not None:
        raise Rejected("capability_probe_unverified")

    intake_intent = receipt("intake-intent.json")
    plan_intent = receipt("plan-intent.json")
    check_intent(intake_intent)
    check_intent(plan_intent)
    dispatch = receipt("plan-dispatch.json")
    if dispatch["wire"] != plan_intent["wire"]:
        raise Rejected("plan_dispatch_binding_unverified")
    check_usage(dispatch["before_io"], calls=1, pending=1)
    intake = receipt("intake-result.json")
    plan = receipt("plan-result.json")
    if intake["observed_review"] is not True or plan["outcome"] != "materialized":
        raise Rejected("intake_or_plan_outcome_unverified")
    for result in (intake, plan):
        if result["failure"] is not None or not all(
            positive_int(result[field]) for field in ("provider_prompt_tokens", "provider_output_tokens")
        ):
            raise Rejected("phase_observation_unverified")
        check_usage(result["durable_usage"], calls=1)
    review = receipt("intake-review-check.json")
    if review["must_with_existing_test"] is not True or review["exact_original_quote"] is not True:
        raise Rejected("literal_intake_review_unverified")
    ledger = receipt("plan-ledger-check.json")
    if (not all(ledger[field] is True for field in (
            "selected_change_and_test", "test_depends_on_change", "distinct_outcomes", "change_ready", "test_pending",
        )) or ledger["test_minimum_cases"] != 2):
        raise Rejected("initial_plan_ledger_unverified")

    reopened = receipt("plan-reopen.json")
    linked = reopened["linked_usage"]
    check_usage(linked, calls=2)
    intake_usage, plan_usage = intake["durable_usage"], plan["durable_usage"]
    started, remaining = reopened["run_started_millis"], reopened["run_remaining_millis"]
    if (reopened["goal_done"] is not False or reopened["files_unchanged"] is not True
            or not positive_int(reopened["original_bytes"])
            or reopened["test_proof_selected"] is not True or reopened["test_processes_run"] != 0
            or reopened["mutations"] != 0 or (reopened["ready_steps"], reopened["pending_steps"]) != (1, 1)
            or reopened["intake_usage"] != intake_usage or reopened["plan_usage"] != plan_usage
            or any(linked[field] != intake_usage[field] + plan_usage[field] for field in ("prompt_tokens", "output_tokens"))
            or not positive_int(started) or not positive_int(remaining)
            or started + remaining != reopened["original_deadline_millis"]):
        raise Rejected("reopened_initial_plan_unverified")


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
    runner = None
    failure = None
    try:
        _, runner, baseline, freeze = load_frozen(repository)
        launcher = Path(__file__)
        helpers = {
            f"fixtures/plan22-p4/{name}": sha(read_small(launcher.with_name(name), 2 << 20))
            for name in ("run_a10_edit.py", "run_intake.py", "run_edit_branches.py")
        }
        for name in P0_HELPERS:
            path = f"fixtures/local-agent-eval-v1/{name}"
            helpers[path] = freeze["inputs_sha256"][path]
        write_once(root / "launch.json", {
            "schema": "a3.plan22.p4-initial-plan-launch.v1",
            "launcher_sha256": sha(read_small(launcher, 2 << 20)), "helper_sha256": helpers,
            "p0_input_set_sha256": P0_INPUT_SET, "p0_profile_sha256": P0_PROFILE,
            "worker_test": TEST, "outer_timeout_seconds": TIMEOUT_SECONDS, "tag": TAG,
        })
        execution = run_owned_native(
            repository, root, args.worker, runner, baseline,
            test=TEST, output_env="A3_PLAN22_P4_PLAN_RUN_DIR", timeout=TIMEOUT_SECONDS,
            check_before=check_warm_context,
        )
        validate_attempt(*execution)
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
        "scope": "local intake and initial plan to budget-bound run",
    }
    write_once(root / "result.json", result)
    print(json.dumps(result))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
