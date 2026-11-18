#!/usr/bin/env python3
"""Run one local 4k Q05 source freshness control. Semantic review stays separate."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys

P4 = Path(__file__).resolve().parent.parent / "plan22-p4"
sys.path.insert(0, str(P4))
from run_a10_edit import (  # noqa: E402
    P0_INPUT_SET, P0_PROFILE, PROFILE, TAG, Rejected, load_frozen, read_json,
    read_small, sha, write_once,
)
from run_edit_branches import check_loaded_context, check_warm_context  # noqa: E402
from run_intake import positive_int, run_owned_native, validate_native_execution  # noqa: E402

TEST = (
    "plan22_goal_contract_tests::existing_test_tests::changed_tests::project_ask_tests::"
    "live_q05::live_ollama_4k_ask_reads_both_q05_revisions"
)
SCHEMA = "a3.plan22.p6.q05-live.v1"
TIMEOUT_SECONDS = 390
CASE, VARIANT, TARGET = "Q05", "development", "config.py"
HEX = re.compile(r"[0-9a-f]{64}")
HASH_FIELDS = ("snapshot_id", "index_run_id", "source_sha256", "source_blake3", "wire_blake3")
TURN_FIELDS = {*HASH_FIELDS, "previous_snapshot_id", "range_start", "range_end", "request_bytes",
               "num_ctx", "num_predict", "think", "prompt_tokens", "output_tokens", "repaired", "parts"}
PART_KINDS = {"observation", "interpretation", "limit"}
SOURCES = (
    "crates/a3-application/src/project_ask.rs",
    "crates/a3-application/src/project_ask_sources.rs",
    "crates/a3-application/src/bound_edit_source.rs",
    "crates/a3-storage-libsql/src/plan22_project_ask_tests.rs",
    "crates/a3-storage-libsql/src/plan22_p6_live_q05.rs",
)


def frozen_sources(repository, cases):
    suite = read_json(repository / "fixtures/local-agent-eval-v1/suite.json")
    pinned, = [row for row in suite["cases"] if (row["case_id"], row["variant"]) == (CASE, VARIANT)]
    before = cases.fixture_files(CASE, VARIANT)
    after = cases.intervention_files(CASE, VARIANT)
    for files, hashes in ((before, pinned["initial_file_sha256"]),
                          (after, pinned["intervention_file_sha256"])):
        if set(files) != {TARGET} or hashes != {TARGET: sha(files[TARGET])}:
            raise Rejected("frozen_q05_source_unverified")
    if before == after:
        raise Rejected("frozen_q05_edit_has_no_effect")
    return before[TARGET], after[TARGET]


def valid_turn(turn, source, previous):
    context, output = PROFILE
    framing = (context + 15) // 16 + 128
    if not isinstance(turn, dict) or set(turn) != TURN_FIELDS:
        return False
    if (any(not isinstance(turn[key], str) or not HEX.fullmatch(turn[key]) for key in HASH_FIELDS)
            or turn["previous_snapshot_id"] != previous or turn["source_sha256"] != sha(source)
            or type(turn["range_start"]) is not int or turn["range_start"] != 0
            or type(turn["range_end"]) is not int or turn["range_end"] != len(source)
            or (turn["num_ctx"], turn["num_predict"]) != PROFILE or turn["think"] is not False
            or type(turn["repaired"]) is not bool or not positive_int(turn["request_bytes"])
            or turn["request_bytes"] + framing + output > context
            or not positive_int(turn["prompt_tokens"]) or turn["prompt_tokens"] > turn["request_bytes"] + framing
            or not positive_int(turn["output_tokens"]) or turn["output_tokens"] > output):
        return False
    parts = turn["parts"]
    return isinstance(parts, list) and 1 <= len(parts) <= 6 and all(
        isinstance(part, dict) and set(part) == {"kind", "text"}
        and isinstance(part["kind"], str) and part["kind"] in PART_KINDS
        and isinstance(part["text"], str) and bool(part["text"].strip())
        and len(part["text"].encode("utf-8")) <= 1_600 for part in parts
    )


def validate_attempt(native, owned, before, after, sources):
    validate_native_execution(native, owned, before, after)
    check_loaded_context(after)
    if read_json(native / "native-start.json", 8_192) != {"schema": SCHEMA, "tag": TAG}:
        raise Rejected("native_test_identity_unverified")
    probe = read_json(native / "probe-result.json", 8_192)
    if probe["verified"] is not True or probe["invocations"] != 1 or probe["failure"] is not None:
        raise Rejected("capability_probe_unverified")
    final = read_json(native / "native-final.json", 32 * 1024)
    if (set(final) != {"schema", "before", "after", "controlled_source_edit", "privileged_actions"}
            or final["schema"] != SCHEMA or final["controlled_source_edit"] is not True
            or type(final["privileged_actions"]) is not int or final["privileged_actions"] != 0):
        raise Rejected("native_q05_completion_unverified")
    first, second = final["before"], final["after"]
    if (not valid_turn(first, sources[0], None)
            or not valid_turn(second, sources[1], first["snapshot_id"])
            or any(first[key] == second[key] for key in HASH_FIELDS)):
        raise Rejected("native_q05_revision_change_unverified")
    if read_json(native / "before.json", 16 * 1024) != first:
        raise Rejected("retained_q05_initial_answer_unverified")
    comparison = "".join(
        label + "\n" + "".join(f"- {part['text']} [config.py, Zeile 1]\n" for part in turn["parts"])
        for label, turn in (("Aktueller Stand", second), ("Vorheriger Stand (historische Quelle)", first))
    )
    if read_small(native / "answer.txt", 32 * 1024) != comparison.encode("utf-8"):
        raise Rejected("q05_historical_comparison_unverified")
    worktree = native / "worktree"
    target = worktree / TARGET
    if (worktree.is_symlink() or target.is_symlink()
            or {entry.name for entry in worktree.iterdir()} != {".git", TARGET}
            or read_small(target, 8_192) != sources[1]):
        raise Rejected("q05_final_scope_or_source_unverified")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--worker", required=True, type=Path)
    parser.add_argument("--run-dir", required=True, type=Path)
    args = parser.parse_args()
    repository = args.repository.resolve(strict=True)
    parent = (repository / "target" / "plan22-p6-runs").resolve()
    root = args.run_dir.resolve()
    if not parent.is_relative_to(repository) or root.parent != parent or root.exists():
        parser.error("use a fresh direct directory under target/plan22-p6-runs")
    parent.mkdir(parents=True, exist_ok=True)
    root.mkdir()
    (root / "native").mkdir()
    runner = None
    failure = None
    try:
        cases, runner, baseline, _ = load_frozen(repository)
        sources = frozen_sources(repository, cases)
        candidate_sources = {name: sha(read_small(repository / name, 2 << 20)) for name in SOURCES}
        write_once(root / "launch.json", {
            "schema": "a3.plan22.p6.q05-launch.v2", "case": CASE, "variant": VARIANT,
            "launcher_sha256": sha(read_small(Path(__file__), 2 << 20)),
            "helper_sha256": {name: sha(read_small(P4 / name, 2 << 20))
                              for name in ("run_a10_edit.py", "run_intake.py", "run_edit_branches.py")},
            "candidate_source_sha256": candidate_sources,
            "p0_input_set_sha256": P0_INPUT_SET, "p0_profile_sha256": P0_PROFILE,
            "worker_test": TEST, "tag": TAG, "outer_timeout_seconds": TIMEOUT_SECONDS,
            "turn_timeout_seconds": 120, "probe_timeout_seconds": 120,
        })
        execution = run_owned_native(
            repository, root, args.worker, runner, baseline,
            test=TEST, output_env="A3_PLAN22_P6_Q05_RUN_DIR", timeout=TIMEOUT_SECONDS,
            check_before=check_warm_context,
        )
        validate_attempt(*execution, sources)
        if candidate_sources != {name: sha(read_small(repository / name, 2 << 20)) for name in SOURCES}:
            raise Rejected("q05_candidate_source_changed")
    except Rejected as error:
        failure = str(error)
    except (OSError, ValueError, KeyError, TypeError, KeyboardInterrupt) as error:
        failure = type(error).__name__
    except Exception as error:
        if runner is None or not isinstance(error, runner.RunnerFailure):
            raise
        failure = str(error)
    result = {
        "structural_passed": failure is None, "failure": failure,
        "manual_review_required": failure is None, "independently_correct": None,
    }
    write_once(root / "result.json", result)
    print(json.dumps(result))
    return 0 if result["structural_passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
