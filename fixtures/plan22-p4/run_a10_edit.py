#!/usr/bin/env python3
"""One explicit local P4 Edit job, outside the frozen P0 suite."""
from __future__ import annotations

import argparse
import ast
import hashlib
import importlib
import json
import os
from pathlib import Path
import sys
import tempfile

TAG = "qwen38-4k:latest"
CASE = "A10"
VARIANT = "development"
TARGET = "step_one.py"
PROFILE = (4096, 1024)
P0_INPUT_SET = "7ba991a39bf2437b4fdbf353b9d879df9de2e8564cef7efbc08bf3598da66f52"
P0_PROFILE = "a06ad6cc81b622b8ec67d478e3be9cb3b010415f19e47731c25ac46594a2b000"
TEST = (
    "plan22_goal_contract_tests::existing_test_tests::changed_tests::"
    "middle_edit_tests::p4_live_edit::live_ollama_4k_a10_first_edit_window"
)


class Rejected(Exception):
    """Closed supervisor failure code."""


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write_once(path, value):
    with path.open("x", encoding="utf-8", newline="\n") as output:
        json.dump(value, output, ensure_ascii=False, indent=2)
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())


def read_small(path, limit):
    if path.is_symlink() or not path.is_file():
        raise Rejected("missing_or_linked_input")
    with path.open("rb") as handle:
        data = handle.read(limit + 1)
    if len(data) > limit:
        raise Rejected("oversized_input")
    return data


def read_json(path, limit=1 << 20):
    return json.loads(read_small(path, limit))


def load_frozen(repository):
    fixture_root = repository / "fixtures" / "local-agent-eval-v1"
    freeze = read_json(fixture_root / "p0-freeze.json")
    if freeze["input_set_sha256"] != P0_INPUT_SET or freeze["profile_freeze_sha256"] != P0_PROFILE:
        raise Rejected("p0_identity_mismatch")
    input_hashes = freeze["inputs_sha256"]
    for name in ("cases.py", "baseline.json", "suite.json", "run_baseline.py", "supervisor.py", "order_trace.py"):
        path = fixture_root / name
        relative = path.relative_to(repository).as_posix()
        if sha(read_small(path, 2 << 20)) != input_hashes[relative]:
            raise Rejected("p0_input_changed")
    baseline_raw = read_small(fixture_root / "baseline.json", 1 << 20)
    if sha(baseline_raw) != P0_PROFILE:
        raise Rejected("p0_profile_changed")
    sys.dont_write_bytecode = True
    sys.path.insert(0, str(fixture_root))
    return importlib.import_module("cases"), importlib.import_module("run_baseline"), json.loads(baseline_raw), freeze


def prepare(root, repository, cases, baseline, freeze):
    request = cases.materialize(CASE, VARIANT, root)
    original = request["objective"].encode("utf-8")
    if len(original) != 34_371 or original.find(b"R28: Sp") != 32_912:
        raise Rejected("frozen_original_shape_mismatch")
    with (root / "original.utf8").open("xb") as output:
        output.write(original)
    expected = cases.fixture_files(CASE, VARIANT)
    suite = read_json(repository / "fixtures" / "local-agent-eval-v1" / "suite.json")
    pinned = [item for item in suite["cases"] if (item["case_id"], item["variant"]) == (CASE, VARIANT)]
    hashes = {name: sha(body) for name, body in expected.items()}
    if len(pinned) != 1 or pinned[0]["initial_file_sha256"] != hashes or pinned[0]["objective"] != request["objective"]:
        raise Rejected("materialization_does_not_match_frozen_manifest")
    for name, digest in hashes.items():
        if sha(read_small(root / "worktree" / name, 16 * 1024)) != digest:
            raise Rejected("materialized_bytes_changed")
    profile = baseline["models"][TAG]
    if (profile["context_limit"], profile["output_limit"]) != PROFILE:
        raise Rejected("frozen_4k_profile_changed")
    write_once(root / "p4-input.json", {
        "schema": "a3.plan22.p4-edit-input.v1", "case": CASE, "variant": VARIANT,
        "target": TARGET, "original_bytes": len(original), "original_sha256": sha(original),
        "initial_file_sha256": hashes, "p0_input_set_sha256": freeze["input_set_sha256"],
        "p0_profile_sha256": P0_PROFILE, "tag": TAG, "profile": profile,
        "profile_policy": baseline["profile_policy"], "paired_baseline": False,
        "task_model_attempt_limit": 2, "structural_repair_limit": 1,
        "capability_probe_separate": True,
        "outer_timeout_seconds": 480, "full_a10_claim": False,
    })
    return expected


def affine(node, depth=0):
    """Evaluate a closed subset of Python integer expressions symbolically."""
    if depth > 8:
        raise Rejected("oracle_expression_scope")
    if isinstance(node, ast.Name) and node.id == "value":
        return (1, 0)
    if isinstance(node, ast.Constant) and type(node.value) is int and abs(node.value) <= 1_000_000:
        return (0, node.value)
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.UAdd, ast.USub)):
        a, b = affine(node.operand, depth + 1)
        sign = -1 if isinstance(node.op, ast.USub) else 1
        return (sign * a, sign * b)
    if isinstance(node, ast.BinOp) and isinstance(node.op, (ast.Add, ast.Sub)):
        a, b = affine(node.left, depth + 1)
        c, d = affine(node.right, depth + 1)
        sign = -1 if isinstance(node.op, ast.Sub) else 1
        return (a + sign * c, b + sign * d)
    raise Rejected("oracle_expression_scope")


def check_effect(worktree, expected):
    files = {}
    for path in worktree.rglob("*"):
        relative = path.relative_to(worktree)
        if relative.parts[0] == ".git":
            continue
        if path.is_symlink():
            raise Rejected("unexpected_worktree_link")
        if path.is_file():
            files[relative.as_posix()] = read_small(path, 16 * 1024)
    if set(files) != set(expected) or any(files[name] != body for name, body in expected.items() if name != TARGET):
        raise Rejected("protected_files_changed")
    if files[TARGET] == expected[TARGET]:
        raise Rejected("target_unchanged")
    original = ast.parse(expected[TARGET])
    actual = ast.parse(files[TARGET])
    if len(actual.body) != 1 or type(actual.body[0]) is not ast.FunctionDef:
        raise Rejected("oracle_function_shape")
    result = actual.body[0].body
    if len(result) != 1 or type(result[0]) is not ast.Return:
        raise Rejected("oracle_function_shape")
    actual.body[0].body = [ast.Pass()]
    original.body[0].body = [ast.Pass()]
    if ast.dump(actual) != ast.dump(original):
        raise Rejected("oracle_signature_changed")
    a, b = affine(result[0].value)
    inputs = [-31, 0, -10**120, 10**120]
    if (a, b) != (1, 1) or any(type(a * value + b) is not int or a * value + b != value + 1 for value in inputs):
        raise Rejected("oracle_wrong_integer_formula")
    return {
        "scope": "one expression in the frozen step_one.compute signature",
        "method": "closed_affine_ast",
        "exact_int_formula": True,
        "negative_and_huge_checks": len(inputs),
        "protected_files": len(expected) - 1,
        "host_model_code_executed": False,
        "product_verification_evidence": False,
    }


def verify_provider(observed, baseline):
    pinned = baseline["models"][TAG]
    for key in ("digest", "template_sha256", "parameters_sha256"):
        if observed[key] != pinned[key]:
            raise Rejected("model_identity_changed")
    if observed["inherited_numeric_parameters"].get("num_ctx") != PROFILE[0]:
        raise Rejected("model_context_tag_changed")


def verify_edit_receipts(root):
    def read_call(prefix, calls, repairs):
        intent = read_json(root / f"{prefix}-intent.json")
        dispatch = read_json(root / f"{prefix}-dispatch.json")
        result = read_json(root / f"{prefix}-result.json")
        reserved = dispatch["before_io"]
        settled = result["durable_usage"]
        for usage, unobserved in ((reserved, 1), (settled, 0)):
            if (usage["calls"], usage["repairs"], usage["unobserved_calls"], usage["unknown_calls"]) != (
                calls, repairs, unobserved, 0
            ):
                raise Rejected("exact_dispatch_evidence_missing")
        if (result["stream"]["dispatches"] != 1 or intent["capacity_error"] is not None
                or dispatch["wire"] != intent["wire"]):
            raise Rejected("exact_dispatch_evidence_missing")
        return intent, result

    initial, result = read_call("edit", 1, 0)
    repair_artifacts = [(root / f"edit-repair-{kind}.json").exists()
                        for kind in ("intent", "dispatch", "result")]
    if any(repair_artifacts):
        if not all(repair_artifacts) or result["execution"] != "failed":
            raise Rejected("unexpected_structural_repair")
        repair, result = read_call("edit-repair", 2, 1)
        if (repair["job_blake3"] != initial["job_blake3"]
                or repair["profile"] != initial["profile"]
                or repair["request_projection_blake3"] == initial["request_projection_blake3"]
                or repair["wire"]["body_blake3"] == initial["wire"]["body_blake3"]):
            raise Rejected("structural_repair_binding_missing")
    if result["execution"] != "proposal":
        raise Rejected("native_proposal_missing")
    reopened = read_json(root / "reopen-result.json")
    if reopened["durable_usage"] != result["durable_usage"]:
        raise Rejected("reopened_edit_usage_changed")


def run_once(root, repository, binary, runner, baseline, expected):
    worker, digest = runner.freeze_worker(repository, binary)
    lock = Path(tempfile.gettempdir()) / "a3-plan22-ollama-gpu.lock"
    with runner.GpuLock(lock):
        before = runner.model_metadata(TAG)
        verify_provider(before, baseline)
        write_once(root / "runtime-before.json", {
            "worker_sha256": digest, "worker_test": TEST,
            "model": before, "host": runner.runtime_metadata(repository),
            "p0_ollama_version": baseline["ollama_version"],
            "same_server_as_p0": before["server_version"] == baseline["ollama_version"],
        })
        env = dict(os.environ)
        env.pop("A3_NATIVE_LIBSQL_UNIT_TEST", None)
        env.pop("A3_NATIVE_LIBSQL_SUCCESS_MARKER", None)
        env["A3_PLAN22_P4_RUN_DIR"] = str(root)
        result, _, _ = runner.run_owned_process(
            [str(worker), TEST, "--ignored", "--exact", "--nocapture", "--test-threads=1"],
            cwd=repository, timeout=480, env=env, capture_limit=16 * 1024,
            terminate_on_output_limit=True,
        )
        write_once(root / "owned-worker.json", result)
        after = runner.model_metadata(TAG)
        verify_provider(after, baseline)
        write_once(root / "runtime-after.json", after)
        if before["server_version"] != after["server_version"]:
            raise Rejected("ollama_server_changed")
        if result["exit_code"] != 0 or result["timed_out"] or result["output_limit"] or result["interrupted"]:
            raise Rejected("native_attempt_failed")
        if read_json(root / "native-final.json").get("success") is not True:
            raise Rejected("native_completion_missing")
        verify_edit_receipts(root)
        actual_context = [model for model in after["selected_model_loaded"] if
                          model.get("context_length") == PROFILE[0] and
                          model.get("digest") == before["digest"]]
        if len(actual_context) != 1:
            raise Rejected("ollama_effective_context_unobserved")
        oracle = check_effect(root / "worktree", expected)
        write_once(root / "independent-effect.json", oracle)
    return {"passed": True, "full_a10_done": False, "full_p4_gate": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--run-dir", required=True, type=Path)
    parser.add_argument("--worker", type=Path)
    parser.add_argument("--prepare-only", action="store_true")
    args = parser.parse_args()
    if not args.prepare_only and args.worker is None:
        parser.error("--worker is required for the explicit live attempt")
    repository = args.repository.resolve(strict=True)
    root = args.run_dir.resolve()
    root.mkdir(parents=True, exist_ok=False)
    runner = None
    try:
        cases, runner, baseline, freeze = load_frozen(repository)
        expected = prepare(root, repository, cases, baseline, freeze)
        result = {"prepared": True, "generation": False} if args.prepare_only else run_once(
            root, repository, args.worker, runner, baseline, expected
        )
    except Rejected as error:
        result = {"passed": False, "failure": str(error), "full_a10_done": False}
    except (OSError, ValueError, KeyError, SyntaxError, RuntimeError, KeyboardInterrupt) as error:
        # Retain a closed classifier and preserve preceding receipts for diagnosis.
        result = {"passed": False, "failure": type(error).__name__, "full_a10_done": False}
    except Exception as error:
        if runner is None or not isinstance(error, runner.RunnerFailure):
            raise
        result = {"passed": False, "failure": str(error), "full_a10_done": False}
    write_once(root / "result.json", result)
    print(json.dumps(result))
    return 0 if result.get("passed") or result.get("prepared") else 1


if __name__ == "__main__":
    raise SystemExit(main())
