# Plan 10: Quellenlokaler Recherchevergleich

Stand: 2026-09-08. Ausgangscommit: `5b70fbe`.
Entscheidungen: [ADR-0100](../adrs/0100-quellenlokale-recherche-im-vergleich.md)
und [ADR-0101](../adrs/0101-grosszuegigere-originalzitate-in-source-review.md).
Fortsetzung des [Verifikationsprotokolls](10-RESEARCH_VALIDATION.md).

## Ergebnis und Grenzen

Der native, ausdrücklich gewählte Vergleich `source-local` ist implementiert und
über den echten Researcher geprüft. Der Produktstandard bleibt `joint`.
78 reale Recherchefälle über vier eingefrorene Stände zeigen konkrete
Protokoll-/Budgetfehler und deren Korrekturen. Die abschließenden zwölf V2-Fälle
erreichen alle Completed und WorkReady ohne Nutzerhalt, einschließlich aller sechs
Planaufgaben. Acht bestehen zusätzlich die unveränderte Begriffrubrik v3.

Das ist **keine fachliche Gesamtfreigabe**. Vier Auditantworten lassen weiterhin
den konkreten Writer aus. Falsche Persistenzbehauptungen, überlappende Teilantworten
und ein Plan mit falschem Befehlsnamen bleiben nachweisbar. Die gestufte Methode
wird deshalb nicht zum Produktstandard. Der mutierende Agent, der gesondert
gesperrte SourceGuided-Codingvergleich und die breitere Modellabnahme werden durch
diese read-only Versuche weder ersetzt noch als erledigt markiert.

## Implementierter Schnitt

Vor der ersten Analyze-/SummarizeOriginals-Phase inventarisiert der Core einmal
je Abschnitt höchstens vier tatsächlich gelieferte aktuelle Originalfenster einzeln.
Jeder Aufruf erhält den vollständigen unveränderten Originalauftrag, die aktive
Core-Frage mit Art/Status und genau ein Original. Die Verantwortung für andere
Dateien und den Endabschluss bleibt beim Core. Die Vollabdeckungsanweisungen der
gemeinsamen Analyse werden nicht als widersprüchlicher Einzelquellenauftrag kopiert.

SourceReview V2 liefert eine kurze, ausdrücklich unbestätigte Interpretation und
ein bis vier exakte eindeutige Zitate. Der unabhängige Decoder prüft Version,
vollständige Feldmenge, doppelte JSON-Schlüssel, UTF-8-Bytegrenzen, tatsächliche
Quellpositionen und auch überlappende Mehrdeutigkeit. Revision, Source-ID und
Fenster stammen vom Core; Herkunft bestätigt nicht den Inhalt der Interpretation.

Alle Starts, Transportretries und Einzelrepairs verbrauchen die bestehenden
gemeinsamen Grenzen. Vor und nach jedem Aufruf revalidiert der Safe Reader die
Originale. Der besessene Job behält Cancellation und Gesamtfrist. Feldgenaue
Repairdiagnosen enthalten nur Kategorien und Zahlen, keine verworfenen Quell- oder
Modellinhalte. Keine zusätzliche Leseberechtigung, Persistenz, Providerkonfiguration,
Abhängigkeit oder Ausführungsautorität entsteht.

Nur zu aktuell gelieferten Originalfenstern passende Hinweise werden vollständig
an Analyze/SummarizeOriginals angehängt. Alte E-Labels werden nicht wiederverwendet;
der Core bindet den aktuellen Anker. Pflichtauftrag und Originale bleiben bytegleich.
Design/DesignTests erhalten keine zusätzlichen Reviewhinweise: Im Zwischenstand
hatte deren Weitergabe einen echten Plan-Kontextüberlauf verursacht. Ein tatsächlicher
Hint-Überlauf bleibt ein ehrlicher ContextLimit statt verdecktem Baseline-Fallback.

## Großzügigere Aufteilung innerhalb der Profilgrenzen

Ein feldgenauer V1-Nachtest belegt die zu knappe Einzelzitatgrenze:
`eval-1788867488062.jsonl` endet für Orniths Plan nach zwei Aufrufen, weil sowohl
Primärantwort als auch Einzelrepair ein 260-Byte-Zitat bei erlaubten 256 Bytes
liefern. Flash überschreitet dieselbe Grenze mit 265 Bytes. Die allgemeine
Fehlermeldung hatte zuvor nicht einmal das betroffene Feld benannt.

V2 erhöht diese künstliche Grenze auf **512 UTF-8-Bytes je Zitat**.
192 Bytes Interpretation, vier Zitate und 4096 Bytes Gesamtdokument bleiben bestehen.
Zusätzlich prüft die Hint-Vorprüfung nur den minimal möglichen Umschlag statt
ungenutzte Maximallängen hypothetischer Hinweise zu reservieren. Der tatsächliche
komplette Hint muss später weiterhin passen. Es werden weder Originale gekürzt noch
Kontext-/Outputgrenzen überschritten.

Reproduzierte produktive Recherche-Paketbudgets (nicht echte Tokenizer-Messungen):

| Profil Kontext / Output | FormatFieldOnly | RepeatSchemaInPrompt |
| --- | ---: | ---: |
| 8192 / 2048 | 3409 Bytes | 1600 Bytes |
| 16384 / 4096 | 9553 Bytes | 7744 Bytes |

Diese äußere produktive Berechnung wurde in diesem Schnitt nicht erhöht.
ConservativeUtf8BytesV1 rechnet konservativ ein UTF-8-Byte als eine Token-Einheit;
Schema, Pflichtsteuerung, Antwort und Sicherheitsreserve brauchen ebenfalls Platz.
Der echte Mehrmodusvertrag besteht auch mit dem tatsächlichen 3409-Byte-Paket.
Dort passen in Ask Manager und Plugins vollständig; in der kompakteren
Plan-/Agent-Bestandsaufnahme passt zusätzlich Storage. Tests mit 4096/8192
Paketbytes sind zusätzliche Bytefenster, keine Behauptung über Modellkontexttokens.
Providerverträge prüfen vollständige SourceReview-Pakete und 768-Byte-Repairtexte
bei beiden Schema-Groundings in Ask, Plan und Agent-Vorbereitung.

## Reale Versuchsreihe

Unveränderte öffentliche Fixture: `fixtures/research-eval-v1`.
Audit ist Familie 1, Variante 1 (Ask); CSV-Plan ist Familie 3, Variante 0 (Plan).
Die ursprünglichen Fragen, fünf Fixturedateien und Begriffrubrik v3 sind unverändert.
Die nativen Tests verwenden Index, Safe Reader, libSQL und denselben Researcher
wie die Anwendung. Jeder Lauf prüft die unveränderten Fixturebytes abschließend.

In den ersten beiden Ständen läuft für jedes der sechs Modelle dieselbe Auditfrage
in der Reihenfolge joint/source-local/source-local/joint (AB/BA).
Spätere Einzel-Nachtests isolieren konkrete Fehler, liefern aber keinen weiteren
gegenbalancierten Nachweis für einen generellen Vorteil der finalen Methode.

| Stand | Fälle | Completed | Begriffrubrik v3 |
| --- | ---: | ---: | ---: |
| initial: V1 mit widersprüchlichem gemeinsamen Arbeitsauftrag | 24 Audit | 20 | 3 |
| scoped: tatsächlicher Core-Schritt, begrenzte Einzelquellenverantwortung | 24 Audit + 6 Plan | 24 | 7 |
| repair: zusätzlich feldgenaue Diagnose, noch 256-Byte-Zitate | 6 Audit + 6 Plan | 10 | 7 |
| v2: zusätzlich 512-Byte-Zitate | 6 Audit + 6 Plan | 12 | 8 |

Die gemeinsame Baseline schließt in beiden AB/BA-Ständen jeweils 12/12 Auditfälle
ab, mit jeweils 1/12 bestandenen Wortprüfungen. Source-local schließt zunächst
8/12 und nach der Aufgabentrennung 9/12 Auditfälle ab (Wortprüfung 2/12 bzw. 3/12).
Mehr Aufrufe sind kein Qualitätsbeleg: joint braucht dabei 3–4 Aufrufe,
source-local im zweiten Vergleich 5–8. Repeated adaptive reads sind in allen
78 gespeicherten Fällen null. Quelleninterpretation kann selbst scheitern;
sie ist nicht kostenlos und löst das Inhaltsproblem nicht automatisch.

Lokale Modelle liefen ausschließlich nacheinander. Luna und Flash nutzten die
bereits freigegebenen nativen Credential-Slots; keine Schlüsselübernahme in Shell
oder Berichte. Die Profile blieben unverändert: Luna 16384/2048, Qwen 8192/2048,
Ornith/Flash/Gemma/Granite jeweils 16384/4096.
Der native Settings-Katalog bleibt vor/nach allen 78 Fällen bytegleich:
SHA-256 `aecc7ef93abe29daa97747f2216e87f49aef99ce9490942fc11ee5c65c09ab32`.
Es wurden keine Modelle installiert oder Settings umgeschrieben.

### Abschließende V2-Fälle

Alle folgenden Fälle schließen ab und sind WorkReady. „Bytes“ ist die Summe der
an die Modellaufrufe übergebenen Transkripte einschließlich Review/Repair, nicht
die Größe eines einzelnen Kontextfensters und nicht gemessener Provider-Tokenverbrauch.
Aufrufe werden einschließlich Review/Repair gezählt. Laufzeiten werden wegen
teilweise parallel laufender CPU-Gates nicht als Geschwindigkeitsvergleich verwendet.

| Modell | Fall | Aufrufe | Bytes gesamt | Zusatzreads | Begriffrubrik |
| --- | --- | ---: | ---: | ---: | --- |
| gpt-5.6-luna | Audit | 6 | 14419 | 1 | bestanden |
| ornith-1.5:9b | Audit | 6 | 14783 | 1 | fehlt `write` |
| gpt-5.6-luna | Plan | 6 | 24244 | 1 | bestanden |
| ornith-1.5:9b | Plan | 6 | 23561 | 1 | bestanden |
| gemini-3.8-flash | Audit | 6 | 14471 | 1 | bestanden |
| qwen38-8k:latest | Audit | 5 | 11332 | 0 | fehlt `write` |
| gemini-3.8-flash | Plan | 6 | 23177 | 1 | bestanden |
| qwen38-8k:latest | Plan | 5 | 13597 | 0 | bestanden |
| gemma4:12b | Audit | 7 | 16101 | 1 | fehlt `write` |
| gemma4:12b | Plan | 8 | 31727 | 1 | bestanden |
| granite4.2:8b | Audit | 8 | 21016 | 1 | fehlt `write` |
| granite4.2:8b | Plan | 8 | 26502 | 1 | bestanden |

Gemma korrigiert nicht im Original gefundene Zitate im Einzelrepair.
Granite korrigiert mehrdeutige beziehungsweise fehlende Originalzitate.
Orniths vorheriger 260-Byte-Planabbruch tritt im V2-Nachtest nicht mehr auf.
Das sind beobachtete Einzelfallnachweise, keine statistische Zuverlässigkeitsgarantie.

### Getrennte Inhaltsprüfung

Alle sechs abschließenden Auditantworten wurden am Original gelesen:

- Die Storage-Methoden geben nur `('json'/'sqlite', self.filepath, tasks)` zurück;
  sie schreiben nichts. Der wirkliche Auditwriter ist
  `output.write(f'{event}: {task}\\n')` innerhalb von `_log`.
- Luna nennt den Writer und vermeidet im Nachtest die ausdrückliche Behauptung,
  dass `save_tasks` tatsächlich auf die Platte schreibt. Zwei Antworten überlappen.
- Flash besteht die Wortprüfung, behauptet aber weiterhin „Persistierung“.
  Die bestandene Rubrik ist damit nachweislich kein semantischer Korrektheitsbeweis.
- Ornith behauptet ausdrücklich Persistenz, auch in eigenen Reviewhinweisen,
  und lässt den konkreten Writer aus.
- Qwen und Gemma lassen den Writer aus. Qwens Ask-Paket enthält nur die beiden
  Pflichtdateien; die Wirkung eines nicht gelieferten Storage-Methodenrumpfs
  darf aus dessen Namen nicht als implementiert gelten.
- Granite behauptet unter anderem Speicherung etwa in `tasks.json`, lässt
  `output.write` aus und liefert überlappende, unterschiedlich genaue Teilantworten.

Zusätzlich ist im abschließenden Granite-Plan der konkrete Befehlsname
`import` statt des verlangten `import-csv` festgelegt. Das Wort `import-csv`
steht zugleich im Testabschnitt, sodass die unveränderte Begriffrubrik besteht.
Alle sechs Pläne wurden erstellt; eine vollständige fachliche Abnahme oder
Implementierungsverifikation der sechs Pläne wurde nicht durchgeführt.

**Offen:** tatsächliche implementierte Operationen von vermuteten Callee-Effekten
unterscheiden, Teilantwortduplikate vermeiden und explizite Auftragsliterale bis in
Entwurf/Testplan erhalten. Eine weitere Iteration muss diese Fehler unabhängig
nachweisen. Bloß mehr Quellenanker, mehr Prompttext, großzügigere Wortprüfungen
oder Modellselbstbewertung dürfen nicht als Lösung gelten.

## Reproduktion und Qualität

Nur nach vorhandener expliziter Modell-/Providerfreigabe ausführen.
Nativer Testfilter:
`agent_session_manager::research_followup_tests::coherent_contract::matrix::research_approved_model_matrix`
(den aktuellen vollqualifizierten Namen mit `research-tests.exe --list` prüfen).
Die Standbinaries unter `target/reports/<Verzeichnis>/research-tests.exe` werden
mit `--ignored research_approved_model_matrix --nocapture --test-threads=1` gestartet.

Umgebung je Fall: `A3_RESEARCH_ANALYSIS_METHOD=joint|source-local`,
`A3_RESEARCH_EVAL_CASE=1:1|3:0`, `A3_RESEARCH_EVAL_REPETITIONS=1`.
Lokale Wahl über `A3_LOCAL_RESEARCH_MODEL`; für externe Modelle stattdessen
`A3_CONFIGURED_RESEARCH_CATALOG` und das bestehende explizite Provider-/Modellpaar
`A3_RESEARCH_EVAL_PROVIDER`/`A3_RESEARCH_EVAL_MODEL`.
Keine gleichzeitige lokale und Katalogauswahl. Capability-Probe und unveränderte
Profil-/Credential-Zulassung bleiben erforderlich.

Finale Rust-Gates mit `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`:

- `cargo fmt --all -- --check`
- `cargo test -p a3-desktop --all-features --lib source_review --offline --locked --jobs 2 -- --test-threads=1`: 8 bestanden.
- `cargo test -p a3-application --lib source_review --offline --locked --jobs 2 -- --test-threads=1`: 5 bestanden.
- `cargo test -p a3-desktop --all-features --lib research_v5_missing_original_receives_exact_current_groups_in_its_single_repair --offline --locked --jobs 2 -- --test-threads=1`: 1 bestanden.
- `cargo clippy --workspace --all-targets --all-features --offline --locked --jobs 2 -- -D warnings`: bestanden.
- `cargo test --workspace --all-features --offline --locked --jobs 2 -- --test-threads=1`: alle 84 Ergebnisgruppen einschließlich Doctests grün; 1289 bestanden, 19 explizit ignoriert.
- `pnpm check:links` und `git diff --check`: bestanden. Bekannte Node-25.6.1-/24.14-Enginewarnung; kein Frontendcode geändert.

Ein **Zwischenstand** des vollständigen Workspace-Gates
(`source-review-repair-workspace.log`) meldete beim bestehenden isolierten
`research_v5_missing_original_receives_exact_current_groups_in_its_single_repair`
einen Kindprozess-Exit vor Abschluss. Der unveränderte gezielte Nachtest und das
finale vollständige Gate bestehen. Die Ursache des einzelnen Zwischenfehlers ist
nicht nachgewiesen; weder Retry noch abgeschwächte Assertion kaschieren ihn.

Die vier finalen lokalen Audit-Testwrapper enden wegen der fehlenden
Begriffrubrik mit Exit 101, die übrigen finalen acht mit Exit 0. Die JSONL-Daten
belegen getrennt den tatsächlichen Rechercheabschluss. Ein Wrapperfehler allein
darf nicht als Controllerabsturz oder als Inhaltskorrektheit interpretiert werden.

### Artefaktidentität

Alle unten genannten Rohdateien liegen unter `target`, sind nicht eingecheckt
und dürfen bei einer späteren bewussten Buildbereinigung fehlen. Dieses Protokoll
bewahrt Befund, Fallzuordnung und SHA-256; es enthält weder Zugangsdaten noch
unbeschränkte private Quell-/Modelltexte.

| Stand | Verzeichnis unter target/reports | Binary SHA-256 |
| --- | --- | --- |
| initial | `research-source-local-20260908` | `04fbcb7c1c2c6b76dfd20f894ea9259845847f82df8c85b14a2c2ee1064a1a96` |
| scoped | `research-source-local-scoped-20260908` | `ac82793371ddb5b399f9a30c67a069428551ff1114df7e80da6d2dd66007a3b7` |
| repair | `research-source-local-repair-20260908` | `8df782a765c4726d0c4b69bec77ff395f84e28be17533e2849ebde38418ab33c` |
| v2 | `research-source-local-v2-20260908` | `2b0361741681d7444c6908417fb6cb7a0858adea24446e416f1d13aadd84a348` |

| Finales Gate-Log unter target/reports | SHA-256 |
| --- | --- |
| `source-review-v2-targeted.log` | `4e1f02e03c753723698218900820febd99a362af65c873aa3ac13f87e7e96b95` |
| `source-review-v2-codec.log` | `0846067c79663b2a22e842f4daeb88b72ad11522b5033ec43f1dde2ef1393e07` |
| `source-review-v2-coverage-recheck.log` | `4807e86b5c557025cdab675378983a4620b54ed82a19ea3c23f75b82e4ec5911` |
| `source-review-v2-clippy.log` | `89b27927a869a087ceec7062b2b29d14ddba70293bea3c6a6328dfe5e0e390e2` |
| `source-review-v2-workspace.log` | `22b3aea8ad4f9a0e61af7713b6b054dce5e356bc194812ece6d7741268df19d2` |

### Vollständiges JSONL-Manifest

Je Datei genau ein Fall unter `target/research-eval`.
A=Audit, P=Plan; J=Joint, S=SourceLocal. Abschluss und Wortprüfung sind getrennt.

| Stand | Modell | Fall / Methode | Abschluss / Wortprüfung | JSONL | SHA-256 |
| --- | --- | --- | --- | --- | --- |
| initial | gpt-5.6-luna | A / J | ja / nein | `eval-1788866121364.jsonl` | `db1d77e58fb005d35d3650c24ed410c88628e8ccc25caeeef7c27e98f39a67de` |
| initial | gpt-5.6-luna | A / S | ja / nein | `eval-1788866136335.jsonl` | `e459066dd7677c2a0ac6cc62f47420e51050e5a59094f363c12863227828641d` |
| initial | gpt-5.6-luna | A / S | ja / ja | `eval-1788866158483.jsonl` | `d58472194bf271e59a9ebaaf04018b2ac32d4328eb610debde32d1a7f1f1669e` |
| initial | ornith-1.5:9b | A / J | ja / nein | `eval-1788866167121.jsonl` | `dd7f9b2da140c197ed90605b407ce9d79b9d80bcd753cca4959001fa187c79ce` |
| initial | gpt-5.6-luna | A / J | ja / nein | `eval-1788866178193.jsonl` | `e72ce242a3c7a9c564dae73402d204bd27016c7106aaf599aa55bfb1847fbdce` |
| initial | ornith-1.5:9b | A / S | ja / nein | `eval-1788866187526.jsonl` | `b14911c3b6211b66f5b92bffe003769eab82ae85b160435935b241b35508de88` |
| initial | ornith-1.5:9b | A / S | ja / nein | `eval-1788866212805.jsonl` | `46da52b9adc47dda89e0d3039215662e3f2b49aef950624bb0435872d818825c` |
| initial | ornith-1.5:9b | A / J | ja / nein | `eval-1788866237815.jsonl` | `d3bf3ee37ae0c9c8ec0d7b8bca8a0f37b4ae110acfc2a182e74c6ba401111ab5` |
| initial | qwen38-8k:latest | A / J | ja / nein | `eval-1788866263714.jsonl` | `c50055ad068b87f460c0fef2726215754b68b6b1913062cc570a6de227eb3763` |
| initial | qwen38-8k:latest | A / S | ja / nein | `eval-1788866288681.jsonl` | `7d774b91d225973644e898547ce690ed01384bcec387dfe57e60f9545c38b373` |
| initial | gemini-3.8-flash | A / J | ja / nein | `eval-1788866297244.jsonl` | `78c7942c480a62cf947660ae02c94b56c4b161daa19baf676fc917081aa8ce2f` |
| initial | gemini-3.8-flash | A / S | ja / nein | `eval-1788866315796.jsonl` | `84a19f102ee389b3e4b803e9478357a5c3bff5990471de06277e23dd51d1006a` |
| initial | qwen38-8k:latest | A / S | ja / nein | `eval-1788866320145.jsonl` | `df0eab265e77cc93bbd1e0bc4675afe654114fc32311ecc4c73b7adfc28f26eb` |
| initial | qwen38-8k:latest | A / J | ja / nein | `eval-1788866351042.jsonl` | `7a45c057039dff92261d777513299014bab6b9c92779cecdfc83ac4f79d04d33` |
| initial | gemini-3.8-flash | A / S | ja / ja | `eval-1788866360670.jsonl` | `838eed5c23336535b994167ea9b2f129319ef83d56370382627eb02c1d6c6e5e` |
| initial | gemma4:12b | A / J | ja / nein | `eval-1788866378395.jsonl` | `e387dc32260a9e28b0527927ee31677a85588eb1ac8bb84327f574722c9bcc82` |
| initial | gemini-3.8-flash | A / J | ja / ja | `eval-1788866393155.jsonl` | `1be99acc938a809e4782ee14178a228dd93e45348010acc3edf3223ec0c58677` |
| initial | gemma4:12b | A / S | nein / nein | `eval-1788866404183.jsonl` | `aa16802185d017d51025ef1ebc05fe8cf54332bf0666ed7214d5785aed3479f2` |
| initial | gemma4:12b | A / S | nein / nein | `eval-1788866422304.jsonl` | `5669b07f03191a591e27bf5259805909b3e9a866b9990fe962ae644af656bad2` |
| initial | gemma4:12b | A / J | ja / nein | `eval-1788866440115.jsonl` | `89fbf0d0b1e6c76945c74aaae724323f3ff55769664bc24ddce04ac09bda2721` |
| initial | granite4.2:8b | A / J | ja / nein | `eval-1788866466111.jsonl` | `ac15ffa70ffe68d6027a0440d24f895e7a8be034f3002e55c5a0464a4cd2d54a` |
| initial | granite4.2:8b | A / S | nein / nein | `eval-1788866482273.jsonl` | `5cc36014cf68baedbbb3450e9b2ab4ec6fd83080f72ac9bcd42b9b55b5d11fd1` |
| initial | granite4.2:8b | A / S | nein / nein | `eval-1788866493038.jsonl` | `02dacdcf19b9d0e07e9548375e7cfd2393b4d16f572774303d4f1442dbf99070` |
| initial | granite4.2:8b | A / J | ja / nein | `eval-1788866503626.jsonl` | `a07d35ea6375660fc0e95de486fff26f1068b3b037c352838f60508421ecbea7` |
| scoped | gpt-5.6-luna | A / J | ja / nein | `eval-1788866712411.jsonl` | `7efae98b9c6299f50d77274df376800a1da43bb96e993f075070415bbb6f8cc5` |
| scoped | gpt-5.6-luna | A / S | ja / nein | `eval-1788866725234.jsonl` | `4591dcd773a40957166bdf63169f1c0a59d64087facf8e66b57b5d3f48e88496` |
| scoped | ornith-1.5:9b | A / J | ja / nein | `eval-1788866727260.jsonl` | `bbf11f5401ecb5ae5097d7df2ce26804d9f219748360fa1d5a595e63af704f75` |
| scoped | gpt-5.6-luna | A / S | ja / nein | `eval-1788866745950.jsonl` | `d9fd2f76a92de7d054f76fc3db4b7d16f364067f62a9e25ad3fb43ddfbaf6743` |
| scoped | ornith-1.5:9b | A / S | ja / ja | `eval-1788866747480.jsonl` | `e0e40b8876bec5e460ceb96ad7250c9cb981f8fc0a6eab89b9b4ccae39995f3f` |
| scoped | gpt-5.6-luna | A / J | ja / nein | `eval-1788866765632.jsonl` | `fc15cf8aa4a5e1cb1213ec7f3b477b61dde8e9c04a648a8ee2e326bf7b57a904` |
| scoped | ornith-1.5:9b | A / S | ja / ja | `eval-1788866774251.jsonl` | `9cd3f5689d9fa1dfcf384e5ac5ece4f105f8e85c6ff3c8ad8d4f134ec57c5d27` |
| scoped | gpt-5.6-luna | P / S | nein / nein | `eval-1788866778483.jsonl` | `5bde1c6de3e94f7d454a656b9c58d7e7eda0385c92561c12191cd6f5f26a04c1` |
| scoped | gemini-3.8-flash | A / J | ja / nein | `eval-1788866788478.jsonl` | `8be29fa94d0c8e67347a8ca3c92499317e91fb452a6975834590342b291d5553` |
| scoped | ornith-1.5:9b | A / J | ja / nein | `eval-1788866800385.jsonl` | `77d537eca8c0cb4e5d5013913bf263ef2320a23a6a8e05eaec3a5cb33aa196bb` |
| scoped | gemini-3.8-flash | A / S | ja / ja | `eval-1788866804017.jsonl` | `f4c6566c2a341f10451a89cf73b68d6c315824dd32632ec676f7da7139ff3f48` |
| scoped | ornith-1.5:9b | P / S | nein / nein | `eval-1788866820232.jsonl` | `d589c85021e990d68449fe7b9ba7eeb1802993e48c0d2bd14108d250a6e61650` |
| scoped | qwen38-8k:latest | A / J | ja / nein | `eval-1788866840605.jsonl` | `80bc4952f25c14721b61036449df4001315d7ef2ad83b49aa29444f122204b5a` |
| scoped | gemini-3.8-flash | A / S | nein / nein | `eval-1788866843517.jsonl` | `d0433bf89407353ca93fa49cbe8e5d70a0a7b8db6ab00fdec1a461838eda1972` |
| scoped | qwen38-8k:latest | A / S | ja / nein | `eval-1788866867770.jsonl` | `4d01fd7648505d1db5846cbf08991f1679c39c8e35c5821c103f77a82705aee5` |
| scoped | gemini-3.8-flash | A / J | ja / ja | `eval-1788866870374.jsonl` | `b902f2f865d35762612b3632fb11d340c2a7fa6799fd0244046903957ff2ef37` |
| scoped | gemini-3.8-flash | P / S | ja / ja | `eval-1788866886365.jsonl` | `1521a6757ada44e6e7d3225af956e2fdd7327249dea0add2e220d3b49bbc310c` |
| scoped | qwen38-8k:latest | A / S | ja / nein | `eval-1788866902131.jsonl` | `9dd15c4eac61caf9e37f03cf58239fe39c6b15a4425dc93b3576c7414efa7a2a` |
| scoped | qwen38-8k:latest | A / J | ja / nein | `eval-1788866935615.jsonl` | `d79dc0f8b7f2016127e0dbb8b80aa46310f6ba6032d759753df27f14b1537dfb` |
| scoped | qwen38-8k:latest | P / S | ja / ja | `eval-1788866961115.jsonl` | `4f66f35ba4078bc9451afe2c508b5c4222fbfb4cd04dc63af98b74ece85cde70` |
| scoped | gemma4:12b | A / J | ja / nein | `eval-1788866998053.jsonl` | `084b2277a6753f36184195815064d17e8f98422cd59fa69577d9c27986c7eebc` |
| scoped | gemma4:12b | A / S | ja / nein | `eval-1788867024536.jsonl` | `9adb6b083217728e654e62451a0ef1ece472dab69e6f1d0d77e42945a11655f9` |
| scoped | gemma4:12b | A / S | ja / nein | `eval-1788867055329.jsonl` | `e17c68b0512b52ab9361a1adb74d36d26e96130dfe33a3116b34cb906730c328` |
| scoped | gemma4:12b | A / J | ja / nein | `eval-1788867085009.jsonl` | `c66358247e96ba1404f97c5fe94e8d94805279e5003565fd643efc280fc4087e` |
| scoped | gemma4:12b | P / S | ja / ja | `eval-1788867110882.jsonl` | `25cf0f8f38598e083e3a80309b1d43d13272c080e63b9706cb3614864b968262` |
| scoped | granite4.2:8b | A / J | ja / nein | `eval-1788867149518.jsonl` | `5d80e05bc60745a9932c0b1d09351fad0c6d337430d08667425a42e907ac2816` |
| scoped | granite4.2:8b | A / S | nein / nein | `eval-1788867165904.jsonl` | `474340c296661f0ccf3a756822aa2750380a7b5749a0f51b7240287706446e67` |
| scoped | granite4.2:8b | A / S | nein / nein | `eval-1788867178880.jsonl` | `8e3c958d5a351b63d87728d493044a10589b4aff7a164be524d0268e0c21d6d4` |
| scoped | granite4.2:8b | A / J | ja / nein | `eval-1788867192391.jsonl` | `39cdf44a26f0b3805ea50ba53456fc5630f01f4d95595d51274813f4ce08d424` |
| scoped | granite4.2:8b | P / S | nein / nein | `eval-1788867209231.jsonl` | `fd139c6f1679b950b26f96a4219daf6c230f0816971fa83d768c0df2b903a1ed` |
| repair | gpt-5.6-luna | A / S | ja / ja | `eval-1788867444096.jsonl` | `5015b465f4860e37a73ca25f2450d23b93493f2cb604c665ae009bfe3361a875` |
| repair | ornith-1.5:9b | A / S | ja / nein | `eval-1788867459059.jsonl` | `432ddd69f85971815f21a7cde386c7695866289e80f5df95b874c71de4dbc6bb` |
| repair | gpt-5.6-luna | P / S | ja / ja | `eval-1788867465595.jsonl` | `bfc680a2d69330aa8762a06da19a4552def5fcf45bf20448d86ac79a2a33e821` |
| repair | ornith-1.5:9b | P / S | nein / nein | `eval-1788867488062.jsonl` | `33a8f539f4522fa75c43a6c7d12465506ebb89c37090891f5637ab326da35816` |
| repair | gemini-3.8-flash | A / S | ja / ja | `eval-1788867499430.jsonl` | `9ebaba84f26c4a31292f29d42297d7bf6cb025dcc845b798f747bb391350e3d3` |
| repair | qwen38-8k:latest | A / S | ja / nein | `eval-1788867508698.jsonl` | `8e80eebdf08883343aebf15dc4e5677eeb58e16e994ec57a30e9630a7eb087ae` |
| repair | gemini-3.8-flash | P / S | ja / ja | `eval-1788867528262.jsonl` | `d7fc5bd7a7a6e38945de151579957318fe4396e513f5ab384ca9f93c3d3d61b7` |
| repair | qwen38-8k:latest | P / S | ja / ja | `eval-1788867544540.jsonl` | `781701290472587998b14c720ebe76ea33559d39d8c5417353a0adb0c2ea08a0` |
| repair | gemma4:12b | A / S | ja / nein | `eval-1788867582513.jsonl` | `90b92e5ea1ea1f5ebb8f8751548322b6a19b1dbbd002d14831aba8fb18b19757` |
| repair | gemma4:12b | P / S | ja / ja | `eval-1788867613056.jsonl` | `108f95332cfd4873563873e82d97d77c0ee1f2a608578421df6e6d1fe399d28f` |
| repair | granite4.2:8b | A / S | ja / ja | `eval-1788867651374.jsonl` | `4fa49a11b3482d44e52db0c2ef33a02dbf3d194b33671a77927405fe7106e2f3` |
| repair | granite4.2:8b | P / S | nein / nein | `eval-1788867671707.jsonl` | `59585045652808f990be653b1c6c86127e8e21818f6cc040ece3e2b9d52e80fd` |
| v2 | gpt-5.6-luna | A / S | ja / ja | `eval-1788867999754.jsonl` | `dda0d5c2c08d0d82197ae8e949d5d022fdb057e2a9457b22893dd0577421847f` |
| v2 | ornith-1.5:9b | A / S | ja / nein | `eval-1788868013599.jsonl` | `b117e87bafcaa7476b9b9493cd6a98aa474c20a3c8cbdfb1e377776af25a9350` |
| v2 | gpt-5.6-luna | P / S | ja / ja | `eval-1788868019302.jsonl` | `ac7c37ec63ae6e4615761efe5fda7fcff4f17620652e684e78f2318540edbcdb` |
| v2 | ornith-1.5:9b | P / S | ja / ja | `eval-1788868040436.jsonl` | `43499467044bd22aaa31b6323a8d02b1c1f49d84f0e9c389d25b66e4c372cf06` |
| v2 | gemini-3.8-flash | A / S | ja / ja | `eval-1788868069720.jsonl` | `c2a2e75afc54b42c830a2a9d5021d33aae9f0a9b511a2d53005d7a26f86f7d7a` |
| v2 | qwen38-8k:latest | A / S | ja / nein | `eval-1788868077680.jsonl` | `5893a8dd5be2d059c00617d099244a1ef229827fe4b1569b0d904f250c12a8b3` |
| v2 | gemini-3.8-flash | P / S | ja / ja | `eval-1788868092450.jsonl` | `56483a7a2bc9fbb4be3da9215a82cc068f6848e680952f9d9e022309525f246f` |
| v2 | qwen38-8k:latest | P / S | ja / ja | `eval-1788868113358.jsonl` | `ab5de6ba6ea6e6694f9bd0c7643a53011af8f15011e63108323b0f1f428e5265` |
| v2 | gemma4:12b | A / S | ja / nein | `eval-1788868153416.jsonl` | `325b63e814d174307a00ee6d06330dcdb78192b54a8d73aba8fa94e9620a08bd` |
| v2 | gemma4:12b | P / S | ja / ja | `eval-1788868188659.jsonl` | `6e3775fd8e49b031282ee381936ab2ada9230b099b7173566e959c3fd989772a` |
| v2 | granite4.2:8b | A / S | ja / nein | `eval-1788868281024.jsonl` | `fdbe0bdd7a4e2e9523937639dee7bd60862c98ffc55d64bfde0e4c82b75c875d` |
| v2 | granite4.2:8b | P / S | ja / ja | `eval-1788868305130.jsonl` | `27926f07da2ff2aae4afe0e5157a220ca8f7e8c0f76a56b4497e09682603577b` |
