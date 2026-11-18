#!/usr/bin/env python3
"""Run the retained local 4k two-window Intake component under the shared GPU lock."""
from __future__ import annotations

import argparse
import json
from pathlib import Path

from run_a10_edit import (
    P0_INPUT_SET, P0_PROFILE, PROFILE, TAG, Rejected, load_frozen, read_json, read_small,
    sha, write_once,
)
from run_edit_branches import (
    P0_HELPERS, check_intent, check_loaded_context, check_usage, check_warm_context,
)
from run_intake import positive_int, run_owned_native, validate_native_execution

TEST = (
    "plan22_goal_contract_tests::existing_test_tests::changed_tests::middle_edit_tests::"
    "p4_live_edit::live_intake::live_intake_windows::"
    "live_ollama_4k_two_window_intake_promotes_exact_goal"
)
TIMEOUT_SECONDS = 480
FIRST_BYTES = len(b"Rename the README heading to Guide.\r\n\r\n")
ORIGINAL_BYTES = FIRST_BYTES + len(b"Keep source.py unchanged.")
DEADLINE_MILLIS = 360_001


def validate_attempt(native, owned, before, after):
    validate_native_execution(native, owned, before, after)
    check_loaded_context(after)

    def receipt(name):
        return read_json(native / name, 8_192)

    if receipt("native-start.json") != {"version": 1, "phase": "intake_windows", "tag": TAG}:
        raise Rejected("native_test_identity_unverified")
    final = receipt("native-final.json")
    if final["version"] != 1 or final["success"] is not True or final["goal_done"] is not False:
        raise Rejected("native_completion_missing")
    probe = receipt("probe-result.json")
    if probe["verified"] is not True or probe["invocations"] != 1 or probe["failure"] is not None:
        raise Rejected("capability_probe_unverified")

    ranges = ([0, FIRST_BYTES], [FIRST_BYTES, ORIGINAL_BYTES])
    prompt_total = output_total = 0
    for number, (span, kind) in enumerate(zip(ranges, ("must", "constraint"), strict=True), 1):
        intent = receipt(f"window-{number}-intent.json")
        check_intent(intent)
        reserved_prompt, reserved_output = intent["reserved_prompt"], intent["reserved_output"]
        expected_revision = None if number == 1 else 1
        if (intent["range"] != span or intent["review_revision"] != expected_revision
                or not positive_int(reserved_prompt) or reserved_output != PROFILE[1]
                or reserved_prompt + reserved_output > PROFILE[0]):
            raise Rejected("intake_window_intent_unverified")

        observed = receipt(f"window-{number}-result.json")
        usage = observed["usage"]
        prompt, output = observed["reported_prompt_tokens"], observed["reported_output_tokens"]
        if (observed["observed_review"] is not True or observed["failure"] is not None
                or observed["finding_count"] != 1 or observed["question_count"] != 0
                or observed["finding_kind"] != kind or observed["finding_range"] != span
                or observed["original_deadline_millis"] != DEADLINE_MILLIS
                or not positive_int(prompt) or prompt > reserved_prompt
                or not positive_int(output) or output > reserved_output):
            raise Rejected("literal_intake_window_observation_unverified")
        check_usage(usage, calls=number)
        prompt_total += reserved_prompt
        output_total += reserved_output
        if (usage["prompt_tokens"], usage["output_tokens"]) != (prompt_total, output_total):
            raise Rejected("cumulative_intake_usage_unverified")

    reopened = receipt("intake-windows-reopen.json")
    if (reopened["original_bytes"] != ORIGINAL_BYTES or reopened["review_revision"] != 2
            or reopened["goal_original_exact"] is not True or reopened["goal_promoted"] is not True
            or reopened["goal_done"] is not False or reopened["run_started"] is not False
            or reopened["protected_worktree_unchanged"] is not True
            or (reopened["first_range"], reopened["second_range"]) != ranges
            or reopened["original_deadline_millis"] != DEADLINE_MILLIS
            or (reopened["captured_at_millis"], reopened["duration_limit_millis"]) != (1, 360_000)
            or reopened["usage"] != usage):
        raise Rejected("reopened_intake_windows_unverified")


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
            "schema": "a3.plan22.p4-intake-windows-launch.v1",
            "launcher_sha256": sha(read_small(launcher, 2 << 20)), "helper_sha256": helpers,
            "p0_input_set_sha256": P0_INPUT_SET, "p0_profile_sha256": P0_PROFILE,
            "worker_test": TEST, "outer_timeout_seconds": TIMEOUT_SECONDS, "tag": TAG,
        })
        execution = run_owned_native(
            repository, root, args.worker, runner, baseline,
            test=TEST, output_env="A3_PLAN22_P4_INTAKE_WINDOWS_RUN_DIR", timeout=TIMEOUT_SECONDS,
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
        "scope": "two local intake windows to exact reopened Goal",
    }
    write_once(root / "result.json", result)
    print(json.dumps(result))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
