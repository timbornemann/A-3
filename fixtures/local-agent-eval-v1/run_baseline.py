#!/usr/bin/env python3
"""One isolated, retained Plan 22 baseline attempt. Standard library only.

The worker is the native production-path test, never a direct model-generation
request. Importing this module does not contact Ollama, build, or start a worker.
"""
from __future__ import annotations

import argparse
import contextlib
import ctypes
import datetime as dt
import hashlib
import importlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request
import uuid
import supervisor as intervention_control

MODELS = {
    "ornith-1.5:9b": (8192, 2048),
    "qwen38-27b-iq3s:latest": (8192, 2048),
    "qwen38-4k:latest": (4096, 1024),
    "qwen38-8k:latest": (8192, 2048),
    "qwen38-16k:latest": (16384, 4096),
    "bonsai-2:27b": (8192, 2048),
}
CASES = tuple(f"A{i:02}" for i in range(1, 13)) + tuple(f"Q{i:02}" for i in range(1, 7))
WORKER_TEST = "agent_session_manager::plan22_baseline::current_production_baseline"
ORIGIN = "http://127.0.0.1:11434"
MAX_JSON_BYTES = 1024 * 1024
MAX_WORKER_BYTES = 512 * 1024
CAPTURE_BYTES = 64 * 1024
SCHEMA = "a3.plan22.baseline-attempt.v1"
BASELINE_COMMIT = "38a07ce97f4f1d84c68101e531fdb027be5fa529"
BASELINE_TREE = "927b30dba9544307c179b67d6d58a2d421830739"


def task_budget(case_id):
    if case_id == "A01" or case_id.startswith("Q"):
        return 360
    return 1800 if case_id in ("A05", "A10") else 900


def load_series_freeze():
    path = Path(__file__).with_name("baseline.json")
    if not path.is_file():
        raise RunnerFailure("baseline_freeze_missing")
    value = read_json(path)
    if (not isinstance(value, dict) or value.get("schema") != "a3.plan22.baseline-freeze.v1"
            or value.get("baseline_commit") != BASELINE_COMMIT or value.get("baseline_tree") != BASELINE_TREE
            or not isinstance(value.get("models"), dict)):
        raise RunnerFailure("invalid_baseline_freeze")
    for model, (context, output) in MODELS.items():
        item = value["models"].get(model)
        if (not isinstance(item, dict) or not re.fullmatch(r"[0-9a-f]{64}", str(item.get("digest", "")))
                or item.get("context_limit") != context or item.get("output_limit") != output):
            raise RunnerFailure("invalid_baseline_freeze")
        if any(not re.fullmatch(r"[0-9a-f]{64}", str(item.get(key, "")))
               for key in ("template_sha256", "parameters_sha256")):
            raise RunnerFailure("invalid_baseline_freeze")
    policy = value.get("profile_policy", {})
    for key, expected in (("schema_grounding", "FormatFieldOnly"), ("temperature_milli", 0),
                          ("top_p_milli", 1000), ("parallelism", 1), ("thinking", False),
                          ("token_counter", "ConservativeUtf8BytesV1"), ("stop_sequences", [])):
        if policy.get(key) != expected:
            raise RunnerFailure("invalid_baseline_freeze")
    return value


class RunnerFailure(Exception):
    """A closed public error code; never include process output or source text."""


def utc_now():
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="milliseconds")


def canonical_json(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def sha256_bytes(value):
    return hashlib.sha256(value).hexdigest()


def file_sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(64 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def atomic_json(path, value):
    path = Path(path)
    temporary = path.with_name(path.name + "." + uuid.uuid4().hex + ".tmp")
    try:
        with temporary.open("xb") as output:
            output.write(canonical_json(value) + b"\n")
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    finally:
        with contextlib.suppress(FileNotFoundError):
            temporary.unlink()


def read_json(path, maximum=MAX_WORKER_BYTES):
    with Path(path).open("rb") as source:
        body = source.read(maximum + 1)
    if len(body) > maximum:
        raise RunnerFailure("worker_report_too_large")
    try:
        return json.loads(body)
    except (UnicodeError, ValueError) as error:
        raise RunnerFailure("invalid_worker_json") from error


class _Capture:
    def __init__(self, limit):
        self.limit = limit
        self.tail = bytearray()
        self.total = 0
        self.digest = hashlib.sha256()
        self.failed = False

    def read(self, stream):
        try:
            while block := stream.read(8192):
                self.total += len(block)
                self.digest.update(block)
                self.tail.extend(block)
                if len(self.tail) > self.limit:
                    del self.tail[:len(self.tail) - self.limit]
        except (OSError, ValueError):
            self.failed = True

    def summary(self):
        return {"bytes": self.total, "sha256": self.digest.hexdigest(),
                "retained_bytes": len(self.tail), "truncated": self.total > self.limit,
                "read_failed": self.failed}


class _WindowsJob:
    """An owned kill-on-close job, including descendants after worker exit."""
    def __init__(self, process):
        from ctypes import wintypes

        class Basic(ctypes.Structure):
            _fields_ = [("PerProcessUserTimeLimit", ctypes.c_longlong),
                        ("PerJobUserTimeLimit", ctypes.c_longlong),
                        ("LimitFlags", wintypes.DWORD),
                        ("MinimumWorkingSetSize", ctypes.c_size_t),
                        ("MaximumWorkingSetSize", ctypes.c_size_t),
                        ("ActiveProcessLimit", wintypes.DWORD),
                        ("Affinity", ctypes.c_size_t), ("PriorityClass", wintypes.DWORD),
                        ("SchedulingClass", wintypes.DWORD)]

        class IO(ctypes.Structure):
            _fields_ = [(name, ctypes.c_ulonglong) for name in
                        ("ReadOperationCount", "WriteOperationCount", "OtherOperationCount",
                         "ReadTransferCount", "WriteTransferCount", "OtherTransferCount")]

        class Extended(ctypes.Structure):
            _fields_ = [("BasicLimitInformation", Basic), ("IoInfo", IO),
                        ("ProcessMemoryLimit", ctypes.c_size_t),
                        ("JobMemoryLimit", ctypes.c_size_t),
                        ("PeakProcessMemoryUsed", ctypes.c_size_t),
                        ("PeakJobMemoryUsed", ctypes.c_size_t)]

        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.CreateJobObjectW.argtypes = [ctypes.c_void_p, wintypes.LPCWSTR]
        kernel.CreateJobObjectW.restype = wintypes.HANDLE
        kernel.SetInformationJobObject.argtypes = [wintypes.HANDLE, ctypes.c_int,
                                                  ctypes.c_void_p, wintypes.DWORD]
        kernel.SetInformationJobObject.restype = wintypes.BOOL
        kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        kernel.OpenProcess.restype = wintypes.HANDLE
        kernel.AssignProcessToJobObject.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
        kernel.AssignProcessToJobObject.restype = wintypes.BOOL
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        kernel.CloseHandle.restype = wintypes.BOOL
        self.kernel = kernel
        self.handle = kernel.CreateJobObjectW(None, None)
        if not self.handle:
            raise RunnerFailure("process_owner_unavailable")
        limits = Extended()
        limits.BasicLimitInformation.LimitFlags = 0x2000  # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        owned_process = None
        try:
            if not kernel.SetInformationJobObject(self.handle, 9, ctypes.byref(limits), ctypes.sizeof(limits)):
                raise RunnerFailure("process_owner_unavailable")
            owned_process = kernel.OpenProcess(0x0100 | 0x0001, False, process.pid)
            if not owned_process or not kernel.AssignProcessToJobObject(self.handle, owned_process):
                raise RunnerFailure("process_owner_unavailable")
        except BaseException:
            self.close()
            raise
        finally:
            if owned_process:
                kernel.CloseHandle(owned_process)

    def close(self):
        if self.handle:
            self.kernel.CloseHandle(self.handle)
            self.handle = None


def _kill_unowned(process):
    if os.name == "nt":
        subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                       stderr=subprocess.DEVNULL, timeout=10, check=False,
                       creationflags=subprocess.CREATE_NO_WINDOW)
    else:
        with contextlib.suppress(ProcessLookupError):
            os.killpg(process.pid, signal.SIGTERM)
        if process.poll() is None:
            with contextlib.suppress(subprocess.TimeoutExpired):
                process.wait(timeout=2)
        with contextlib.suppress(ProcessLookupError):
            os.killpg(process.pid, signal.SIGKILL)


def run_owned_process(argv, *, cwd, timeout, env=None, capture_limit=CAPTURE_BYTES,
                      input_bytes=None, terminate_on_output_limit=False, on_poll=None):
    """Start once, drain bounded captures, and own the entire process tree."""
    if not argv or timeout <= 0 or capture_limit < 1:
        raise RunnerFailure("invalid_process_request")
    if input_bytes is not None and (not isinstance(input_bytes, bytes) or len(input_bytes) > 64 * 1024):
        raise RunnerFailure("invalid_process_input")
    options = {"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP | subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {"start_new_session": True}
    started = time.monotonic()
    process = subprocess.Popen([str(item) for item in argv], cwd=str(cwd), env=env,
                               stdin=subprocess.DEVNULL if input_bytes is None else subprocess.PIPE,
                               stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, shell=False, **options)
    owner = None
    captures = [_Capture(capture_limit), _Capture(capture_limit)]
    threads = []
    input_errors = []
    timed_out = interrupted = output_limit = False
    supervisor_error = None
    try:
        if os.name == "nt":
            try:
                owner = _WindowsJob(process)
            except BaseException:
                _kill_unowned(process)
                process.wait(timeout=10)
                raise
        for stream, capture in zip((process.stdout, process.stderr), captures):
            thread = threading.Thread(target=capture.read, args=(stream,), daemon=False)
            thread.start()
            threads.append(thread)
        if input_bytes is not None:
            def write_input():
                try:
                    process.stdin.write(input_bytes)
                    process.stdin.flush()
                except (OSError, ValueError):
                    input_errors.append(True)
                finally:
                    with contextlib.suppress(OSError, ValueError):
                        process.stdin.close()
            thread = threading.Thread(target=write_input, daemon=False)
            thread.start()
            threads.append(thread)
        try:
            while process.poll() is None:
                if on_poll is not None:
                    try:
                        on_poll(process)
                    except intervention_control.SupervisorFailure as error:
                        supervisor_error = error.code
                        break
                if terminate_on_output_limit and any(capture.total > capture.limit for capture in captures):
                    output_limit = True
                    break
                remaining = timeout - (time.monotonic() - started)
                if remaining <= 0:
                    timed_out = True
                    break
                try:
                    process.wait(timeout=min(0.05, remaining))
                except subprocess.TimeoutExpired:
                    pass
        except KeyboardInterrupt:
            interrupted = True
    finally:
        if owner is not None:
            owner.close()
        elif os.name != "nt" or process.poll() is None:
            _kill_unowned(process)
        process.wait(timeout=10)
        for thread in threads:
            thread.join(timeout=10)
        for stream in (process.stdout, process.stderr):
            stream.close()
        for thread in threads:
            if thread.is_alive():
                thread.join(timeout=2)
        if any(thread.is_alive() for thread in threads):
            raise RunnerFailure("process_pipe_cleanup_failed")
    output_limit = output_limit or (terminate_on_output_limit and any(c.total > c.limit for c in captures))
    result = {"process_id": process.pid, "exit_code": process.returncode, "timed_out": timed_out,
              "interrupted": interrupted, "duration_seconds": round(time.monotonic() - started, 6),
              "output_limit": output_limit, "stdin_write_failed": bool(input_errors),
              "supervisor_error": supervisor_error,
              "stdout": captures[0].summary(), "stderr": captures[1].summary()}
    # Tails exist only in this transient return value; never include them in report.json.
    return result, bytes(captures[0].tail), bytes(captures[1].tail)


class GpuLock:
    """OS lock authority belongs to the live open handle, never file existence."""
    def __init__(self, path):
        self.path = Path(path)
        self.file = None

    def __enter__(self):
        self.file = self.path.open("a+b")
        try:
            self.file.seek(0, os.SEEK_END)
            if self.file.tell() == 0:
                self.file.write(b"\0")
                self.file.flush()
            self.file.seek(0)
            if os.name == "nt":
                import msvcrt
                msvcrt.locking(self.file.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl
                fcntl.flock(self.file.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as error:
            self.file.close()
            self.file = None
            raise RunnerFailure("gpu_lock_busy") from error
        return self

    def __exit__(self, *unused):
        if self.file is not None:
            if os.name == "nt":
                import msvcrt
                self.file.seek(0)
                msvcrt.locking(self.file.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                import fcntl
                fcntl.flock(self.file.fileno(), fcntl.LOCK_UN)
            self.file.close()
            self.file = None


def _metadata_json(path, payload=None):
    if path not in ("/api/version", "/api/tags", "/api/show", "/api/ps"):
        raise RunnerFailure("invalid_metadata_endpoint")
    request = urllib.request.Request(ORIGIN + path,
                                     data=None if payload is None else canonical_json(payload),
                                     headers={"Content-Type": "application/json"},
                                     method="GET" if payload is None else "POST")
    # No redirect is followed: even metadata must stay on the fixed loopback origin.
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            return None
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    try:
        with opener.open(request, timeout=10) as response:
            body = response.read(MAX_JSON_BYTES + 1)
        if len(body) > MAX_JSON_BYTES:
            raise RunnerFailure("metadata_too_large")
        return json.loads(body)
    except RunnerFailure:
        raise
    except (OSError, ValueError) as error:
        raise RunnerFailure("metadata_unavailable") from error


def model_metadata(model):
    if model not in MODELS:
        raise RunnerFailure("invalid_model")
    version = _metadata_json("/api/version")
    tags = _metadata_json("/api/tags")
    if not isinstance(version, dict) or not isinstance(tags, dict) or not isinstance(tags.get("models"), list):
        raise RunnerFailure("invalid_model_metadata")
    selected = [entry for entry in tags.get("models", []) if entry.get("name") == model]
    if len(selected) != 1 or not re.fullmatch(r"[0-9a-f]{64}", str(selected[0].get("digest", ""))):
        raise RunnerFailure("model_not_installed_or_ambiguous")
    tag = selected[0]
    show = _metadata_json("/api/show", {"model": model, "verbose": False})
    loaded = _metadata_json("/api/ps")
    if not isinstance(show, dict) or not isinstance(loaded, dict) or not isinstance(loaded.get("models"), list):
        raise RunnerFailure("invalid_model_metadata")
    information = show.get("model_info", {})
    modelfile = str(show.get("modelfile", ""))
    weight = re.search(r"(?im)^FROM .*[/\\]sha256-([0-9a-f]{64})\s*$", modelfile)
    parameters = str(show.get("parameters") or "")
    numeric_parameters = {}
    for line in parameters.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[0] in ("temperature", "top_p", "top_k", "min_p", "num_ctx", "num_predict", "repeat_penalty", "seed"):
            try:
                numeric_parameters[parts[0]] = float(parts[1])
            except ValueError:
                pass
    return {"tag": model, "digest": tag["digest"], "size_bytes": tag.get("size"),
            "details": {key: tag.get("details", {}).get(key) for key in
                        ("format", "family", "parameter_size", "quantization_level", "context_length", "runner")},
            "server_version": version.get("version"),
            "capabilities": show.get("capabilities", []),
            "template_sha256": sha256_bytes(str(show.get("template") or "").encode()),
            "system_sha256": sha256_bytes(str(show.get("system") or "").encode()),
            "inherited_numeric_parameters": numeric_parameters,
            "parameters_sha256": sha256_bytes(parameters.encode()),
            "model_info_sha256": sha256_bytes(canonical_json(information)),
            "weights_blob_sha256": weight.group(1) if weight else None,
            "selected_model_loaded": [{key: item.get(key) for key in
                                ("name", "digest", "context_length", "size_vram")}
                               for item in loaded.get("models", []) if item.get("name") == model],
            "other_models_loaded_count": sum(item.get("name") != model for item in loaded.get("models", []))}


def runtime_metadata(repository):
    versions = {"python": platform.python_version(), "os": platform.platform(),
                "machine": platform.machine(), "processor": platform.processor(),
                "logical_cpus": os.cpu_count()}
    if os.name == "nt":
        from ctypes import wintypes
        class MemoryStatus(ctypes.Structure):
            _fields_ = [("length", wintypes.DWORD), ("load", wintypes.DWORD)] + [
                (name, ctypes.c_ulonglong) for name in ("total_physical", "available_physical",
                "total_pagefile", "available_pagefile", "total_virtual", "available_virtual", "extended")]
        memory = MemoryStatus()
        memory.length = ctypes.sizeof(memory)
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.GlobalMemoryStatusEx.argtypes = [ctypes.POINTER(MemoryStatus)]
        kernel.GlobalMemoryStatusEx.restype = wintypes.BOOL
        versions["physical_memory_bytes"] = memory.total_physical if kernel.GlobalMemoryStatusEx(ctypes.byref(memory)) else None
    else:
        with contextlib.suppress(OSError, ValueError):
            versions["physical_memory_bytes"] = os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_PHYS_PAGES")
    for label, command in (("git", ["git", "--version"]), ("cargo", ["cargo", "--version"]),
                           ("rustc", ["rustc", "--version"]),
                           ("git_commit", ["git", "rev-parse", "HEAD"])):
        try:
            result, output, _ = run_owned_process(command, cwd=repository, timeout=10, capture_limit=1024)
            versions[label] = output.decode("utf-8", "replace").strip() if result["exit_code"] == 0 else "unavailable"
        except (OSError, RunnerFailure):
            versions[label] = "unavailable"
    try:
        result, output, _ = run_owned_process(
            ["nvidia-smi", "--query-gpu=name,driver_version,memory.total", "--format=csv,noheader"],
            cwd=repository, timeout=10, capture_limit=2048)
        versions["gpu"] = output.decode("utf-8", "replace").strip().splitlines() if result["exit_code"] == 0 else []
    except (OSError, RunnerFailure):
        versions["gpu"] = []
    return versions


def validate_case(fields, case_id, variant):
    if not isinstance(fields, dict) or fields.get("case_id", case_id) != case_id or fields.get("variant", variant) != variant:
        raise RunnerFailure("invalid_case_identity")
    if fields.get("mode") not in ("agent", "ask"):
        raise RunnerFailure("invalid_case_mode")
    objective = fields.get("objective")
    if not isinstance(objective, str) or not objective.strip() or len(objective.encode()) > 64 * 1024:
        raise RunnerFailure("invalid_case_objective")
    paths = fields.get("allowed_write_paths", [])
    if not isinstance(paths, list) or len(paths) > 64:
        raise RunnerFailure("invalid_case_write_paths")
    for item in paths:
        if (not isinstance(item, str) or not item or len(item.encode()) > 4096 or
                "\\" in item or ":" in item or item.startswith("/") or
                any(part in ("", ".", "..") for part in item.split("/")) or
                any(ord(char) < 32 for char in item)):
            raise RunnerFailure("invalid_case_write_paths")
    processes = []
    for item in fields.get("allowed_processes", []):
        if not isinstance(item, dict) or item.get("cwd", ".") != ".":
            raise RunnerFailure("invalid_case_process")
        argv = item.get("argv")
        if argv is None and "executable" in item:
            argv = [item["executable"], *item.get("arguments", [])]
        if (not isinstance(argv, list) or not 1 <= len(argv) <= 32 or
                any(not isinstance(arg, str) or "\0" in arg for arg in argv) or
                not re.fullmatch(r"[A-Za-z0-9_.-]{1,128}", argv[0]) or
                any("\r" in arg or "\n" in arg for arg in argv) or
                sum(len(arg.encode()) for arg in argv) > 8192):
            raise RunnerFailure("invalid_case_process")
        processes.append({"executable": argv[0], "arguments": argv[1:]})
    if len(processes) > 32 or (fields["mode"] == "ask" and (paths or processes)):
        raise RunnerFailure("invalid_case_capability")
    effects = fields.get("process_effect_paths", [])
    if (not isinstance(effects, list) or len(effects) > 1 or
            any(item != ".a3-probe-notes.json" for item in effects) or
            (effects and case_id != "A05")):
        raise RunnerFailure("invalid_case_process_effect")
    networks = fields.get("allowed_network_requests", [])
    if not isinstance(networks, list) or len(networks) > 12 or (networks and case_id != "A04"):
        raise RunnerFailure("invalid_case_network_scope")
    seen_network = set()
    for entry in networks:
        if (not isinstance(entry, dict) or set(entry) != {"method", "url"} or
                entry.get("method") not in ("GET", "POST", "PUT", "DELETE", "PATCH") or
                entry.get("url") not in ("http://127.0.0.1:18765/", "http://127.0.0.1:18765/health",
                                         "http://127.0.0.1:18765/missing", "http://127.0.0.1:8765/",
                                         "http://127.0.0.1:8765/health", "http://127.0.0.1:8765/missing")):
            raise RunnerFailure("invalid_case_network_scope")
        pair = (entry["method"], entry["url"])
        if pair in seen_network:
            raise RunnerFailure("invalid_case_network_scope")
        seen_network.add(pair)
    if fields["mode"] == "ask" and (effects or networks):
        raise RunnerFailure("invalid_case_capability")
    return {"mode": "Agent" if fields["mode"] == "agent" else "Ask", "objective": objective,
            "allowed_write_paths": paths, "allowed_processes": processes,
            "process_effect_paths": effects, "allowed_network_requests": networks}


def _git_initialize(worktree):
    environment = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    environment.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
                       GIT_CONFIG_SYSTEM=os.devnull, GIT_TERMINAL_PROMPT="0")
    for command in (["git", "-c", "core.hooksPath=", "-c", "init.templateDir=", "init", "--initial-branch=main"],
                    ["git", "-c", "core.hooksPath=", "add", "--all"]):
        result, _, _ = run_owned_process(command, cwd=worktree, timeout=30, env=environment)
        if result["exit_code"] != 0 or result["timed_out"]:
            raise RunnerFailure("fixture_git_initialization_failed")


def _safe_worker(value):
    if not isinstance(value, dict):
        raise RunnerFailure("invalid_worker_report")
    result = {}
    if "order_evidence" in value:
        result["order_validation"] = intervention_control.order_trace.validate(value["order_evidence"])
    legacy = intervention_control.safe_legacy_failure(value.get("legacy_failure_evidence"))
    if legacy is not None:
        result["legacy_failure_evidence"] = legacy
    metadata = intervention_control.safe_agent_answer_metadata(value.get("agent_answer_metadata"))
    if metadata is not None:
        result["agent_answer_metadata"] = metadata
    if type(value.get("agent_answer_truncated")) is bool:
        result["agent_answer_truncated"] = value["agent_answer_truncated"]
    if isinstance(value.get("agent_answer"), str):
        result["agent_answer_bytes"] = len(value["agent_answer"].encode())
        result["agent_answer_sha256"] = sha256_bytes(value["agent_answer"].encode())
    if "effect_scope" in value:
        scope = value["effect_scope"]
        names = ("patch_path_count", "exact_process_count", "process_effect_path_count", "exact_network_request_count")
        if not isinstance(scope, dict) or any(not intervention_control.count(scope.get(key), 64) for key in names):
            raise RunnerFailure("invalid_worker_effect_scope")
        result["effect_scope"] = {key: scope[key] for key in names}
    if "report_projection_truncated" in value:
        if type(value["report_projection_truncated"]) is not bool:
            raise RunnerFailure("invalid_worker_projection_flag")
        result["report_projection_truncated"] = value["report_projection_truncated"]
    if "projection_records_omitted" in value:
        omitted = value["projection_records_omitted"]
        if not isinstance(omitted, dict) or any(not intervention_control.count(item, 1_000_000)
                for key, item in omitted.items() if key in ("journal", "metrics")):
            raise RunnerFailure("invalid_worker_projection_counts")
        result["projection_records_omitted"] = {key: omitted[key] for key in ("journal", "metrics") if key in omitted}
    proof = intervention_control.safe_evidence(value.get("observation_evidence"))
    if proof is not None:
        result["observation_evidence"] = proof
    scope = intervention_control.safe_denied_scope(value.get("denied_scope"))
    if scope is not None:
        result["denied_scope"] = scope
    if "journal" in value:
        try:
            result["journal"] = intervention_control.safe_journal(value["journal"])
        except intervention_control.SupervisorFailure as error:
            raise RunnerFailure(error.code) from error
    scalar_keys = ("version", "status", "stage", "phase", "case_id", "variant", "mode", "controller_status",
                   "error_code", "reason_code", "done", "completed", "calls", "model_calls",
                   "prompt_tokens", "output_tokens", "tool_calls", "repairs", "mutations",
                   "turns", "context_limit", "output_limit", "structured_output_verified",
                   "worker_attempt", "terminal", "worker_succeeded", "failure_code",
                   "formal_completed", "oracle_evaluated", "session_state", "run_state", "harness_status", "engine",
                   "exact_grants", "elapsed_millis", "probe_elapsed_millis", "task_elapsed_millis",
                   "ask_answer_truncated", "product_failed", "product_halted", "executor_failure", "product_stop_reason")
    for key in scalar_keys:
        item = value.get(key)
        if isinstance(item, (bool, int, float)) or item is None:
            if key in value:
                result[key] = item
        elif isinstance(item, str) and len(item) <= 160 and re.fullmatch(r"[A-Za-z0-9_.:/ -]*", item):
            result[key] = item
    for key in ("usage", "run_usage", "counters", "request_counts", "cleanup", "interventions", "profile"):
        if isinstance(value.get(key), dict):
            result[key] = {name: item for name, item in value[key].items()
                           if isinstance(name, str) and re.fullmatch(r"[a-zA-Z0-9_]{1,80}", name)
                           and isinstance(item, (bool, int, float)) and not isinstance(item, str)}
    ledger = value.get("ledger")
    if isinstance(ledger, dict):
        result["ledger"] = {key: ledger[key] for key in ("revision", "replans")
                            if isinstance(ledger.get(key), int)}
        result["ledger"]["steps"] = []
        for item in ledger.get("steps", [])[:256]:
            status = item.get("status") if isinstance(item, dict) else item
            if isinstance(status, str) and re.fullmatch(r"[A-Za-z_]{1,80}", status):
                result["ledger"]["steps"].append(status)
        result["ledger"]["verification"] = []
        for item in ledger.get("verification", [])[:256]:
            if not isinstance(item, dict):
                raise RunnerFailure("invalid_worker_verification_summary")
            method = item.get("method")
            if not isinstance(method, str) or not re.fullmatch(r"[A-Za-z_]{1,80}", method):
                raise RunnerFailure("invalid_worker_verification_method")
            summary = {"method": method}
            for key in ("acceptance_criteria_count", "evidence_count"):
                count = item.get(key)
                if type(count) is not int or not 0 <= count <= 65536:
                    raise RunnerFailure("invalid_worker_verification_count")
                summary[key] = count
            passed = item.get("passed")
            if passed is not None and type(passed) is not bool:
                raise RunnerFailure("invalid_worker_verification_result")
            summary["passed"] = passed
            result["ledger"]["verification"].append(summary)
    proof = value.get("product_verification")
    if isinstance(proof, dict):
        count = proof.get("evidence_count")
        if type(count) is not int or not 0 <= count <= 65536:
            raise RunnerFailure("invalid_worker_evidence_count")
        if type(proof.get("truncated")) is not bool or proof.get("external_oracle_evidence_imported") is not False:
            raise RunnerFailure("invalid_worker_evidence_boundary")
        evidence = proof.get("evidence")
        if not isinstance(evidence, list) or len(evidence) > 128:
            raise RunnerFailure("invalid_worker_evidence_set")
        cleaned = []
        for artifact in evidence:
            if (not isinstance(artifact, dict) or artifact.get("kind") not in
                    ("command", "test", "diff", "diagnostic", "user_confirmation") or
                    type(artifact.get("fresh")) is not bool):
                raise RunnerFailure("invalid_worker_evidence_kind")
            cleaned.append({"kind": artifact["kind"], "fresh": artifact["fresh"]})
        result["product_verification"] = {"evidence_count": count, "truncated": proof["truncated"],
                                          "evidence": cleaned, "external_oracle_evidence_imported": False}
    attempts = value.get("mutation_attempts")
    if isinstance(attempts, list):
        result["mutation_attempt_count"] = len(attempts)
        result["mutation_attempts"] = []
        for attempt in attempts[:256]:
            if not isinstance(attempt, dict):
                raise RunnerFailure("invalid_worker_mutation_summary")
            summary = {key: attempt[key] for key in ("kind", "status", "application_state")
                       if isinstance(attempt.get(key), str) and
                       re.fullmatch(r"[A-Za-z_]{1,80}", attempt[key])}
            if isinstance(attempt.get("requires_reconciliation"), bool):
                summary["requires_reconciliation"] = attempt["requires_reconciliation"]
            result["mutation_attempts"].append(summary)
    metrics = value.get("metrics")
    if isinstance(metrics, dict):
        result["metrics"] = {key: item for key, item in metrics.items()
                             if (item is None or isinstance(item, (bool, int, float))) and
                             isinstance(key, str) and re.fullmatch(r"[a-zA-Z0-9_]{1,80}", key)}
        result["metrics"]["records"] = []
        for item in metrics.get("records", [])[:256]:
            if not isinstance(item, dict):
                raise RunnerFailure("invalid_worker_metrics")
            record = {key: item[key] for key in ("message_bytes", "schema_bytes", "output_bytes",
                      "prompt_tokens", "output_tokens", "first_output_millis", "elapsed_millis")
                      if isinstance(item.get(key), (int, float))}
            for key in ("stage", "finish", "status", "failure"):
                if isinstance(item.get(key), str) and re.fullmatch(r"[a-zA-Z0-9_^ .:-]{1,80}", item[key]):
                    record[key] = item[key]
            for key in ("message_blake3", "schema_blake3"):
                if intervention_control.is_digest(item.get(key)):
                    record[key] = item[key]
                elif key == "schema_blake3" and item.get(key) is None:
                    record[key] = None
            result["metrics"]["records"].append(record)
    answer = value.get("ask_answer")
    if isinstance(answer, str):
        result["answer_bytes"] = len(answer.encode())
        result["answer_sha256"] = sha256_bytes(answer.encode())
    return result


def build_worker(repository):
    command = ["cargo", "test", "-p", "a3-desktop", "--lib", "--offline", "--locked",
               "--no-run", "--message-format=json"]
    result, output, _ = run_owned_process(command, cwd=repository, timeout=1800,
                                         capture_limit=4 * 1024 * 1024)
    if result["exit_code"] != 0 or result["timed_out"] or result["interrupted"]:
        raise RunnerFailure("worker_build_failed")
    candidates = set()
    for line in output.splitlines():
        try:
            item = json.loads(line)
        except (ValueError, UnicodeError):
            continue
        if (item.get("reason") == "compiler-artifact" and item.get("profile", {}).get("test")
                and item.get("target", {}).get("name") in ("a3_desktop", "a3-desktop") and item.get("executable")):
            candidates.add(Path(item["executable"]).resolve())
    if len(candidates) != 1:
        raise RunnerFailure("worker_binary_not_identified")
    return candidates.pop()


def freeze_worker(repository, source):
    """Publish a content-addressed copy once; compilation cannot replace it."""
    repository, source = Path(repository).resolve(), Path(source).resolve()
    digest = file_sha256(source)
    destination_dir = repository / "target" / "plan22-frozen" / digest
    destination_dir.mkdir(parents=True, exist_ok=True)
    if not destination_dir.resolve().is_relative_to(repository):
        raise RunnerFailure("frozen_root_outside_repository")
    destination = destination_dir / ("worker.exe" if os.name == "nt" else "worker")
    if destination.exists():
        if not destination.is_file() or file_sha256(destination) != digest:
            raise RunnerFailure("frozen_worker_integrity_failed")
        return destination, digest
    temporary = destination_dir / ("worker-" + uuid.uuid4().hex + ".tmp")
    try:
        with source.open("rb") as incoming, temporary.open("xb") as output:
            shutil.copyfileobj(incoming, output, length=64 * 1024)
            output.flush()
            os.fsync(output.fileno())
        if file_sha256(temporary) != digest:
            raise RunnerFailure("worker_changed_while_freezing")
        temporary.chmod(source.stat().st_mode & 0o777)
        try:
            # Atomic, no-replace publication on the same filesystem.
            os.link(temporary, destination)
        except FileExistsError:
            if file_sha256(destination) != digest:
                raise RunnerFailure("frozen_worker_integrity_failed")
    finally:
        with contextlib.suppress(FileNotFoundError):
            temporary.unlink()
    if file_sha256(destination) != digest:
        raise RunnerFailure("frozen_worker_integrity_failed")
    return destination, digest


def harness_source_hashes(repository):
    repository = Path(repository)
    paths = [repository / "apps/desktop/src-tauri/src/plan22_baseline.rs",
             repository / "apps/desktop/src-tauri/src/plan22_interventions.rs",
             repository / "apps/desktop/src-tauri/src/agent_session_manager.rs"]
    native_directory = repository / "apps/desktop/src-tauri/src/plan22_baseline"
    if native_directory.is_dir():
        paths.extend(sorted(native_directory.rglob("*.rs")))
    paths.extend(repository / "fixtures/local-agent-eval-v1" / name for name in
                 ("run_baseline.py", "supervisor.py", "order_trace.py", "cases.py", "oracle.py", "test_runner.py", "test_supervisor.py", "test_order_trace.py", "baseline.json"))
    if len(paths) > 64:
        raise RunnerFailure("too_many_harness_sources")
    return {path.relative_to(repository).as_posix(): file_sha256(path)
            for path in paths if path.is_file()}


def aggregate_metrics(stages):
    """Native resume restores attempt-wide counters; summing stages duplicates usage."""
    metrics = [stage.get("report", {}).get("metrics", {}) for stage in stages]
    result = {"process_stage_count": len(stages), "counter_scope": "attempt_cumulative"}
    for key in ("calls", "probe_requests", "completed", "stream_errors", "output_bytes",
                "message_utf8_bytes", "format_schema_utf8_bytes", "reported_prompt_tokens_sum",
                "reported_output_tokens_sum", "missing_prompt_usage_calls", "missing_output_usage_calls"):
        values = [item.get(key) for item in metrics]
        known = bool(values) and all(type(value) is int and value >= 0 for value in values)
        if known and any(after < before for before, after in zip(values, values[1:])):
            raise RunnerFailure("native_metrics_regressed")
        result[key] = values[-1] if known else None
    for key in ("provider_prompt_tokens", "provider_output_tokens"):
        values = [item.get(key) for item in metrics]
        known = bool(values) and all(type(value) is int and value >= 0 for value in values)
        if known and any(after < before for before, after in zip(values, values[1:])):
            raise RunnerFailure("native_metrics_regressed")
        result[key] = values[-1] if known else None
    return result


def stage_summary(native, artifact):
    result = _safe_worker(native)
    # Full native stage reports stay in worker-start.json / worker.json. Avoid
    # duplicating bounded but large history arrays inside report.json.
    for key in ("journal", "ledger", "product_verification", "mutation_attempts"):
        result.pop(key, None)
    if "metrics" in result:
        result["metrics"].pop("records", None)
    return {"report": result, "report_artifact": artifact}


def run_attempt(*, repository, run_dir, model, case_id, variant, phase, worker,
                materialize, evaluate, metadata_provider=model_metadata,
                runtime_provider=runtime_metadata, native_timeout=None, worker_command=None,
                gpu_lock_path=None, worker_timeout=None, freeze_provider=load_series_freeze,
                supervisor_factory=None):
    """Dependency seams are for offline runner tests; CLI never bypasses metadata."""
    repository, run_dir = Path(repository).resolve(), Path(run_dir).resolve()
    if model not in MODELS or case_id not in CASES or variant not in ("development", "holdout") or phase not in ("pilot", "baseline"):
        raise RunnerFailure("invalid_attempt_selection")
    native_timeout = task_budget(case_id) if native_timeout is None else native_timeout
    if not 0 < native_timeout <= 3600:
        raise RunnerFailure("invalid_attempt_timeout")
    if not run_dir.is_dir() or any(run_dir.iterdir()):
        raise RunnerFailure("attempt_directory_not_empty")
    context, output = MODELS[model]
    started = time.monotonic()
    report = {"schema": SCHEMA, "version": 1, "attempt_id": run_dir.name,
              "started_at": utc_now(), "phase": phase, "case_id": case_id,
              "variant": variant, "model": model, "status": "preparing", "worker_starts": 0,
              "profile": {"context_limit": context, "output_limit": output,
                          "schema_grounding": "FormatFieldOnly", "temperature_milli": 0,
                          "top_p_milli": 1000, "thinking": False, "parallelism": 1},
              "native_timeout_seconds": native_timeout, "probe_timeout_seconds": 120,
              "outer_worker_timeout_seconds": native_timeout + 150 if worker_timeout is None else worker_timeout,
              "independently_correct": False, "independent_status": "not_evaluated"}
    report["baseline_commit"] = BASELINE_COMMIT
    report["baseline_tree"] = BASELINE_TREE
    report["outer_attempt_worker_budget_seconds"] = (native_timeout + (300 if case_id == "A10" else 150)
                                                      if worker_timeout is None else worker_timeout)
    report_path = run_dir / "report.json"
    atomic_json(report_path, report)
    try:
        fields = materialize(case_id, variant, run_dir)
        request_fields = validate_case(fields, case_id, variant)
        worktree = run_dir / "worktree"
        if not worktree.is_dir() or worktree.resolve().parent != run_dir:
            raise RunnerFailure("invalid_fixture_worktree")
        (run_dir / "appdata").mkdir(exist_ok=False)
        _git_initialize(worktree)
        report["runtime"] = runtime_provider(repository)
        resolved_worker = Path(worker).resolve() if worker is not None else build_worker(repository)
        if not resolved_worker.is_file():
            raise RunnerFailure("worker_binary_missing")
        report["source_worker_binary"] = str(resolved_worker)
        frozen_worker, digest = freeze_worker(repository, resolved_worker)
        report["worker_binary_sha256"] = digest
        report["frozen_worker_binary"] = str(frozen_worker)
        report["harness_sources_sha256"] = harness_source_hashes(repository)
        series_freeze = freeze_provider()
        report["series_freeze_sha256"] = sha256_bytes(canonical_json(series_freeze))
        report["model_metadata"] = metadata_provider(model)
        expected_model = series_freeze.get("models", {}).get(model, {})
        if (any(report["model_metadata"].get(key) != expected_model.get(key)
                for key in ("digest", "template_sha256", "parameters_sha256")) or
                (series_freeze.get("ollama_version") is not None and
                 report["model_metadata"].get("server_version") != series_freeze["ollama_version"]) or
                expected_model.get("context_limit") != context or expected_model.get("output_limit") != output):
            raise RunnerFailure("metadata_mismatch")
        request = {"version": 2, "case_id": case_id, "variant": variant,
                   "model": model, "context_limit": context, "output_limit": output,
                   "timeout_seconds": native_timeout, **request_fields}
        atomic_json(run_dir / "request.json", request)
        report["request_sha256"] = sha256_bytes(canonical_json(request))
        report["request_file_sha256"] = file_sha256(run_dir / "request.json")
        report["fixture"] = {key: fields[key] for key in
                             ("protected_file_sha256", "expected_test_ids", "required_observations")
                             if key in fields}
        report["status"] = "ready"
        atomic_json(report_path, report)
        environment = {key: value for key, value in os.environ.items() if not key.startswith("A3_")}
        environment["A3_PLAN22_RUN_DIR"] = str(run_dir)
        environment["A3_PLAN22_STAGE"] = "start"
        command = worker_command or [str(frozen_worker), "--exact", WORKER_TEST,
                                     "--ignored", "--nocapture", "--test-threads=1"]
        lock_path = gpu_lock_path or Path(tempfile.gettempdir()) / "a3-plan22-ollama-gpu.lock"
        report["preflight_elapsed_seconds"] = round(time.monotonic() - started, 6)
        if supervisor_factory is None:
            fixture_cases = importlib.import_module("cases")
            supervision = intervention_control.Supervisor(run_dir, case_id, variant,
                fixture_cases.fixture_files(case_id, variant), fixture_cases.intervention_files(case_id, variant))
        else:
            supervision = supervisor_factory(run_dir, case_id, variant)
        report["native_processes"] = []
        report["worker_stages"] = []
        report["native_retry_count"] = 0
        report["deliberate_restarts"] = 0
        with GpuLock(lock_path):
            worker_budget_started = time.monotonic()
            report["status"] = "running"
            report["worker_starts"] = 1
            atomic_json(report_path, report)
            if file_sha256(frozen_worker) != digest:
                raise RunnerFailure("frozen_worker_integrity_failed")
            process, _, _ = run_owned_process(command, cwd=repository,
                                              timeout=report["outer_worker_timeout_seconds"],
                                              env=environment, on_poll=supervision.poll)
            report["native_process"] = process
            report["native_processes"].append({"stage": "start", **process})
            native = read_json(run_dir / "worker.json") if (run_dir / "worker.json").is_file() else {}
            supervision.record_process("start", process, native)
            report["worker_stages"].append({"stage": "start", **stage_summary(native, "worker.json")})
            report["aggregate_native_metrics"] = aggregate_metrics(report["worker_stages"])
            remaining = supervision.prepare_restart(native, process,
                request_file_sha256=report["request_file_sha256"], task_budget_seconds=native_timeout)
            if remaining is not None:
                report["worker_stages"][0]["report_artifact"] = "worker-start.json"
                # This is an explicit checkpoint continuation, never failure retry.
                before_resume = metadata_provider(model)
                if any(before_resume.get(key) != report["model_metadata"].get(key)
                       for key in ("digest", "template_sha256", "parameters_sha256", "server_version")):
                    raise RunnerFailure("metadata_mismatch")
                if file_sha256(frozen_worker) != digest:
                    raise RunnerFailure("frozen_worker_integrity_failed")
                resume_timeout = min(remaining + 150, report["outer_attempt_worker_budget_seconds"]
                                     - (time.monotonic() - worker_budget_started))
                if resume_timeout <= 0:
                    raise RunnerFailure("restart_deadline_exhausted")
                report["deliberate_restarts"] = 1
                report["worker_starts"] = 2
                report["restart"] = supervision.restart_receipt
                atomic_json(report_path, report)
                environment["A3_PLAN22_STAGE"] = "resume"
                process, _, _ = run_owned_process(command, cwd=repository,
                    timeout=resume_timeout,
                    env=environment, on_poll=supervision.poll)
                report["native_process"] = process
                report["native_processes"].append({"stage": "resume", **process})
                native = read_json(run_dir / "worker.json") if (run_dir / "worker.json").is_file() else {}
                supervision.record_process("resume", process, native)
                report["worker_stages"].append({"stage": "resume", **stage_summary(native, "worker.json")})
                report["aggregate_native_metrics"] = aggregate_metrics(report["worker_stages"])
                if native.get("phase") == "restart_ready":
                    raise RunnerFailure("restart_already_consumed")
        report["native_process"] = process
        if process["interrupted"]:
            report["status"] = "interrupted"
        elif process["timed_out"]:
            report["status"] = "timeout"
        elif process.get("supervisor_error"):
            report["status"] = "supervisor_error"
            report["error_code"] = process["supervisor_error"]
        elif process["exit_code"] != 0:
            report["status"] = "native_error"
        elif process["stdout"]["read_failed"] or process["stderr"]["read_failed"]:
            report["status"] = "capture_error"
        else:
            report["status"] = "worker_exited"
        worker_path = run_dir / "worker.json"
        native = {}
        if worker_path.is_file():
            native = read_json(worker_path)
            report["worker"] = _safe_worker(native)
            if native.get("case_id", case_id) != case_id or native.get("variant", variant) != variant:
                raise RunnerFailure("worker_identity_mismatch")
        elif report["status"] == "worker_exited":
            report["status"] = "missing_worker_report"
        # Only native journal-derived closed proofs plus our own scoped receipts.
        # In particular no worker/model manual_review is imported.
        observations = supervision.observations(native)
        report["trusted_observations"] = observations
        if case_id == "A10":
            report["order_validation"] = supervision.order_validation(native)
        if supervision.receipt is not None:
            report["intervention"] = supervision.receipt
        answer = intervention_control.bound_agent_answer(native) if case_id == "A12" else native.get("ask_answer")
        verdict = evaluate(case_id, variant, worktree, answer, observations)
        if not isinstance(verdict, dict) or not isinstance(verdict.get("passed"), bool):
            raise RunnerFailure("invalid_oracle_verdict")
        environment_failure = verdict.get("environment_failure")
        if environment_failure not in (None, "default_port_unavailable", "loopback_alias_unavailable",
                                       "default_listener_inspection_unavailable"):
            raise RunnerFailure("invalid_oracle_environment_failure")
        blocked_checks = [check for check in verdict.get("checks", [])
                          if isinstance(check, dict) and check.get("environment_blocked") is True]
        if ((environment_failure is not None and (verdict["passed"] or not blocked_checks))
                or any(environment_failure is None or check.get("id") != "oracle_environment"
                       or check.get("detail") != environment_failure or check.get("passed") is not False
                       for check in blocked_checks)):
            raise RunnerFailure("invalid_oracle_environment_failure")
        report["oracle"] = verdict
        report["independently_correct"] = verdict["passed"]
        report["independent_status"] = ("oracle_environment_failure" if environment_failure else
                                       "manual_review_pending" if verdict.get("manual_review_required")
                                        else "correct" if verdict["passed"] else "incorrect")
        native_success = (process["exit_code"] == 0 and not process["timed_out"] and
                          not process["interrupted"] and native.get("terminal") is True and
                          native.get("worker_succeeded") is True)
        review_pending = verdict.get("manual_review_required") is True
        known_failed = any(isinstance(check, dict) and check.get("passed") is False and
            check.get("environment_blocked") is not True and
            not (review_pending and check.get("id") == "operator_semantic_review_record")
            for check in verdict.get("checks", []))
        report["false_done"] = (native.get("formal_completed") is True and
            (known_failed or (not verdict["passed"] and not review_pending and not environment_failure)))
        report["false_done_review_pending"] = (native.get("formal_completed") is True and review_pending
                                                and not known_failed)
        report["false_done_environment_pending"] = (native.get("formal_completed") is True
                                                   and bool(environment_failure) and not known_failed)
        if report["status"] == "worker_exited":
            if not native_success:
                report["status"] = "native_failure"
            elif native.get("product_failed") is True or native.get("session_state") == "Failed":
                report["status"] = "product_failed"
            elif native.get("product_halted") is True:
                report["status"] = "product_halted"
            elif native.get("formal_completed") is not True:
                report["status"] = "product_halted" if native.get("session_state") == "AwaitingUser" else "incomplete"
            elif environment_failure:
                report["status"] = "oracle_environment_failure"
            elif request_fields["mode"] == "Ask" and (not isinstance(native.get("ask_answer"), str)
                    or not native["ask_answer"].strip() or native.get("ask_answer_truncated") is True):
                report["status"] = "incomplete"
            elif case_id == "A12" and (not isinstance(answer, str) or not answer.strip()):
                report["status"] = "incomplete"
            elif known_failed:
                report["status"] = "incorrect"
            elif verdict.get("manual_review_required"):
                report["status"] = "manual_review_pending"
            elif not verdict["passed"]:
                report["status"] = "incorrect"
            else:
                report["status"] = "passed" if native.get("formal_completed") is True else "incomplete"
        if (native.get("failure_code") == "intervention_not_implemented" or
                native.get("harness_status") == "harness_incomplete"):
            report["status"] = "harness_incomplete"
        # Tags such as :latest are mutable. Re-reading metadata performs no
        # generation and must not authorize a changed model as the same attempt.
    except KeyboardInterrupt:
        report["status"] = "interrupted"
    except RunnerFailure as error:
        report["status"] = "metadata_mismatch" if str(error) == "metadata_mismatch" else "runner_error"
        report["error_code"] = str(error)
    except intervention_control.SupervisorFailure as error:
        report["status"] = "supervisor_error"
        report["error_code"] = error.code
    except (OSError, ValueError, TypeError, KeyError):
        report["status"] = "runner_error"
        report["error_code"] = "runner_operation_failed"
    except Exception:
        report["status"] = "runner_error"
        report["error_code"] = "runner_internal_error"
    finally:
        if report["worker_starts"] and "model_metadata" in report:
            report["outcome_before_metadata_check"] = report["status"]
            try:
                report["model_metadata_after"] = metadata_provider(model)
                stable_fields = ("digest", "template_sha256", "system_sha256", "parameters_sha256",
                                 "model_info_sha256", "weights_blob_sha256", "server_version")
                report["metadata_stable"] = all(report["model_metadata"].get(key) ==
                                                 report["model_metadata_after"].get(key) for key in stable_fields)
                if not report["metadata_stable"]:
                    report["status"] = "metadata_changed"
            except (OSError, ValueError, RunnerFailure):
                report["metadata_stable"] = False
                report["status"] = "metadata_unavailable"
        report["finished_at"] = utc_now()
        report["case_elapsed_seconds"] = round(time.monotonic() - started, 6)
        atomic_json(report_path, report)
    return report


def find_repository(script):
    for candidate in Path(script).resolve().parents:
        if (candidate / "Cargo.toml").is_file() and (candidate / "AGENTS.md").is_file():
            return candidate
    raise RunnerFailure("repository_not_found")


def main(argv=None):
    if os.name != "nt" and __name__ == "__main__":
        def terminate(signum, frame):
            raise KeyboardInterrupt
        signal.signal(signal.SIGTERM, terminate)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", required=True, choices=tuple(MODELS))
    parser.add_argument("--case", required=True, choices=CASES)
    parser.add_argument("--variant", choices=("development", "holdout"), default="development")
    parser.add_argument("--phase", choices=("pilot", "baseline"), default="pilot")
    parser.add_argument("--worker", type=Path)
    args = parser.parse_args(argv)
    try:
        repository = find_repository(__file__)
        parent = repository / "target" / "plan22-runs"
        parent.mkdir(parents=True, exist_ok=True)
        if not parent.resolve().is_relative_to(repository):
            raise RunnerFailure("run_root_outside_repository")
        run_dir = parent / (dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ") + "-" + uuid.uuid4().hex)
        run_dir.mkdir(exist_ok=False)
        def materialize_case(case_id, variant, directory):
            return importlib.import_module("cases").materialize(case_id, variant, directory)

        def owned_oracle(case_id, variant, worktree, answer, observations):
            oracle_module = importlib.import_module("oracle")
            observations_file = run_dir / "oracle_observations.json"
            atomic_json(observations_file, observations)
            result, output, _ = run_owned_process(
                [sys.executable, str(Path(oracle_module.__file__).resolve()), "--case", case_id,
                 "--variant", variant, "--worktree", str(worktree),
                 "--worker-report", str(run_dir / "worker.json"),
                 "--observations-file", str(observations_file)], cwd=repository,
                timeout=120, capture_limit=256 * 1024, terminate_on_output_limit=True)
            if result["interrupted"]:
                raise KeyboardInterrupt
            if (result["timed_out"] or result["output_limit"] or result["stdout"]["truncated"]
                    or result["stdout"]["read_failed"] or result["stderr"]["read_failed"]):
                raise RunnerFailure("oracle_process_failed")
            try:
                verdict = json.loads(output)
            except (ValueError, UnicodeError) as error:
                raise RunnerFailure("invalid_oracle_output") from error
            verdict["process"] = result
            return verdict

        report = run_attempt(repository=repository, run_dir=run_dir, model=args.model,
                             case_id=args.case, variant=args.variant, phase=args.phase,
                             worker=args.worker, materialize=materialize_case,
                             evaluate=owned_oracle)
        print(json.dumps({"report": str(run_dir / "report.json"), "status": report["status"],
                          "independently_correct": report["independently_correct"]}))
        return 0 if report["status"] == "passed" else 1
    except (RunnerFailure, OSError, ImportError) as error:
        print(json.dumps({"status": "runner_error", "error_code": str(error) if isinstance(error, RunnerFailure) else "runner_setup_failed"}))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
