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
`ornith-1.5:9b`, `gemma4:12b`, `gpt-oss:20b` and `granite4.2:8b` at
16,384 context / 4,096 output, and
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
By default this selects the executable Coding profile. To test another explicitly approved
stored provider without changing roles, set both `A3_RESEARCH_EVAL_PROVIDER` and
`A3_RESEARCH_EVAL_MODEL`. The only reviewed pairs are `openai` / `gpt-5.6-luna` and
`gemini` / `gemma-4-26b-a4b-it`. The slot must already be enabled and connection-verified;
the target must appear in a fresh bounded catalog and pass the real capability probe.
Existing matching Coding settings are preserved; otherwise the ephemeral fixture profile
is 16,384 context with 2,048 output for Luna or 4,096 for Google Gemma. Nothing is persisted.
Clear both override variables for the default selection, and clear all catalog/override
variables before a local test. Ambiguous local-plus-catalog selections fail before any read
or provider request. Local tests also check catalog membership before capability probing.
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

- `completed` retains the historical measure: research returned without a continuation
  request. It does not by itself prove a finished plan. Starting with `rubric_version=2`,
  `passed` additionally requires no `user_halt`, durable `work_ready=true`, and the
  necessary-concept rubric. `work_ready` is the Core's current required-question state,
  not a model's self-rating or a semantic proof. An absent work state cannot pass.
- Rubric v2 recognizes U+2010/U+2011 typography as ASCII hyphens in prose; e.g. `UTF‑8`
  must not be reported as absent. Required concepts and original fixture questions are
  unchanged. This does not normalize code identifiers for evidence admission. Old
  reports without a version keep their original v1 meaning and are never rewritten.
- Rubric v3 additionally requires complete identifiers for the fixture's concrete
  methods and `DictReader`. `Writer` cannot satisfy `write`, `audit_log.txt` cannot
  satisfy `_log`, and `get_task_response` cannot satisfy `get_task`. Qualified calls,
  Markdown code formatting and parentheses remain valid. Prose concepts retain the
  previous matching rules. This is still only a necessary-term check, not static
  analysis, call-order validation or proof of claimed side effects. V1/v2 reports
  keep their original scores; comparisons must name the rubric version explicitly.
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
- Optional `decision_diagnostics` retains at most 24 returned decision records: phase,
  a BLAKE3 fingerprint of length-/role-framed transcript bytes, at most eight delivered
  anchor/file-number mappings, independently decoded result anchors, and numeric/boolean
  output-shape diagnostics. File numbers refer only to this fixed fixture's `FILES`
  order: main, manager, storage, plugins, api (zero-based). Arbitrary paths, source text
  and free-form output fields are not included. Failed provider calls have no returned
  decision record; old reports lack this field. The fingerprint excludes provider-side
  processing and the separately supplied system/schema, and is not an evidence or
  semantic-truth judgment. Diagnostics do not alter prompts, admission or the rubric.

Semantic review additionally checks selection precedence, complete caller/dispatcher/
callback/writer order and destination, actual error conversion, and consistent proposed
interfaces, failure policy and tests. Necessary-term success alone cannot close a known
content defect. Gate results and remaining limits are tracked in
[Plan 10](../../docs/plans/10-RESEARCH_WORK_STATE.md).
