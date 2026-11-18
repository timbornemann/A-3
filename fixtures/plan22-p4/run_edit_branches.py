#!/usr/bin/env python3
"""Run one closed local 4k Edit branch fixture under the shared GPU lock."""
from __future__ import annotations

import argparse
import json
from pathlib import Path

from run_a10_edit import (
    P0_INPUT_SET, P0_PROFILE, PROFILE, TAG, Rejected, load_frozen, read_json, read_small,
    sha, write_once,
)
from run_intake import ALTERNATE_EDIT_TAGS, positive_int, run_owned_native, validate_native_execution

TEST_PREFIX = (
    "plan22_goal_contract_tests::existing_test_tests::changed_tests::"
    "middle_edit_tests::p4_live_edit::live_edit_branches::"
)
CASES = {
    "caller": ("caller", "caller", "live_ollama_4k_edit_requests_actual_caller"),
    "too-large": ("too_large", "too_large", "live_ollama_4k_edit_reports_indivisible_output_too_large"),
    "continuation": ("caller_continuation", "caller", "live_ollama_4k_edit_proposes_cap_after_actual_caller_delivery"),
    "known-caller": ("known_caller", "proposal", "live_ollama_4k_edit_requires_current_caller_before_dispatch"),
}
P0_HELPERS = ("cases.py", "run_baseline.py", "supervisor.py", "order_trace.py")
ORNITH_TAG = "ornith-1.5:9b"
REPAIR_RECEIPTS = ("edit-repair-intent.json", "edit-repair-dispatch.json", "edit-repair-result.json")


def check_loaded_context(model, *, allow_unloaded=False, tag=TAG):
    loaded = model["selected_model_loaded"]
    if model["tag"] != tag or (
        (loaded or not allow_unloaded) and (len(loaded) != 1
            or loaded[0]["name"] != tag or loaded[0]["digest"] != model["digest"]
            or loaded[0]["context_length"] != PROFILE[0])
    ):
        raise Rejected("selected_model_context_unverified")


def check_warm_context(model):
    check_loaded_context(model, allow_unloaded=True)


def check_usage(usage, calls, pending=0, repairs=0):
    counts = (usage["calls"], usage["repairs"], usage["unknown_calls"], usage["unobserved_calls"])
    if any(type(value) is not int for value in counts) or counts != (calls, repairs, 0, pending) or not all(
        positive_int(usage[field]) for field in ("prompt_tokens", "output_tokens")
    ):
        raise Rejected("edit_usage_unverified")


def check_intent(intent):
    wire = intent["wire"]
    if (intent["capacity_error"] is not None or not intent["job_blake3"]
            or (wire["num_ctx"], wire["num_predict"]) != PROFILE
            or wire["think"] is not False or not positive_int(wire["body_bytes"])):
        raise Rejected("edit_wire_unverified")


def validate_truncated_repair(receipt, first, initial_dispatch, initial):
    previous = initial["durable_usage"]
    check_usage(previous, calls=1)
    if (initial["expected"] != "too_large" or initial["actual"] is not None
            or initial["failure"] != "OutputLimit" or initial["dispatches"] != 1
            or initial["finish"] != "OutputLimit"
            or not positive_int(initial["provider_prompt_tokens"])
            or initial["provider_prompt_tokens"] > first["charged_prompt_tokens"]
            or initial["provider_output_tokens"] != PROFILE[1]
            or previous["prompt_tokens"] != first["charged_prompt_tokens"]
            or previous["output_tokens"] != first["charged_output_tokens"]
            or any(initial_dispatch["before_io"][field] != previous[field]
                   for field in ("prompt_tokens", "output_tokens"))):
        raise Rejected("edit_output_limit_unverified")

    intent, dispatch, result = (receipt(name) for name in REPAIR_RECEIPTS)
    check_intent(intent)
    if (intent["reason"] != "output_limit" or intent["job_blake3"] != first["job_blake3"]
            or not first["request_projection_blake3"] or not intent["request_projection_blake3"]
            or intent["request_projection_blake3"] == first["request_projection_blake3"]
            or intent["wire"]["body_blake3"] == first["wire"]["body_blake3"]
            or dispatch["wire"] != intent["wire"]):
        raise Rejected("edit_truncated_repair_binding_unverified")
    charged = dispatch["before_io"]
    usage = result["durable_usage"]
    check_usage(charged, calls=2, pending=1, repairs=1)
    check_usage(usage, calls=2, repairs=1)
    for field in ("prompt_tokens", "output_tokens"):
        allowance = intent["charged_" + field]
        reported = result["provider_" + field]
        if (not positive_int(allowance) or not positive_int(reported) or reported > allowance
                or charged[field] != previous[field] + allowance or usage[field] != charged[field]):
            raise Rejected("edit_truncated_repair_usage_unverified")
    if (result["expected"] != "too_large" or result["actual"] != "too_large"
            or result["failure"] is not None or result["dispatches"] != 1 or result["finish"] != "Stop"):
        raise Rejected("edit_truncated_repair_outcome_unverified")
    return usage


def validate_attempt(case, native, owned, before, after, *, tag=TAG):
    if tag != TAG:
        check_loaded_context(after, tag=tag)
    validate_native_execution(native, owned, before, after)
    if tag == TAG:
        check_loaded_context(after)
    branch, expected, _ = CASES[case]
    caller = case != "too-large"
    continuation = case == "continuation"
    known_caller = case == "known-caller"

    def receipt(name):
        return read_json(native / name, 8_192)

    start = receipt("native-start.json")
    if type(start["version"]) is not int or start != {
        "version": 1, "phase": "edit_branch", "branch": branch, "tag": tag,
    }:
        raise Rejected("native_test_identity_unverified")
    final = receipt("native-final.json")
    if (final["version"] != 1 or final["branch"] != branch
            or final["success"] is not True or final["goal_done"] is not False):
        raise Rejected("native_completion_missing")
    if tag != TAG and receipt("probe-start.json") != {"kind": "capability_probe", "tag": tag}:
        raise Rejected("native_probe_model_unverified")
    probe = receipt("probe-result.json")
    if probe["verified"] is not True or probe["invocations"] != 1 or probe["failure"] is not None:
        raise Rejected("capability_probe_unverified")
    first = receipt("edit-intent.json")
    check_intent(first)
    dispatch = receipt("edit-dispatch.json")
    if first["branch"] != branch or dispatch["wire"] != first["wire"]:
        raise Rejected("edit_dispatch_binding_unverified")
    check_usage(dispatch["before_io"], calls=1, pending=1)
    result = receipt("edit-result.json")
    repaired = case == "too-large" and tag == ORNITH_TAG and result["failure"] == "OutputLimit"
    if repaired:
        usage = validate_truncated_repair(receipt, first, dispatch, result)
    else:
        if any((native / name).exists() for name in REPAIR_RECEIPTS):
            raise Rejected("unexpected_edit_repair")
        if (result["expected"] != expected or result["actual"] != expected or result["failure"] is not None
                or result["dispatches"] != 1 or result["finish"] != "Stop"
                or not all(positive_int(result[field]) for field in ("provider_prompt_tokens", "provider_output_tokens"))):
            raise Rejected("edit_branch_outcome_unverified")
        usage = result["durable_usage"]
        check_usage(usage, calls=1)
    if caller:
        acquired = receipt("caller-acquired.json")
        if ((acquired["start"], acquired["end"], acquired["source_bytes"]) != (53, 135, 82)
                or acquired["delivered_to_model"] is not False):
            raise Rejected("caller_acquisition_unverified")
    if known_caller:
        preflight = receipt("edit-known-caller-preflight.json")
        delivery = receipt("edit-known-caller-delivery.json")
        oracle = receipt("edit-known-caller-oracle.json")
        zero_usage = {
            "calls": 0, "repairs": 0, "unknown_calls": 0, "unobserved_calls": 0,
            "prompt_tokens": 0, "output_tokens": 0,
        }
        if (preflight["outcome"] != "need_caller" or type(preflight["dispatches"]) is not int
                or preflight["dispatches"] != 0 or preflight["usage"] != zero_usage
                or any(type(value) is not int for value in preflight["usage"].values())
                or not preflight["job_blake3"] or preflight["job_blake3"] == first["job_blake3"]
                or preflight["source_revision_blake3"] != acquired["revision_blake3"]
                or (delivery["policy_version"], delivery["ordinal"], delivery["new_bytes"],
                    delivery["start"], delivery["end"]) != (1, 1, 82, 53, 135)
                or delivery["revision_blake3"] != acquired["revision_blake3"]
                or delivery["wire_blake3"] != first["wire"]["body_blake3"]
                or oracle["wire_blake3"] != delivery["wire_blake3"]
                or (oracle["caller_start"], oracle["caller_end"], oracle["timeout_cap_milliseconds"])
                    != (53, 135, 30_000)
                or oracle["caller_delivered_to_model"] is not True
                or oracle["selected_function_verified"] is not True or oracle["applied"] is not False):
            raise Rejected("known_caller_delivery_unverified")
    if continuation:
        next_intent = receipt("edit-continuation-intent.json")
        check_intent(next_intent)
        next_result = receipt("edit-continuation-result.json")
        oracle = receipt("edit-continuation-oracle.json")
        if (next_intent["job_blake3"] == first["job_blake3"]
                or next_result["actual"] != "proposal" or next_result["failure"] is not None
                or not all(positive_int(next_result[field]) for field in ("provider_prompt_tokens", "provider_output_tokens"))
                or oracle["wire_blake3"] != next_intent["wire"]["body_blake3"]
                or oracle["caller_delivered_to_model"] is not True
                or oracle["selected_function_verified"] is not True or oracle["applied"] is not False):
            raise Rejected("caller_continuation_unverified")
        usage = next_result["durable_usage"]
        check_usage(usage, calls=2)
    reopened = receipt("edit-branch-reopen.json")
    if (reopened["branch"] != branch or reopened["goal_done"] is not False
            or reopened["runtime_obligations_open"] != 1 or reopened["files_unchanged"] is not True
            or reopened["mutations"] != 0 or reopened["policy_decisions"] != 0
            or reopened["read_only_tool_events"] != int(caller)
            or reopened["caller_acquired"] is not caller
            or reopened["caller_delivered_to_model"] is not (continuation or known_caller)
            or reopened["usage"] != usage):
        raise Rejected("reopened_edit_branch_unverified")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", required=True, choices=tuple(CASES))
    parser.add_argument("--model", choices=ALTERNATE_EDIT_TAGS, help="unpaired TooLarge measurement")
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--worker", required=True, type=Path)
    parser.add_argument("--run-dir", required=True, type=Path)
    args = parser.parse_args()
    if args.model is not None and args.case != "too-large":
        parser.error("--model is only available with --case too-large")
    tag = args.model or TAG
    comparison = {"comparison": "unpaired"} if args.model is not None else {}
    repository = args.repository.resolve(strict=True)
    parent = (repository / "target" / "plan22-p4-runs").resolve()
    root = args.run_dir.resolve()
    if not parent.is_relative_to(repository) or root.parent != parent or root.exists():
        parser.error("use a fresh direct directory under target/plan22-p4-runs")
    parent.mkdir(parents=True, exist_ok=True)
    root.mkdir()
    test = TEST_PREFIX + CASES[args.case][2]
    timeout = 600 if args.case == "continuation" else 480
    runner = None
    failure = None
    try:
        _, runner, baseline, freeze = load_frozen(repository)
        launcher = Path(__file__)
        helpers = {
            f"fixtures/plan22-p4/{name}": sha(read_small(launcher.with_name(name), 2 << 20))
            for name in ("run_a10_edit.py", "run_intake.py")
        }
        for name in P0_HELPERS:
            path = f"fixtures/local-agent-eval-v1/{name}"
            helpers[path] = freeze["inputs_sha256"][path]
        write_once(root / "launch.json", {
            "schema": "a3.plan22.p4-edit-branch-launch.v1", "case": args.case,
            "launcher_sha256": sha(read_small(launcher, 2 << 20)), "helper_sha256": helpers,
            "p0_input_set_sha256": P0_INPUT_SET, "p0_profile_sha256": P0_PROFILE,
            "worker_test": test, "outer_timeout_seconds": timeout, "tag": tag,
            **comparison,
        })
        execution = run_owned_native(
            repository, root, args.worker, runner, baseline,
            test=test, output_env="A3_PLAN22_P4_EDIT_BRANCH_RUN_DIR", timeout=timeout,
            check_before=lambda model: check_loaded_context(model, allow_unloaded=True, tag=tag),
            model_tag=tag,
        )
        validate_attempt(args.case, *execution, tag=tag)
    except Rejected as error:
        failure = str(error)
    except (OSError, ValueError, KeyError, TypeError, KeyboardInterrupt) as error:
        failure = type(error).__name__
    except Exception as error:
        if runner is None or not isinstance(error, runner.RunnerFailure):
            raise
        failure = str(error)
    result = {
        "passed": failure is None, "failure": failure, "case": args.case, "full_p4_gate": False,
    }
    if comparison:
        result.update(tag=tag, **comparison)
    write_once(root / "result.json", result)
    print(json.dumps(result))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
