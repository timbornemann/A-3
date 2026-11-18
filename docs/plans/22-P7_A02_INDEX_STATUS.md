# P7: tatsächlicher A02-Indexvorlauf

Stand: 2026-11-12. Die Windows- und Linuxprüfungen sowie die gemeinsamen
Qualitätsgates sind bestanden; die Indexvoraussetzung im Plan ist abgehakt.
Dies ist ausschließlich die Indexvoraussetzung für A02: kein Modell hat hier
`adjust` korrigiert, und weder A02 als Codingfall noch P7 sind dadurch abgenommen.

## Unveränderte Eingaben und Grenzen

Beide Tests materialisieren A02 erneut aus dem eingefrorenen P0-Generator. Sie
prüfen dessen SHA-256, das vollständige Suite-Manifest und den P0-Hashvertrag vor
dem Import sowie alle vier erzeugten Dateien. `subject.py` besitzt unverändert
308099 beziehungsweise 308100 Bytes, UTF-8-BOM und 5604 CRLF ohne einzelne LF.
Originalfenster und Null-basierte Zeile bleiben `154024..154080` beziehungsweise
`154024..154081`, Zeile 2801. Die Tests verwenden echte native Git-Identität,
isoliertes Candidate-AppData und den vorhandenen öffentlichen Refresh-Pfad.

Inputlimit 4 MiB, Parserbudget 2 Sekunden, Parserpool-Wartezeit 500 ms,
Strukturgrenzen und Compilerbudget 30 Sekunden bleiben unverändert. Der frische
Compiler hat Poolgröße eins. Produktion und Kandidat erhalten keinen neuen
Protokoll-, Datenbank- oder Berechtigungspfad.

## Reproduzierter Fehler und enger Fix

Der gültige unveränderte Windowslauf 002 scheiterte bei beiden Varianten mit
`Compiler(TimedOut)` in der zuletzt beobachteten Parse-Phase. Ein jeweils neuer
Python-Adapter scheiterte mit denselben Quellbytes ebenfalls mit `TimedOut`.
Veröffentlichung und kalter Reopen wurden dabei nicht erreicht. Der zuvor
aufbewahrte Lauf 001 verwendete irrtümlich Debugsymbole und zählt nicht als
Abnahmeversuch im vereinbarten Buildprofil.

Eine getrennte begrenzte Prozessprobe lokalisierte zusätzliche Kosten: Schon
der rohe native Parser benötigte im ursprünglichen unoptimierten Build 12,125
beziehungsweise 13,058 Sekunden. Wiederholter indizierter Zugriff auf die 5603
Wurzelkinder kostete weitere 2,295 beziehungsweise 2,240 Sekunden; Vorwärtscursor
mit Umkehr nur der neu angehängten Stackelemente benötigten etwa 1,863 ms und
ergaben dieselben Knotenfolgen-Digests. Das ist eine Messung dieser Fixtures,
keine Behauptung einer allgemeinen asymptotischen Laufzeit.

`Cargo.toml` optimiert jetzt ausschließlich die bereits gepinnten Pakete
`tree-sitter` und `tree-sitter-python` in `dev` mit Stufe 2. Die normalen
Testbuilds erben diese Einstellung; Cargo-JSON aus dem echten A02-Testlauf belegt
Stufe 2 für beide Abhängigkeiten und weiterhin Stufe 0 mit ausgeschalteten
Debugsymbolen für die Rust-Anwendung und das Testprogramm. Es gibt keine neue
Abhängigkeit und keinen ausschließlich per CLI erfolgreichen Abnahmepfad.

Drei gemessene Schleifen verwenden nun Vorwärtscursor: die gemeinsame
Baumprüfung sowie Pythons Kindlisten und Main-Guard-Suche. Bei Stackarbeit wird
ausschließlich der neu angehängte Bereich umgekehrt. Quellreihenfolge und
vorhandene ausstehende Geschwister bleiben erhalten. Die gemeinsame Kindlisten-
Schleife prüft Abbruch und Zeitbudget auch während des Auffüllens breiter Listen.

## Gemessener Nachweis

Alle Werte stammen von denselben eingefrorenen Development-/Holdout-Dateien;
die einzelnen Messungen sind keine allgemeine Hardware- oder Erfolgsprognose.

| Prüfung | Development | Holdout |
| --- | ---: | ---: |
| Ursprünglicher vollständiger Refresh | TimedOut nach 2699 ms | TimedOut nach 2648 ms |
| Nur Parserpakete optimiert: vollständiger Python-Adapter | 1990 ms | 1976 ms |
| Dauerhafte Optimierung plus Cursor: frischer öffentlicher Parserpool | 1472 ms | 1477 ms |
| Dauerhafte Optimierung plus Cursor: vollständiger Python-Adapter | 1486 ms | 1473 ms |
| Tatsächlicher erster Full-Refresh mit neuer Publikation | 1670 ms | 1668 ms |
| Kalter Store-Reopen mit identischer Publikation | 81 ms | 79 ms |

Die ursprünglichen Timeoutwerte sind zensierte Fehlversuche, keine vollständigen
Vorher-Latenzen. Daraus wird kein Faktor für den gesamten Indexpfad abgeleitet.
Die unabhängigen rohen Parser- und Kindlistenproben ersetzen keine
Produktpublikation.

Zwei gewöhnliche native Tests prüfen jeweils `Full`, neue Veröffentlichung,
exakte Dateirevisionen, alle vier Python-/Manifestanalysen mit vollständiger
Coverage ohne Diagnostics, den gültigen synthetischen Modulbereich und die
exakte `adjust`-Namensauswahl im Originalfunktionsfenster. Nach dem Drop von
Compiler, Refresh und Store laden neue Candidate-Stores dieselbe vollständige
Publikation, Runidentität und Dateirevisionen. Alle vier Dateien bleiben vor und
nach der Indexierung sowie nach dem Reopen bytegleich.

Sieben Pooltests bestehen einschließlich neuer Stackreihenfolge-, Abbruch-
während-Enqueue- und Tiefenlimit-/Lease-Recovery-Regressionen. Neun Python-
Featuretests sichern unter anderem Symbol-IDs, Scopes und einen späten Main-Guard
nach breiten Kommentarlisten. Vier gemeinsame Adapterverträge prüfen JSON,
Rust, TypeScript/JavaScript und Python einschließlich vorhandener Fehler-,
Abbruch-, Größenlimit-, Progress- und Wiederverwendungsfälle.

Reproduzierbare Belege liegen unter `target/plan22-p7-runs/`:
`a02-index-native-red-checkpoint-001.json`, `a02-tree-child-access-001.json`,
`a02-parser-public-probe-001.json`, `a02-parser-opt2-probe-001.json`,
`a02-parser-public-after-001.json` und den beiden Logs
`a02-index-native-windows-003-stdout.log` sowie
`a02-index-native-windows-003-stderr.log`. Fehlversuche bleiben erhalten.

Die Abschlussabnahme und weitere Modellarbeit folgen weiter
[Plan 22](22-SMALL-LOCAL-MODEL-AGENT-CORE.md).

## Gemeinsame Abschlussprüfung vom 2026-11-12

Der frische vollständige Windowslauf besteht mit 2047 Tests über 103 Ziele,
null Fehlern und 42 ausdrücklich ignorierten Tests. Beide benannten nativen
A02-Tests bestehen auch innerhalb dieses Workspace-Laufs. Der zusätzliche native
Connection-Lifecycle-Test, Workspace-Clippy mit `-D warnings`, Formatierung,
Links und Diffprüfung bestehen. Die unabhängige Herkunftsprüfung vergleicht alle
69 Dateien des libSQL-Archivs mit dem Vendorbaum und bestätigt ausschließlich die
dokumentierten Änderungen aus ADR-0055.

Der eindeutig besessene Linuxlauf besteht mit 60 Tests über sieben Ziele,
null Fehlern und einem ausdrücklich ignorierten Live-Modelltest. Development
publiziert nach 1680 ms und lädt kalt nach 80 ms identisch; Holdout nach 1604 ms
beziehungsweise 70 ms. Ein bestehendes fachfremdes Domain-`unused_mut` bleibt
sichtbar; ein Linux-Clippy-Gate wird hier nicht behauptet.

Alle 32 gebundenen Quellen und Eingaben stimmen vor der Dokumentationsaktualisierung
mit dem gemeinsamen Freeze überein. Die unterbrochenen vorherigen Läufe bleiben
erhalten und zählen nicht als erfolgreiche Gates. Der Sammelbeleg
`target/plan22-p7-runs/a02-http-v3-combined-gate-001.json` bindet die finalen
Manifeste, die unabhängige Abnahme und die Vendor-Herkunftsprüfung über SHA-256.
Diese Abnahme betrifft ausschließlich den tatsächlichen Indexaufbau; die
Korrektur von `adjust` durch ein lokales Modell bleibt offen.
