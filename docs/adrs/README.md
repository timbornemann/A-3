# Architecture Decision Records

Die ADRs dokumentieren langfristige Entscheidungen von A^3.

## Status

- Proposed: zur Diskussion
- Accepted: verbindliche Baseline
- Deprecated: noch vorhanden, aber nicht für neue Arbeit
- Superseded: durch ein benanntes ADR ersetzt
- Rejected: erwogen und bewusst nicht gewählt

Akzeptierte ADRs werden nicht inhaltlich umgeschrieben. Eine neue Entscheidung erhält eine neue Nummer und verweist mit Supersedes auf das alte ADR.

## Verantwortung und Freigabe

Verantwortlicher Maintainer und finale Freigabeinstanz für ADRs ist **Tim Bornemann**.

- Neue langfristige Entscheidungen beginnen als `Proposed` auf Basis von [0000-template.md](0000-template.md).
- Der Maintainer prüft die Entscheidung gegen Sicherheitsregeln, bestehende ADRs und Architekturregeln und setzt ihren Status nach Review auf `Accepted` oder `Rejected`.
- Änderungen an akzeptierten Entscheidungen erfolgen ausschließlich über ein neues ADR, das die ersetzte Entscheidung ausdrücklich als `Superseded` referenziert.
- Implementierung und Planfortschritt dürfen eine neue Architekturentscheidung erst nach deren Annahme voraussetzen.

## Index

| ADR                                                          | Entscheidung                                               |
| ------------------------------------------------------------ | ---------------------------------------------------------- |
| [0001](0001-product-identity-and-scope.md)                   | Produktidentität und V1-Scope                              |
| [0002](0002-tauri-rust-svelte-desktop.md)                    | Tauri, Rust und Svelte                                     |
| [0003](0003-modular-monolith-and-dependencies.md)            | Modularer Monolith und Abhängigkeitsrichtung               |
| [0004](0004-libsql-local-persistence.md)                     | Lokale libSQL-Persistenz hinter Port                       |
| [0005](0005-worktree-scoped-storage.md)                      | Worktree-bezogener Speicher außerhalb des Repositories     |
| [0006](0006-deterministic-index-before-llm.md)               | Deterministischer Index vor LLM-Kartierung                 |
| [0007](0007-evidence-graph-and-hybrid-retrieval.md)          | Evidence Graph und hybride Suche                           |
| [0008](0008-epistemic-memory-and-invalidation.md)            | Epistemisches Memory und Invalidierung                     |
| [0009](0009-context-compiler.md)                             | Context Compiler und Tokenbudgets                          |
| [0010](0010-single-controller-state-machine.md)              | Einzelner Controller mit Zustandsmaschine                  |
| [0011](0011-local-model-provider-abstraction.md)             | Lokale Modellprovider-Abstraktion (Superseded by ADR-0018) |
| [0012](0012-safe-tools-and-approval-policy.md)               | Sichere Werkzeuge und Freigaben                            |
| [0013](0013-goal-contract-ledger-and-event-journal.md)       | Goal Contract, Task Ledger und Event Journal               |
| [0014](0014-cross-platform-release-and-quality.md)           | Plattformübergreifende Release- und Qualitätsstrategie     |
| [0015](0015-language-adapter-scope.md)                       | Initialer Sprachumfang und LanguageAdapter                 |
| [0016](0016-evidence-based-worktree-reconciliation.md)       | Evidenzbasierte Worktree-Umzugs-Reconciliation             |
| [0017](0017-bounded-repository-discovery.md)                 | Begrenzte deterministische Repository-Discovery            |
| [0018](0018-model-provider-port-ownership.md)                | ModelProvider-Port an der Application-Grenze               |
| [0019](0019-durable-mutation-reconciliation.md)              | Dauerhafte Mutationsdisposition und Reconciliation         |
| [0020](0020-agent-runtime-ownership-and-pause.md)            | Agent-Laufzeitbesitz und kooperative Pause                 |
| [0021](0021-bounded-agent-inspection.md)                     | Begrenzte taskgebundene Diff- und Verification-Inspektion  |
| [0022](0022-task-bound-approval-center.md)                   | Taskgebundenes Approval Center mit expliziter Fortsetzung  |
| [0023](0023-local-settings-and-model-activation.md)          | Lokale Settings und evidenzgebundene Modellaktivierung     |
| [0024](0024-semantic-design-tokens-and-accessible-themes.md) | Semantische Designtokens und zugängliche Themes            |
| [0025](0025-bounded-desktop-rendering-and-lifecycle.md)      | Begrenztes Desktop-Rendering und UI-Lebenszyklus           |
| [0026](0026-explicit-local-provider-model-discovery.md)      | Explizite Providerverwaltung und lokale Modellerkennung    |
| [0027](0027-google-gemini-model-provider.md)                 | Google Gemini Model-Provider Adapter                       |
| [0028](0028-provider-credentials-and-explicit-gemini-remote-access.md) | OS-Credentials und expliziter Gemini-Remotezugriff |
| [0029](0029-core-owned-project-catalog-and-restoration.md)    | Core-Projektkatalog und sichere Wiederherstellung          |
| [0030](0030-bounded-evidence-source-preview.md)               | Begrenzte evidence-gebundene Source-Vorschau               |
| [0031](0031-progressive-code-atlas-index-evidence.md)         | Progressiver Code Atlas und aktuelle Index-Evidence        |
| [0032](0032-openai-model-provider.md)                         | OpenAI Model-Provider und expliziter Remotezugriff         |
| [0033](0033-chatbasierter-agent-workspace.md)                 | Chatbasierter Agent Workspace                              |
| [0034](0034-deep-map-run-journal-and-current-index-lifecycle.md) | Deep-Map-Laufjournal und Current-Index-Lifecycle         |
| [0035](0035-monotone-index-attempt-coordinate-across-rebuilds.md) | Monotone Index-Laufkoordinate über Rebuilds            |
| [0036](0036-deep-map-user-facing-run-dashboard.md)                 | Nutzerorientiertes Deep-Map-Laufdashboard              |
| [0037](0037-nachvollziehbare-adaptive-ask-recherche.md)            | Nachvollziehbare adaptive Ask-Recherche                 |
| [0038](0038-agentische-mehr-runden-recherche.md)                   | Agentische Mehr-Runden-Recherche                       |
| [0039](0039-evidenzgebundene-slash-commands.md)                    | Evidenzgebundene Slash Commands                        |
| [0040](0040-konsistente-arbeitsweg-projektion-und-quellenverweise.md) | Konsistente Arbeitsweg-Projektion und Quellenverweise |
| [0041](0041-sichere-moduswechsel-und-dauerhafte-nachrichtenwarteschlange.md) | Sichere Moduswechsel und Queue                       |
| [0042](0042-adaptiver-agent-arbeitsplan.md)                                  | Adaptiver Agent-Arbeitsplan                          |

## Neue ADRs

Kopiere [0000-template.md](0000-template.md), verwende die nächste vierstellige Nummer und ergänze betroffene Dokumente und Pläne.
