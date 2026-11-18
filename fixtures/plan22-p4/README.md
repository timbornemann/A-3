# Plan 22 P4 local component measurements

## Edit

This opt-in component test measures one A10 development window on the frozen `qwen38-4k:latest` profile. It materializes the exact original request and worktree from P0. The native test binds all 30 original ranges, gives the model the two current lines of `step_one.py` plus the active projection and all six global constraints, and uses the real local Ollama adapter. The capability probe is a separate call. The Edit job reserves its initial call before transport. An eligible structural failure permits one repair for the same current window and the same job. The repair changes the request instruction, receives its own durable reservation, and leaves the original receipts intact.

Build the storage unit-test binary with the workspace `a3-provider` dev-dependency and select the executable reported by Cargo. Then run the supervisor with a fresh output directory.

```powershell
cargo test -p a3-storage-libsql --lib --offline --locked --jobs 2 --no-run
python -B fixtures/plan22-p4/run_a10_edit.py --worker target/debug/deps/a3_storage_libsql-<actual>.exe --run-dir target/plan22-p4-runs/a10-first-edit-002
```

`--prepare-only` can use a separate fresh directory without a worker. It validates the frozen inputs and materializes the case without contacting Ollama. The supervisor uses the shared GPU lock and an owned process. The ignored native test is selected exactly and its Windows child wrapper permits only one attempt. Choose a distinct output directory for each authorized attempt and retain previous attempts.

`edit-intent.json` records the exact encoded body size and projection before capacity admission. `edit-dispatch.json` records the existing durable reservation immediately before the projected provider transport. `edit-result.json` records the observed stream counters and settled usage. A repair uses separate `edit-repair-*` receipts. The supervisor verifies both reservations and checks that their cumulative usage survives reopening. The capability probe has separate start/result receipts and its interface does not expose token counters. The before/after runtime receipts retain model, context, server, worker and hardware metadata. A missing observed 4096-token context fails the supervisor check. Server drift from the frozen P0 baseline is recorded because this is a candidate component measurement.

The real patch preview, exact approval, Apply and index publication verify the one path effect. The independent oracle checks the unchanged worktree inventory and the exact integer formula from a closed syntax subset without executing generated Python. After closing and reopening the native store, the local Diff step must be completed and all 24 functional obligations must remain open. This run does not qualify the full A10 case, later source discovery, the P4 gate, or product functional Done.


## Intake to Run

The Intake fixture sends one literal README request through the local `qwen38-4k:latest` provider. It requires an exact Must finding before promotion and checks after reopening that the resulting Run carries the settled Intake usage and original deadline. The Ready step is supplied by the fixture, and the worktree files stay byte-identical.

Use the storage test executable built above and an unused direct directory under `target/plan22-p4-runs`.

```powershell
python -B fixtures/plan22-p4/run_intake.py --worker target/debug/deps/a3_storage_libsql-<actual>.exe --run-dir target/plan22-p4-runs/intake-readme-003
```

The launcher records its input hashes, pinned worker and runtime metadata before starting one exact ignored test under the shared GPU lock. Native receipts and the isolated worktree live in the new directory's `native` subdirectory. A passing result requires the capability probe, the native start and completion receipts, the exact scoped classification, settled usage and a matching reopened Run. Existing attempt directories are retained. The result covers this Intake component only.


## Edit evidence and size branches

The branch launcher accepts `caller`, `too-large`, `continuation` and `known-caller`. Use the same storage test executable and a fresh direct run directory for each attempt.

```powershell
python -B fixtures/plan22-p4/run_edit_branches.py --case caller --worker target/debug/deps/a3_storage_libsql-<actual>.exe --run-dir target/plan22-p4-runs/edit-caller-001
```

`caller` requires an actual `NeedEvidence(Caller)` and one exact read-only caller acquisition. `too-large` requires the typed `TooLarge` reply for an indivisible literal update. `continuation` also sends the acquired caller in a second bounded request and checks the resulting proposal with the native integer-cap oracle. `known-caller` blocks the first Edit request at the Core boundary, reads and journals the current caller, then sends a single 4k Edit request with the new source. It checks the observed wire, new source coverage, the same integer-cap oracle and exact reopened usage. All branches preserve the worktree and leave the original obligation open. The launcher checks the same pinned profile, actual loaded 4k context, and matching native branch and reopen receipts under the shared GPU lock. Each attempt retains its own artifacts, including the exact worker and helper hashes.


For an unpaired TooLarge measurement, pass `--case too-large` with `--model bonsai-2:27b` or `--model ornith-1.5:9b` and a fresh run directory. The tag must already be installed. The 4,096-context, 1,024-output profile disables thinking. The launcher checks model identity and the effective loaded context for each attempt.

For the unpaired Ornith TooLarge measurement, pass `--case too-large --model ornith-1.5:9b`. A confirmed output-limit stop permits one repair of that same Edit job with a shorter-complete instruction. The launcher records both dispatches separately and checks their combined charges after reopen. Other outcomes follow the original one-call branch contract.

## Intake and initial Plan to Run

This component uses the actual local 4k Intake and Plan phases on the manifest-free arithmetic-mean fixture. The Plan selects a scoped change and a dependent check with the real existing unittest command. Reopen must retain the Ready Ledger, both settled calls and the original Run deadline.

```powershell
python -B fixtures/plan22-p4/run_initial_plan.py --worker target/debug/deps/a3_storage_libsql-<actual>.exe --run-dir target/plan22-p4-runs/initial-plan-average-001
```

Use a fresh direct run directory and the V52 test executable. The shared supervisor retains the pinned worker and helper hashes, 4k runtime metadata and native receipts. This component leaves the code and tests byte-identical, with no test execution or task completion. It covers the initial Plan component of P4.
