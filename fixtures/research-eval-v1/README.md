# Research evaluation v1

Synthetic, local test project under the repository license. No private TaskFlow source,
provider credentials or external dependencies. Tests only index and read these files;
they do not execute the sample application or modify original projects.

Four fixed families (storage selection, audit path, REST 404, CSV plan), three equivalent
formulations and five repetitions yield 60 observations per implementation. The runner
reports completion separately from a versioned required-concept rubric, model calls,
context UTF-8 bytes, elapsed time and user-continuation state. The concept rubric is a
necessary structural check, not a proof of every natural-language claim; retain answers
for review. Missing results and unsupported conclusions are failures, not successful
bounded-unknown answers for these fully answerable fixture questions.

The opted-in ignored desktop test `research_approved_model_matrix` uses either the explicitly
approved local model or the explicitly approved configured-provider catalog snapshot.
Set `A3_RESEARCH_EVAL_REPETITIONS=5` for the full matrix; default is one smoke repetition.
Provider settings and original files remain unchanged. Standard research budgets apply.

## Reproduction and approval

Live tests are ignored by default and must only be opted in after explicit user approval.
Check that the selected model is already installed and locally resident using Ollama's
loopback `/api/tags` and `/api/show`; a remote model/host is not local residency.
Do not pull models or change application settings. The reviewed local profiles are
`ornith-1.5:9b` and `gemma4:12b` at 16,384 context / 4,096 output, and
`qwen38-8k:latest` at **8,192 context / 2,048 output**. Historical 4B/16k runs remain
separate observations. Local profiles use temperature 0, parallelism 1, conservative
UTF-8 counting and FormatFieldOnly, plus a real structured-output capability probe.

From a checkout of the current implementation, after that approval:

```powershell
$env:A3_LOCAL_RESEARCH_MODEL='ornith-1.5:9b'
$env:A3_RESEARCH_EVAL_REPETITIONS='5'
$env:RUST_TEST_NOCAPTURE='1'
Remove-Item Env:A3_CONFIGURED_RESEARCH_CATALOG -ErrorAction SilentlyContinue
cargo test -p a3-desktop --lib research_approved_model_matrix --offline --locked -- --ignored --nocapture --test-threads=1
```

For the explicitly approved configured-provider check, instead set
`A3_CONFIGURED_RESEARCH_CATALOG` to the existing application's `catalog.db` path.
The runner prints provider, model, context and output only, loads settings read-only
and uses the existing native credential adapter. It does not update or migrate that catalog.
One repetition runs all 12 smoke cases; five runs all 60. Optional
`A3_RESEARCH_EVAL_CASE='family:variant'` selects a diagnostic case (both zero-based),
which must not be presented as a full matrix. Remove that variable for full acceptance.

For the before measurement, use a separate detached worktree at `31e9db7`, and apply
[baseline-31e9db7.patch](baseline-31e9db7.patch) there with `git apply --check` first.
The reviewed patch adds only the frozen synthetic fixture, opt-in test adapters,
the identical production budget calculation under `cfg(test)`, test visibility and
owned native-test isolation. It does not change baseline research, prompt, controller,
schema or provider behavior. Use a separate `CARGO_TARGET_DIR`, the same model/profile,
the same five repetitions and the same command above. Do not run two local matrices
concurrently. On Windows a copied test executable may be used to avoid locking the
build output while other checks compile; preserve its version and execute the same filter.

## Measurement boundaries

Each attempt writes a new JSONL file under that checkout's `target/research-eval/`.
`A3_EVAL_REPORT` identifies it; records retain the public fixture answers for review.
Native child crashes can produce partial files before the existing bounded worker retry:
only a complete 60-record report qualifies as a full matrix, and failed attempts remain
visible. Originals are checked byte-for-byte after every case. No private source is used.

- `completed` measures terminal research without a continuation request; `passed` also
  requires the unchanged necessary-concept rubric. Neither proves semantic truth.
- `user_halt` also catches a returned `QUESTION:`. Baseline has only completion state;
  missing newer metric fields mean **unavailable**, never zero.
- `adaptive_reads` counts durable access starts, and `repeated_adaptive_reads` counts
  additional starts for identical question/scope/access keys. Initial reads, hydration
  and freshness probes are intentionally not included. Different overlapping targets
  are not identical reads.
- Calls include repairs and retries during the case, but not the preceding capability
  probe. `context_utf8_bytes` counts transcript bytes, not system/schema bytes or billed
  tokens. Local conservative usage accounting is not provider billing.
- Elapsed time is end-to-end case wall time on this host, not a portable speed guarantee.
  Concurrent compilation and native retries can affect it. Compare the same profile;
  changing a model is not an isolated harness performance comparison.

Semantic review additionally checks selection precedence, complete caller/dispatcher/
callback/writer order and destination, actual error conversion, and consistent proposed
interfaces, failure policy and tests. Necessary-term success alone cannot close a known
content defect. Gate results and remaining limits are tracked in
[Plan 10](../../docs/plans/10-RESEARCH_WORK_STATE.md).
