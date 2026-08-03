# Architecture Decision Records

Die ADRs dokumentieren langfristige Entscheidungen von A^3.

## Status

- Proposed: zur Diskussion
- Accepted: verbindliche Baseline
- Deprecated: noch vorhanden, aber nicht für neue Arbeit
- Superseded: durch ein benanntes ADR ersetzt
- Rejected: erwogen und bewusst nicht gewählt

Akzeptierte ADRs werden nicht inhaltlich umgeschrieben. Eine neue Entscheidung erhält eine neue Nummer und verweist mit Supersedes auf das alte ADR.

## Index

| ADR | Entscheidung |
| --- | --- |
| [0001](0001-product-identity-and-scope.md) | Produktidentität und V1-Scope |
| [0002](0002-tauri-rust-svelte-desktop.md) | Tauri, Rust und Svelte |
| [0003](0003-modular-monolith-and-dependencies.md) | Modularer Monolith und Abhängigkeitsrichtung |
| [0004](0004-libsql-local-persistence.md) | Lokale libSQL-Persistenz hinter Port |
| [0005](0005-worktree-scoped-storage.md) | Worktree-bezogener Speicher außerhalb des Repositories |
| [0006](0006-deterministic-index-before-llm.md) | Deterministischer Index vor LLM-Kartierung |
| [0007](0007-evidence-graph-and-hybrid-retrieval.md) | Evidence Graph und hybride Suche |
| [0008](0008-epistemic-memory-and-invalidation.md) | Epistemisches Memory und Invalidierung |
| [0009](0009-context-compiler.md) | Context Compiler und Tokenbudgets |
| [0010](0010-single-controller-state-machine.md) | Einzelner Controller mit Zustandsmaschine |
| [0011](0011-local-model-provider-abstraction.md) | Lokale Modellprovider-Abstraktion |
| [0012](0012-safe-tools-and-approval-policy.md) | Sichere Werkzeuge und Freigaben |
| [0013](0013-goal-contract-ledger-and-event-journal.md) | Goal Contract, Task Ledger und Event Journal |
| [0014](0014-cross-platform-release-and-quality.md) | Plattformübergreifende Release- und Qualitätsstrategie |
| [0015](0015-language-adapter-scope.md) | Initialer Sprachumfang und LanguageAdapter |

## Neue ADRs

Kopiere [0000-template.md](0000-template.md), verwende die nächste vierstellige Nummer und ergänze betroffene Dokumente und Pläne.

