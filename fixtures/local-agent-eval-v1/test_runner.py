"""Offline runner lifecycle tests; fake workers, no Ollama/GPU generation."""
from __future__ import annotations

import json
import contextlib
import io
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
import subprocess

import run_baseline as runner


class RunnerTests(unittest.TestCase):
    def test_product_proof_is_separate_from_external_oracle(self):
        native = {"ledger": {"steps": ["Completed"], "verification": [
            {"method": "DiffInvariant", "acceptance_criteria_count": 1,
             "passed": True, "evidence_count": 1}]},
            "product_verification": {"evidence_count": 1, "truncated": False,
                "external_oracle_evidence_imported": False,
                "evidence": [{"kind": "diff", "fresh": True, "raw_content": "never retained"}]}}
        summary = runner._safe_worker(native)
        self.assertEqual(summary["ledger"]["verification"][0]["method"], "DiffInvariant")
        self.assertEqual(summary["product_verification"]["evidence"], [{"kind": "diff", "fresh": True}])
        native["product_verification"]["external_oracle_evidence_imported"] = True
        with self.assertRaises(runner.RunnerFailure):
            runner._safe_worker(native)

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="a3-plan22-runner-test-")
        self.root = Path(self.temporary.name)
        self.run_dir = self.root / "attempt"
        self.run_dir.mkdir()
        self.worker = self.root / "fake_worker.py"
        self.oracle_observations = None

    def tearDown(self):
        self.temporary.cleanup()

    def materialize(self, case_id, variant, run_dir):
        worktree = Path(run_dir) / "worktree"
        worktree.mkdir()
        (worktree / "example.py").write_text("value = 1\n", encoding="utf-8")
        return {"case_id": case_id, "variant": variant,
                "mode": "ask" if case_id.startswith("Q") else "agent",
                "objective": "Public fake fixture objective",
                "allowed_write_paths": [] if case_id.startswith("Q") else ["example.py"],
                "allowed_processes": []}

    def metadata(self, model):
        return {"tag": model, "digest": "a" * 64, "server_version": "fake-no-network"}

    def evaluate(self, case_id, variant, worktree, answer, observations):
        self.oracle_observations = observations
        return {"passed": True, "manual_review_required": False,
                "checks": [{"id": "fake-control", "passed": True, "detail": "fake oracle"}]}

    def attempt(self, source, **options):
        self.worker.write_text(source, encoding="utf-8")
        return runner.run_attempt(
            repository=self.root, run_dir=self.run_dir, model="qwen38-4k:latest",
            case_id=options.pop("case_id", "A01"), variant="development", phase="pilot",
            worker=Path(sys.executable), materialize=self.materialize,
            evaluate=options.pop("evaluate", self.evaluate),
            metadata_provider=options.pop("metadata_provider", self.metadata),
            freeze_provider=options.pop("freeze_provider", lambda: {"models": {
                "qwen38-4k:latest": {"digest": "a" * 64, "context_limit": 4096, "output_limit": 1024}}}),
            runtime_provider=lambda _: {"test": "offline"},
            worker_command=[sys.executable, str(self.worker)],
            gpu_lock_path=self.root / "test-gpu.lock", **options)

    @staticmethod
    def fake_prelude():
        return """import json, os, pathlib, sys, time
root = pathlib.Path(os.environ['A3_PLAN22_RUN_DIR'])
initial = json.loads((root / 'report.json').read_text())
assert initial['status'] == 'running'
assert initial['worker_starts'] == 1
counter = root / 'invocations.txt'
counter.write_text(counter.read_text() + '1' if counter.exists() else '1')
"""

    @staticmethod
    def fake_final(**fields):
        value = {"version": 1, "terminal": True, "worker_succeeded": True,
                 "formal_completed": True, "case_id": "A01", "variant": "development", **fields}
        return "(root / 'worker.json').write_text(" + repr(json.dumps(value)) + ")\n"

    def test_crash_is_retained_and_worker_is_never_retried(self):
        report = self.attempt(self.fake_prelude() + "sys.stdout.write('private-source-sentinel')\nsys.exit(7)\n")
        self.assertEqual(report["status"], "native_error")
        self.assertEqual(report["native_process"]["exit_code"], 7)
        self.assertEqual((self.run_dir / "invocations.txt").read_text(), "1")
        self.assertEqual(report["worker_starts"], 1)
        self.assertNotIn("private-source-sentinel", (self.run_dir / "report.json").read_text())
        self.assertTrue(report["metadata_stable"])

    def test_timeout_terminates_owned_worker_and_keeps_partial_report(self):
        code = self.fake_prelude() + "(root / 'worker.json').write_text(json.dumps({'version':1,'terminal':False,'phase':'probing'}))\ntime.sleep(60)\n"
        report = self.attempt(code, worker_timeout=0.2)
        self.assertEqual(report["status"], "timeout")
        self.assertTrue(report["native_process"]["timed_out"])
        self.assertFalse(report["worker"]["terminal"])
        self.assertLess(report["native_process"]["duration_seconds"], 10)
        self.assertEqual((self.run_dir / "invocations.txt").read_text(), "1")

    def test_large_worker_output_is_drained_but_not_saved_as_source(self):
        report = self.attempt(self.fake_prelude() + "sys.stdout.write('s' * 500000)\nsys.stderr.write('e' * 400000)\n" + self.fake_final())
        self.assertEqual(report["status"], "passed")
        for name, total in (("stdout", 500000), ("stderr", 400000)):
            capture = report["native_process"][name]
            self.assertEqual(capture["bytes"], total)
            self.assertTrue(capture["truncated"])
            self.assertLessEqual(capture["retained_bytes"], runner.CAPTURE_BYTES)
        self.assertLess((self.run_dir / "report.json").stat().st_size, 16000)

    def test_success_requires_terminal_success_and_formal_completion(self):
        report = self.attempt(self.fake_prelude() + self.fake_final(terminal=False, worker_succeeded=False))
        self.assertEqual(report["status"], "native_failure")
        self.assertTrue(report["independently_correct"])

    def test_unfinished_harness_is_distinct_from_product_failure(self):
        report = self.attempt(self.fake_prelude() + self.fake_final(
            worker_succeeded=False, failure_code="intervention_not_implemented",
            interventions={"implemented": False}))
        self.assertEqual(report["status"], "harness_incomplete")

    def test_worker_manual_review_and_intervention_claims_are_not_oracle_authority(self):
        report = self.attempt(self.fake_prelude() + self.fake_final(
            ask_answer="A fake public answer", observations={"manual_review": {"source": "operator"},
                                                          "process_restarted": True}))
        self.assertEqual(self.oracle_observations, {})
        self.assertNotIn("observations", report["worker"])
        self.assertIn("answer_sha256", report["worker"])
        self.assertNotIn("A fake public answer", (self.run_dir / "report.json").read_text())

    def test_mutable_model_tag_invalidates_attempt_without_retry(self):
        calls = []

        def metadata(model):
            calls.append(model)
            return {"digest": ("a" if len(calls) == 1 else "b") * 64}

        report = self.attempt(self.fake_prelude() + self.fake_final(), metadata_provider=metadata)
        self.assertEqual(report["status"], "metadata_changed")
        self.assertFalse(report["metadata_stable"])
        self.assertEqual(len(calls), 2)
        self.assertEqual((self.run_dir / "invocations.txt").read_text(), "1")

    def test_series_digest_mismatch_prevents_any_worker_start(self):
        report = self.attempt("raise SystemExit('must not run')\n",
                              metadata_provider=lambda _: {"digest": "b" * 64})
        self.assertEqual(report["status"], "metadata_mismatch")
        self.assertEqual(report["worker_starts"], 0)
        self.assertFalse((self.run_dir / "invocations.txt").exists())
        self.assertTrue((self.run_dir / "report.json").is_file())

    def test_series_template_mismatch_prevents_any_worker_start(self):
        report = self.attempt("raise SystemExit('must not run')\n",
                              metadata_provider=lambda _: {"digest": "a" * 64, "template_sha256": "b" * 64})
        self.assertEqual(report["status"], "metadata_mismatch")
        self.assertEqual(report["worker_starts"], 0)

    def test_invalid_case_request_is_retained_before_any_worker_start(self):
        original = self.materialize

        def invalid(case_id, variant, run_dir):
            fields = original(case_id, variant, run_dir)
            fields["allowed_write_paths"] = ["../outside.txt"]
            return fields

        self.materialize = invalid
        report = self.attempt("raise SystemExit('must not run')\n")
        self.assertEqual(report["status"], "runner_error")
        self.assertEqual(report["error_code"], "invalid_case_write_paths")
        self.assertEqual(report["worker_starts"], 0)
        self.assertTrue((self.run_dir / "report.json").is_file())

    def test_cli_rejects_unreviewed_models_without_metadata(self):
        with contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as result:
                runner.main(["--model", "unreviewed:latest", "--case", "A01"])
        self.assertEqual(result.exception.code, 2)

    def test_gpu_lock_uses_live_os_handle_and_releases_after_close(self):
        path = self.root / "gpu.lock"
        path.write_bytes(b"stale-file-is-not-authority")
        with runner.GpuLock(path):
            with self.assertRaises(runner.RunnerFailure):
                with runner.GpuLock(path):
                    self.fail("second live owner acquired the lock")
        with runner.GpuLock(path):
            pass

    def test_profile_and_case_budgets_are_pinned_before_execution(self):
        self.assertEqual(runner.MODELS["qwen38-4k:latest"], (4096, 1024))
        self.assertEqual(runner.MODELS["ornith-1.5:9b"], (8192, 2048))
        self.assertEqual(runner.task_budget("Q01"), 360)
        self.assertEqual(runner.task_budget("A03"), 900)
        self.assertEqual(runner.task_budget("A10"), 1800)

    def test_frozen_worker_is_immutable_across_source_binary_changes(self):
        source = self.root / "worker-source.exe"
        source.write_bytes(b"first native binary fixture")
        frozen, digest = runner.freeze_worker(self.root, source)
        source.write_bytes(b"second native binary fixture")
        self.assertEqual(frozen.read_bytes(), b"first native binary fixture")
        self.assertEqual(runner.file_sha256(frozen), digest)
        second, second_digest = runner.freeze_worker(self.root, source)
        self.assertNotEqual(second, frozen)
        self.assertNotEqual(second_digest, digest)

    def test_frozen_worker_tampering_is_rejected_before_reuse(self):
        source = self.root / "worker-source.exe"
        source.write_bytes(b"original fixture")
        frozen, _ = runner.freeze_worker(self.root, source)
        frozen.write_bytes(b"tampered copy")
        with self.assertRaisesRegex(runner.RunnerFailure, "frozen_worker_integrity_failed"):
            runner.freeze_worker(self.root, source)

    def test_oracle_input_is_bounded_and_output_overflow_terminates(self):
        self.worker.write_text("import sys\nbody = sys.stdin.buffer.read()\nprint(len(body))\n", encoding="utf-8")
        result, output, _ = runner.run_owned_process([sys.executable, str(self.worker)],
                                                    cwd=self.root, timeout=5, input_bytes=b"input fixture")
        self.assertEqual(result["exit_code"], 0)
        self.assertEqual(output.strip(), b"13")
        with self.assertRaisesRegex(runner.RunnerFailure, "invalid_process_input"):
            runner.run_owned_process([sys.executable, str(self.worker)], cwd=self.root,
                                     timeout=5, input_bytes=b"x" * 65537)
        self.worker.write_text("import sys,time\nwhile True: sys.stdout.write('x'*8192); sys.stdout.flush()\n", encoding="utf-8")
        result, output, _ = runner.run_owned_process([sys.executable, str(self.worker)],
                                                    cwd=self.root, timeout=5, capture_limit=1024,
                                                    terminate_on_output_limit=True)
        self.assertTrue(result["output_limit"])
        self.assertFalse(result["timed_out"])
        self.assertLess(result["duration_seconds"], 5)
        self.assertLessEqual(len(output), 1024)

    def test_keyboard_interrupt_path_closes_the_owned_process(self):
        self.worker.write_text("import time\ntime.sleep(60)\n", encoding="utf-8")
        real_wait = subprocess.Popen.wait
        calls = []

        def interrupt_once(process, *args, **kwargs):
            if not calls:
                calls.append(True)
                raise KeyboardInterrupt
            return real_wait(process, *args, **kwargs)

        with mock.patch.object(subprocess.Popen, "wait", interrupt_once):
            result, _, _ = runner.run_owned_process([sys.executable, str(self.worker)],
                                                    cwd=self.root, timeout=5)
        self.assertTrue(result["interrupted"])
        self.assertFalse(result["timed_out"])
        self.assertIsNotNone(result["exit_code"])
        self.assertLess(result["duration_seconds"], 5)

    @unittest.skipUnless(os.name == "nt", "local Windows descendant ownership smoke")
    def test_windows_job_closes_descendant_after_successful_parent_exit(self):
        self.worker.write_text("import subprocess,sys,pathlib\nchild=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)'])\npathlib.Path('child.pid').write_text(str(child.pid))\n", encoding="utf-8")
        result, _, _ = runner.run_owned_process([sys.executable, str(self.worker)], cwd=self.root, timeout=5)
        self.assertEqual(result["exit_code"], 0)
        pid = int((self.root / "child.pid").read_text())
        import ctypes
        from ctypes import wintypes
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        kernel.OpenProcess.restype = wintypes.HANDLE
        kernel.GetExitCodeProcess.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        handle = kernel.OpenProcess(0x1000, False, pid)
        if handle:
            code = wintypes.DWORD()
            try:
                self.assertTrue(kernel.GetExitCodeProcess(handle, ctypes.byref(code)))
                self.assertNotEqual(code.value, 259)  # STILL_ACTIVE
            finally:
                kernel.CloseHandle(handle)


if __name__ == "__main__":
    unittest.main()
