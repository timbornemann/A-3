# Plan 22 P5 A01 measurement

This isolated candidate series uses the frozen A01 development task. It has
three Ornith 8192/2048 attempts, three primary Qwen 8192/2048 attempts and one
Qwen 4096/1024 attempt. The holdout and the paired P10 matrix have their own
gates. All seven slots must observe actual task generation. A passing slot
requires both Plan and Edit. The series needs at least two correct completions
per primary, the positive 4k completion, zero FalseDone and zero scope
violations. Recovery injections belong to a separate series.

The supervisor requires the compiled, exact ignored P5 storage test
live_ollama_a01_completes_budget_bound_source_run. That worker and the approved
static source guard are prerequisites. The supervisor alone provides no
generation or product completion proof.

From the repository root, after the native gate is available, register once
with its existing test executable and the predeclared exact Ollama server
version. The server can differ from P0, so this is an unpaired series.

Registration writes series.v3 and the hashed runtime-registration.json before
the first slot. Each profile pins the actual raw parameter, model, template,
system, model-info and weight hashes. Under declared server drift, a raw
parameter hash difference is accepted only if the complete list contains the
same unique numeric keys and exact decimal values as P0. Both observations for
each slot must match the registration. The final observation also requires
one loaded row with the exact model digest and context. Retained v1 and v2
series remain available for status review.

    python -B fixtures/plan22-p5/run_a01.py register --series-dir target/plan22-p5-runs/a01-series-001 --worker target/debug/deps/a3_storage_libsql-<actual>.exe --server-version <pinned-version>
    python -B fixtures/plan22-p5/run_a01.py attempt --series-dir target/plan22-p5-runs/a01-series-001 --slot ornith-01
    python -B fixtures/plan22-p5/run_a01.py status --series-dir target/plan22-p5-runs/a01-series-001

Run the slots in the order stored in series.json. An existing slot directory
is consumed even after interruption. Status retains all seven rows, including
unstarted slots and partial artifact directories. Register a new identified
series if the worker or candidate source changes. The shared GPU lock and
owned-process runner retain bounded process metrics. The task receives 360
seconds, the separate capability probe 120 seconds and worker shutdown 30
seconds. The owned external oracle has its own 120-second limit.

## Native worker contract

A3_PLAN22_P5_RUN_DIR points to the single slot directory. The worker reads
request.json, the exact P0 materializer output, and launch.json
(a3.plan22.p5.a01.launch.v1). Launch binds attempt_id, case_id, variant,
model, context_limit, output_limit, task_timeout_seconds, probe_timeout_seconds,
outer_timeout_seconds, request_sha256, original_sha256 and worker_sha256.
The worker uses worktree/ and isolated appdata/ under this directory.

The worker writes native-start.json (a3.plan22.p5.a01.native-start.v1) and
native-final.json (a3.plan22.p5.a01.native-final.v1) once. Both carry
attempt_id, launch_sha256 and request_sha256, using SHA-256 of the exact file
bytes. The final receipt adds these fields.

- run_state and formal_completed come from the actual durable controller.
  source_controller and capability_probe_verified are booleans.
- bound_intake records complete literal original coverage, zero provider
  calls, and the original capture and deadline. The frozen A01 contract
  explicitly binds the function outcome, protected test and exact argv. The
  fixture passes those typed roles to the Core before the real Plan and Edit.
- phase_dispatches is an array of actual task dispatch receipts. Each has
  phase (plan or edit), kind (initial or structural_repair),
  wire_blake3, request_bytes, num_ctx, num_predict, think, finish,
  prompt_tokens and output_tokens. The capability probe is separate.
- product_test carries argv, cwd, timeout_seconds, test_count, outcome,
  snapshot_current, typed_test_evidence, static_source_proof and source_sha256.
  The hash covers the exact subject.py bytes admitted by the static guard,
  including when the actual scoped test fails. Passing
  requires the exact P0 argv and root cwd, at most 30 seconds, one actual
  passed test, current typed TestEvidence and the approved source guard.
- reopen carries run_state and current_evidence_bound from a cold read.
  effect_scope_violations is an array of closed codes. A failed worker can
  leave product_test and reopen null and record the observed outcome.

A worker exit code or capability probe alone cannot qualify. The supervisor
checks the exact loaded model digest and context, source and worker hashes,
current native proof, and full worktree inventory. Status rechecks the retained
initial and final byte copies of each pass against the frozen initial and
recorded final inventories. The supervisor runs the SHA-verified P0 oracle on another
owned result copy outside the model worktree after the native source-proof
hash matches the exact final bytes. The external oracle is a
functional check within the P0 boundary. Its outcome never enters the
product's VerificationEvidence. Product Done with independently wrong bytes
or an observed scope violation is recorded as FalseDone. Missing independent
evidence leaves the assessment pending. Done without the required current
native proof is classified as false_done_unverified.

## Separate controlled correction

The native recovery control uses a known wrong edit and the unchanged frozen
A01 unittest to produce a real failure. The same Run is reopened, its failed
step is replaced by a bounded edit and a dependent copy of the original test
obligation, and the model gets the current source and bounded failure
diagnostic. Only the correction is model-generated. The fixture independently
checks the proposed Python before issuing an exact approval. The controller
then runs the existing test, verifies current evidence, and checks the
original Run and model budget after another cold reopen.

    python -B fixtures/plan22-p5/run_source_correction.py --repository . --worker target/debug/deps/a3_storage_libsql-<actual>.exe --run-dir target/plan22-p5-correction-runs/correction-001

Use a fresh directory for each owned measurement. The worker uses
`qwen38-4k:latest` at 4096/1024, with one correction call and no structural
repair in the passing trace. The controlled initial failure does not count
toward the seven A01 completion slots.

