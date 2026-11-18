#!/usr/bin/env python3
"""Preregister isolated Ask attempts and retain their evidence for operator review."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "plan22-p5"))
import run_a01 as common  # noqa: E402

ROOT = common.ROOT
RUNS = ROOT / "target/plan22-p6-series"
PREFIX = "a3.plan22.p6.ask."
TEST = (
    "plan22_goal_contract_tests::existing_test_tests::changed_tests::project_ask_tests::"
    "live_matrix::live_ollama_p6_ask_answers_one_frozen_slot"
)
VARIANT = "development"
MODELS = (("ornith", common.ORNITH), ("qwen27", common.QWEN))
CASES = ("Q03", "Q01", "Q02", "Q04", "Q05", "Q06")
CONTROL_CASES = {
    "q03-control": ("Q03",), "q01-control": ("Q01",), "q05-control": ("Q05",),
    "q02-control": ("Q02",), "q04-control": ("Q04",), "q06-control": ("Q06",),
}
OUTER_TIMEOUT = 270  # One 120 s Ask turn, separate 120 s probe and 30 s cleanup.
Q05_OUTER_TIMEOUT = OUTER_TIMEOUT + 120
WORKER_ENV = "A3_PLAN22_P6_ASK_RUN_DIR"
FILES = ("request.json", "launch.json", "native-start.json", "native-final.json",
         "probe-result.json", "runtime-before.json", "runtime-after.json", "owned-worker.json")
Q05_FILES = ("intervention-request.json", "intervention.json", "intervention-supervisor.json",
             "q05-initial-config.py", "q05-final-config.py", "q05-before-answer.json")
WIRE_FIELDS = ("wire_blake3", "request_bytes", "num_ctx", "num_predict", "think",
               "prompt_tokens", "output_tokens")
CALL_FIELDS = {*WIRE_FIELDS, "finish", "repair"}
PHASE_FIELDS = {"sources", "part_count", "calls"}


def slots(scope):
    repetitions = range(1, 4) if scope == "matrix" else (1,)
    return [
        {"attempt_id": f"{case.lower()}-{name}-{repeat:02}", "case_id": case, "variant": VARIANT,
         "model": tag, "context_limit": common.PROFILES[tag][0], "output_limit": common.PROFILES[tag][1]}
        for case_index, case in enumerate(CASES if scope == "matrix" else CONTROL_CASES[scope])
        for repeat in repetitions
        for name, tag in (MODELS if (case_index + repeat) % 2 else MODELS[::-1])
    ]


def source_manifest(runner):
    manifest = common.source_manifest(runner)
    launcher = Path(__file__).resolve()
    manifest[launcher.relative_to(ROOT).as_posix()] = runner.file_sha256(launcher)
    return manifest


def gpu_lock(runner):
    return runner.GpuLock(Path(tempfile.gettempdir()) / "a3-plan22-ollama-gpu.lock")


def register(directory, worker, server, scope):
    common.require(re.fullmatch(r"[0-9]+[.][0-9]+[.][0-9]+", server), "invalid_server_version")
    worker = worker.resolve(strict=True)
    common.require(worker.parent.name == "deps" and worker.parent.parent.name == "debug"
                   and worker.is_file() and not common.linked(worker)
                   and re.fullmatch(r"a3_storage_libsql-[0-9a-f]+(?:[.]exe)?", worker.name),
                   "invalid_storage_worker_path")
    directory.mkdir(exist_ok=False)
    _, runner, baseline, _ = common.p0_inputs(directory, create=True)
    manifest = source_manifest(runner)
    frozen, digest = runner.freeze_worker(ROOT, worker)
    owned, stdout, _ = runner.run_owned_process(
        [frozen, TEST, "--list", "--ignored", "--exact"], cwd=ROOT, timeout=30,
        env=common.clean_environment(), capture_limit=16 * 1024, terminate_on_output_limit=True,
    )
    common.require(common.process_ok(owned) and stdout.decode().splitlines().count(f"{TEST}: test") == 1,
                   "missing_exact_native_worker_test")
    try:
        with gpu_lock(runner):
            pins = {tag: common.model_pin(runner, runner.model_metadata(tag), baseline, tag, server)
                    for _, tag in MODELS}
    except runner.RunnerFailure as error:
        raise common.Rejected(str(error)) from error
    manifest_path = directory / "source-manifest.json"
    common.json_once(manifest_path, manifest)
    planned = slots(scope)
    common.json_once(directory / "series.json", {
        "schema": PREFIX + "series.v1", "scope": scope, "slots": planned, "candidate_unpaired": True,
        "p0_input_set_sha256": common.P0_INPUT_SET, "p0_freeze_sha256": common.P0_FREEZE,
        "source_manifest_sha256": common.sha(common.small(manifest_path)), "server_version": server,
        "p0_server_version": baseline["ollama_version"], "runtime_pins": pins,
        "worker": frozen.relative_to(ROOT).as_posix(), "worker_sha256": digest, "worker_test": TEST,
        "native_retries": 0, "native_phase_receipts": True,
    })
    return {"registered": str(directory), "scope": scope, "planned": len(planned)}


def registered(directory):
    raw = common.small(directory / "series.json")
    series = common.parse(raw)
    common.require(series.get("schema") == PREFIX + "series.v1"
                   and series.get("scope") in (*CONTROL_CASES, "matrix")
                   and series.get("slots") == slots(series["scope"])
                   and series.get("worker_test") == TEST and series.get("native_retries") == 0
                   and series.get("native_phase_receipts", True) is True
                   and series.get("candidate_unpaired") is True
                   and series.get("p0_input_set_sha256") == common.P0_INPUT_SET
                   and series.get("p0_freeze_sha256") == common.P0_FREEZE
                   and common.HEX.fullmatch(series.get("worker_sha256", ""))
                   and set(series.get("runtime_pins", {})) == {tag for _, tag in MODELS},
                   "invalid_registered_series")
    return series, common.sha(raw)


def inventory(worktree, expected, *, initialized):
    common.require(not common.linked(worktree) and worktree.is_dir(), "invalid_ask_worktree")
    entries = {path.name: path for path in worktree.iterdir()}
    if initialized:
        git = entries.pop(".git", None)
        common.require(git is not None and not common.linked(git) and git.is_dir(), "invalid_ask_git_metadata")
    common.require(set(entries) == set(expected), "unexpected_ask_paths")
    return {name: common.sha(common.small(path, 16 * 1024)) for name, path in entries.items()}


def prepare(root, slot, cases, copied, digest):
    case = slot["case_id"]
    pinned, = [entry for entry in common.read_json(copied / "suite.json")["cases"]
               if (entry["case_id"], entry["variant"]) == (case, VARIANT)]
    request = cases.materialize(case, VARIANT, root)
    expected = {key: value for key, value in pinned.items()
                if key not in ("initial_file_sha256", "intervention_file_sha256")}
    common.require(request == expected and request["mode"] == "ask"
                   and all(not request[key] for key in ("allowed_write_paths", "allowed_processes",
                                                        "allowed_network_requests", "process_effect_paths")),
                   "frozen_ask_contract_changed")
    hashes = pinned["initial_file_sha256"]
    common.require(inventory(root / "worktree", hashes, initialized=False) == hashes,
                   "frozen_initial_inventory_changed")
    request_sha = common.sha(common.small(root / "request.json"))
    launch = {"schema": PREFIX + "launch.v1", **slot, "request_sha256": request_sha, "worker_sha256": digest}
    common.json_once(root / "launch.json", launch)
    identity = {key: launch[key] for key in ("attempt_id", "case_id", "variant", "model", "request_sha256")}
    return pinned, {**identity, "launch_sha256": common.sha(common.small(root / "launch.json"))}


def valid_wire(wire, context, output, *, observed_usage=True):
    positive = common.positive_int
    framing = (context + 15) // 16 + 128
    if (not isinstance(wire["wire_blake3"], str) or not common.HEX.fullmatch(wire["wire_blake3"])
            or not positive(wire["num_ctx"]) or not positive(wire["num_predict"])
            or (wire["num_ctx"], wire["num_predict"]) != (context, output) or wire["think"] is not False
            or not positive(wire["request_bytes"]) or wire["request_bytes"] + framing + output > context):
        return False
    prompt, produced = wire["prompt_tokens"], wire["output_tokens"]
    if not observed_usage:
        return prompt is None and produced is None
    return positive(prompt) and prompt <= wire["request_bytes"] + framing and positive(produced) and produced <= output


def valid_aliases(aliases, delivered):
    return (isinstance(aliases, list) and 1 <= len(aliases) <= 4
            and all(type(alias) is int and 1 <= alias <= delivered for alias in aliases)
            and len(set(aliases)) == len(aliases))


def valid_phases(turn, context, output):
    phases = turn.get("phases")
    if not isinstance(phases, list) or not 1 <= len(phases) <= 4:
        return False
    parts = turn["parts"]
    delivered = len(turn["delivered"])
    used_sources = set()
    consumed = 0
    repaired = False
    core_count = 0
    last_model_call = None
    for phase in phases:
        if not isinstance(phase, dict):
            return False
        core = phase.get("core") is True
        if set(phase) != (PHASE_FIELDS | {"core"} if core else PHASE_FIELDS):
            return False
        aliases, count, calls = phase["sources"], phase["part_count"], phase["calls"]
        if (not valid_aliases(aliases, delivered) or not common.positive_int(count)
                or consumed + count > len(parts) or not isinstance(calls, list)):
            return False
        phase_sources = set(aliases)
        phase_parts = parts[consumed:consumed + count]
        if any(not phase_sources.issuperset(part["sources"]) for part in phase_parts):
            return False
        used_sources.update(phase_sources)
        consumed += count
        if core:
            core_count += 1
            if core_count > 2 or calls or count != 1 or phase_parts[0]["kind"] != "observation":
                return False
            continue
        if len(calls) not in (1, 2):
            return False
        for number, call in enumerate(calls):
            if not isinstance(call, dict) or set(call) != CALL_FIELDS or call["repair"] is not (number == 1):
                return False
            finish = call["finish"]
            if (finish not in (None, "stop", "output_limit")
                    or (number == len(calls) - 1 and finish != "stop")
                    or not valid_wire(call, context, output, observed_usage=finish is not None)):
                return False
        if len(calls) == 2:
            if calls[0]["wire_blake3"] == calls[1]["wire_blake3"]:
                return False
            repaired = True
        last_model_call = calls[-1]
    return (last_model_call is not None and consumed == len(parts)
            and used_sources == set(range(1, delivered + 1))
            and all(turn[key] == last_model_call[key] for key in WIRE_FIELDS)
            and turn["repaired"] is repaired)


def valid_turn(turn, source_lengths, context, output, *, previous=None, require_phases=False):
    if (not isinstance(turn, dict) or turn.get("previous_snapshot_id", "missing") != previous
            or not all(common.HEX.fullmatch(turn[key]) for key in ("snapshot_id", "index_run_id"))
            or turn["source_binding_verified"] is not True or type(turn["repaired"]) is not bool
            or not valid_wire(turn, context, output)):
        return False
    phased = require_phases or "phases" in turn
    delivered, parts = turn["delivered"], turn["parts"]
    if not isinstance(delivered, list) or not 1 <= len(delivered) <= (16 if phased else 4):
        return False
    source_keys = set()
    for source in delivered:
        start, end = source["start_byte"], source["end_byte"]
        if (set(source) != {"path", "content_blake3", "start_byte", "end_byte"}
                or not common.HEX.fullmatch(source["content_blake3"])
                or type(start) is not int or type(end) is not int
                or not 0 <= start < end <= source_lengths.get(source["path"], 0)):
            return False
        source_keys.add((source["path"], source["content_blake3"], start, end))
    if phased and len(source_keys) != len(delivered):
        return False
    return isinstance(parts, list) and 1 <= len(parts) <= 6 and all(
        set(part) == {"kind", "text", "sources"} and part["kind"] in ("observation", "interpretation", "limit")
        and isinstance(part["text"], str) and bool(part["text"].strip()) and len(part["text"].encode()) <= 1_600
        and valid_aliases(part["sources"], len(delivered)) for part in parts
    ) and (not phased or valid_phases(turn, context, output))


def validate_q05(root, final, slot, supervision, *, require_phases=False):
    turns = final.get("turns")
    common.require(isinstance(turns, list) and len(turns) == 2, "q05_requires_two_native_answers")
    first, second = turns
    common.require(common.read_json(root / "q05-before-answer.json") == first,
                   "q05_initial_answer_changed_after_edit")
    context, output = slot["context_limit"], slot["output_limit"]
    initial = supervision.initial_files["config.py"]
    replacement = supervision.replacements["config.py"]
    common.require(valid_turn(first, {"config.py": len(initial)}, context, output, require_phases=require_phases)
                   and valid_turn(second, {"config.py": len(replacement)}, context, output,
                                  previous=first["snapshot_id"], require_phases=require_phases),
                   "q05_turn_binding_unverified")
    initial_source, updated_source = first["delivered"], second["delivered"]
    common.require(len(initial_source) == len(updated_source) == 1
                   and initial_source[0]["start_byte"] == updated_source[0]["start_byte"] == 0
                   and initial_source[0]["end_byte"] == len(initial)
                   and updated_source[0]["end_byte"] == len(replacement)
                   and initial_source[0]["content_blake3"] != updated_source[0]["content_blake3"]
                   and all(first[key] != second[key] for key in ("snapshot_id", "index_run_id", "wire_blake3")),
                   "q05_current_config_revision_unverified")

    receipt = supervision.receipt
    common.require(receipt is not None and final.get("intervention") == {"request_id": receipt["request_id"]},
                   "q05_supervisor_edit_unobserved")
    request = common.read_json(root / "intervention-request.json")
    expected = {
        "version": 1, "request_id": receipt["request_id"], "case_id": "Q05",
        "stage": "after_initial_answer", "relative_paths": ["config.py"],
        "at_before_first_patch_approval": False,
        "before_blake3": {"config.py": initial_source[0]["content_blake3"]},
    }
    ack = common.read_json(root / "intervention.json")
    journal = common.read_json(root / "intervention-supervisor.json")
    common.require(request == expected
                   and receipt["before_sha256"] == {"config.py": common.sha(initial)}
                   and receipt["after_sha256"] == {"config.py": common.sha(replacement)}
                   and ack == {**receipt, "expected_before_hex": {"config.py": initial.hex()},
                               "expected_after_hex": {"config.py": replacement.hex()}}
                   and journal == {**receipt, "status": "applied", "applied_paths": ["config.py"]},
                   "q05_supervisor_receipt_unverified")


def validate_native(root, identity, slot, source_lengths, *, supervision=None, require_phases=False):
    start = common.read_json(root / "native-start.json")
    common.require(start == {"schema": PREFIX + "native-start.v1", **identity}, "native_start_unverified")
    final = common.read_json(root / "native-final.json")
    common.require(final.get("schema") == PREFIX + "native-final.v1"
                   and all(final.get(key) == value for key, value in identity.items())
                   and type(final.get("privileged_actions")) is int and final["privileged_actions"] == 0,
                   "native_ask_scope_unverified")
    probe = common.read_json(root / "probe-result.json")
    common.require(probe.get("verified") is True and probe.get("invocations") == 1 and probe.get("failure") is None,
                   "capability_probe_unverified")
    if supervision is not None:
        validate_q05(root, final, slot, supervision, require_phases=require_phases)
        return
    turns = final.get("turns")
    common.require(isinstance(turns, list) and len(turns) == 1 and valid_turn(
        turns[0], source_lengths, slot["context_limit"], slot["output_limit"], require_phases=require_phases),
        "native_ask_turn_unverified")


def attempt(directory, slot_id):
    series, series_digest = registered(directory)
    common.require(series.get("native_phase_receipts") is True, "native_phase_receipts_registration_required")
    position, = [index for index, item in enumerate(series["slots"]) if item["attempt_id"] == slot_id]
    slot = series["slots"][position]
    cases, runner, baseline, copied = common.p0_inputs(directory)
    manifest = directory / "source-manifest.json"
    manifest_raw = common.small(manifest)
    expected_manifest = common.parse(manifest_raw)
    common.require(common.sha(manifest_raw) == series["source_manifest_sha256"]
                   and expected_manifest == source_manifest(runner), "candidate_sources_changed")
    worker = (ROOT / series["worker"]).resolve(strict=True)
    digest = series["worker_sha256"]
    common.require(worker.parent == ROOT / "target/plan22-frozen" / digest and worker.name in ("worker", "worker.exe")
                   and runner.file_sha256(worker) == digest, "candidate_worker_changed")
    root = directory / slot_id
    q05 = slot["case_id"] == "Q05"
    artifact_names = FILES + Q05_FILES if q05 else FILES
    failure = None
    scope_violation = None
    with gpu_lock(runner):
        for prior in series["slots"][:position]:
            previous = directory / prior["attempt_id"]
            common.require(previous.is_dir() and not common.linked(previous), "slot_out_of_order")
        root.mkdir(exist_ok=False)
        initial = None
        initialized = False
        supervision = None
        try:
            pinned, identity = prepare(root, slot, cases, copied, digest)
            initial = pinned["initial_file_sha256"]
            original = cases.fixture_files(slot["case_id"], VARIANT)
            if q05:
                replacement = cases.intervention_files("Q05", VARIANT)
                common.require(set(original) == set(replacement) == {"config.py"}
                               and pinned["intervention_file_sha256"] == {
                                   "config.py": common.sha(replacement["config.py"]),
                               }, "frozen_q05_edit_changed")
                supervision = runner.intervention_control.Supervisor(root, "Q05", VARIANT, original, replacement)
                common.write_once(root / "q05-initial-config.py", original["config.py"])
            runner._git_initialize(root / "worktree")
            initialized = True
            model, server = slot["model"], series["server_version"]
            pin = series["runtime_pins"][model]
            before = runner.model_metadata(model)
            common.json_once(root / "runtime-before.json", {"model": before, "host": runner.runtime_metadata(ROOT)})
            common.require(common.model_pin(runner, before, baseline, model, server) == pin, "runtime_before_unverified")
            env = {**common.clean_environment(), WORKER_ENV: str(root)}
            owned, _, _ = runner.run_owned_process(
                [worker, TEST, "--ignored", "--exact", "--nocapture", "--test-threads=1"], cwd=ROOT,
                timeout=Q05_OUTER_TIMEOUT if q05 else OUTER_TIMEOUT, env=env, capture_limit=16 * 1024,
                terminate_on_output_limit=True, on_poll=None if supervision is None else supervision.poll,
            )
            common.json_once(root / "owned-worker.json", owned)
            after = runner.model_metadata(model)
            common.json_once(root / "runtime-after.json", after)
            common.require(expected_manifest == source_manifest(runner), "candidate_sources_changed_during_attempt")
            common.require(common.model_pin(runner, after, baseline, model, server) == pin
                           and common.loaded_context(after, pin), "runtime_after_unverified")
            common.require(owned.get("supervisor_error") is None and common.process_ok(owned), "native_attempt_failed")
            common.require(common.sha(common.small(root / "request.json")) == identity["request_sha256"]
                           and common.sha(common.small(root / "launch.json")) == identity["launch_sha256"],
                           "native_input_changed")
            lengths = {name: len(body) for name, body in original.items()}
            validate_native(root, identity, slot, lengths, supervision=supervision, require_phases=True)
        except (common.Rejected, runner.RunnerFailure) as error:
            failure = str(error)
        except runner.intervention_control.SupervisorFailure as error:
            failure = error.code
        except (OSError, KeyError, TypeError, ValueError, AttributeError, KeyboardInterrupt) as error:
            failure = type(error).__name__
        if initialized:
            try:
                expected = initial
                if supervision is not None and supervision.receipt is not None:
                    expected = {**initial, **supervision.receipt["after_sha256"]}
                scope_violation = inventory(root / "worktree", expected, initialized=True) != expected
                if q05 and scope_violation is False and supervision.receipt is not None:
                    after_config = common.small(root / "worktree/config.py", 16 * 1024)
                    common.require(common.sha(after_config) == expected["config.py"], "q05_final_config_changed")
                    common.write_once(root / "q05-final-config.py", after_config)
            except common.Rejected:
                scope_violation = True
            except OSError:
                scope_violation = None
            final = common.optional_receipt(root, "native-final.json")
            if type(final.get("privileged_actions")) is int and final["privileged_actions"] > 0:
                scope_violation = True
        structural = failure is None and scope_violation is False
        result = {"schema": PREFIX + "attempt.v1", **slot, "series_sha256": series_digest,
                  "structural_passed": structural, "scope_violation": scope_violation,
                  "failure": failure, "manual_review_required": structural, "independently_correct": None,
                  "artifact_sha256": {name: common.sha(common.small(root / name)) for name in artifact_names
                                      if (root / name).is_file() and not common.linked(root / name)
                                      and (root / name).stat().st_size <= common.MAX_FILE_BYTES}}
        common.json_once(root / "result.json", result)
    return result


def status(directory):
    series, digest = registered(directory)
    rows = []
    for slot in series["slots"]:
        root = directory / slot["attempt_id"]
        expected_files = set(FILES + Q05_FILES if slot["case_id"] == "Q05" else FILES)
        result = common.optional_receipt(root, "result.json")
        state = "incomplete" if root.exists() else "unstarted"
        try:
            hashes = result["artifact_sha256"]
            if (result["schema"] == PREFIX + "attempt.v1" and result["series_sha256"] == digest
                    and all(result.get(key) == value for key, value in slot.items())
                    and set(hashes) <= expected_files
                    and all(common.sha(common.small(root / name)) == value for name, value in hashes.items())):
                state = ("manual_review_pending" if set(hashes) == expected_files and result["structural_passed"] is True
                         and result["manual_review_required"] is True and result["scope_violation"] is False
                         else "structural_failure")
        except (OSError, KeyError, TypeError, AttributeError, common.Rejected):
            pass
        rows.append({**slot, "status": state, "independently_correct": None})
    return {"scope": series["scope"], "planned": len(rows), "slots": rows, "p6_gate_passed": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="operation", required=True)
    for operation in ("register", "attempt", "status"):
        command = commands.add_parser(operation)
        command.add_argument("--series-dir", type=Path, required=True)
        if operation == "register":
            command.add_argument("--scope", choices=(*CONTROL_CASES, "matrix"), required=True)
            command.add_argument("--worker", type=Path, required=True)
            command.add_argument("--server-version", required=True)
        if operation == "attempt":
            command.add_argument("--slot", choices=[item["attempt_id"] for item in slots("matrix")], required=True)
    args = parser.parse_args()
    try:
        directory = args.series_dir.absolute()
        common.require(directory.parent.resolve() == RUNS and directory.resolve().parent == RUNS
                       and re.fullmatch(r"[a-z0-9][a-z0-9-]{0,63}", directory.name), "invalid_direct_series_directory")
        if args.operation == "register":
            common.require(not RUNS.exists() or not common.linked(RUNS), "linked_series_root")
            RUNS.mkdir(exist_ok=True)
            result = register(directory, args.worker, args.server_version, args.scope)
        else:
            common.require(directory.is_dir() and not common.linked(directory), "invalid_series_directory")
            result = status(directory) if args.operation == "status" else attempt(directory, args.slot)
        print(json.dumps(result))
        return int(args.operation == "attempt" and not result["structural_passed"])
    except (OSError, ValueError, KeyError, TypeError, common.Rejected) as error:
        print(json.dumps({"status": "supervisor_failure",
                          "failure": str(error) if isinstance(error, common.Rejected) else "invalid_measurement_io"}))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
