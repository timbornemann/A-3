# Plan 10: Ausgeschriebene Befehlsnamen in Änderung und Testplan

Stand: 2026-09-08. Ausgangspunkt: `2c5fde7`.

## Ziel und überprüfte Grenze

Der [vorige Nachtest](10-SCHEMA_GROUNDING_VALIDATION.md) lässt Granite den verlangten
Befehl `import-csv` zu `import` ändern. Die Gesamtwortprüfung besteht trotzdem, weil
der Testabschnitt den richtigen Namen enthält. Das ist ein falscher Plan, kein
Darstellungsfehler. [ADR-0103](../adrs/0103-explizite-befehlsnamen-im-plan.md)
ergänzt eine notwendige Namensprüfung je neu eingehendem Design-Ergebnis und einen
passenden Hinweis im vorhandenen Einzelrepair. Der zunächst ebenfalls getestete
Hinweis im regulären Kontext wird nach
[ADR-0104](../adrs/0104-befehlspruefung-ohne-kontextdopplung.md) zurückgenommen.

Die Application-Projektion erkennt ausschließlich die dokumentierte explizite
Aufrufsyntax. Sie ist weder Shellparser noch LLM-basierte Anforderungserkennung.
Sie führt nichts aus, behauptet keine Script-Existenz und erhält vollständigen
Originalauftrag, aktuelle Originale, Voraussetzungen, Quellenregeln und alle
Budgets. Historische Ergebnisse und Ask-/Bestandsfragen werden nicht umgeschrieben.
Ein Token neben widersprüchlicher Prosa ist weiterhin keine fachliche Abnahme.

## Offline-Nachweis

Der neue Regressionstest mit `python audit.py export-events <destination>` war
vor der Korrektur rot: `Implement and test export ...` wurde in Q2 zugelassen.
Danach lehnt derselbe Test den falschen Namen sowohl in Q2 als auch Q3 ab, selbst
wenn Auftrag und zugelassene Voraussetzung den korrekten Namen schon enthalten.
Case-Varianten, verlängerte Namen und Dateinamen können den exakten Befehl nicht
ersetzen. Allgemeine Ask-Interpretationen bleiben davon unberührt.

Die reine Projektion prüft Runner-/Dateiendungszuordnung, vollständige Platzhalter,
relative Scripts, normale Satzzeichen, UTF-8-Abgrenzung, mehrere Namen, Deduplizierung,
64-Byte-Namen sowie Vier-Namen-/32-KiB-Bounds. Nicht verstandene Prosa/Shellsyntax und
Überlauf ergeben keine vorgetäuschte Teilabdeckung. Vier maximale Namen passen
vollständig in den phasenspezifischen Repair unter 768 Bytes.

Die echte Git-/Index-/Reader-/libSQL-Fixture arbeitet mit dem unveränderten
konservativen 8192/2048-Profil in Plan und Agent-Vorbereitung, jeweils für Q2 und Q3:
Einmal umbenannt führt zu genau einem Repair und anschließendem Abschluss.
Zweimal umbenannt bleibt offen, erzeugt weder Ergebnis noch Analyse-Receipt und
beginnt keine zusätzlichen Reads. Originalpaket und Voraussetzungen bleiben beim
Repair bytegleich, alle Fixturedateien bleiben unverändert. Ein neuer Storage-
Adapter lädt denselben dauerhaften Arbeitsstand; Wiederherstellung mit aktuellen
Evidence-Zuordnungen wählt dieselbe offene Pflicht. Dies ist kein Prozesscrashtest.

Nachtrag aus der [Textlimit-Regression](10-TEXT_LIMIT_REPAIR_VALIDATION.md): Die
ursprünglich zwei benannten Realtests iterierten Q2/Q3 außerhalb der Windows-
libSQL-Isolationsgrenze. Da deren Kindprozess nach einer Fixture endet, wurde
tatsächlich Q2 wiederholt, nicht auch Q3 ausgeführt. Die obige Aussage zur
damaligen realen Q3-Abdeckung war daher zu weitgehend. Die unabhängigen Application-
Prüfungen und Liveberichte sind davon nicht betroffen. Vier jetzt getrennt
benannte Realtests weisen Q2 und Q3 jeweils einmalig und wiederholt ungültig nach,
einschließlich Wiederöffnung und unverändertem 8k-Kontext.

## Kontrollierte Live-Stände

Alle Aufrufe nutzen die unveränderte öffentliche CSV-Planfrage `3:0`, `joint`,
`format-only`, eine Wiederholung und die bestehende Wortprüfung v3. Lokale Modelle
werden ausschließlich nacheinander geladen. Credentials bleiben in vorhandenen
nativen Slots. Kein privater Quelltext, Settings-Write oder mutierender Agenttest.

- A: bisheriger Stand ohne Namensprüfung, eingefrorenes
  `target/reports/research-grounding-20260908/research-short-repair-tests.exe`,
  SHA-256 `2a909ab5f87c73717c75d3c0bf6cf64af3febefc6fe907efc42b47946c5e3f44`.
- B: Namensprüfung und konkreter Repair ohne ausgeschriebene Resultat-Längengrenze,
  `target/reports/research-commands-20260908/research-command-tests.exe`,
  SHA-256 `77af5d3e6d83e8e7b4d1b1fc070037ce225025ab0368e53126bbb26f2314403f`.
- C: identische Prüfung; der Namensrepair nennt zusätzlich `text <=4096 UTF-8 bytes`,
  `target/reports/research-commands-20260908/research-command-v2-tests.exe`,
  SHA-256 `0daa103b2c1fd2b3b0c98ff386346d47ec018c9a68ef0b7068445fadbdd388f4`.
  Eine danach ergänzte Offline-Assertion öffnet einen zweiten Storage-Adapter;
  sie verändert keinen Live- oder Produktpfad.
- D: finaler Stand ohne zusätzlichen primären Kontexthinweis, mit derselben
  Namensprüfung und gezieltem begrenztem Einzelrepair;
  `target/reports/research-commands-20260908/research-command-final-tests.exe`,
  SHA-256 `34adcc11f34c7e2461928628cf29fc90ac0175113f4375cbbcf857b338dad260`.

Reproduktion: passender eingefrorener Stand mit
`--ignored research_approved_model_matrix --nocapture --test-threads=1`,
`A3_RESEARCH_EVAL_CASE=3:0`, `A3_RESEARCH_EVAL_REPETITIONS=1`,
`A3_RESEARCH_ANALYSIS_METHOD=joint`, `A3_RESEARCH_EVAL_GROUNDING=format-only` und
dem bestehenden expliziten Provider-/Modellselektor aus dem vorigen Nachweis.

### Granite A/B/B/A und gezielter C-Nachtest

Beide A-Fälle schließen mit drei Aufrufen ab und bestehen den Wortcheck, enthalten
aber denselben falschen `import`-Entwurf. Beide B-Fälle verhindern diesen Abschluss;
die Primärentwürfe sind jeweils 4046 Textbytes lang und lassen den richtigen Namen
aus. Der Einzelrepair erzeugt jeweils 7628 Textbytes und scheitert bereits am
unveränderten Decoderlimit 4096. Alle Streams enden mit Stop, nicht OutputLimit.
Es gibt keine zweite Reparatur oder Wiederholung desselben Analysepakets.

Der C-Nachtest ändert nur die explizite Längenangabe im Namensrepair. Der Repair
wird mit 5781 Textbytes kürzer, bleibt aber ungültig. Das ist kein erfolgreicher
Generationsnachweis und kein Anlass, die Grenzen zu erhöhen. Die Namensprüfung
verhindert einen belegten falschen Abschluss; sie macht Granite noch nicht zu einem
zuverlässigen Planmodell. Eine allgemeine Verbesserung der Inhaltsqualität oder
weniger Nutzerhalte ist damit ausdrücklich nicht belegt.

### Sieben C-Fälle und Rücknahme des primären Kontexthinweises

C schließt fünf von sieben Fällen ab, jeweils mit bestandenem Wortcheck und drei
Aufrufen: Luna, Flash, Qwen, Ornith und lokales Gemma. Google Gemma erreicht den
Namenscheck gar nicht und bleibt in der Bestandsanalyse bei zweimal OutputLimit.
Bei Ornith nennt der Änderungsentwurf jetzt drei Positionsargumente (`project_id`,
`title`, `filepath`) statt eines Dateipfads. Der frühere A-Stand verlangte nur
`filepath`. Dieser zusätzliche Inhaltsfehler wird nicht durch einen richtigen
Befehlsnamen gerechtfertigt. Der primäre Namenshinweis hat keinen belastbaren
Nutzenbeleg und wird deshalb entfernt, während der richtige Originalauftrag
und die deterministische Prüfung vollständig erhalten bleiben.

### Finaler D-Nachtest und umgekehrtes Vergleichspaar

| Modell | Abschluss / Wortprüfung | Aufrufe | kumulierte Kontextbytes | adaptive / wiederholte Reads |
| --- | --- | ---: | ---: | --- |
| Granite | ja / ja, zweimal | 4, 4 | 25340, 25340 | 1/0, 1/0 |
| Luna | ja / ja | 3 | 20736 | 1/0 |
| Flash | ja / ja | 3 | 17695 | 1/0 |
| Qwen 8k | ja / ja | 3 | 10269 | 0/0 |
| Ornith | ja / ja | 3 | 20300 | 1/0 |
| lokales Gemma | ja / ja | 3 | 18051 | 1/0 |
| Google Gemma | nein / nein | 2 | 9441 | 1/0 |

Sechs von sieben Modellen schließen D ab; einschließlich des zweiten Granite-
Nachweises sind es sieben von acht D-Fällen. Google Gemma scheitert unverändert
vor der Entwurfsphase in Analyse und Einzelrepair an OutputLimit. Alle lokalen
Aufrufe, auch die zusätzlichen Vergleichspaare, laufen strikt nacheinander.

Der gegenläufige Granite-Vergleich verwendet A2/D1 und D2/A3. Dazwischen liegen
die vollständig mitberichteten C- und übrigen Modellfälle; es ist keine verdeckt
kontinuierliche oder statistisch breit abgesicherte Versuchsreihe. Beide D-Fälle
korrigieren denselben falschen primären Entwurf durch genau einen Repair:
Die beiden initialen Transkripte sind in allen vier Fällen byteidentisch laut
BLAKE3 `caf4bea88dc6373eb523124493acd46347c00430aa782a1ab68deebc26692f69`
und `2ea4c69a739d2961ca0b72efadc8a015fbbd0a914ec7a421700e5a6c6afeb4f4`.
Der ursprüngliche 1107-Byte-Entwurf nennt `import`; der gültige 1623-Byte-Repair
legt dagegen ausdrücklich `import-csv` mit genau einem Dateipfadargument fest.
Der folgende Testentwurf erhält diesen Namen. A2 und A3 schließen denselben
falschen `import`-Entwurf weiterhin ungeprüft ab. Der Erfolg beruht nicht auf einer
größeren Ausgabe, gelockerter Rubrik oder zusätzlichen Readrunde.

Der zusätzliche Repair kostet einen Modellaufruf. `elapsed_ms` liegt bei D bei
17742/17790 statt A2/A3 13847/14003; dies sind native Fixturemesswerte, keine
allgemeine Performanceaussage. Ornith kehrt unter D zum Entwurf mit einem
Positionsargument `filepath` zurück. Der erfolglose Kontexthinweis wird nicht
beibehalten; Prüfung und begrenzte gezielte Reparatur dagegen schon.

## Offene Inhaltsgrenzen

Der Vergleich belegt eine korrigierbare Namensverletzung, keine allgemeine
semantische Planfreigabe. Die gelesenen Entwürfe/Recherchegrundlagen enthalten
weiterhin erfundene Persistenz: Die beiden `save_tasks`-Implementierungen der
unveränderten Fixture geben nur ein Tupel zurück. Granite hat zwar den richtigen
Änderungsnamen, seine alte Recherchegrundlage schlägt aber weiterhin `import`
vor; der Testentwurf nennt überwiegend Testkategorien statt konkreter Eingaben
und erwarteter Ergebnisse. Orniths Basis fordert teilweise sogar schon vorhandene
Projektvalidierung erneut an. Diese Bestandsinterpretationen werden nicht durch
den Namenscheck zu Fakten oder verifizierten Anforderungen.

Der vollständige CLI-Aufrufvertrag, konsistente Fehlerpolitik, konkrete Testorakel
und das Trennen vorhandener Implementierung von erfundenen zukünftigen Vorgaben
bleiben offen. Ein nächster kontrollierter Ansatz muss die Weitergabe solcher
Interpretationen an die Designphase untersuchen, ohne nach
[ADR-0064](../adrs/0064-budgetierte-bestandsuebergabe-an-entwuerfe.md) benötigte
Originalinformationen zu verlieren. Bloß weitere Wortchecks oder zusätzliche
prompteigene Wiederholungen sind kein Ersatz für diesen Nachweis.

## Qualitätsgates und Integrität

Der Zwischenstand C besteht Clippy und das vollständige Workspace-Gate mit 1301
bestandenen / 19 ignorierten Tests. Der abschließende D-Stand besteht erneut die
gezielten Mehrmodus-/Storage-/Namensregressionen sowie alle finalen Gates:

- `cargo fmt --all -- --check`: Exit 0.
- `cargo test -p a3-application --lib request_commands_ --offline --locked --jobs 2 -- --test-threads=1`:
  eine neue Projektionsregression bestanden, unverändert im finalen Workspace-Gate enthalten.
- `cargo test -p a3-desktop --all-features --lib research_explicit_command_ --offline --locked --jobs 2 -- --test-threads=1`:
  eine Rot→Grün-Zulassungs-/Phasen-/Boundsregression bestanden.
- `cargo test -p a3-desktop --all-features --lib research_command_names_ --offline --locked --jobs 2 -- --test-threads=1`:
  zwei echte Mehrmodus-/Reopen-Regressionen bestanden.
- `cargo clippy --workspace --all-targets --all-features --offline --locked --jobs 2 -- -D warnings`:
  Exit 0; `target/reports/research-commands-final-clippy.log`.
- `cargo test --workspace --all-features --offline --locked --jobs 2 -- --test-threads=1`:
  Exit 0; 1301 bestanden, 19 ausdrücklich ignoriert, null Fehler in 84 Suites;
  `target/reports/research-commands-final-workspace.log`.

Der vorherige Clippy-Fehler betraf nur einen
unnötigen Clone im neuen Test und wurde ohne Allow oder Warnungsabsenkung behoben.

Cargo verwendet `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0` und
`CARGO_PROFILE_TEST_DEBUG=0`; alle Läufe sind offline/locked mit zwei Buildjobs
und einem Testthread. `pnpm check:links` und `git diff --check` bestehen; der
vorhandene Node-Enginehinweis (25.6.1 statt 24.14.0) bleibt. Kein Frontendcode,
Providerprofil, DB-/Wire-Schema oder Sicherheitsrecht wurde geändert.
Der Settingskatalog ist nach allen 20 Fällen weiterhin bytegleich:
SHA-256 `aecc7ef93abe29daa97747f2216e87f49aef99ce9490942fc11ee5c65c09ab32`.
Keine Targetdatei oder frühere Rohantwort wurde gelöscht/umgeschrieben.


## Unveränderte Rohberichte

Alle 20 JSONL-Artefakte liegen unter `target/research-eval/`. A/B/C/D beziehen
sich auf die oben getrennt eingefrorenen Stände. A3 und der zweite D-Fall sind
das abschließende gegenläufige Paar. Die 60 Modellaufrufe verbrauchen kumuliert
352822 Kontext-UTF-8-Bytes, nicht 352822 Tokens; insgesamt 18 adaptive Reads,
keiner identisch wiederholt. Die unveränderten Modelltexte bleiben in den lokalen
Originalartefakten, nicht als bereinigte Antworten im Quellrepository.

| Stand | Modell | Datei | SHA-256 |
| --- | --- | --- | --- |
| A | granite4.2:8b | `eval-1788874383561.jsonl` | `fa8a8693c426e55740079a4546ddf696119ee7f7ecb179298b6d336d636d179c` |
| B | granite4.2:8b | `eval-1788874508015.jsonl` | `372b1b38edadc8e8639c867ded3230e9f42eb9b8698f21ef2d4a43e72872929f` |
| B | granite4.2:8b | `eval-1788874548035.jsonl` | `02a6a48d8b2c8d0c88bd40d13c11b3d7263b9d0ddbb893284f3fe277cf38094f` |
| A | granite4.2:8b | `eval-1788874588053.jsonl` | `28eabbe650bb31b511075ccabe8618b4e45c6cf2e3ccce40149aa3ee0b1b80c0` |
| C | granite4.2:8b | `eval-1788874758196.jsonl` | `1e4f224dcedf3a9dcf3c4578bfe373786c2bb7a4dd5f4553ba1f453a148fcae5` |
| C | gpt-5.6-luna | `eval-1788874798618.jsonl` | `8f546f7f77c69e067483294b718fd7af890d0c8a77139fe4face0778330691e5` |
| C | gemini-3.8-flash | `eval-1788874822928.jsonl` | `48966ba64cc59d5c0967aa1167af2cc0f602ae308165464b25a79b78bc65cb03` |
| C | qwen38-8k:latest | `eval-1788874840608.jsonl` | `7fc6a8d0f4b59f353adef8f98a661edcbeb1e70b9fe704a54af9242bf07d83ff` |
| C | gemma-4-26b-a4b-it | `eval-1788874848273.jsonl` | `b5bd2d5b32e50c166aa38d3c624f7182de472fb7eaeb7304da67a341219f11f0` |
| C | ornith-1.5:9b | `eval-1788874878548.jsonl` | `eb19fa12bafe007f9801b1f0ffae1bb834b934a43179c5942b67617e84d3c439` |
| C | gemma4:12b | `eval-1788874908965.jsonl` | `53426d703c21dcbaf0c02511278981d8b3d5f1f09b5174131c3396c5598a3b49` |
| D | granite4.2:8b | `eval-1788875459747.jsonl` | `d738ad324a2728c1928478a280fff43e854201dae2a78149dbd50194deccf022` |
| D | gpt-5.6-luna | `eval-1788875470640.jsonl` | `658ac3cdcaf421a4111528b59ac2cdb1a7e72d15c89339bb63776e3274ec99a3` |
| D | ornith-1.5:9b | `eval-1788875485345.jsonl` | `90fc6561eb7dafa9f5bd7cf3102f741a7879fb68c11e7674d92608574ab1c042` |
| D | gemini-3.8-flash | `eval-1788875499453.jsonl` | `7b05e7b3a202979e7cd5f3ec83c510acde7cb031cf26b4b6688073cf8505401c` |
| D | qwen38-8k:latest | `eval-1788875520813.jsonl` | `97b736bef7c9b0d11b8a9601996a430d0647679f073055c14f9ff64792d7e110` |
| D | gemma-4-26b-a4b-it | `eval-1788875523573.jsonl` | `ee39963cc6572d1aeed354f88c463fb8c1575192ae71dcadba4defd20b1cb814` |
| D | gemma4:12b | `eval-1788875556883.jsonl` | `0e99e5de965117442a5b28c6549b9d1c51d615b0733effd51d42229a85d93eda` |
| D | granite4.2:8b | `eval-1788875623579.jsonl` | `8a3d15f0aa805b6b8c49d1ce896abee1b4b9b58ffa163b1005f5b1b9af00863a` |
| A | granite4.2:8b | `eval-1788875645402.jsonl` | `a1be4f2899d89608bbc4919dc3853d275d13206b0aac733a4cc46915e1efa427` |
