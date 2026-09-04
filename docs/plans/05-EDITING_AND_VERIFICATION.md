# Plan 05: Sicheres Editing, Prozesse und Verifikation

Ziel: Der Agent kann kontrolliert Code ändern und die Änderung gegen Akzeptanzkriterien verifizieren.

Alle angenommenen ADRs einschließlich 0018 bleiben bindend. Für diesen Plan unmittelbar relevant
sind insbesondere 0003, 0006, 0008, 0010, 0012, 0013, 0014 und 0017.

## E1 Policy Engine

Abhängigkeiten: Gate M6

- [x] ActionClass und RiskLevel
- [x] Root-, Path-, Process-, Network- und Git-Policy
- [x] PolicyDecision mit Begründung
- [x] ApprovalRequest und scopegebundene Approval
- [x] Ablauf und Widerruf
- [x] Audit Events

Akzeptanz:

- jede privilegierte Action erhält genau eine PolicyDecision;
- Freigabe für einen Pfad gilt nicht automatisch für andere Pfade;
- Sicherheitsregeln können nicht durch Workspace-Regeln gelockert werden.

Verifiziert am 2026-08-06: Der Domain-Kern leitet Klasse und Risiko aus der geschlossenen
Root-/Path-/Process-/Network-/Git-Action ab und besitzt keine Workspace-Repräsentation zum Lockern
der Systembaseline. Der zentrale Application-Use-Case erzeugt pro Auswertung genau eine begründete
Decision und genau ein content-freies RunEvent. Knowledge-Schema V18 persistiert Requests, Grants,
Decisions, Grant/Widerruf und One-time-Consumption atomar mit der Runsequenz. Der gemeinsame
Storagevertrag belegt Reopen, Pfad-Mismatch ohne Verbrauch, exakte Consumption, Widerruf,
Workspace-Deny und vollständigen CAS-Rollback. Windows-Workspace-Gates, Rustdoc, Node 24.14.0 mit
pnpm 11.9.0, 20 Frontend- und vier Tooltests, Build, 45 Markdown-Dateien mit 66 lokalen Links sowie
der vollständige Linux-`quality`-Job über `act` sind grün.

## E2 Secure File Tools

Abhängigkeiten: E1

- [x] begrenztes Datei-Lesen
- [x] Directory Listing mit Ignore-Policy
- [x] Canonicalization nach Symlinkauflösung
- [x] Special-File-Ablehnung
- [x] Secretklassifikation und Redaction
- [x] Read Evidence

Akzeptanz:

- Traversal- und Symlink-Escape-Fixtures werden abgelehnt;
- Binär- und Großdateien werden nicht unkontrolliert gelesen;
- erlaubte Subtree-Reads funktionieren plattformübergreifend.

Verifiziert am 2026-08-06: Versionierte Domainverträge erzwingen snapshot-/worktreegebundene,
vorwärts paginierte Directory-Seiten mit höchstens 256 direkten Kindern und konkreter
`FileRevision`-Evidence. Die Application-Ports bleiben read-only, abbrechbar und content-frei im
Fehlerfall. `a3-workspace` liest ausschließlich reguläre, kanonisch rootgebundene Dateien in
64-KiB-Blöcken bis 4 MiB, prüft Handle, Ziel und vollständigen Hash erneut und blockiert Binary-
und Secret-Kandidaten. Listings verwenden den ignore-gefilterten Published Index und wenden die
nicht übersteuerbare `DiscoveryPolicy::v1` zusätzlich erneut an. Die öffentliche Contract-Suite
belegt unter Windows vier Fälle einschließlich Junction-Escape; der Linux-Quality-Job belegt alle
fünf Fälle einschließlich Unix-Socket-Ablehnung. Workspace-Clippy, Rustdoc, Node 24.14.0 mit pnpm
11.9.0, 20 Frontend- und vier Tooltests, Build, 45 Markdown-Dateien mit 66 lokalen Links sowie der
vollständige Linux-`quality`-Job sind grün. Der vollständige Windows-Workspace-Test erreichte nur
wegen des unveränderten nativen libSQL-Migrationstests keinen grünen Sammelstatus: dessen Worker
endete nach den drei gemäß Gate zulässigen Versuchen jeweils mit `0xc0000005` vor dem
Abschlussmarker; Assertions und sämtliche E2-Tests blieben grün.

## E3 PatchAction

Abhängigkeiten: E1, E2

- [x] erwarteter Snapshot und Content Hash
- [x] Add, Update, Move und Delete als getrennte Operationen
- [x] Patchvorschau
- [x] atomare Dateioperation soweit möglich
- [x] Konflikt bei Useränderung
- [x] keine automatische Überschreibung
- [x] Post-Patch Change Set

Akzeptanz:

- konkurrierende Useränderung verhindert Anwendung;
- Patch außerhalb Root wird abgelehnt;
- Line-Ending und Encoding-Fixtures bleiben korrekt;
- Diff und Evidence zeigen den tatsächlichen neuen Inhalt.

Verifiziert am 2026-08-06: `PatchActionSchemaVersion::V1` bindet Run, Worktree, Published
Snapshot, TaskStep, Verification sowie erwartete und neue BLAKE3-Hashes an getrennte Add-,
Update-, Move- und Delete-Operationen und einen content-freien Policy-Fingerprint. Der schmale
Application-Port erzwingt eine verbrauchte exakte Approval-Entscheidung. `a3-workspace` prüft
Root, Symlink-/Reparse-Komponenten, Published Revisionen, Live-Hashes und Zielabwesenheit vor der
Vorschau und erneut nach vollständigem Same-Directory-Staging unmittelbar vor der ersten
Mutation. Add und Move überschreiben kein bestehendes Ziel; Update und Delete verwenden atomare
Dateisystemoperationen soweit von der Plattform bereitgestellt. Vollständige und kanonisch
partielle `PatchChangeSet`s tragen die tatsächlichen Revisionen und Invalidierungspfade.

Vier Domain-/Policy-Tests, ein Authorization-Test und sechs öffentliche Workspace-Contracts
prüfen Binary-/Secret-Ablehnung, exakte und global auf 64 KiB begrenzte Vorschau, UTF-8-BOM,
CRLF und Nicht-ASCII-Bytes, alle vier Operationen, tatsächliche Post-Write-Hashes, Useränderung
zwischen Preview und Apply, No-Replace, Symlink-/Junction-Escape und einen späten Konflikt nach
bereits sichtbarer Teilmutation. Formatcheck, Workspace-Clippy mit allen Targets und Features bei
`-D warnings`, Rustdoc bei `-D warnings`, Frontendformat/Lint/Typecheck, 20 Frontend- und vier
Tooltests, Build, 45 Markdown-Dateien mit 66 Links, Dependency-Report und der vollständige
Linux-`quality`-Job über `act` sind grün. Der Windows-Workspace-Sammellauf erreichte wegen des
unveränderten nativen libSQL-`knowledge_contract`-Workers keinen grünen Gesamtstatus: zwei gemäß
Gate zulässige frische Sammelversuche endeten mit `0xc0000005`; der isolierte Wiederholungs-Worker
bestand alle acht Tests mit Abschlussmarker, anschließend bestanden die sonst nicht erreichten
sechs E3-, sieben Project-Catalog- und 26 Shared-Storage-Contracts separat.

## E4 ProcessRunner

Abhängigkeiten: E1

- [x] argv-basierter Start
- [x] CWD- und Executable-Policy
- [x] Env-Allowlist
- [x] Timeout und Cancellation
- [x] Outputcap und Stream Events
- [x] Prozessbaumbeendigung Windows, Linux, macOS
- [x] Network-Klassifikation

Akzeptanz:

- Shellzeichen in Argumenten werden nicht interpretiert;
- Endlosprozess endet nach Timeout;
- Child-Prozess bleibt nach Cancel nicht bestehen;
- Outputoverflow blockiert den Prozess nicht.

Verifiziert am 2026-08-09: `ProcessSpecSchemaVersion::V1` bindet exakte argv-Grenzen, Run,
Worktree, kanonisches CWD, Executable, eine sortierte Env-Allowlist, positive Timeout- und getrennte
Outputlimits, eine optionale Bindung an einen validierten TaskStep sowie Network Scope in einen
domain-separierten Policy-Fingerprint.
Shell Mode ist nicht konstruierbar. Nur plan-gebundene bekannte sichere Commands ohne Netzwerk
können eine echte `SystemAutomatic`-Entscheidung konsumieren; Open- und Netzwerkaktionen bleiben
freigabepflichtig. `AuthorizedProcessSpec` lehnt jede Abweichung von Run, Fingerprint, Scope,
ActionClass oder Risiko ab.

Der Workspace-Adapter startet ausschließlich `std::process::Command` mit einzelnen Argumenten,
geschlossenem stdin, geleerter Umgebung und explizit injizierten Allowlist-Werten. CWD und
Executable werden kanonisiert; relative Executables dürfen nur eine Komponente besitzen und werden
in einem expliziten absoluten `PATH` gesucht. Windows akzeptiert keine `.bat`-/`.cmd`-Interpreter.
Entdeckte Commands erlauben neben `PATH` nur `TEMP`, `TMP` und `TMPDIR`; explizite Temp-Werte
verhindern insbesondere, dass Compiler bei geleerter Umgebung auf geschützte Windows-Verzeichnisse
zurückfallen.
`command-group` 5.0.1 kapselt Unix Process Groups und Windows Job Objects mit Kill-on-Close. Beide
Reader-Threads, der Gruppenprozess und der begrenzte 32-Slot-Channel besitzen einen Owner; Timeout,
Cancellation und Event-Backpressure beenden die Gruppe und joinen alle Reader. stdout und stderr
werden in 8-KiB-Blöcken auch nach dem Retained Limit bis EOF gedraint. Nur valides, secret-geprüftes
UTF-8 darf in lückenlose Stream-Events oder das Resultat gelangen; Bytezahl und BLAKE3-Digest
umfassen dennoch den vollständigen Stream. `Denied` bleibt eine Policy-Klassifikation und behauptet
keine OS-Netzwerksandbox.

Sieben Domain-/Policy-Tests, zwei Application-Autorisierungstests und sechs öffentliche
Workspace-Contracts prüfen Shell-Metazeichen als ein Argument, CWD-/Executable- und Env-Policy,
Timeout, Kindprozess-Cancellation, 2-MiB-Overflow bei 1-KiB-Retention, Secret-Redaction,
Eventsequenz und Event-Sink-Abbruch. Dieselbe Suite ist unter Windows und im vollständigen
Linux-`quality`-Job grün. Der gemeinsame Unix-Pfad und Vertrag sind in der CI-Plattformmatrix für
macOS ARM64 und x86_64 verdrahtet; das übergeordnete Drei-OS-Prozessbaumgate bleibt bis zu diesen
echten Plattformläufen offen.

Formatcheck, Workspace-Clippy über alle Targets/Features mit `-D warnings`, Rustdoc mit
`-D warnings`, Node 24.14.0/pnpm 11.9.0, Frontendformat/Lint/Typecheck, 20 Frontend- und vier
Tooltests, Build, 45 Markdown-Dateien mit 66 Links, der Lizenzbericht ohne unbekannte Lizenzen und
der vollständige Linux-`quality`-Job über `act` sind grün. Zwei frische Windows-Workspace-
Sammelläufe erreichten ausschließlich wegen der unveränderten nativen libSQL-Worker
`index_run_lifecycle_serializes_mutation_and_never_false_publishes` beziehungsweise
`verified_module_cards_publish_atomically_with_evidence_and_search_projection` keinen grünen
Gesamtstatus; beide endeten mit `0xc0000005` und bestanden anschließend isoliert mit
Abschlussmarker. Die vollständige `a3-workspace`-Suite mit allen sechs E4-Contracts ist separat
grün.

## E5 Command Discovery

Abhängigkeiten: Fast Index, E4

- [x] sichere Test-, Build-, Lint- und Formatbefehle aus Manifesten ableiten
- [x] Commands als ProcessSpec anzeigen
- [x] Allowlist je Projekt bestätigen
- [x] kein automatisches Paketinstall
- [x] Working Directory je Package

Akzeptanz:

- Rust-, Node- und Python-Fixtures erhalten korrekte Standardbefehle;
- Lockfile-fehlender Installversuch startet nicht automatisch;
- Monorepo-Kommandos laufen im richtigen Package.

Verifiziert am 2026-08-09: Der versionierte Domain-Katalog akzeptiert ausschließlich die
geschlossenen Arten Test, Build, Lint und Format. Der Katalog-Digest bindet den Worktree; jeder
Command-Digest bindet CWD, Executable, argv, Env, Limits und konkrete Manifest- oder
Source-Evidence. Jede evidenzwirksame Katalogänderung macht eine Bestätigung stale. Vorschauen
bleiben bis zur exakten,
projektbezogenen Allowlist-Bestätigung ungebunden und niemals automatisch ausführbar. Erst die
zusätzliche Bindung an einen validierten TaskStep erzeugt einen `KnownSafe`-`ProcessSpec`; dessen
Ausführung bleibt dem zentralen E4-Policy- und Runner-Pfad vorbehalten.

Die Application Discovery arbeitet ausschließlich auf dem veröffentlichten Fast-Index. Rust erhält
offline und lockfilegebundene Cargo-Kommandos, Node nur benannte Test-/Build-/Lint-/Format-Skripte
mit genau einem nächstgelegenen indexierten pnpm-, npm- oder Yarn-Marker und Python nur statisch aus
seinen Manifestrelationen ableitbare Module. Install- und Lifecycle-Skripte sind nicht
repräsentierbar; ein fehlender oder mehrdeutiger Package-Manager-Marker erzeugt keinen Node-Command.
Jeder Monorepo-Command trägt das Package-Verzeichnis als eigenes CWD. Die append-only
Knowledge-Schema-V19-Persistenz speichert monotone Allowlist-Revisionen mit Compare-and-Swap und
Worktree-Isolation; nur die dokumentierte Identity-Reconciliation darf IDs kaskadieren.

Zwei Domain-Tests, drei Application-Tests, der gemeinsame LibSQL-Allowlist-Contract sowie die echte
Rust-/pnpm-Monorepo-/Python-Fixture belegen Ableitung, Bestätigung, Freshness, Reopen, stale CAS,
fehlendes Lockfile, ausgeschlossene Installbefehle und Package-CWD. Formatcheck, Workspace-Clippy
über alle Targets/Features mit `-D warnings`, Rustdoc mit `-D warnings`, Node 24.14.0/pnpm 11.9.0,
Frontendformat/Lint/Typecheck, 20 Frontend- und vier Tooltests, Build, 45 Markdown-Dateien mit 66
Links, Lizenzbericht sowie der vollständige Linux-`quality`-Job über `act` sind grün. Der
Windows-Workspace-Sammellauf erreichte nach grünen Rust-Tests erneut den bereits dokumentierten
nativen libSQL-Worker-Absturz `0xc0000005`; der vollständige Linux-Lauf bestand dieselben 27
Knowledge-Contracts einschließlich E5.

## E6 Verification Engine

Abhängigkeiten: E3 bis E5

- [x] VerificationSpec-Typen: Command, Test, DiffInvariant, Diagnostic, UserConfirm
- [x] schmalste relevante Prüfung zuerst
- [x] Ergebnisse als TestEvidence oder CommandEvidence
- [x] Muss- und Soll-Kriterien
- [x] Freshness und Snapshotbindung
- [x] Acceptance-Verifier

Akzeptanz:

- Exitcode allein ohne erwartete Semantik genügt nicht bei spezialisierten Verifikationen;
- Done wird bei fehlgeschlagenem Muss-Kriterium blockiert;
- Useränderung nach Test macht betroffene Verification stale.

Verifiziert am 2026-08-10: Neue Ledger-Schritte verwenden die geschlossene operationale
`VerificationSpec`-Union mit deterministischer Narrow-to-Broad-Reihenfolge und konkretem
Criterion-Mapping. Immutable V1-Artifacts bewahren content-freie Command-, strukturierte Test-,
Diagnostic-, vollständige Diff- oder scopegenaue UserConfirm-Semantik samt Present-/Absent-
Freshness-Abhängigkeiten. Diff-Evidence stammt entweder aus dem exakten E3-Patchresultat oder aus
zwei geordneten vollständigen Published Indexes, sodass `NoChanges` operational und exakt
snapshotgebunden beweisbar ist. Erfolg wird ausschließlich aus Spec, Artifact und aktuellem Published
Index abgeleitet; Exitcode 0 ohne strukturierte Testsemantik scheitert. Der produktive
`DeterministicAcceptanceVerifier` prüft exakt die Must-Evidence sowie einen ankergleichen
regenerierten Run-Memory-Checkpoint; Should-only darf ohne Evidence abschließen, eine offene
aktuelle taskbezogene Hypothesis blockiert weiterhin `Done`.

Knowledge-Schema V20 persistiert Must/Should, Step-Mappings, operationale Specs und alle fünf
Evidence-Varianten samt disjunkten Patch-/Index-Diffquellen mit stabiler ID-Rekonstruktion. Der
gemeinsame Adaptervertrag belegt Timeout und Cancellation ohne Teilwrite, idempotentes Append,
Reopen, Acceptance und gezielte Stale-Ablehnung nach einer betroffenen
Useränderung; Migrationstests belegen leeres V20-Schema und vollständigen V19→V20-Rollback.
Rustfmt, Workspace-Clippy über alle Targets/Features mit `-D warnings`, Rustdoc mit `-D warnings`,
Node 24.14.0/pnpm 11.9.0, Frontendformat/Lint/Typecheck, 20 Frontend- und vier Tooltests, Build,
45 Markdown-Dateien mit 66 Links, Lizenzbericht sowie der vollständige Linux-`quality`-Job über
`act` sind grün. Der Windows-Sammellauf verlor erneut ausschließlich den bekannten nativen
libSQL-Worker `knowledge_upgrades_from_every_supported_predecessor` nach drei Versuchen mit
`0xc0000005`; die isolierten V20-Schema-/Rollbacktests und der vollständige Linux-Lauf bestanden,
letzterer die gesamte Suite ohne Retry.

## E7 Mutating Controller

Abhängigkeiten: E1, E3, E6

- [x] ApplyPatch- und Run-Action-Schemas
- [x] ein Mutations-Lock pro Worktree
- [x] Policy und Approval vor Ausführung
- [x] sofortige Indexinvalidierung nach Patch
- [x] Context Recompile
- [x] Verify und Replan
- [x] Fortschrittsdetektor

Erweiterung vom 2026-09-04 nach ADR-0042: Replan ist nun auch im produktiven Conversation-Executor
geschlossen. Ein neuer Befund oder ein deterministisch klassifizierter Mutationsfehler kann einen
offenen Schritt ersetzen und ein zusätzliches begrenztes Todo einschieben. Höchstens acht
automatische Revisionen sind pro Run erlaubt; jede Änderung und jeder Test bleibt ein eigener
verifizierter Ledger-Schritt, und ein notwendiger Nutzerentscheid wird als Haltepunkt statt als
technischer Laufzeitfehler gemeldet.

Akzeptanz:

- kein zweiter mutierender Turn parallel;
- Modell arbeitet nach Patch nicht mit altem Codekontext weiter;
- wiederholte identische Fehlaktion triggert Replan oder Stop.

Verifiziert am 2026-08-11: `AgentAction` V2 erweitert den rückwärtskompatibel lesbaren V1-Vertrag
ausschließlich um vollständig strukturierte `ApplyPatch`- und kataloggebundene `Run`-Aktionen;
rohe argv- oder Shellfelder sind in Schema und Runtime-Decoder nicht darstellbar. Der injizierte,
nicht globale `WorktreeMutationCoordinator` hält genau einen nicht klonbaren Lease pro Worktree
für Patch und Prozess und zählt content-identische Fehler deterministisch als Retry, Replan und
Stop. `ExecuteMutatingAgentAction` prüft Run-, Ledger-, Step-, Snapshot- und Command-Anker,
persistiert zentrale Policy und scopegenaue Approval vor dem Adapteraufruf und schließt einen
erfolgreichen Toolversuch atomar mit content-freiem ToolEvent und Runprojektion ab. Knowledge-
Schema V21 bewahrt historische V1-Actionklassen, speichert alle sechs V2-Klassen geschlossen und
rollt den atomaren Abschluss bei Runsequenzkonflikt vollständig zurück. Jede sichtbare vollständige
oder partielle Patchwirkung publiziert zuerst die exakten Änderungspfade; nur ein Context Pack mit
dem neuen Run-/Ledger-/Published-Snapshot darf anschließend entstehen. Diff-Schritte schließen
ausschließlich mit typisierter aktueller Evidence ab; Test- und Diagnostic-Semantik wird nie aus
Exitcode allein erfunden.

Die reale E7-End-to-End-Suite belegt einen unveränderten Worktree während `AwaitApproval`, exakten
One-time-Grantverbrauch, Patch plus Reindex vor Context, Diff-Verifikation sowie Worktree-
Serialisierung und `Replan` nach der zweiten identischen fehlgeschlagenen Run-Aktion. Rustfmt,
Workspace-Clippy über alle Targets/Features mit `-D warnings`, Rustdoc mit `-D warnings`, die
gezielten Domain-/Application-/Storage-/E7-Verträge, Node 24.14.0/pnpm 11.9.0,
Frontendformat/Lint/Typecheck, 20 Frontend- und vier Tooltests, Build, 45 Markdown-Dateien mit 66
Links, Lizenzbericht sowie der vollständige Linux-`quality`-Job über `act` sind grün. Der Windows-
Sammellauf verlor ausschließlich den bereits dokumentierten nativen libSQL-Worker
`knowledge_upgrades_from_every_supported_predecessor` nach drei erfolgreichen fachlichen Läufen
beim Teardown mit `0xc0000005`; die isolierten V21-Schema-, Rollback-, Run-Journal-, Recovery- und
E7-Verträge sowie der vollständige Linux-Lauf bestanden ohne Retry.

## E8 Failure Recovery

Abhängigkeiten: E7

- [x] Patch Conflict
- [x] Test Failure
- [x] Tool Timeout
- [x] Provider Disconnect
- [x] DB Busy oder Corruption
- [x] User Cancel
- [x] App Crash zwischen Patch und Journal

Akzeptanz:

- keine Recovery verwirft fremde Änderungen;
- Zustand zeigt klar Applied, NotApplied oder Unknown;
- Unknown erfordert Reconciliation vor weiterer Mutation.

Verifiziert am 2026-08-11: Knowledge-Schema V22 speichert vor jeder Mutation atomar einen
content-freien `Unknown/required`-Versuch mit Action-Fingerprint und Aktionsart. Vollständige oder
partielle Patchwirkungen und terminal beobachtete Prozesse werden `Applied`; Konflikt, Ablehnung,
Spawnfehler und Cancellation vor Prozessstart werden `NotApplied`; Timeout, Cancellation nach
Prozessstart, verlorene Prozessbeobachtung und ein ausgefallener Resultat-/Journalabschluss bleiben
`Unknown`. Erfolgreiche Mutationen schreiben Toolresultatdigest, ToolEvent, Runprojektion,
Lifecycle und Disposition gemeinsam. Ein `Unknown` sperrt den gesamten Worktree atomar. Die
Reconciliation wiederholt und verwirft nichts, sondern publiziert unter demselben Mutations-Lease
einen vollständigen aktuellen Repositorysnapshot; erst ein anschließender atomarer Recovery-
`Replan` öffnet weitere Mutationen, `Resume` bleibt gesperrt.

Domain-, Application-, gemeinsame Storage-, V22-Migrations-, Workspace- und reale
Agent-Harness-Verträge belegen Patchkonflikt und partiellen Patch ohne Verlust fremder Änderungen,
fehlgeschlagene Command-Verifikation als `Applied`, Timeout und Cancellation nach Start als
`Unknown`, Cancellation vor Start als `NotApplied`, Providerdisconnect ohne Adaptermutation,
Store-Unverfügbarkeit/-Korruption vor Adapteröffnung sowie einen echten Patch mit simuliertem
Crash vor Journalabschluss, zusätzlicher fremder Datei, Vollscan-Reconciliation und erzwungenem
Replan. Rustfmt, Workspace-Clippy über alle Targets/Features mit `-D warnings`, Rustdoc, die
gezielten Windows-Verträge und der vollständige Linux-`quality`-Job über `act` sind grün; dieser
Job umfasst sämtliche Workspace-Tests, Frontendformat/Lint/Typecheck, 20 Frontend- und vier
Tooltests, Build, 46 Markdown-Dateien mit 74 lokalen Links und den Lizenzbericht. Der Windows-
Sammellauf erreichte erneut ausschließlich den bereits dokumentierten nativen libSQL-Teardownfehler
`0xc0000005` im `knowledge_contract`; die isolierten fachlichen E8-, Storage- und Migrationstests
sowie der vollständige Linux-Lauf bestanden ohne Retry.

## E9 End-to-End Coding Tasks

Abhängigkeiten: E8

- [x] kleiner lokaler Bugfix
- [x] Änderung über zwei Module
- [x] Test ergänzen
- [x] absichtlich scheiternder Plan und Replan
- [x] zwischenzeitliche Useränderung
- [x] Context Compaction während Aufgabe

Akzeptanz:

- jede Änderung besitzt Goal, Step, Patch, Evidence und Verification;
- keine Aufgabe endet Done mit rotem Muss-Test;
- Evalresultate sind reproduzierbar gespeichert.

Verifiziert am 2026-08-11: `a3.agent-coding-eval.v1` führt fünf kleine, selbstenthaltene und
offline ausführbare Python-Repositories über den realen libSQL-, Fast-Index-, Command-Discovery-,
Policy-/Approval-, Workspace-Patch-, argv-Process-, Context-, Evidence- und Acceptance-Pfad. Das
reviewbare Golden Result fixiert für jeden Fall finalen Runzustand, durable Goal-/Step-Anker,
Patch-, Evidence- und Verification-Nachweis, Erhalt fremder Änderungen sowie Replan- und
Compaction-Anzahl. Zwei vollständige Durchläufe in unabhängigen Repositories und Datenbanken
müssen bytegleich dieselbe geordnete Projektion erzeugen; nach `Done` werden Goal, vollständiges
Ledger samt Store-Version und materialisierter Run erneut aus libSQL geladen und verglichen.

Der Replan-Fall wendet zunächst einen plausiblen, aber falschen Patch an. Zwei echte rote
Must-Test-Verifikationen erzwingen den begrenzten `Replan`; der historische Step samt fehlgeschlagener
Verification bleibt erhalten. Ein Completion-Versuch wird als `IncompleteLedger` abgelehnt. Eine
zwischenzeitlich angelegte Nutzerdatei erscheint vor dem Replan in einem neuen vollständigen
Published Snapshot, wird vom Ledger-/Run-Replan übernommen und bleibt nach dem korrekten
Ersatz-Step bytegleich erhalten. Der Compaction-Fall verifiziert Step 1, regeneriert zweimal
denselben `RunMemoryCheckpoint` ausschließlich aus autoritativen Quellen, reinjiziert ihn mit Goal,
abgeschlossenem und offenem Step in den nächsten deterministischen Context und journalisiert den
Compile, bevor Step 2 über Patch und Test bis `Done` läuft.

Rustfmt, Workspace-Clippy über alle Targets/Features mit `-D warnings`, Rustdoc, der fokussierte
zweifach reproduzierte E9-Contract, die direkt ausgeführten lokalen Frontend-/Tool-Gates mit 20
Frontend- und vier Tooltests, Build, 47 Markdown-Dateien mit 74 lokalen Links, Lizenzbericht und
der vollständige Linux-`quality`-Job über `act` sind grün. Der Windows-Workspace-Sammellauf
erreichte erneut ausschließlich wegen des dokumentierten nativen libSQL-Teardowns `0xc0000005` in
`knowledge_upgrades_from_every_supported_predecessor` nach drei Worker-Versuchen keinen grünen
Gesamtstatus; derselbe Contract bestand unmittelbar danach isoliert mit Abschlussmarker. Der
vollständige Linux-Lauf bestand ohne Retry.

## Gate M7

ADR-0039 ergänzt keine neue Mutationsart. `/review`, `/debug`, `/doc`, `/refactor` und `/test`
präzisieren im Agent-Modus ausschließlich Ziel, Reihenfolge und Verification. Jeder bestätigte
Fund muss als eigener evidence-gebundener Ledger-Schritt ausführbar bleiben; Hypothesen sind keine
Änderungsgrundlage. Prozesse bleiben auf aktuelle, manifestbelegte Test-, Build-, Lint- und
Format-Commands mit direktem argv, zentraler Policy, bestehenden Approvals und begrenzter Ausgabe
beschränkt. Ask und Plan führen keine Projektprozesse aus.

- [x] Slash Commands erweitern weder Allowlist noch Approval- oder Prozessautorität
- [x] Agent-Command-Constraints bleiben getrennt von typisierten AgentActions
- [x] jeder veränderte bestätigte Fund besitzt aktuelle Verification

Abnahme vom 2026-09-04: Das Command-Profil wird als nicht autorisierende, typisierte
Constraint an Recherche, Context Compiler und Materialisierung übergeben. Prozessausführung bleibt
ausschließlich über den vorhandenen manifestbelegten Katalog, direkte argv, zentrale Policy,
Approval und Verification möglich; Ask und Plan erhalten keine Prozess- oder Mutationsaktion.
AgentAction-Schema und Controllergrenze bleiben davon getrennt. Im Agent-Modus erzwingt das
Core-Profil pro bestätigtem, eigenständig änderbarem Fund einen Top-Level-Punkt in
`Implementation Changes`. Der Materializer bildet diese Punkte auf eine sequenzielle Ledger-Kette
mit eigener Verification-Spezifikation und Evidence-Erwartung ab. Nach erfolgreicher Verification
startet ein atomarer Application-Use-Case genau den nächsten bereiten Schritt; erst wenn alle
Schritte `Completed` sind, darf die Acceptance-Prüfung beginnen. Unit-Regressionen belegen
Zerlegung, Fallback bei nicht strukturierten Plänen und den atomaren Übergang
`Verify → Execute` zum nächsten Schritt.

- [x] Security-Negativsuite grün
- [x] Prozessbaumtests auf drei OS
- [x] Mutations-Lock und Snapshotkonflikte grün
- [x] alle E2E-Tasks mit vollständigem Audit
- [x] kein Netzwerk oder Paketinstall ohne Approval
- [x] keine destruktive Gitaktion im automatischen Pfad

Verifiziert am 2026-08-11: Die E2-, E3-, E5-, E7-, E8- und E9-Verträge decken Traversal,
Symlink-/Junction-Escape, Secret- und Binary-Grenzen, Snapshot- und Useränderungskonflikte,
worktreeweite Mutationsserialisierung, vollständige Mutation-Dispositionen sowie fünf
reproduzierbare Coding-E2E-Aufgaben mit dauerhaftem Goal-, Step-, Patch-, Evidence-,
Verification- und Run-Journal-Nachweis ab. Installationskommandos sind im geschlossenen
Command-Katalog nicht darstellbar; Netzwerk, Shellmodus, Destruktion und schreibende Gitaktionen
bleiben approval-pflichtig beziehungsweise außerhalb des automatischen Controllerpfads.

Der öffentliche GitHub-Actions-Lauf `31340939100` auf Commit `e2e35f8a` führte den vollständigen
nativen Workspace-Teststand einschließlich der ProcessRunner-Contracts erfolgreich unter Windows
x86_64, Linux x86_64, macOS ARM64 und macOS x86_64 aus. Damit ist auch der in E4 zunächst offen
gebliebene reale Drei-OS-Prozessbaumbeleg erbracht. Die späteren E7- bis E9-Abnahmen bestanden
zusätzlich den vollständigen lokalen Linux-`quality`-Job und ihre fokussierten Windows-Verträge.
