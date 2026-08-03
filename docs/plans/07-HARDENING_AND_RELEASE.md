# Plan 07: Evaluation, Hardening und V1-Release

Ziel: A^3 wird messbar schnell, sicher, stabil und auf allen Zielplattformen verteilbar.

Relevante ADRs: alle akzeptierten ADRs, besonders 0012 und 0014

## Q1 Reproduzierbares Evalset

Abhängigkeiten: Gate M8

- [ ] lizenzkompatible Fixture-Repositories pinnen
- [ ] Retrievalfragen mit erwarteten Evidence IDs
- [ ] Architekturfragen
- [ ] Bugfix-, Refactor- und Testaufgaben
- [ ] Langlauf- und Compaction-Aufgaben
- [ ] User-Edit- und Stale-Evidence-Szenarien
- [ ] normalisiertes Ergebnisformat

Akzeptanz:

- Eval läuft offline;
- Modell- und Harnessmetriken sind getrennt;
- erwartete Ergebnisse sind reviewbar und versioniert.

## Q2 Performance Harness

Abhängigkeiten: Q1

- [ ] App Startup
- [ ] Cold und Incremental Index
- [ ] DB-Größe
- [ ] Exact, FTS, Graph und Hybrid Search
- [ ] Context Compile
- [ ] UI Responsiveness
- [ ] Provider TTFT und Tokens pro Sekunde
- [ ] End-to-End Tasklatenz

Akzeptanz:

- Maschinenprofil und Dataset werden mit Messung gespeichert;
- P50, P95 und Streuung statt Einzelwert;
- Regressionthresholds in CI für stabile lokale Benchmarks.

## Q3 RTX-5070-Ti-Profile

Abhängigkeiten: Q2

- [ ] verfügbare VRAM- und Modellserverkonfiguration dokumentieren
- [ ] mehrere passende lokale Codingmodelle mit identischem Eval testen
- [ ] Quantisierung, Kontext und Batchparameter messen
- [ ] Mapping-, Coding- und Embeddinglast getrennt testen
- [ ] Scheduler gegen GPU-Konkurrenz abstimmen
- [ ] empfohlene ModelProfiles als Daten, nicht als Hardcode

Akzeptanz:

- Empfehlung basiert auf Taskqualität, TTFT, Tokens pro Sekunde und VRAM;
- A^3 funktioniert weiterhin mit anderem Providerprofil;
- kein Benchmark behauptet Vergleichbarkeit bei unterschiedlichen Harnessversionen.

## Q4 Threat Model und Security Review

Abhängigkeiten: Gate M7

- [ ] Assets, Akteure und Trust Boundaries
- [ ] Prompt Injection aus Repository
- [ ] Path Traversal und Symlinks
- [ ] Command und Argument Injection
- [ ] Secret Exfiltration
- [ ] kompromittierter lokaler Modellserver
- [ ] manipulierte Projektdateien
- [ ] Update- und Supply-Chain-Risiken
- [ ] Abuse- und Regressiontests

Akzeptanz:

- jede hohe Bedrohung hat Prävention, Detection und Recovery;
- kritische Befunde vor Beta geschlossen;
- Restrisiken in Nutzerunterlagen.

## Q5 Resilience

- [ ] Crash während DB-Migration
- [ ] Crash während Index Publish
- [ ] Crash vor und nach Patchjournal
- [ ] Stromausfallähnlicher abrupt kill
- [ ] volle Disk
- [ ] korrupte DB
- [ ] Provider hängt oder streamt ungültig
- [ ] Watcher verliert Events

Akzeptanz:

- kein stiller Datenverlust dauerhafter Tasks;
- regenerierbare Daten können sicher rebuilt werden;
- Unknown-Mutationszustand erzwingt Reconciliation.

## Q6 Dependency und Supply Chain

- [ ] minimale Dependency Features
- [ ] Vulnerability Scan
- [ ] Lizenzinventar
- [ ] Lockfile Policy
- [ ] reproduzierbare und gepinnte CI Actions
- [ ] SBOM
- [ ] Updateprozess und Changelog

Akzeptanz:

- keine ungeklärte kritische Schwachstelle;
- keine unvereinbare Lizenz;
- Release enthält SBOM.

## Q7 Packaging Windows

- [ ] Releasebuild auf Windows
- [ ] WebView2-Prerequisite und Installerverhalten
- [ ] Prozessbaum- und Pfad-Smoke
- [ ] Code Signing
- [ ] Install, Upgrade, Uninstall
- [ ] Benutzerdateien bei Uninstall nur nach klarer Wahl

## Q8 Packaging Linux

- [ ] unterstützte Distributionbaseline
- [ ] WebKitGTK-Abhängigkeiten
- [ ] AppImage und/oder native Pakete entscheiden
- [ ] XDG-Pfade
- [ ] Wayland- und X11-Smoke
- [ ] Desktop Entry und Dateidialog

## Q9 Packaging macOS

- [ ] Apple-Silicon-Build
- [ ] x86_64-Strategie
- [ ] App Bundle und DMG
- [ ] Code Signing
- [ ] Notarisierung
- [ ] Gatekeeper-, Pfad- und Prozess-Smoke

## Q10 Update und Migration

- [ ] signierter Updatekanal
- [ ] keine automatische Aktivierung ohne Nutzerpolicy
- [ ] DB-Backup vor nicht trivialer Migration
- [ ] Update von letzter Beta auf Release Candidate
- [ ] Rollbackgrenzen dokumentieren
- [ ] Downgrade öffnet neuere DB nicht schreibend

Akzeptanz:

- manipuliertes Update wird abgelehnt;
- fehlgeschlagenes Update zerstört keinen Taskzustand;
- Schema-Migration aus realem Vorversionsartefakt besteht.

## Q11 Dokumentation

- [ ] Installation je Plattform
- [ ] Ollama-Verbindung und ModelProfile
- [ ] Projekt öffnen und Indexmodi
- [ ] Project Map und Evidence
- [ ] Agentenworkflow und Approvals
- [ ] Datenschutz und gespeicherte Daten
- [ ] Backup, Rebuild und Cleanup
- [ ] Troubleshooting
- [ ] bekannte Grenzen

## Release Candidate Gate M9/M10

- [ ] Evalbaseline und Performancebericht
- [ ] alle kritischen Securitytests grün
- [ ] drei Plattforminstaller bestehen Clean-Machine-Smoke
- [ ] Upgrade- und Migrationstest grün
- [ ] kein Critical oder High Releaseblocker
- [ ] bekannte Abweichungen mit Owner und Folgetask

## V1 Gate M11

- [ ] alle Definition-of-Done-Punkte aus QUALITY_GATES
- [ ] Produktname überall exakt A^3
- [ ] offline Kernworkflow vollständig
- [ ] Goal, Evidence, Ledger und Verification im E2E nachgewiesen
- [ ] stale Fact Leakage gleich null im Eval
- [ ] keine Mutation außerhalb Root im Security-Eval
- [ ] signierte Artefakte und Checksums
- [ ] Release Notes, SBOM, Datenschutz und Troubleshooting veröffentlicht

## Nach V1, ausdrücklich nicht vorziehen

- zusätzliche LanguageAdapter
- eingebettete Inferenzengine
- optionale verschlüsselte Synchronisation
- Plugin-SDK
- kontrollierte Git-Commit- und PR-Workflows
- kollaborative Tasks
- alternative Storageadapter einschließlich neuer Turso-Engine

Jeder Punkt benötigt vor Implementierung eine neue Scopeentscheidung oder ein ADR.

