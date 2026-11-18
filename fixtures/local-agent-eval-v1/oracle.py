"""Independent finite functional checks. Never a general correctness/sandbox claim.

Candidate Python imports execute in a separate owned child, not this supervisor.
Inputs and expected outputs are kept here, outside the agent worktree. This is
functional independence, not confidentiality against hostile host-privileged code.
"""
from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time

import cases
import run_baseline as process_owner

MAX_OUTPUT = 256 * 1024
ENVIRONMENT_FAILURE_CODES = {"default_port_unavailable", "loopback_alias_unavailable",
                             "default_listener_inspection_unavailable"}


class OracleEnvironmentFailure(Exception):
    """A closed infrastructure failure, never a skip or candidate verdict."""


def _exclusive_socket(host: str, port: int) -> socket.socket:
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        if os.name == "nt":
            listener.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
        listener.bind((host, port))
        listener.listen(1)
        return listener
    except BaseException:
        listener.close()
        raise


def _owned_ipv4_listeners(process: subprocess.Popen, owner) -> set[tuple[str, int]]:
    """Inspect OS-owned listeners before making any default-port request."""
    if os.name == "nt":
        import ctypes
        from ctypes import wintypes
        import struct

        # The existing kill-on-close owner includes a server child, if any.
        if owner is None or not owner.handle:
            raise OracleEnvironmentFailure("default_listener_inspection_unavailable")
        ids = ctypes.create_string_buffer(8 + 256 * ctypes.sizeof(ctypes.c_size_t))
        returned = wintypes.DWORD()
        query = owner.kernel.QueryInformationJobObject
        query.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p,
                          wintypes.DWORD, ctypes.POINTER(wintypes.DWORD)]
        query.restype = wintypes.BOOL
        if not query(owner.handle, 3, ids, len(ids), ctypes.byref(returned)):
            raise OracleEnvironmentFailure("default_listener_inspection_unavailable")
        assigned, count = struct.unpack_from("<II", ids.raw)
        if count > 256 or assigned != count:
            raise OracleEnvironmentFailure("default_listener_inspection_unavailable")
        owned_pids = {ctypes.c_size_t.from_buffer_copy(ids.raw, 8 + i * ctypes.sizeof(ctypes.c_size_t)).value
                      for i in range(count)}
        table = ctypes.WinDLL("iphlpapi", use_last_error=True).GetExtendedTcpTable
        table.argtypes = [ctypes.c_void_p, ctypes.POINTER(wintypes.ULONG), wintypes.BOOL,
                          wintypes.ULONG, ctypes.c_int, wintypes.ULONG]
        table.restype = wintypes.DWORD
        size = wintypes.ULONG()
        status = table(None, ctypes.byref(size), False, socket.AF_INET, 3, 0)
        for _ in range(3):
            if status not in (0, 122) or size.value < 4 or size.value > 1024 * 1024:
                raise OracleEnvironmentFailure("default_listener_inspection_unavailable")
            buffer = ctypes.create_string_buffer(size.value)
            status = table(buffer, ctypes.byref(size), False, socket.AF_INET, 3, 0)
            if status == 0:
                break
        if status != 0:
            raise OracleEnvironmentFailure("default_listener_inspection_unavailable")
        rows, = struct.unpack_from("<I", buffer.raw)
        if rows > 16384 or 4 + rows * 24 > len(buffer):
            raise OracleEnvironmentFailure("default_listener_inspection_unavailable")
        result = set()
        for i in range(rows):
            state, address, port, _remote_address, _remote_port, pid = struct.unpack_from(
                "<6I", buffer.raw, 4 + i * 24)
            if state == 2 and pid in owned_pids:  # MIB_TCP_STATE_LISTEN
                result.add((socket.inet_ntoa(struct.pack("<I", address)), socket.ntohs(port & 0xFFFF)))
        return result
    if sys.platform.startswith("linux") and Path("/proc/net/tcp").is_file():
        # Process groups are the POSIX owner's boundary; do not inspect HTTP on
        # an unrelated process merely because it happens to occupy this port.
        try:
            inodes = set()
            for entry in Path("/proc").iterdir():
                if not entry.name.isdigit():
                    continue
                try:
                    if os.getpgid(int(entry.name)) != process.pid:
                        continue
                    for descriptor in (entry / "fd").iterdir():
                        target = os.readlink(descriptor)
                        if target.startswith("socket:[") and target.endswith("]"):
                            inodes.add(target[8:-1])
                except (FileNotFoundError, ProcessLookupError, PermissionError):
                    continue
            result = set()
            with Path("/proc/net/tcp").open(encoding="ascii") as source:
                for index, line in enumerate(source):
                    if index > 16384:
                        raise OracleEnvironmentFailure("default_listener_inspection_unavailable")
                    fields = line.split()
                    if index and fields[3] == "0A" and fields[9] in inodes:
                        address, port = fields[1].split(":")
                        result.add((socket.inet_ntoa(bytes.fromhex(address)[::-1]), int(port, 16)))
            return result
        except (OSError, ValueError, IndexError) as error:
            raise OracleEnvironmentFailure("default_listener_inspection_unavailable") from error
    raise OracleEnvironmentFailure("default_listener_inspection_unavailable")


def _http_default_checks(root: Path, variant: str) -> list[dict]:
    # Reserve both addresses before starting the candidate. Do not contact any
    # preexisting listener, and do not silently skip an unavailable environment.
    reservations = []
    try:
        for host, code in [("127.0.0.1", "default_port_unavailable"),
                           ("127.0.0.2", "loopback_alias_unavailable")]:
            try:
                reservations.append(_exclusive_socket(host, 8765))
            except OSError as error:
                raise OracleEnvironmentFailure(code) from error
    finally:
        for reservation in reservations:
            reservation.close()
    process = subprocess.Popen(
        [sys.executable, "-B", "server.py"], cwd=root, env=_environment(),
        stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        **_spawn_options(),
    )
    owner = None
    if os.name == "nt":
        try:
            owner = process_owner._WindowsJob(process)
        except BaseException:
            stop_owned_process(process)
            raise
    result = []
    try:
        listeners = set()
        end = time.monotonic() + 5
        while time.monotonic() < end and process.poll() is None:
            listeners = _owned_ipv4_listeners(process, owner)
            if any(port == 8765 for _host, port in listeners):
                break
            time.sleep(0.03)
        exact = ("127.0.0.1", 8765) in listeners
        result.append(check("default_host_and_port_listener", exact))
        if any(port == 8765 for _host, port in listeners):
            try:
                with _exclusive_socket("127.0.0.2", 8765):
                    alias_available = True
            except OSError:
                alias_available = False
            # Windows SO_REUSEADDR can permit this alias bind even beside a
            # wildcard listener. The OS-owned address, not this proxy alone,
            # is therefore mandatory for the host-default verdict.
            result.append(check("default_host_not_wildcard", alias_available and
                                not any(host == "0.0.0.0" and port == 8765
                                        for host, port in listeners)))
        if exact:
            # Only the independently attributed candidate listener is contacted.
            target = "/" if variant == "development" else "/health"
            expected = b"Hello, world!\n" if variant == "development" else b"Healthy\n"
            connection = http.client.HTTPConnection("127.0.0.1", 8765, timeout=2)
            try:
                # An HTTP/1.0 server may actively close each response. Avoid
                # leaving its fixed test port in TIME_WAIT between variants;
                # the complete bounded response is still checked before close.
                import struct
                connection.connect()
                connection.sock.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER,
                                           struct.pack("HH" if os.name == "nt" else "ii", 1, 0))
                connection.request("GET", target)
                response = connection.getresponse()
                body = response.read(MAX_OUTPUT + 1)
                content = response.getheader("Content-Type", "").lower().replace(" ", "")
                valid = (response.status == 200 and body == expected
                         and content == "text/plain;charset=utf-8")
            except (OSError, http.client.HTTPException):
                valid = False
            finally:
                connection.close()
            result.append(check("default_start_actual_http", valid))
    finally:
        stop_owned_process(process, owner)
    result.append(check("default_server_process_stopped", process.poll() is not None))
    return result


def stop_owned_process(process: subprocess.Popen, owner=None) -> None:
    if owner is not None:
        owner.close()  # Job lifetime, including descendants after parent exit.
    elif os.name == "nt" and process.poll() is None:
        subprocess.run(
            ["taskkill", "/PID", str(process.pid), "/T", "/F"],
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            timeout=5, check=False,
        )
    elif os.name != "nt":
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=3)


def _spawn_options() -> dict:
    return ({"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP}
            if os.name == "nt" else {"start_new_session": True})


def _environment() -> dict[str, str]:
    # No ambient PYTHONPATH, credentials or unrelated environment in the child.
    names = ("PATH", "SYSTEMROOT", "WINDIR", "TEMP", "TMP", "TMPDIR", "RUSTUP_HOME",
             "CARGO_HOME", "RUSTUP_TOOLCHAIN", "USERPROFILE", "LOCALAPPDATA",
             "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "LIB", "LIBPATH",
             "INCLUDE", "VCINSTALLDIR", "VCToolsInstallDir", "WindowsSdkDir",
             "WindowsSDKVersion", "VSINSTALLDIR", "PROCESSOR_ARCHITECTURE",
             "PROCESSOR_ARCHITEW6432", "COMSPEC", "PATHEXT", "APPDATA",
             "ProgramData", "SystemDrive")
    environment = {key: os.environ[key] for key in names if key in os.environ}
    environment["PYTHONUTF8"] = "1"  # Frozen oracle locale, same for both compared cores.
    return environment


def run_owned(argv: list[str], cwd: Path, payload: dict | None = None,
              timeout: float = 30) -> dict:
    """Use the runner's shared bounded drain, timeout and complete tree owner."""
    metadata, output, _stderr = process_owner.run_owned_process(
        argv, cwd=cwd, timeout=timeout, env=_environment(),
        input_bytes=None if payload is None else json.dumps(payload, ensure_ascii=False).encode(),
        capture_limit=MAX_OUTPUT, terminate_on_output_limit=True,
    )
    if (metadata.get("output_limit") or metadata["stdout"].get("truncated")
            or metadata["stderr"].get("truncated")):
        return {"ok": False, "error": "oracle_output_limit"}
    if metadata.get("timed_out") or metadata.get("interrupted"):
        return {"ok": False, "error": "candidate_timeout"}
    if (metadata.get("stdin_write_failed") or
            metadata["stdout"].get("read_failed") or metadata["stderr"].get("read_failed")):
        return {"ok": False, "error": "candidate_io_failure"}
    if metadata.get("exit_code") != 0:
        return {"ok": False, "error": "candidate_exit_failure", "exit_code": metadata.get("exit_code")}
    return {"ok": True, "output": output.decode("utf-8", errors="strict")}


PYTHON_OBSERVER = r'''
import contextlib
import importlib
import io
import json
import os
import pathlib
import sys
import unittest

root = pathlib.Path(sys.argv[1]).resolve()
request = json.load(sys.stdin)
sys.path.insert(0, str(root))
os.chdir(root)
out = {}
with open(os.devnull, "w") as sink, contextlib.redirect_stdout(sink), contextlib.redirect_stderr(sink):
    if request["kind"] == "call":
        module = importlib.import_module(request["module"])
        function = getattr(module, request["function"])
        results = []
        for arguments in request["arguments"]:
            before = json.dumps(arguments, ensure_ascii=False, sort_keys=True)
            try:
                value = function(*arguments)
                result = {"value": value, "type": type(value).__name__}
            except Exception as error:
                result = {"error": type(error).__name__}
            result["input_unchanged"] = before == json.dumps(arguments, ensure_ascii=False, sort_keys=True)
            results.append(result)
        out = {"results": results}
    elif request["kind"] == "invoice":
        pricing = importlib.import_module("pricing")
        calls = []
        original = pricing.discounted_total
        def observed(cents, rate):
            calls.append([cents, rate])
            return original(cents, rate)
        pricing.discounted_total = observed
        invoice = importlib.import_module("invoice")
        results = []
        for lines, rate in request["arguments"]:
            before = list(lines)
            calls.clear()
            try:
                value = invoice.invoice_total(lines, rate)
                result = {"value": value, "type": type(value).__name__,
                          "delegated": [sum(before), rate] in calls}
            except Exception as error:
                result = {"error": type(error).__name__, "delegated": False}
            result["input_unchanged"] = lines == before
            results.append(result)
        out = {"results": results}
    elif request["kind"] == "tests":
        suite = unittest.defaultTestLoader.discover(str(root / "tests"), top_level_dir=str(root / "tests"))
        def ids(s):
            for child in s:
                if isinstance(child, unittest.TestSuite):
                    yield from ids(child)
                else:
                    yield child.id()
        discovered = sorted(ids(suite))
        result = unittest.TestResult()
        suite.run(result)
        out = {"test_ids": discovered, "tests_run": result.testsRun,
               "errors": len(result.errors), "failures": len(result.failures),
               "successful": result.wasSuccessful()}
    elif request["kind"] == "a11":
        import inspect
        pricing = importlib.import_module("pricing")
        invoice = importlib.import_module("invoice")
        reports = importlib.import_module("reports")
        out = {"parameters": list(inspect.signature(pricing.discount).parameters),
               "results": [[cents, pricing.discount(cents, request["basispoints"]), invoice.total(cents), reports.summary(cents)]
                           for cents in request["inputs"]]}
    else:
        raise ValueError("unknown observer operation")
print(json.dumps(out, ensure_ascii=False))
'''


def python_observe(root: Path, payload: dict) -> dict:
    result = run_owned([sys.executable, "-I", "-B", "-c", PYTHON_OBSERVER, str(root)], root, payload)
    if not result["ok"]:
        return result
    try:
        return {"ok": True, "value": json.loads(result["output"])}
    except (ValueError, TypeError):
        return {"ok": False, "error": "invalid_observer_result"}


def check(identifier: str, passed: bool, detail: str = "") -> dict:
    return {"id": identifier, "passed": bool(passed), "detail": detail}


def _call_checks(root: Path, module: str, function: str, arguments: list,
                 expected: list, kind: str | None = None) -> list[dict]:
    observed = python_observe(root, {"kind": "call", "module": module,
                                    "function": function, "arguments": arguments})
    output = [check(f"{module}.{function}.executed", observed.get("ok", False),
                    observed.get("error", ""))]
    if not observed.get("ok"):
        return output
    results = observed["value"].get("results", [])
    passed = len(results) == len(expected)
    for actual, required in zip(results, expected):
        if isinstance(required, dict) and "error" in required:
            passed = passed and actual.get("error") == required["error"]
        else:
            passed = passed and actual.get("value") == required and actual.get("input_unchanged") is True
            if kind is not None:
                passed = passed and actual.get("type") == kind
    output.append(check(f"{module}.{function}.independent_inputs", passed))
    return output


def _protected(root: Path, definition: dict) -> list[dict]:
    result = []
    for name, required in definition["protected_file_sha256"].items():
        path = root / name
        actual = hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() and not path.is_symlink() else None
        result.append(check(f"protected:{name}", actual == required))
    return result


def _python_test_checks(root: Path, definition: dict) -> list[dict]:
    observed = python_observe(root, {"kind": "tests"})
    result = [check("existing_tests_executed", observed.get("ok", False), observed.get("error", ""))]
    if not observed.get("ok"):
        return result
    value = observed["value"]
    expected = definition["expected_test_ids"]
    # A11 supervisor adds one explicitly pinned test after plan materialization.
    if definition["case_id"] == "A11":
        expected = expected + ["test_reports.NewCaller.test_existing_ten_percent_behavior"]
    result.append(check("nonzero_expected_test_ids", bool(expected) and
                        value.get("test_ids") == sorted(expected) and
                        value.get("tests_run") == len(expected)))
    if definition["case_id"] == "A12":
        result.append(check("known_unrelated_failure_preserved",
                            value.get("tests_run") == 2 and value.get("failures") == 1 and value.get("errors") == 0))
    else:
        result.append(check("existing_tests_pass", value.get("successful") is True))
    return result


def _http_checks(root: Path, variant: str) -> list[dict]:
    path = "/" if variant == "development" else "/health"
    expected = b"Hello, world!\n" if variant == "development" else b"Healthy\n"
    if not (root / "server.py").is_file():
        return [check("actual_server_process", False, "missing_server")]
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    process = subprocess.Popen(
        [sys.executable, "-B", "server.py", "--host", "127.0.0.1", "--port", str(port)],
        cwd=root, env=_environment(), stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, **_spawn_options(),
    )
    owner = None
    if os.name == "nt":
        try:
            owner = process_owner._WindowsJob(process)
        except BaseException:
            stop_owned_process(process)
            raise
    result = []
    try:
        ready = False
        end = time.monotonic() + 8
        while time.monotonic() < end and process.poll() is None:
            try:
                with socket.create_connection(("127.0.0.1", port), timeout=0.1):
                    ready = True
                break
            except OSError:
                time.sleep(0.03)
        result.append(check("actual_server_process", ready))
        if not ready:
            return result
        for method, target, status, body in [
            ("GET", path, 200, expected), ("GET", "/missing-eval-route", 404, None),
            ("POST", path, 405, None), ("PUT", path, 405, None),
            ("DELETE", path, 405, None), ("PATCH", path, 405, None),
        ]:
            connection = http.client.HTTPConnection("127.0.0.1", port, timeout=2)
            try:
                connection.request(method, target, body=b"" if method != "GET" else None)
                response = connection.getresponse()
                actual_body = response.read(MAX_OUTPUT + 1)
                valid = response.status == status and len(actual_body) <= MAX_OUTPUT
                if body is not None:
                    valid = valid and actual_body == body
                    content = response.getheader("Content-Type", "").lower().replace(" ", "")
                    valid = valid and content == "text/plain;charset=utf-8"
                if status == 405:
                    valid = valid and response.getheader("Allow") == "GET"
                result.append(check(f"http:{method}:{target}", valid))
            except (OSError, http.client.HTTPException):
                result.append(check(f"http:{method}:{target}", False, "http_check_failed"))
            finally:
                connection.close()
    finally:
        stop_owned_process(process, owner)
    result.append(check("server_process_stopped", process.poll() is not None))
    bad = run_owned([sys.executable, "-B", "server.py", "--port", "invalid"], root)
    result.append(check("invalid_cli_rejected", not bad["ok"] and bad.get("exit_code", 0) != 0))
    # Keep a listening socket open throughout this separate child invocation:
    # a released ephemeral reservation would race the candidate's bind.
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as occupied:
        if os.name == "nt":
            occupied.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
        occupied.bind(("127.0.0.1", 0))
        occupied.listen(1)
        occupied_port = occupied.getsockname()[1]
        blocked = run_owned(
            [sys.executable, "-B", "server.py", "--host", "127.0.0.1", "--port", str(occupied_port)],
            root, timeout=3,
        )
        result.append(check("occupied_port_rejected", not blocked["ok"] and
                            type(blocked.get("exit_code")) is int and blocked["exit_code"] != 0))
    try:
        result += _http_default_checks(root, variant)
    except OracleEnvironmentFailure as error:
        # Preserve all prior deterministic HTTP checks. Only this unperformed
        # default-port proof is blocked by the environment.
        blocked = check("oracle_environment", False, str(error))
        blocked["environment_blocked"] = True
        result.append(blocked)
    return result


def _cli_checks(root: Path, variant: str) -> list[dict]:
    if not (root / "main.py").is_file() or not (root / "notes.py").is_file():
        return [check("cli_files_present", False)]
    result = []
    # Runtime data is supervisor-created after model work, with a different frozen name per variant.
    store = root / f".eval-notes-{variant}.json"
    if store.exists():
        return [check("fresh_oracle_store", False, "oracle_store_already_exists")]
    titles = ["Grüße 🚀", "second"] if variant == "development" else ["東京 🌱", "another title"]
    def invoke(*arguments: str) -> dict:
        return run_owned([sys.executable, "-B", "main.py", "--store", store.name, *arguments], root)
    try:
        absent = invoke("list")
        try:
            empty = absent["ok"] and json.loads(absent.get("output", "")) == []
        except (ValueError, TypeError):
            empty = False
        result.append(check("missing_store_lists_empty", empty))
        first = invoke("add", titles[0])
        second = invoke("add", titles[1])
        listing = invoke("list")
        try:
            one = json.loads(first.get("output", ""))
            two = json.loads(second.get("output", ""))
            rows = json.loads(listing.get("output", ""))
            saved = json.loads(store.read_text(encoding="utf-8"))
            valid = (all(item.get("ok") for item in [first, second, listing]) and
                     isinstance(one["id"], int) and not isinstance(one["id"], bool) and one["id"] > 0 and
                     two["id"] == one["id"] + 1 and
                     one["title"] == titles[0] and two["title"] == titles[1] and rows == [one, two] and saved == rows)
        except (ValueError, KeyError, TypeError, OSError):
            valid = False
        result.append(check("cli_cross_process_persistence", valid))
        before = store.read_bytes() if store.is_file() else b""
        for identifier, title in [("empty_title_no_mutation", ""),
                                  ("whitespace_title_no_mutation", " \t "),
                                  ("spaces_title_no_mutation", "   ")]:
            invalid = invoke("add", title)
            result.append(check(identifier, invalid.get("exit_code") == 2 and
                                store.is_file() and store.read_bytes() == before))
        store.write_bytes(b"{not-json\n")
        corrupt = invoke("add", "should not be stored")
        result.append(check("corrupt_store_not_overwritten", not corrupt["ok"] and
                            type(corrupt.get("exit_code")) is int and corrupt["exit_code"] != 0 and
                            store.read_bytes() == b"{not-json\n"))
        corrupt_listing = invoke("list")
        result.append(check("corrupt_store_list_rejected_without_overwrite", not corrupt_listing["ok"] and
                            type(corrupt_listing.get("exit_code")) is int and corrupt_listing["exit_code"] != 0 and
                            store.read_bytes() == b"{not-json\n"))
    finally:
        # Exact supervisor-owned data file only, never recursive cleanup.
        if store.exists() and not store.is_symlink():
            store.unlink()
    return result


def _node_checks(root: Path, variant: str) -> list[dict]:
    node = shutil.which("node")
    if node is None:
        return [check("node_available", False, "missing_existing_toolchain")]
    separator = " / " if variant == "development" else " | "
    samples = [[" A ", "", " B "], ["", "  "], [" Grüße ", "東京"], ["a", "b", "c"], []]
    script = r"""
        import { pathToFileURL } from 'node:url';
        const { formatLabel } = await import(pathToFileURL(process.argv[1]).href);
        const inputs = JSON.parse(process.argv[2]);
        const out = inputs.map(parts => {
          const before = JSON.stringify(parts);
          const value = formatLabel(parts);
          return {value, unchanged: before === JSON.stringify(parts), type: typeof value};
        });
        console.log(JSON.stringify(out));
    """
    execution = run_owned([node, "--input-type=module", "-e", script,
                           str(root / "formatter.ts"), json.dumps(samples, ensure_ascii=False)], root)
    valid = False
    if execution["ok"]:
        try:
            actual = json.loads(execution["output"])
            valid = len(actual) == len(samples) and all(
                item == {"value": separator.join(p.strip() for p in sample if p.strip()), "unchanged": True, "type": "string"}
                for item, sample in zip(actual, samples))
        except (ValueError, TypeError):
            pass
    result = [check("typescript_actual_execution", valid, execution.get("error", ""))]
    tests = run_owned([node, "--test", "tests/format.test.mjs"], root)
    text = tests.get("output", "")
    result.append(check("node_existing_test_nonzero", tests["ok"] and
                        "trims and removes empty labels" in text and
                        ("# tests 1" in text or "tests 1" in text)))
    return result


def _rust_checks(root: Path, variant: str) -> list[dict]:
    cargo = shutil.which("cargo")
    if cargo is None:
        return [check("cargo_available", False, "missing_existing_toolchain")]
    existing = run_owned([cargo, "test", "--offline", "--locked"], root, timeout=60)
    text = existing.get("output", "")
    result = [check("cargo_existing_tests", existing["ok"] and "partial_page" in text
                    and "zero_size" in text and "2 passed" in text, existing.get("error", ""))]
    # Separate std-only checker crate, no registry dependency or mutable test installation.
    values = [(0, 1), (10, 4), (2**64 - 1, 2), (2**64 - 1, 2**64 - 1),
              (2**64 - 1, 1), (12345 + cases._variant(variant), 37), (17, 0)]
    with tempfile.TemporaryDirectory(prefix="a3-p0-rust-oracle-") as directory:
        observer = Path(directory)
        (observer / "src").mkdir()
        cargo_path = str(root).replace("\\", "/").replace('"', '\\"')
        (observer / "Cargo.toml").write_text(
            '[package]\nname="a3-independent-pages"\nversion="0.1.0"\nedition="2024"\n'
            '[workspace]\n[dependencies]\na3-eval-pages={path="' + cargo_path + '"}\n', encoding="utf-8")
        assertions = []
        for items, size in values:
            required = "None" if size == 0 else f"Some({(items // size) + (items % size != 0)}u64)"
            assertions.append(f"assert_eq!(a3_eval_pages::pages({items}u64,{size}u64),{required});")
        (observer / "src/main.rs").write_text("fn main() {\n" + "\n".join(assertions)
                                            + '\nprintln!("A3_P0_ORACLE_ASSERTIONS_PASSED");\n}\n', encoding="utf-8")
        hidden = run_owned([cargo, "run", "--offline", "--quiet"], observer, timeout=60)
        result.append(check("rust_independent_overflow_and_zero_inputs", hidden["ok"] and
                            hidden.get("output", "").strip() == "A3_P0_ORACLE_ASSERTIONS_PASSED",
                            hidden.get("error", "")))
    return result


def _operator_review(answer: str | None, observations: dict,
                     definition: dict) -> tuple[list[dict], bool]:
    result = [check("nonempty_answer", isinstance(answer, str) and bool(answer.strip()))]
    review = observations.get("manual_review")
    required = set(definition["manual_rubric"])
    valid_review = (isinstance(review, dict) and review.get("source") == "operator" and
                    isinstance(review.get("reviewer"), str) and bool(review["reviewer"].strip()) and
                    review.get("rubric_revision") == cases.REVISION and
                    isinstance(review.get("decisions"), dict) and set(review["decisions"]) == required and
                    all(type(value) is bool for value in review["decisions"].values()))
    result.append(check("operator_semantic_review_record", valid_review))
    if valid_review:
        result.extend(check(f"semantic:{name}", review["decisions"][name]) for name in sorted(required))
    return result, not valid_review


def _semantic_review(case_id: str, variant: str, root: Path, answer: str | None,
                     observations: dict, definition: dict) -> tuple[list[dict], bool]:
    result, pending = _operator_review(answer, observations, definition)
    if case_id == "Q05":
        expected = cases.intervention_files(case_id, variant)["config.py"]
        result.append(check("post_edit_config_bytes", (root / "config.py").read_bytes() == expected))
        for flag in ("initial_answer_observed", "source_edit_injected", "fresh_evidence_observed"):
            result.append(check(f"controlled:{flag}", observations.get(flag) is True))
    if case_id == "Q06":
        result.append(check("ask_capability_events", observations.get("privileged_action_count") == 0
                            and type(observations.get("privileged_action_count")) is int))
    return result, pending


def _agent_report_review(answer: str | None, observations: dict,
                         definition: dict) -> tuple[list[dict], bool]:
    # Missing semantic review is never the primary cause for an early product stop.
    # These completion fields are native/supervisor projections, not model claims.
    eligible = (observations.get("product_formal_completed") is True
                and observations.get("product_run_state", "Done") == "Done")
    if not eligible:
        return [check("agent_report_review_eligible", False, "product_not_completed")], False
    result, pending = _operator_review(answer, observations, definition)
    return [check("agent_report_review_eligible", True)] + result, pending


def _public_agent_answer(native: dict) -> str | None:
    metadata = native.get("agent_answer_metadata")
    if not (
        isinstance(metadata, dict)
        and metadata.get("entry_kind") in ("FinalReport", "AssistantSummary")
        and type(metadata.get("entry_sequence")) is int
        and 0 < metadata["entry_sequence"] <= 2**64 - 1
        and metadata.get("task_matches") is True
        and native.get("agent_answer_truncated") is False
        and isinstance(native.get("agent_answer"), str)
    ):
        return None
    return native["agent_answer"]


def evaluate(case_id: str, variant: str, worktree: str | Path, answer: str | None = None,
             observations: dict | None = None) -> dict:
    root = Path(worktree).resolve()
    definition = cases.definition(case_id, variant)
    observations = {} if observations is None else observations
    result = []
    manual = False
    environment_failure = None
    try:
        if definition["mode"] == "ask":
            # Q05 is the sole supervisor-authorized source change.
            protected = dict(definition)
            if case_id == "Q05":
                protected["protected_file_sha256"] = {}
            result += _protected(root, protected)
            reviewed, manual = _semantic_review(case_id, variant, root, answer, observations, definition)
            result += reviewed
        else:
            result += _protected(root, definition)
            for flag in definition["required_observations"]:
                result.append(check(f"controlled:{flag}", observations.get(flag) is True,
                                    "requires runner-derived journal/control evidence, never model assertion"))
            values = [-10**18, -101, -2, -1, 0, 1, 2, 40, 41, 42, 100, 10**18]
            if case_id in ("A01", "A02", "A12"):
                delta = -1 if variant == "holdout" else 1
                result += _call_checks(root, "subject", "adjust", [[i] for i in values], [i + delta for i in values], "int")
                if case_id == "A02":
                    original = cases.large_file(variant)
                    actual = (root / "subject.py").read_bytes()
                    start = b"# BEGIN_EDIT_WINDOW\r\n"
                    end = b"# END_EDIT_WINDOW\r\n"
                    prefix = original[:original.index(start) + len(start)]
                    suffix = original[original.index(end):]
                    result.append(check("large_file_outside_window_byte_identical",
                                        len(original) > 200_000 and actual.startswith(prefix) and actual.endswith(suffix)
                                        and actual.count(start) == 1 and actual.count(end) == 1))
            elif case_id == "A03":
                scale = cases.rate_scale(variant)
                rates = [0, 1, scale // 10, scale // 3, scale // 2, scale - 1, scale]
                pairs = [[cents, rate] for cents in [0, 1, 99, 101, 1501, 1234567] for rate in rates]
                result += _call_checks(root, "pricing", "discounted_total", pairs,
                                       [cents * (scale - rate) // scale for cents, rate in pairs], "int")
                arguments = [[lines, rate] for lines in [[], [0], [1], [101, 200, 7], [100000, 9999]] for rate in rates]
                observed = python_observe(root, {"kind": "invoice", "arguments": arguments})
                good = observed.get("ok", False)
                actual = observed.get("value", {}).get("results", [])
                good = good and len(actual) == len(arguments)
                for item, (lines, rate) in zip(actual, arguments):
                    total = sum(lines) * (scale - rate) // scale
                    good = good and item == {"value": f"{cases.currency(variant)}{total // 100}.{total % 100:02d}",
                                             "type": "str", "delegated": True, "input_unchanged": True}
                result.append(check("invoice_behavior_and_real_delegation", good, observed.get("error", "")))
            elif case_id == "A04":
                result += _http_checks(root, variant)
                result.append(check("no_test_scaffolding", not (root / "tests").exists()
                                    and not any(root.glob("test_*.py"))))
            elif case_id == "A05":
                result += _cli_checks(root, variant)
                result.append(check("no_test_scaffolding", not (root / "tests").exists()
                                    and not any(root.glob("test_*.py"))))
            elif case_id == "A06":
                result += _node_checks(root, variant)
            elif case_id == "A07":
                result += _rust_checks(root, variant)
            elif case_id == "A08":
                inputs = ([[1, 2], [-2, 3], [2, 4, 9], [100, 1, -1], [], [7]]
                          if variant == "development" else [[2, 7], [-9, 4], [8, 1, 6], [91, 2, -2], [], [11]])
                expected = [sum(item) / len(item) if item else {"error": "ValueError"} for item in inputs]
                result += _call_checks(root, "average", "average", [[item] for item in inputs], expected, "float")
            elif case_id == "A09":
                samples = ["  Welt  ", "Grüße 🌱", "", "  ", "\t東京\n"]
                result += _call_checks(root, "subject", "render", [[item] for item in samples],
                                       [item.strip() + "!" for item in samples], "str")
                result.append(check("actual_user_owned_prefix_preserved",
                                    (root / "subject.py").read_bytes().startswith(cases.user_edit_bytes(variant))))
            elif case_id == "A10":
                for module, expression, _ in cases.long_functions(variant):
                    arguments = [value for group in range(6)
                                 for value in cases.long_requirement_inputs(group, variant, oracle=True)]
                    result += _call_checks(root, module, "compute", [[i] for i in arguments],
                                           [cases.calculate(expression, i) for i in arguments], "int")
                result.append(check("all_30_original_requirement_ids",
                                    set(observations.get("covered_requirement_ids", [])) ==
                                    {f"R{i:02d}" for i in range(1, 31)}))
                result.append(check("same_long_original_request",
                                    observations.get("original_objective_sha256") ==
                                    hashlib.sha256(cases.long_objective(variant).encode()).hexdigest()))
                extras = {str(p.relative_to(root)).replace("\\", "/") for p in root.rglob("*")
                          if p.is_file() and ".git" not in p.relative_to(root).parts and
                          "__pycache__" not in p.relative_to(root).parts}
                result.append(check("no_extra_production_or_test_files",
                                    extras == set(cases.fixture_files(case_id, variant))))
            elif case_id == "A11":
                rate = cases.migration_percent(variant) * 100
                execution = python_observe(root, {"kind": "a11", "basispoints": rate,
                                                  "inputs": [0, 1, 99, 1500, 99999]})
                good = execution.get("ok", False)
                value = execution.get("value", {})
                good = good and value.get("parameters") == ["cents", "basispoints"]
                good = good and value.get("results") == [
                    [cents, cents * (10000 - rate) // 10000, cents * (10000 - rate) // 10000,
                     cents * (10000 - rate) // 10000]
                    for cents in [0, 1, 99, 1500, 99999]]
                result.append(check("new_caller_and_migrated_interface", good, execution.get("error", "")))
                injected = cases.intervention_files(case_id, variant)
                path = root / "tests/test_reports.py"
                result.append(check("injected_test_preserved",
                                    path.is_file() and path.read_bytes() == injected["tests/test_reports.py"]))
            if case_id not in ("A04", "A05", "A06", "A07"):
                result += _python_test_checks(root, definition)
            if case_id == "A12":
                reviewed, manual = _agent_report_review(answer, observations, definition)
                result += reviewed
    except OracleEnvironmentFailure as error:
        environment_failure = str(error)
        blocked = check("oracle_environment", False, environment_failure)
        blocked["environment_blocked"] = True
        result.append(blocked)
    except (OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError):
        result.append(check("oracle_execution", False, "oracle_or_candidate_failed"))
    for item in result:
        if item.get("environment_blocked") is True and item["detail"] in ENVIRONMENT_FAILURE_CODES:
            environment_failure = item["detail"]
    return {
        "schema": cases.REVISION, "case_id": case_id, "variant": variant,
        "checks": result, "passed": bool(result) and all(item["passed"] for item in result),
        "manual_review_required": manual,
        "environment_failure": environment_failure,
        "boundary": "functional independent checks; no hostile-code sandbox guarantee",
        "product_verification_imported": False,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--case", required=True, choices=cases.CASE_IDS)
    parser.add_argument("--variant", required=True, choices=cases.VARIANTS)
    parser.add_argument("--worktree", required=True)
    parser.add_argument("--worker-report")
    parser.add_argument("--answer-file")
    parser.add_argument("--observations-file")
    parser.add_argument("--review-file")
    args = parser.parse_args()
    answer = None
    native = {}
    if args.worker_report:
        # Only native public answer/completion fields are read, never worker/model review.
        native = json.loads(Path(args.worker_report).read_text(encoding="utf-8"))
        answer = _public_agent_answer(native) if args.case == "A12" else native.get("ask_answer")
    if args.answer_file:
        answer = Path(args.answer_file).read_text(encoding="utf-8")
    observations = {}
    if args.observations_file:
        observations = json.loads(Path(args.observations_file).read_text(encoding="utf-8"))
        observations.pop("manual_review", None)
    if args.case == "A12":
        # Do not let caller-supplied observation flags change native completion.
        observations["product_formal_completed"] = native.get("formal_completed") is True
        observations["product_run_state"] = native.get("run_state")
    if args.review_file:
        observations["manual_review"] = json.loads(Path(args.review_file).read_text(encoding="utf-8"))
    print(json.dumps(evaluate(args.case, args.variant, args.worktree, answer, observations), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    if os.name != "nt":
        def _terminate(_signal, _frame):
            raise KeyboardInterrupt
        signal.signal(signal.SIGTERM, _terminate)
    raise SystemExit(main())
