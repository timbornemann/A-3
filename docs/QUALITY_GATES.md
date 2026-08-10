# Qualitätsgates und Definition of Done

Status: verbindliche Baseline  
Stand: 2026-08-11

## Grundsatz

Qualität ist eine überprüfte Eigenschaft. „Sieht korrekt aus“, erfolgreiche Kompilierung oder eine LLM-Einschätzung reichen nicht als Abschlussnachweis.

## Gate pro Änderung

### Rust

- cargo fmt --check
- cargo clippy --workspace --all-targets --all-features mit -D warnings
- relevante Unit- und Integrationstests
- cargo test --workspace --all-features
- Dokumentation für öffentliche APIs und Invarianten

### Frontend

- Formatter
- Linter ohne Warnungen
- TypeScript Typecheck
- Unit- und Component-Tests der Änderung
- Accessibility-Prüfung für neue Interaktionen

### Persistenz

- Migration von leerer DB
- Upgrade aus jeder unterstützten Vorgängerversion
- Rollback des Appstarts bei fehlgeschlagener Migration ohne Datenverlust
- Contract-Tests gegen temporäre DB
- Goal-Contract-Contracts prüfen atomare initiale Erstellung, Linked-Worktree-Isolation,
  lückenlose Compare-and-Append-Revisionen, Konflikte konkurrierender Writer, unveränderte
  Auditstände und exakte Wiederherstellung nach Reopen.
- Task-Ledger-Contracts prüfen atomare Erstellung und Compare-and-Swap-Ersetzung,
  Linked-Worktree-Isolation, unveränderliche Definitionen sowie Versuchs- und Replan-Historie,
  fehlgeschlagene und erfolgreiche Verifikation, transitive Evidence-Invalidierung und exakte
  Wiederherstellung des vollständigen Aggregats nach Reopen.
- Run-Journal-Contracts prüfen atomaren Start, Linked-Worktree-Isolation, genau einen Gewinner bei
  konkurrierenden Appends derselben Sequenz, lückenloses Paging, atomare Run-Materialisierung,
  Redaction ohne Secret-Fixture, deterministischen begrenzten JSONL-V1-Export und exakte
  Wiederherstellung nach Reopen. Jeder neue Run behält dabei seine ModelProfile-ID und -Version;
  V14-Adaptertests erlauben ausschließlich migrierte Legacy-Nullpaare und lehnen partielle
  Profilreferenzen ab. V16-Contracts prüfen zusätzlich den atomaren Tool-Event-/Metadaten-/
  Evidence-Append ohne Raw Preview sowie den gemeinsamen Ledger-/Run-Commit mit getrennten
  Compare-and-Swap-Konflikten.
- V17-Recovery-Contracts schließen einen Store mit laufendem Toolversuch, öffnen ihn neu und
  verlangen Interrupted plus monotone Retry-Nummer. Sie prüfen frische und stale
  Verification-Evidence, Resume-Ablehnung auf altem Snapshot, transitives Step-Reopen,
  Resume/Replan/Cancel sowie den vollständigen Rollback von Published-Snapshot-, Ledger- und
  Run-Sequenzkonflikten. Nur der atomare Toolresultat-/Journalpfad darf einen Versuch als
  erfolgreich abschließen.
- V18-Policy-Contracts prüfen begründete Decision und Request nach Reopen, exakten Pfad-Scope,
  Mismatch ohne Grantverbrauch, einmalige Consumption, Widerruf, restriktive Workspace-Regeln und
  vollständigen Rollback von Decision, Request, Event und Runprojektion bei veraltetem
  Runsequenz-CAS.
- V19-Command-Allowlist-Contracts prüfen leeren Anfangszustand, exakte Confirmation, Reopen,
  monotone Revisionen, vollständigen Rollback eines veralteten CAS und Worktree-Isolation. Die
  Migrationstests prüfen zusätzlich V18→V19-Rollback, unveränderliche Revisionen und die allein
  erlaubte Worktree-Reconciliation-Cascade.
- V20-Verification-Contracts prüfen alle fünf typisierten Evidence-Varianten, Must-/Should-
  Acceptance, Timeout und Cancellation ohne Teilwrite, idempotentes Append, Reopen sowie gezielte
  Freshness-Ablehnung nach einer betroffenen Indexpublikation. Migrationstests decken leeres
  Schema, jeden Vorgänger bis V19 und vollständigen V19→V20-Rollback ab.
- V21-Contracts prüfen die rückwärtskompatible Rekonstruktion historischer V1-Actionklassen, alle
  sechs V2-Actionklassen und den atomaren Erfolgsabschluss eines mutierenden Toolversuchs zusammen
  mit content-freiem Journal-Event und Runprojektion einschließlich vollständigem Rollback bei
  Runsequenzkonflikt. Migrationstests decken leeres Schema, jeden Vorgänger bis V20 und
  vollständigen V20→V21-Rollback ab.
- Rebuild trennt regenerierbare und dauerhafte Daten korrekt
- Der Windows-libSQL-Test-Harness führt native In-Memory-Tests, jede unabhängige
  Storage-Contract-Phase und jeden libSQL-basierten inkrementellen Index-Contract in einem eigenen
  Worker aus; dieselbe Isolation schützt die Retrieval-Evalbaseline. Erfolg gilt erst nach dem
  Abschlussmarker hinter der letzten Assertion; nur
  `STATUS_ACCESS_VIOLATION` darf höchstens zweimal mit einem frischen Worker wiederholt werden.
  Assertion- und Vertragsfehler werden nie wiederholt. Verwaiste, exakt mit der Worker-PID
  präfixierte Testverzeichnisse werden nach dessen Prozessende entfernt.

### Index und Retrieval

- Golden Fixture für Parseränderungen
- deterministische Wiederholung ergibt identische normalisierte Resultate
- Löschung, Umbenennung und Syntaxfehler getestet
- Graphzyklen terminieren; kürzeste Pfade, Hopgrenze, Resultlimit und Beziehungsevidenz sind getestet
- Fusion-Golden fixiert Policyversion, Stable-ID-Deduplizierung, alle Signale und Exact-vor-Semantic
- Modulbildungs-Contracts prüfen verschachtelte Monorepo-Manifeste, manifestlose Pfadgrenzen,
  deterministische Wiederholung, SCC-Communities, genau eine primäre Membership, aktuelle
  Membership-Evidence, zentrale Symbole, Entrypoints, Tests, Repository Card, Cancellation und
  abgelehnte Progressausgabe
- Die mehrsprachige Deep-Map-Golden indiziert die Rust-, TypeScript- und Python-Produkt-Fixtures
  bis zum atomar veröffentlichten Index und fixiert aktuelle Modul-Evidence, vollständige
  leere-Coverage-Planung, Budget, Schrittverifikation und deterministische Wiederholung.
- Semantic-Card-/Embedding-Contracts prüfen BodyHash-Kanonik, Profil-/Dimensionsisolation,
  Redaction, Cancellation, Disabled ohne Adapterzugriff, persistentes Reopen, native
  dimensionsgebundene Vector-Capability, begrenzten linearen Fallback und semantikexklusiven Rebuild
- Claim-Verifier-Contracts prüfen das strikt versionierte Claim-Schema, exakte Evidence-Auflösung
  gegen den aktuell veröffentlichten Index, Ablehnung erfundener oder veralteter IDs, sichtbare
  Widersprüche, getrennte Classification und Confidence sowie ausschließlich verifizierte,
  atomare Card-Publikation mitsamt Evidence und Lexical-Search-Projektion
- Task-Lens-Contracts prüfen kanonische Goal-/Step-/Fehler-/Pfadseeds, Exact-vor-FTS-vor-Graph/Test-
  vor-Claim-vor-Semantic-Reihenfolge, L0 bis L3, Budget und sichtbare Trunkierung, Digest-
  Determinismus, Indexdelta, Cancellation/Deadline, Produktionscode mit Regressionstest,
  ausgeschlossene Großmodule sowie null stale Fact Leakage
- Invalidierungs-Contracts prüfen direkte Evidence-Änderung vor dem nächsten Read, `Stale` für
  eigene und `NeedsReview` nur für direkt abhängige Cards, Parser-/Mappergründe, stabile
  Direkt-vor-Abhängig-Remapreihenfolge, Queue-Cancellation und -Ersetzung, Erhalt unabhängiger
  aktueller Claims sowie null stale Fact Leakage nach Task-Lens-Rebuild
- Retrieval-Eval zeigt keinen unbegründeten Recall-Rückgang
- keine stale Evidence in Facts

### Model Provider

- Die gemeinsame dev-only Streaming-Contract-Suite prüft den neutralen Stub und den konkreten
  Ollama-Adapter auf exakte Provideridentität, begrenzte Ereignisfolge, genau eine terminale
  Completion am Streamende und dieselbe erwartete `ProviderEvent`-Projektion.
- Der allgemeine Application-Port besitzt keine Ollama-, HTTP- oder Adapter-Payloadtypen; der
  Cargo-Graph zeigt ausschließlich `a3-provider` → `a3-application` → `a3-domain`.
- Der neutrale Stubprovider emittiert exakt skriptbare Events und Fehler, wartet wakebar auf
  Cancellation und speichert ausschließlich content-freie Aufrufmetadaten.
- Der Ollama-Stubserver prüft die exakte Requestabbildung, fragmentierte chunked NDJSON-Antworten,
  Eventreihenfolge, terminale Usage und sauberes Body-Ende vollständig offline.
- Cancellation beendet Connect oder Body-Read und schließt die laufende Response; das
  Gesamttimeout wird vor Headern und während eines stockenden Response-Bodys als `TimedOut`
  normalisiert.
- Endpoint-Contracts prüfen localhost-Normalisierung, abgelehnte Credentials/Pfade, HTTPS-Pflicht
  für Remote, Local-only als Standard und Ablehnung vor jedem Netzwerkversuch ohne Policyfreigabe.
- Parser-Negativtests lehnen Modell-/Rollenabweichung, Tool Calls, zu große oder ungültige NDJSON-
  Daten, fehlenden Abschluss und Daten nach `done` ab. Prompt, Output, Endpoint und Provider-
  Fehlerbody dürfen nicht in Debug- oder normalisierte Fehlertexte gelangen.
- ModelProfile-Tests prüfen alle V1-Limits, deterministische ID-Ableitung, konservative UTF-8-
  Bytezählung, kanonische redigierte Stopbedingungen und Overrides, die Capability-Evidenz nicht
  verändern können. Jeder neue Run behält Profil-ID und Schemaversion nach Reopen.
- Die neutrale Capability-Stub-Suite belegt, dass weder Modellname noch manueller Override eine
  fehlgeschlagene Structured-Output-Probe hochstufen und dass explizite Providerkontextgrenzen vor
  Profilerzeugung gelten.
- Der Ollama-Stubserver prüft `/api/show`, das exakte kleine `/api/chat`-Schema, Profiloptionen,
  erfolgreiche und schemawidrige Probeantworten, Cancellation vor Netzwerk und ein gemeinsames
  Gesamttimeout über beide Requests. Metadaten mit mehreren abweichenden Kontextgrenzen werden
  abgelehnt; nur die exakte Capability `tools` setzt den nicht ausführbaren nativen Toolmodus.

### AgentAction und Prompt

- Domain-Tests prüfen Grenzen und Redaction für Search, paged File Inspect, Testselektor,
  nicht-verifizierende Ledger-Intents sowie die eindeutige Mutationsklassifikation. V1 bleibt als
  read-only Historienvertrag lesbar; V2 ergänzt ausschließlich strukturierte ApplyPatch- und
  kataloggebundene Run-Aktionen.
- Schema- und Decoder-Tests akzeptieren alle sechs V2-Top-Level-Actions und sämtliche fünf Inspect-
  Ziele, lehnen aber unbekannte Toolnamen und Felder, Trailing Text, Traversalpfade, rohe argv-/
  Shellfelder, nicht kanonische IDs, widersprüchliche Patchanker sowie übergroße oder
  kontrollzeichenhaltige Werte ab. V1 bleibt getrennt rückwärtskompatibel dekodierbar; Schema und
  Decoder werden unabhängig geprüft und jede Objektebene ist geschlossen.
- Prompttests zählen den statischen Vertrag mit dem ModelProfile-Counter gegen das feste
  900-Token-Budget, blockieren Profile ohne verifizierten Structured Output und vergleichen die
  optionale kanonische Schemawiederholung mit demselben Provider-Schema.
- Repair-Tests belegen eine nicht clonebare, bei Anweisungserzeugung verbrauchte Befugnis, keine
  Wiederholung geheim markierter ungültiger Rohbytes und terminale Ablehnung eines ebenfalls
  ungültigen zweiten Dokuments.
- Die Gate-M6-End-to-End-Abnahme indiziert und publiziert die Rust-, TypeScript- und Python-
  Produkt-Fixtures real und führt je Fixture zwei neutrale Modellturns über Context Compiler,
  SearchTool, durable Tool-Evidence, Ledger-Verifikation, Run Journal und Acceptance-Verifier bis
  `Done`. Der Repository-Dateibaum bleibt bytegleich. Ein Negativlauf über denselben Stack verlangt
  nach ungültiger Primär- und Reparaturausgabe null Toolaufrufe, null durable Toolversuche und null
  Tool-Journalereignisse.
- Die Gate-E7-End-to-End-Abnahme führt reale Patch- und Prozesspfade über libSQL, zentrale Policy,
  Approval, Workspace-Adapter, Fast Index, Context Compiler, Verification Engine und Run Journal.
  Sie belegt einen unveränderten Worktree während `AwaitApproval`, genau einen Worktree-Lease,
  unmittelbares Reindexieren sichtbarer Patchänderungen, ausschließlich neuen Snapshotkontext,
  Diff-Completion erst nach typisierter Evidence und `Replan` nach der zweiten identischen
  fehlgeschlagenen Run-Aktion.

### Compaction

- Der Domain-Contract kompiliert dasselbe Langlauffixture 64-mal neu aus Goal, Ledger, Run,
  Published Index und Original-Claims. Goal-Referenz, Step-/Attempt-/Run-/Evidence-Quellen, offene
  fehlgeschlagene Verifikation und aktive Hypothesen müssen in jeder Projektion erhalten bleiben;
  stale beziehungsweise evidence-inkompatible Claims bleiben ausgeschlossen. Ein Claim aus einem
  älteren Source-Run bleibt nach einem unabhängigen Publish als Provenienz erhalten, wenn seine
  konkrete Evidence im aktuellen Index weiterhin auflösbar ist.
- Der `RunMemoryCheckpoint` akzeptiert keinen früheren Checkpoint als Eingang. Gleiche
  autoritative Eingaben erzeugen denselben Digest; eine neue Ledger-/Event-Materialisierung ändert
  ihn. Die nur gelesene `RunEventSequence` bleibt unverändert, während der bestehende
  Run-Journal-Contract weiterhin alle Audit-Events nach Reopen nachweist.
- Der Context-Contract prüft die tatsächliche Reinjection von Step Result, offenem Fehler und
  Hypothese mit originalen IDs, deterministische Claim-Deduplizierung sowie die lückenlose
  konservative Budgetrechnung. Run Memory wird vor der Task Lens gegen `CodeAndEvidence`
  reserviert; unpassende Run-/Snapshot-Bindungen und Secret-Kandidaten werden abgelehnt.

### Security Boundary

- Negativtests für Traversal, Symlinks und unerlaubte Roots
- ungültige IPC- und LLM-Payloads abgelehnt; Goal-Contract-V1 fixiert in Rust exakte Schlüssel und
  eine stabile JSON-Form, während der TypeScript-Runtimeparser zusätzlich IDs,
  Revisionsmetadaten, UTF-8-Byte- und Listengrenzen sowie eindeutige Inhalte erneut prüft
- Approval- und Policy-Tests: abgeleitete Klassen/Risiken, unverrückbare Systembaseline,
  Pfad-Scope-Mismatch, Ablauf, Widerruf, One-time-Consumption und ungültige persistierte Formen
- gemeinsamer Storagevertrag für PolicyDecision, Request, Grant, Reopen und atomaren Run-/Approval-
  CAS-Rollback; jede Auswertung erzeugt genau ein typisiertes Audit-Event
- Secure-File-Tool-Contracts prüfen einen erlaubten verschachtelten Read samt exakter Span-Evidence,
  vorwärts paginierte direkte Directory-Kinder aus einem snapshot- und worktreegebundenen
  Published Index sowie konkrete Evidence für abgeleitete Verzeichnisse. Nicht publizierte
  Ignore-Dateien und selbst künstlich publizierte Built-in-Secret-/Generated-Pfade bleiben aus der
  Ausgabe ausgeschlossen.
- Negativverträge lehnen nicht konstruierbare Traversalpfade, einen realen Symlink-/Junction-Escape,
  Unix-Sockets als Sonderdateien, Binary-Präfixe, Secret-Kandidaten und Dateien oberhalb von 4 MiB
  ohne Preview oder Quelldaten im Fehler ab. Windows und der Linux-Quality-Job führen dieselbe
  öffentliche Port-Suite aus; der Unix-Sonderdateifall ist plattformspezifisch zusätzlich aktiv.
- PatchAction-Contracts prüfen kanonische getrennte Add-, Update-, Move- und Delete-Operationen,
  Snapshot- und Hashbindung, exakten Approval-Fingerprint, Binary-/Secret-Ablehnung sowie
  unveränderte UTF-8-BOM-, CRLF- und Nicht-ASCII-Bytes. Die öffentliche Workspace-Port-Suite prüft
  die begrenzte Vorschau, tatsächliche Post-Patch-Hashes, No-Replace, Useränderung zwischen Preview
  und Apply, Symlink-/Junction-Escape und ein explizites partielles Change-Set nach spätem Konflikt.
- ProcessRunner-Contracts kompilieren dasselbe argv-basierte Fixture auf Windows, Linux und macOS.
  Sie prüfen unveränderte Argumentgrenzen trotz Shell-Metazeichen, kanonisches CWD und Executable,
  eine geleerte Umgebung mit expliziter Allowlist, Timeout eines Endlosprozesses, Beendigung eines
  erzeugten Kindprozesses bei Cancellation und lückenlos terminierende Stream-Events. Ein
  Mehr-MiB-Ausgabestrom muss trotz kleinem Retained Limit vollständig gedraint werden; Secret-
  Kandidaten dürfen weder im Resultat noch in Stream-Events erscheinen.
- Command-Discovery-Akzeptanz veröffentlicht reale Rust-, TypeScript-Monorepo- und
  Python-Fixtures über den Fast Index. Sie prüft Cargo `--offline --locked`, eindeutige
  Package-Manager-Evidence, Root- und Package-CWD, Python-Modulbefehle, die Abwesenheit jeder
  Installationskategorie sowie plan-ungebundene, nicht automatisch erlaubte `ProcessSpec`-
  Vorschauen. Das Node-Fixture besitzt bewusst keine Lockdatei; dennoch wird kein Installversuch
  erzeugt oder gestartet.
- Mutationsgrenztests lehnen rohe Modell-argv und Shellfelder ab, serialisieren alle Patch- und
  Prozessaktionen desselben Worktrees, persistieren Policy und Approval vor Ausführung und geben
  nach einer sichtbaren Patchänderung niemals Kontext auf Basis des alten Snapshots aus.
- Secret-Redaction-Test
- Prozessabbruch und Outputlimit getestet

## Testpyramide

| Ebene | Zweck |
| --- | --- |
| Domain Unit | Invarianten und Zustandsübergänge |
| Property | Parser-, Pfad-, Hash- und Zustandskombinationen |
| Adapter Contract | gleiche Semantik je Provider oder Store |
| Golden Fixture | stabile Index- und Context-Ergebnisse |
| Integration | DB, Workspace, Modellstub und Controller |
| End-to-End | Desktop-Workflow auf kleinem Fixture-Repo |
| Evaluation | reale Coding-Aufgaben und Retrievalqualität |
| Platform Smoke | Windows, Linux und macOS |

Tests müssen offline und deterministisch laufen, außer explizit markierten optionalen Provider-Benchmarks.

## Referenz-Fixtures

Mindestens:

- kleines Rust-Workspace-Projekt
- TypeScript-Monorepo
- Python-Package
- gemischtes Repository mit generierten und ignorierten Dateien
- Repository mit Symlinks
- Repository mit absichtlichen Parsefehlern
- großes synthetisches Repository für Performance

Fixtures enthalten keine inkompatibel lizenzierten oder vertraulichen Quellen.

## Performancebudgets

Die Budgets gelten auf einer dokumentierten Referenzmaschine mit 8 CPU-Kernen, 32 GB RAM und NVMe; LLM-Server und Modellgewichte werden bei App-RAM separat ausgewiesen.

| Messung | Ziel für V1 |
| --- | ---: |
| Desktop bis interaktiv, warm | P95 ≤ 2 s |
| Idle-RAM ohne Modellserver | ≤ 200 MB |
| Fast Index, 100.000 LOC cold | P95 ≤ 30 s |
| Ein-Datei-Indexdelta | P95 ≤ 2 s |
| exakte oder FTS-Suche | P95 ≤ 100 ms |
| Context Compile ohne LLM | P95 ≤ 300 ms |
| UI-Interaktion während Indexlauf | keine sichtbare Blockade über 100 ms |
| Cancellation-Reaktion | ≤ 500 ms plus Prozessbeendigung |

Diese Zahlen sind Releaseziele. Wird ein Ziel nicht erreicht, braucht der Release eine dokumentierte Abweichung, Messdaten und einen konkreten Folgetask.

S11 besitzt dafür den reproduzierbaren ignorierten Release-Test
`incremental_index_performance::one_file_delta_meets_the_two_second_p95_target`. Das Fixture umfasst
200 Rust-Dateien und 100.000 LOC; jede der 30 Stichproben misst vom gleich großen Ein-Datei-Write über
Watcher-Debounce, Git-Discovery, BLAKE3-Bestätigung, Ein-Datei-Parse, vollständiges Link/Rank und
atomisches libSQL-Publish. Auf Windows 11 Pro, AMD Ryzen 9 5900XT, 32 GiB RAM und Samsung 970 EVO
Plus NVMe wurden am 2026-08-05 P50 1,202 s und P95 1,305 s gemessen; Watcher-P95 betrug 389 ms und
Refresh-/Publish-P95 922 ms. Die gemessene Ausgangsversion mit zeilenweisen SQL-Aufrufen lag bei
P95 15,286 s, ein erster 900-Parameter-Batch bei 14,493 s. Erst höchstens 30.000 Parameter,
1.024 Zeilen pro Cancellation-Checkpoint und transaktionale Retention supersedeter Projektionen
erreichten das Budget. Diese lokale Messung ersetzt nicht die abschließende V1-Referenzmessung auf
der oben definierten 8-Core-Maschine.

R11 wiederholte dieses Gate nach Erweiterung des atomaren Publishes um Card-Invalidierung. Der
erste 30-Sample-Lauf öffnete die inzwischen größere Knowledge-Datenbank weiterhin für jeden
Snapshot- und Run-Schritt neu und verfehlte das Ziel mit P50 2,299 s, P95 3,362 s,
Watcher-P95 391 ms und Refresh-/Publish-P95 3,047 s. Nach getrenntem, auf vier Worktrees begrenztem
Wiederverwenden bereits vollständig identitäts- und policygeprüfter Mutationshandles erreichte
derselbe unveränderte Release-Test am 2026-08-06 P50 816 ms, P95 884 ms, Watcher-P95 394 ms und
Refresh-/Publish-P95 491 ms. Ein isolierter Diagnoselauf maß den neuen Invalidierungsabschnitt bei
leerem Cardbestand mit rund 0,7 ms pro Publish; daraus wird kein allgemeiner Geschwindigkeitsclaim
für große Cardbestände abgeleitet.

R1 besitzt den reproduzierbaren ignorierten Release-Test
`exact_search_performance::exact_symbol_search_meets_the_100_millisecond_p95_target`. Das Fixture
enthält 50.000 Symbole als Projektion von 100.000 strukturellen Zeilen. Auf derselben lokalen
Windows-11-Maschine wurden am 2026-08-05 für den vor R1 notwendigen vollständigen Index-Load mit
anschließendem Namensscan über fünf Samples P50 652,8 ms und P95 656,8 ms gemessen. Die
indexgestützte Exact Query über 30 Samples erreichte nach begrenztem Wiederverwenden vollständig
verifizierter, identitätsgebundener Datenbankhandles P50 37,0 ms und P95 39,7 ms. Die erste Messung
mit erneutem Open, Migration und Integritätsprüfung pro Query lag bei P50 554,0 ms und P95 570,5 ms.
Auch diese lokale Messung ersetzt nicht die abschließende V1-Referenzmessung.

R2 verwendet dasselbe Fixture und denselben Release-Test für eine absichtlich falsch geschriebene
FTS-Query. Die erste breite Trigram-`OR`-Messung lag bei P50 194,1 ms und P95 195,9 ms; eine reine
Reduktion auf 512 nachbewertete Kandidaten erreichte P50 169,1 ms und P95 201,8 ms und verfehlte das
Gate weiterhin. Die begrenzte Ein-Fehler-Abfrage mit zusätzlichem Endanker erreichte am 2026-08-05
über 30 Samples P50 34,9 ms und P95 35,3 ms. Der unveränderte vollständige Index-Load plus Scan lag
in diesem Lauf über fünf Samples bei P50 1,145 s und P95 1,189 s; Exact Search erreichte P50 38,3 ms
und P95 41,5 ms.

R6 erweitert dasselbe reproduzierbare Fixture um eine primäre Membership für alle 50.000 Symbole
und lädt beim alten Full-Index-Vergleich zusätzlich die vollständige V8-Modulprojektion. Der Lauf
vom 2026-08-05 ergab über fünf Full-Load-/Scan-Samples P50 1,425 s und P95 1,452 s gegenüber
P50 1,145 s und P95 1,189 s vor R6. Über jeweils 30 Querysamples lagen Exact Search bei
P50 38,9 ms und P95 60,6 ms sowie FTS bei P50 36,2 ms und P95 39,5 ms; beide bleiben unter dem
100-ms-Gate. Die Messung dokumentiert damit den zusätzlichen vollständigen Loadaufwand der
evidenzgebundenen Membershipzeilen, ohne daraus eine Geschwindigkeitsverbesserung abzuleiten.

R10 erweitert denselben ignorierten Release-Test um 30 vollständige Task-Lens-Compiles ohne LLM.
Das 50.000-Symbole-Fixture publiziert zusätzlich einen aktuellen, symbolgebundenen Fact; jede
Stichprobe umfasst aktuelle Run-Prüfung, Exact, FTS, Graph/Test, Claim-Rekonstruktion, Fusion,
Budgetierung und Digest. Der unveränderte erste Stand rekonstruierte und kopierte den vollständigen
Index pro Lens und erreichte am 2026-08-06 P50 1,745 s und P95 2,168 s. Eine auf einen Eintrag
begrenzte Shared-Index-Capability, die vor jeder Ausgabe den dauerhaften neuesten Run prüft und bei
Publish/Rebuild aktualisiert wird, erreichte auf derselben lokalen Maschine mit dem verifizierten
Fact P50 251,101 ms und P95 267,307 ms. Im selben finalen Lauf lagen Exact Search bei P95 50,811 ms,
FTS bei P95 37,762 ms und die absichtlich weiterhin tiefe vollständige Indexkopie bei P95 1,254 s.
Damit besteht die Task-Lens-Context-Vorstufe das 300-ms-Gate; die lokale Messung ersetzt nicht die
abschließende V1-Referenzmessung.

H7 erweitert dasselbe Release-Fixture um 30 vollständige Context-Compiles. Jede Stichprobe umfasst
den unveränderten Task-Lens-Pfad sowie Anchor, Bereichsbudgetierung, Zoom-/Claim-/Tool-Packing,
Freshness-, Secret- und Gesamtbudgetprüfung, Promptkonstruktion und `ContextDigest`; ein LLM-Aufruf
ist ausdrücklich nicht enthalten. Im Lauf vom 2026-08-06 lagen die direkte Task Lens bei
P50 134,457 ms und P95 141,473 ms sowie der vollständige Context Compile bei P50 158,352 ms und
P95 215,220 ms. Im selben Prozess lagen Exact bei P95 31,899 ms und FTS bei P95 39,808 ms. Damit
besteht der vollständige H7-Pfad das 300-ms-Gate; die lokale Messung ersetzt weiterhin nicht die
abschließende V1-Referenzmessung.

Modellmetriken werden separat erfasst:

- Time to First Token
- Prompt-Tokens
- Output-Tokens
- Tokens pro Sekunde
- Toolerfolg beim ersten Versuch
- Taskerfolg

## Retrieval- und Agentenevaluation

Der R7-Contract erzeugt aus derselben veröffentlichten Index- und Coverage-Projektion zweimal den
identischen vollständigen `ExplorePlan` und fixiert Manifest → Entrypoint/Zentralsymbol → offene
Modul-Coverage als V1-Golden-Reihenfolge. Separate Grenzfälle belegen Snapshot-/Schemaschutz,
Unknown-Module-Ablehnung, das Überspringen bereits vollständig abgedeckter Module, alle drei
Budgetdimensionen sowie Cancellation, Budget-, Coverage-, Stagnations- und Gain-Stopgründe. Kein
Test ersetzt einen Index durch Modelloutput.

Die Gate-M4/M5-Retrievalbaseline V1 läuft offline über den echten Index-/Publish-/libSQL-Suchpfad
des gemischten Rust-/TypeScript-/Python-Fixtures. Ihre reviewbare Golden-Datei bindet sechs Exact-,
Lexical- und Graphfälle mit sieben Erwartungen an Kanal, native Begründung und Top-5-Rang. Sie
verlangt 100 Prozent Recall@5, fixiert MRR 0,9285, weist aktuelle Run-/Snapshot-/Revision-Bindung
nach und normalisiert zwei Wiederholungen bytegleich. Der spätere Q1-Umfang ergänzt darauf
aufbauend Agenten-, User-Edit-, Stale-Evidence- und Compaction-Aufgaben.

Der Gate-M4/M5-No-Embeddings-Contract führt den aktuellen Anwendungskern über einen real
publizierten gemischten Index, vollständige Deep-Map-Planung und budgetierte Task-Lens-Kompilierung
aus. Zwei Wiederholungen müssen identisch und aktuell sein; mindestens Exact und Graph müssen
vertreten sein, während `SourceChannel::Semantic` ohne injizierten Semantic-Port ausgeschlossen
bleibt. Ein nicht leerer Card-Batch muss außerdem im konstruktiv provider- und cachelosen
`GenerateSemanticEmbeddings::disabled()`-Pfad vollständig als deaktiviert enden.

Ein versioniertes Eval-Set enthält:

- Symbol finden
- Architekturfrage beantworten
- Fehler lokalisieren
- kleinen Bug beheben
- API über mehrere Module ändern
- Test ergänzen
- Änderung nach zwischenzeitlichem User-Edit fortsetzen
- lange Aufgabe nach Context Compaction fortsetzen

Mindestbedingungen vor V1:

- keine stale Facts in 100 Prozent der Invalidierungstests;
- Goal Contract bleibt in 100 Prozent der Langlauf-Fixtures erhalten;
- keine Mutation außerhalb des erlaubten Roots;
- alle Muss-Aufgaben des Eval-Sets besitzen reproduzierbare Baselines;
- Qualitätswerte dürfen durch einen Release nicht unbemerkt sinken.

## Cross-Platform-Matrix

CI baut und testet:

- Windows x86_64
- Linux x86_64
- macOS Apple Silicon
- macOS x86_64, solange unterstützt und praktikabel

Plattformspezifische Installer werden auf der Zielplattform erzeugt und signiert, sobald Distributionsidentitäten verfügbar sind.

## Definition of Done

Ein Arbeitspaket ist Done, wenn:

- alle Akzeptanzkriterien nachweisbar erfüllt sind;
- Architekturregeln und relevante ADRs eingehalten sind;
- erforderliche Tests existieren und bestehen;
- relevante Performancebudgets gemessen sind;
- Fehler-, Abbruch- und Sicherheitswege getestet sind;
- Dokumentation und Schemas aktuell sind;
- finaler Diff keine fremden Änderungen, Secrets oder Debugreste enthält;
- Restunsicherheiten offen dokumentiert sind.

Ein Checklistenpunkt darf erst danach abgehakt werden.
