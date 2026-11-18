# Plan 22 P0 Ausgangsmessung

Status: P0 verifiziert und eingefroren; P1 ausgerichtet, nächstes Arbeitspaket P2  
Stand: 2026-11-06

Der vollständige Umbauauftrag bleibt aktiv. Die Ausgangsmessung P0 aus
[Plan 22](22-SMALL-LOCAL-MODEL-AGENT-CORE.md) ist abgeschlossen. Die ausdrückliche Annahme von
[ADR-0119](../adrs/0119-kleinschrittiger-agentenkern-fuer-lokale-modelle.md) ist
festgehalten. Abgenommen ist die Ausgangsmessung, nicht die Zuverlässigkeit des
Produktkerns. P1 hat die Arbeitsverträge ausgerichtet; Umsetzung P2–P11 und
Modellabnahme bleiben offen.

## Eingefrorene Ausgangslage

- Produktionsstand: Commit `38a07ce97f4f1d84c68101e531fdb027be5fa529`, Tree
  `927b30dba9544307c179b67d6d58a2d421830739`.
- Produktstrategie: unveränderter SourceGuided-Default und tatsächlicher Einstieg
  `AgentSessionManager::submit_for_target_mode`, kein handgebauter Plan.
- [Baselineprofil und sechs Modelldigests](../../fixtures/local-agent-eval-v1/baseline.json)
  wurden vor der ersten Generation gespeichert. Die einzelnen Läufe prüfen die
  unveränderten Digests, Templates und Parameter erneut.
- [Aufgabenpaket](../../fixtures/local-agent-eval-v1/suite.json): 18 Fälle mit je
  Entwicklungs- und zweiter Variante, Erwartungswerten, Schutzdateien und echten
  Prüfungen. Die großen Quellen werden reproduzierbar materialisiert.
- Vergleichsprofil: FormatFieldOnly, Temperatur 0, Top-p 1, Parallelität 1,
  think=false; 8192/2048 für die Primärmodelle und 4096/1024 im 4k-Schnitt.
- [Abschließendes P0-Manifest](../../fixtures/local-agent-eval-v1/p0-freeze.json):
  20 Eingabedateien einschließlich Suite, Orakelkontrollen, Ordertrace und Cargo-
  Lockstand; lokal zusätzlich bytegleiche eingefrorene Kopien. Finales Testbinary
  `b98057ed79167ed83059d413a4ccc15e14cbea24fdde0eee0ac44c1d2e031241`.
  Alle sechs gestarteten Versuche und die zugehörigen Berichtshashes bleiben erhalten.

## Implementierte Messgrenze

Der [native Worker](../../apps/desktop/src-tauri/src/plan22_baseline.rs) ist nur im
Testbuild eingebunden. Er verwendet Recherche, Materialisierung, Produktions-Executor,
echte Policy, exakte Grants und isolierte Datenbanken. Produktiver Steuerungscode
ist nicht ersetzt. Typisierte produktive Verifikationsnachweise werden getrennt
von den unabhängigen Nachprüfungen erfasst.

Die [nativen Interventionstreiber](../../apps/desktop/src-tauri/src/plan22_interventions.rs)
beobachten tatsächliche Prozess-/Ledgerdaten, Nutzeränderungen, Recovery und
zwei Ask-Nachrichten derselben Session. A10 setzt denselben Run ausschließlich
nach einer tatsächlich verifizierten Zwischenlieferung und erfolgreichem
Pause-/Shutdown-Checkpoint in einem neuen Prozess fort. Fehlende ursprüngliche
Kriterienbindung und Run-Memory-Kompaktion bleiben im alten Kern offen.

Der [Supervisor](../../fixtures/local-agent-eval-v1/run_baseline.py) startet jeden
Versuch genau einmal, friert das Testbinary per SHA-256 ein, besitzt Prozessbaum
und GPU-Lock und behält auch Fehler, Abstürze und Timeouts. Fehlende Providerusage
bleibt unbekannt. Er speichert keine Rohprompts oder Aktionsinhalte.

Der [Interventionskanal](../../fixtures/local-agent-eval-v1/supervisor.py) verändert
nur exakt vorab festgelegte Fixturebytes. Ein einmaliger unveränderlicher Marker
verhindert Windows-Leserennen mit dem laufenden Statusschreiber. A10 ist eine
ausdrückliche Fortsetzung, keine Fehlerwiederholung; Budget und Zähler werden
nicht zurückgesetzt oder beim Zusammenführen doppelt gezählt.

A10s Lieferreihenfolge wird unabhängig vom bloßen Ledgerstatus gemessen:
tatsächliche Moduländerung, passende erfolgreiche aktuelle Einzelprüfung, erst
dann die nächste Moduländerung. Versuch, Snapshot und Quellhash bleiben gebunden;
der Restart erhält den exakten bisherigen Trace. Ein Großpatch mit späteren
Prüfungen oder ein inzwischen stale gewordener Erfolg erfüllt dieses Gate nicht.

Die [unabhängigen Orakel](../../fixtures/local-agent-eval-v1/oracle.py) prüfen
Verhalten und Byteerhaltung. Ihre Kontrolllösungen werden niemals Modellinput.
Ask-Antworten und A12s freier Abschlussbericht bleiben ohne separate Operatorbewertung als
ungeprüft ausgewiesen. Ein solcher Zwischenstand zählt nicht als korrekte Antwort.

## Bisherige Verifikation

- 23 native Validierungs-/Metriktests bestanden; der Live-Test bleibt standardmäßig
  ignoriert und wird ausschließlich ausdrücklich über den Supervisor gestartet.
- 90 integrierte Python-Kontrollen bestanden, einschließlich aller 36
  Materialisierungen, echter HTTP-/CLI-/Node-/Cargo-Prüfungen, falscher Konstanten,
  null Tests, Schutzbytes, begrenzter Ausgaben und Prozessbaumbeendigung.
- Rust-Workspace, Workspace-Clippy mit `-D warnings`, Formatierung und lokale
  Markdownlinks bestehen auf dem eingefrorenen Messstand. Die genauen Kommandos
  und Loghashes stehen im P0-Manifest. Python: 90 Tests in 77,914 Sekunden.
- Unabhängiger Integrationsreview der A10-Verdrahtung ohne weiteren Befund;
  dessen 15 gezielte Python-Kontrollen sind Teil der 90, keine zusätzlichen
  Modellmessungen.

Die sechs lokalen Modellversuche sind terminal und unverändert erhalten.
Der [inhaltlich begrenzte Ergebnisbericht](../../fixtures/local-agent-eval-v1/pilot-results-2026-11-06.json)
pinnt Binary-, Quellen-, Modell- und Berichtshashes. Die ersten drei Berichte
wurden durch spätere Befunde nicht umgeschrieben. Es gab keine automatische
Wiederholung. Keiner dieser Versuche ist ein vollständiger Agentenerfolg:

| Fall / Modell | Unabhängiges Ergebnis | Tatsächlicher Abschluss |
| --- | --- | --- |
| A01 / Ornith 9B | Korrekte Änderung; alle acht äußeren Kontrollen bestanden | Run `Failed`, 28 Modellaufrufe, kein produktiver Testnachweis; letzte allgemeine Aktionsausgabe am Outputlimit |
| A01 / Qwen 27B IQ3S | Korrekte Änderung; alle acht äußeren Kontrollen bestanden | Testtreiber verweigerte eine `execute_open`-Aktion; er erlaubte nur KnownSafe. Die tatsächlichen Argumente sind nicht im alten Bericht erhalten |
| Q01 / Qwen 4k | Projektfrage nicht beantwortet | Expliziter Kontexthalt `AwaitingUser` vor der ersten Taskgeneration; Capability-Probe allein ist keine Antwort |
| A09 / Ornith 9B | Kein verwertbarer Modellqualitätswert | Wegen eines erkannten Messprotokollfehlers vor dem Interventionsmarker gezielt gestoppt; partieller Bericht und Operatorgrund erhalten |
| Q05 / Ornith 9B | Echter Quellenwechsel und neue Revisionsbindung nachgewiesen | Zweite Ask-Nachricht derselben Session bleibt unvollständig und hält nach dem Recherche-Repairbudget mit `AwaitingUser`; acht Taskaufrufe, Cleanup vollständig |
| A09 / Qwen 27B IQ3S | Keine Änderung, kein Recoveryerfolg | Reguläres Messende nach drei Taskaufrufen; `SessionFailed`, `RunExecute`, typisierter Executorfehler `Unavailable` vor erster Patchfreigabe; Cleanup vollständig |

Die Prozesszulassung des Testtreibers ist mit einer gezielten Negativprüfung
berichtigt: exakt vorab festgelegtes argv, Root-CWD und verweigertes Netzwerk
dürfen auch im Modus Open die echte einmalige Core-Freigabe durchlaufen. Shell,
andere Argumente und andere Executables bleiben gesperrt. Der ursprüngliche
fehlgeschlagene Versuch wird dadurch weder überschrieben noch als Produktfehler
umgedeutet. Ohne die damaligen Argumente ist nicht bewiesen, ob dessen konkrete
Aktion alle übrigen Manifestgrenzen erfüllte. Weitere Kontrollläufe erhalten eine
eigene Harness-/Binaryversion und geschlossene Matchmetadaten bei Ablehnungen.

Der abgebrochene Ornith-A09-Versuch deckte zusätzlich eine Messlücke auf:
`SessionFailed` bei noch nicht terminalem Run durfte nicht bis zum Gesamtlimit
weiterpollen. Der Worker erkennt jetzt den tatsächlich beendeten Besitzer und
den typisierten Executorfehler. Die Produktionszustandsmaschine wurde dafür
nicht geändert. Der neue Qwen-A09-Lauf belegt diesen regulären Messabschluss.

Die native Beobachter-Checkpointserialisierung verwendet `serde` ausschließlich
als Devdependency aus dem bereits vorhandenen Workspace. Ein manueller zweiter
JSON-Codec wäre fehleranfälliger; `serde_json` allein stellt die benötigten
Derive-Makros nicht bereit und Tauri reexportiert `serde` nicht öffentlich. Die
Lockänderung ergänzt nur den Desktopbezug auf das bereits gepinnte Paket.

Ein früherer Workspace-Lauf scheiterte unverändert im bestehenden
`timeout_kills_the_spawned_child_process` am fehlenden PID-Marker. Der Fehlerlog
bleibt in `target/plan22-p0-workspace-process-marker-failure.log` erhalten;
isoliert bestanden alle neun ProcessRunner-Verträge, anschließend auch der
vollständige aktuelle Workspace. Es wurde keine Timeoutgrenze gelockert.

Bonsai ist unter dem tatsächlich verfügbaren Tag `bonsai-2:27b` samt Digest im
eingefrorenen Modellkatalog enthalten. Die drei Qwen-Kontexttags werden weiterhin
mit ihren ausdrücklich festgelegten Kontextgrenzen und denselben Aufgaben gemessen.

## P0-Abschluss und weitere Abnahme

Der geforderte kleine Vorlauf liegt für beide Primärmodelle und das 4k-Profil
vor. Der durch den KnownSafe-only-Treiber beeinträchtigte Qwen-A01-Versuch zählt
hierfür nicht; der neue Qwen-A09-Versuch verwendet den korrigierten Treiber.
Ein früher Produkthalt bleibt ein Messergebnis und wird nicht durch eine
Unsupported-Sperre verdeckt. Keine Wiederholung bis zum Erfolg und keine
nachträgliche Profilauswahl.

Alle verpflichtenden Fälle besitzen materialisierbare Varianten, unabhängige
Sollprüfungen und einen startbaren Produktionsmesspfad. Der aktuelle Kern kann
dabei bereits vor der eigentlichen Intervention scheitern. Nur Q05 besitzt
bisher einen real erreichten Quellenwechsel; die kontrollierten Rust-/Python-
Beobachtertests sind keine behaupteten freien Modell-Recoveryerfolge.

P0 verlangt noch nicht die vollständige gepaarte P10-Hauptmatrix. Deren
Aufgabenlösungen, positiver 4k-Schnitt und erforderliche menschliche Ask-/A12-
Bewertungen bleiben offen. Das eingefrorene Hauptpaket wird dafür unverändert
verwendet. Nach dem verifizierten P1 beginnt P2 mit dem dauerhaften Auftrag und
seiner Kriterien-/Nachweisbindung im Kandidatenkern.

Vor den ersten Greenfield-/Langlaufruns sind die Effektumfänge korrigiert:
A04/A05 erhalten endliche Probeaufrufe und exakte Loopbackziele, A10 zusätzlich
zu seiner Gesamtsuite vier passende Einzelmodulprüfungen. Die erlaubten
Prüfbefehle stehen auch im öffentlichen Auftrag. Kern und Kandidat erhalten
denselben Umfang. Das korrigiert Messinfrastruktur vor dem Haupteinfrieren;
Modellparameter, Verhaltensziele und Schutzdateien werden nicht gelockert.

## Zusätzlich im Bestand festgestellte Langlaufgrenzen

Der [Produktions-Executor](../../apps/desktop/src-tauri/src/production_agent_run_executor.rs)
übergibt im normalen Codingkontext `run_memory=None`. Ausführungsquittungen sind
vorhanden, belegen aber allein keine Run-Memory-Reinjektion. P0 darf diese nicht
nachträglich einspeisen, um den Langlauffall grün zu machen.

Der bestehende öffentliche Recovery-Replan muss ebenfalls unverändert gemessen
werden: das Schließen eines aktiven Schritts ist noch kein neuer ausführbarer Plan.
Aus diesen Grenzen abgeleitete Fehler sind Baselinebefunde. Sie werden erst in den
vorgesehenen Kandidatenarbeitspaketen behoben.
