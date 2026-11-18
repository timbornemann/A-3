# P7: geprüfter Anschluss von Korrekturplan, Quellenread und Edit

Stand: 2026-11-12. Kandidatenkomposition in isolierten Tests; Produktionsbaseline
bleibt unverändert. P7 ist offen.

## Geprüfter Umfang

- Ein gespeicherter HTTP-Fokus wird über den bestehenden Ledger-/Journal-CAS in
  einen Addresses-Korrekturschritt und den unveränderten abhängigen HTTP-Proof
  zerlegt. Unterstützt sind fehlgeschlagene Proofs ohne Parent und Dependencies.
  Die Anwendung endet in Replan und startet keinen Versuch. Auch die nach einem
  langsamen Quellenread verstrichene ursprüngliche Frist wird berücksichtigt.
- Ein tatsächlich erworbener Zielread kann den vorhandenen Editdispatch binden.
  Die vollständige aktuelle Replace-Seite steht genau einmal im Request. Der
  normale Reservierungs-/Delivery-/Patchpfad bleibt erhalten; gefälschte Stamps,
  stale Quellen und nicht tatsächlich gelieferte BOM-Bytes werden abgewiesen.
  Append und Add sind nicht Bestandteil dieses Anschlusses.
- Der Core normalisiert ausschließlich gepaarte CRLF in der Modellausgabe zu LF,
  nachdem die rohe UTF-8-Bytegrenze geprüft wurde. Der vorhandene Patchcompiler
  stellt die gebundene Ziel-EOL wieder her. Einzelne CR, BOM und andere verbotene
  Steuerzeichen bleiben ungültig. Fenster, Hashes und E3-Freigabe ändern sich nicht.

## Tatsächlicher lokaler Codingversuch

`target-live-qwen4k-003` und `target-live-ornith9b-002` verwenden
`qwen38-4k:latest` beziehungsweise `ornith-1.5:9b`, jeweils 4096 Kontexttokens
und 1024 Outputreserve. Beide erzeugen im ersten Editaufruf ohne Repair die
passende Änderung von `value + 1` auf `value + 2`. Vollständiges aktives Kriterium
und beide Constraints sind im tatsächlichen Request nachgewiesen.

Echte Reservierung, akzeptierter Quellenlieferbeleg, genehmigter Patch,
aktuelle Diff-Evidence und kaltes Wiederöffnen bestehen. Unicode, CRLF,
Fensterumgebung und beide geschützten Nachbardateien bleiben erhalten. Ein
unabhängiger vorhandener Ganzzahl-Quellguard prüft das vollständige Modul;
generierter Pythoncode wird in diesem Komponentenversuch nicht ausgeführt.
Das funktionale Laufzeitkriterium bleibt offen und der Run steht in Verify.
Dies ist kein A02-, A04-, P7- oder allgemeiner Zuverlässigkeitsnachweis.

Die vollständige Versuchsfolge bleibt erhalten: ein I/O-Infrastrukturfehler,
zwei Ablehnungen korrekt formulierter CRLF-Ausgaben durch den alten Decoder,
danach zwei vorab registrierte erfolgreiche Versuche nach dem Corefix.
Die unabhängige Antwortprüfung liegt in
`target/plan22-p7-runs/target-read-local-independent-model-review-002.json`.

## Gemeinsame Qualitätsprüfung

Der Quellstand ist über 129 Datei-Hashes einschließlich aller 69 vendorten
libSQL-Dateien an `coding-core-handoff-004.json` gebunden. Der Modellworker
verwendete denselben Quellstand aus Handoff003; die vierte Fassung ergänzt
ausschließlich Prüfmetadaten.

| Prüfung | Ergebnis |
| --- | --- |
| Windows: vollständiger Workspace, alle Features | 2063 bestanden, 0 Fehler, 43 explizit ignoriert; 103 Ziele |
| Native Connection-Lifecycle-Prüfung | 1 bestanden |
| Linux: elf gezielte Suiten | 100 Testausführungen, 99 unterschiedliche Fälle, 0 Fehler, 2 explizit ignorierte Ausführungen |
| Workspace-Clippy, alle Targets/Features, `-D warnings` | bestanden |
| `cargo fmt --all -- --check`, Markdownlinks, `git diff --check` | bestanden |

Die Linux-Fixtures laufen auf nativem begrenztem tmpfs, weil der Windows-Bindmount
die erforderliche No-Replace-Rename-Semantik nicht unterstützt. Ein eindeutig
besessener Container verwendet ein gepinntes lokales Image ohne Netzwerk.
Buildartefakte und Logs bleiben auf D:. Linux-Clippy wird daraus nicht abgeleitet.

Das kombinierte Gate ist
`target/plan22-p7-runs/coding-core-combined-gate-001.json`, die unabhängige
Quell-/Gateabnahme
`target/plan22-p7-runs/coding-core-independent-acceptance-001.json`.
Ein früheres grünes Gate vor dem Fristfix ist ausdrücklich kein Abschlussbeleg
für diesen Stand.

## Nächste offene Arbeit

Der aktuelle HTTP-Pfad korrigiert noch keinen Servercode. Er benötigt den engen
kalten Herkunftsnachweis des angewendeten Plans, reguläre Aktivierung im selben
Controller, tatsächlichen Zielread/Edit/Patch und anschließend das vollständige
ursprüngliche HTTP-Rezept mit frischen exakten Freigaben. Automatische
Mismatch-Zulassung, Add-/Updateabhängigkeiten und sichere Pensionierung stale
gewordener eigener Erstellungsbelege sind getrennt offen.

Kein weiterer P7-Planpunkt wird durch diese Komponentenprüfung abgehakt.
