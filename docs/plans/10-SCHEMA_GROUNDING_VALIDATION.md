# Plan 10: Schemawiederholung und abgeschnittene Antworten

Stand: 2026-09-08. Ausgangspunkt: `b71005d`.

Der Bericht beschreibt drei getrennt eingefrorene Stände: geschlossene
Grounding-Auswahl, zusätzliche reine Streambeobachtung und anschließend die
phasenspezifische Reparaturkorrektur. Ihre Binary-Hashes sind unten getrennt erfasst.

## Fragestellung und Grenzen

Der [Operationsvergleich](10-SOURCE_OPERATIONS_VALIDATION.md) belegt für Google
Gemma OutputTruncated in Analyze und dessen Einzelrepair. Kleine SourceReview-
Dokumente gelingen. Die bestehende Gemini-Projektion aus
[ADR-0068](../adrs/0068-gemini-schema-arraygrenzen.md) entfernt variable Array-Maxima;
der unabhängige Decoder behält sämtliche ursprünglichen Grenzen. Das rechtfertigt
keinen zusätzlichen Read oder ein Ausführen abgeschnittener Dokumente.

Dieser Vergleich isoliert die bereits implementierte Profiloption
`RepeatSchemaInPrompt` gegenüber `FormatFieldOnly`. Es ist kein neues Antwortschema,
keine neue Recherchephase und keine Änderung des Produktstandards. Die vollständige
Schemawiederholung wird regulär gezählt und kann deshalb weniger Originalplatz
übrig lassen. Es gibt weder abgeschnittene Pflichtpakete noch einen stillen Fallback.

## Native Auswahl und Reproduktion

`A3_RESEARCH_EVAL_GROUNDING` akzeptiert nur `profile` (auch der Default bei fehlendem
Wert), `format-only` oder `repeat-schema`. Eine unbekannte Auswahl scheitert vor
dem Providerzugriff. Eine Abweichung benötigt einen ausdrücklich ausgewählten
Modellnamen und eine neue Capability-Probe; die bloße Übernahme des gespeicherten
Codingprofils ist dafür nicht zulässig. Nur das flüchtige Vergleichsprofil ändert
seine Schemaoption. Kontext, Output, Zählstrategie, Parallelität, Sampling und
Stopbedingungen bleiben erhalten. Es erfolgt kein Settings-Write.

Das native JSONL-Metadatum `model_profile.schema_grounding` weist die tatsächlich verwendete
Option aus. Alte Rohberichte werden nicht ergänzt oder umgeschrieben. Für alle
Modellaufrufe gelten weiterhin die vorhandenen Providerfreigaben und Credential-
Slots; diese Auswahl aktiviert weder einen Provider noch eine Ausführungsbefugnis.

Eingefrorenes Binary:
`target/reports/research-grounding-20260908/research-tests.exe`, SHA-256
`ffc946f3221ca28db45042ccefbf76b42c6281a78a8747b9e1104e2292caa3f9`.

Erster Vergleich: Google `gemma-4-26b-a4b-it`, `joint`, Auditfrage `1:1`, je ein
Fall in Reihenfolge B/A/A/B: B=`repeat-schema`, A=`format-only`. Der Aufruf lautet
`research-tests.exe --ignored research_approved_model_matrix --nocapture --test-threads=1`;
`A3_RESEARCH_EVAL_CASE=1:1` und `A3_RESEARCH_EVAL_REPETITIONS=1`. Derselbe native
explizite Provider-/Modellselektor und der vorhandene Settingskatalog wie zuvor
werden verwendet. Die public Fixture und die Wortprüfung v3 bleiben unverändert.

## Ergebnis des unveränderten Grounding-Vergleichs

16 reale Auditfälle über alle sieben freigegebenen Modelle sind abgeschlossen:
Google Gemma B/A/A/B, alle übrigen einmal B/A. Nur die vier Google-Gemma-Fälle
sind eine gegenbalancierte Reihe; die anderen Paare beweisen keinen allgemeinen
statistischen Vorteil. Alle vier lokalen Modelle liefen nacheinander.

| Modell | B: Abschluss / Wortprüfung | A: Abschluss / Wortprüfung | B / A Modellaufrufe |
| --- | --- | --- | --- |
| Google Gemma | 0/2 / 0/2 | 0/2 / 0/2 | 3,3 / 3,3 |
| Qwen 8k | 0/1 / 0/1 | 1/1 / 0/1 | 5 / 3 |
| Ornith 9b | 1/1 / 0/1 | 1/1 / 0/1 | 3 / 3 |
| Gemma lokal 12b | 1/1 / 0/1 | 1/1 / 0/1 | 3 / 4 |
| Granite 8b | 0/1 / 0/1 | 1/1 / 0/1 | 2 / 3 |
| Luna | 1/1 / 0/1 | 1/1 / 1/1 | 3 / 3 |
| Google Flash | 1/1 / 0/1 | 1/1 / 0/1 | 3 / 3 |

Die Schemawiederholung behebt Google Gemmas zweimaliges OutputTruncated in keinem
der beiden B-Fälle. Qwen liefert bei B vier gültige Belegbedarfe statt einer Antwort;
die Originalfenster sind fragmentierter. Der Core stoppt ohne identische Analyse
erneut auszuführen. Das entspricht dem kleineren tatsächlich verfügbaren Paket
von 1600 statt 3409 Bytes im 8k/2k-Profil, ist aber kein isolierter Beweis, dass
allein die Größe jede Modellentscheidung erklärt. Granite scheitert in B schon
an der Zulassung seiner initialen Fragen und deren Einzelrepair; gültiges JSON
oder ein bestandener Decoder allein autorisiert keinen Arbeitsvertrag.

Auch Lunas einziger bestandener Wortcheck ist **keine fachliche Abnahme**:
`write` steht lediglich in einer abschließenden Methodenliste, und die Antwort
behauptet weiterhin Speicherung durch `save_tasks`. Die vollständige konkrete
Methodenkette und korrekte Effekte sind damit nicht bewiesen. Andere Antworten
lassen `write` aus. Eine allgemeine Umstellung auf Schemawiederholung ist nicht
gerechtfertigt; gespeicherte Profile und Produktdefault bleiben unverändert.

## Streamdiagnose und konkrete Reparaturkorrektur

Ein zusätzlicher unveränderter Google-Gemma-Auditlauf instrumentiert ausschließlich
die durchgereichten neutralen Events. Das native Beobachtungsbinary
`research-observed-tests.exe` im selben Verzeichnis hat SHA-256
`4f3dea715a1f846eeafc431fd1ce481d9d92edbda591c931a98d8edb98a4acfe`.
`eval-1788872397380.jsonl` enthält:

| Schritt | Finish | sichtbare Bytes | Outputtokens | Prompttokens |
| --- | --- | ---: | ---: | ---: |
| Initialize | Stop | 591 | 169 | 779 |
| Analyze Q1 | OutputLimit | 20056 | 4096 | 1247 |
| Einzelrepair Q1 | OutputLimit | 6841 | 4096 | 1366 |

Die beiden Analyseausgaben enthalten im begrenzten Präfix jeweils zwei Vorkommen
von `"anchor_ref"` und eines von `"text"`. Das spricht gegen eine allein endlos
wachsende Quellenliste. Es sind lexikalische Beobachtungen, keine validierten
Feld-/Evidencezahlen oder inhaltlichen Aussagen. Der erste Capture endet nach
16384 Bytes, weshalb dessen JSON-Gültigkeit unbekannt bleibt; der zweite ist
vollständig erfasst und ungültiges JSON. Beide werden unverändert verworfen.

Der Beobachter hält höchstens 16 KiB flüchtiges Präfix und 48 reine Metadatensätze.
Er verändert weder Requests, Events, Fehler, Finishgründe, Deadline noch Cancellation;
unvollständiger Stream-Drop erfindet keine Completion. Präfixe, Texte und Werte
werden nicht gespeichert. Fehlende Provider-Usage bleibt unbekannt statt 0.
Ein Stream ohne Completion erzeugt keinen solchen Datensatz; die schon bestehende
typisierte Fehlerdiagnose bleibt dafür zuständig.

Im tatsächlichen Reparaturpfad ging die vorhandene konkrete Anweisung zur kürzeren
Neuausgabe verloren: Phasenspezifische Hinweise ersetzten den allgemeinen
Trunkierungshinweis durch den Phasenvertrag und einen Fehlercode. Die Korrektur
erhält diesen Vertrag und ergänzt **nur bei Truncated** die Anweisung, ein
wesentlich kürzeres vollständiges Objekt statt einer Fortsetzung auszugeben,
Wiederholungen und kopierte Originalauszüge zu vermeiden. Historische Hinweise,
andere Fehler, Decoder, Quellenzulassung und sämtliche Budgets bleiben erhalten.

Alle drei neuen Regressionen waren vorher rot und bestehen nach der Korrektur:
sechs Phasen mit höchstens 768 Repairbytes sowie echte Ask-/Plan-/Agent-Vorbereitung
über Index, Reader und libSQL. Der gesamte aktuelle Originaltext bleibt im Repair
bytegleich, ein erfolgreicher Repair erzeugt keine Zusatzreads, und zwei Abbrüche
können weder einen Abschluss noch ein Analyse-Receipt oder dritten Versuch erzeugen.

Das Nachtestbinary `research-short-repair-tests.exe` hat SHA-256
`2a909ab5f87c73717c75d3c0bf6cf64af3febefc6fe907efc42b47946c5e3f44`.
Es verwendet für alle sieben Modelle `joint` und `format-only`, jeweils Audit
`1:1` und CSV-Plan `3:0`. Dieser Nachtest ist keine vollständige neue AB/BA-Reihe.

## Ergebnisse der 14 Nachtests mit Reparaturkorrektur

| Modell | Audit: Abschluss / Wortprüfung | Plan: Abschluss / Wortprüfung | Audit / Plan Aufrufe |
| --- | --- | --- | --- |
| Google Gemma | nein / nein | nein / nein | 3 / 2 |
| Qwen 8k | ja / nein | ja / ja | 3 / 3 |
| Ornith 9b | ja / nein | ja / ja | 3 / 3 |
| Gemma lokal 12b | ja / nein | ja / ja | 4 / 3 |
| Granite 8b | ja / nein | ja / ja | 3 / 3 |
| Luna | ja / ja | ja / ja | 4 / 3 |
| Google Flash | ja / ja | ja / ja | 3 / 3 |

Insgesamt: 12/14 Abschlüsse und 8/14 bestandene Wortchecks bei 43 Modellaufrufen,
190370 kumulierten Kontext-UTF-8-Bytes und zwölf adaptiven Reads ohne wiederholten
identischen Read. Kontextbytes sind weder Providerinputtokens noch ein Kontextlimit.
Die vier lokalen Modelle liefen strikt seriell, jeweils Audit vor Plan.

Google Gemma erreicht weiterhin bei beiden Aufgaben in Analyze und Einzelrepair
4096 Outputtokens mit OutputLimit. Beim Audit bleiben 20056 sichtbare Analysebytes;
der Repair liefert jetzt 8481 statt zuvor 6841 Bytes. Beim Plan sind es 20137 und
11191 Bytes. Alle vier beobachteten Präfixe enthalten zwei `"anchor_ref"`- und ein
`"text"`-Vorkommen. Das ist kein positiver Nutzenbeleg für dieses Modell und keine
Rechtfertigung für weitere Wiederholungen. Der erfolgreiche Offline-Nachweis belegt
den vorher verlorenen Fehlerhinweis, nicht die Behebbarkeit jeder Modelltrunkierung.

Lunas zusätzlicher Auditaufruf repariert ungültige Initialize-Struktur bei normalem
Stop; es gab dort kein OutputLimit. Die beiden neu bestandenen externen Audit-
Wortchecks dürfen daher nicht der Trunkierungskorrektur zugeschrieben werden.
Es bleibt außerdem eine Paar-/Einzelbeobachtung ohne statistische Robustheitsfreigabe.

### Fachliche Gegenprüfung an den unveränderten Originalen

Die sechs abgeschlossenen Planantworten und die beiden externen Audit-Worttreffer
wurden gegen `fixtures/research-eval-v1` gelesen. Kein Gesamtfreigabenachweis:

- `JsonStorage.save_tasks` und `SQLiteStorage.save_tasks` geben lediglich ein Tupel
  zurück; sie persistieren nichts. Qwen, Ornith, Luna, Flash und lokales Gemma
  behaupten weiterhin Speicherung oder verlangen entsprechende Backend-Nachweise,
  obwohl diese Implementierung nicht vorliegt. Die Referenz auf den Manager-Aufruf
  allein beweist keinen Seiteneffekt des dynamischen Callees.
- Granite erkennt im Plan die Tupelrückgabe, nennt im Änderungsentwurf aber zweimal
  `import` statt des angeforderten `import-csv`. Der richtige Begriff im Testabschnitt
  lässt die unveränderte Wortprüfung bestehen, heilt den falschen Entwurf jedoch nicht.
- Orniths Plan widerspricht sich bei der vorhandenen Projektvalidierung und führt
  zusätzliche Storage-Pfadarbeiten ein. Lokales Gemma beschreibt beim Test einer
  fehlenden Datei ausdrücklich einen existierenden Pfad. Flashs UTF-8-Beispiel enthält
  ein unquotiertes Komma im Titel. Diese Tests sind keine ausführbaren Abnahmen.
- Lunas Auditantwort nennt `write` nur in einer Methodenliste, während Flash die
  konkrete Kette bis `output.write` nennt. Beide liefern zwei weitgehend doppelte
  Teilantworten; die zweite behauptet jeweils bereits erfolgte Speicherung.

Damit bleiben korrekte Effektaussagen, unveränderte Auftragsliterale, konsolidierte
Antworten und widerspruchsfreie ausführbare Testpläne offene Produktarbeit. Mehr
Anker oder Selbsteinschätzung des Modells sind kein Ersatz für diese Abnahme.

## Abnahme

Die neue geschlossene Auswahl und vollständiger Erhalt aller übrigen
Profileinstellungen bestehen im Unit-Test. Die fünf bestehenden echten
Providerpaket-/Repair-/Phasen-/OutputLimit-Verträge bestehen ebenfalls.
Die zwei Beobachtertests prüfen unveränderte Eventreihenfolge einschließlich
OutputLimit und Cancellation, Retention, UTF-8-Bounds, Redaction und vollständigen
gegenüber abgeschnittenem Capture. Der Zwischenstand mit Beobachter besteht
Clippy und das vollständige Workspace-Gate. Zuvor scheiterte der bestehende
`catalog_is_unlimited_searchable_and_cursor_paged_without_deleting_private_data`
im Gesamtlauf mit Kindprozess-Exit 101; der unveränderte gezielte Nachtest und
das folgende vollständige Gate bestehen. Keine Assertion oder Retryregel wurde
geändert; die Ursache dieses einzelnen Zwischenfehlers ist nicht nachgewiesen.

Finale lokale Gates nach der Reparaturkorrektur sind abgeschlossen:

- `cargo fmt --all -- --check`: Exit 0.
- `cargo test -p a3-desktop --all-features --lib research_truncated_ --offline --locked --jobs 2 -- --test-threads=1`:
  drei Regressionen Rot vor / Grün nach der Korrektur.
- Geschlossene Grounding-Auswahl: ein Test; Streamdiagnose: zwei neue Tests grün.
- `cargo test -p a3-desktop --all-features --lib provider_tests --offline --locked --jobs 2 -- --test-threads=1`:
  fünf bestehende Providerverträge grün.
- `cargo clippy --workspace --all-targets --all-features --offline --locked --jobs 2 -- -D warnings`:
  Exit 0; `target/reports/research-truncation-repair-clippy.log`.
- `cargo test --workspace --all-features --offline --locked --jobs 2 -- --test-threads=1`:
  Exit 0, 1297 bestanden, 19 explizite Tests ignoriert, null Fehler über 84 Suites;
  `target/reports/research-truncation-repair-workspace.log`.
- `pnpm check:links` und `git diff --check`: Exit 0. Der vorhandene Enginehinweis
  Node 25.6.1 statt vorgeschrieben 24.14.0 bleibt; kein Frontendcode wurde geändert.

Cargo lief mit `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0` und
`CARGO_PROFILE_TEST_DEBUG=0`. Kein Target- oder Rohbericht wurde gelöscht.
Der native Settingskatalog ist vor und nach allen 31 Fällen bytegleich:
SHA-256 `aecc7ef93abe29daa97747f2216e87f49aef99ce9490942fc11ee5c65c09ab32`.
Nur öffentliche Fixtureinhalte liefen über zuvor freigegebene native Providerpfade.
Die Nachweise umfassen keine neue mutierende SourceGuided-Liveausführung,
UI-Abnahme oder plattformübergreifende Releasefreigabe. Das Gesamtziel bleibt offen.


## Unveränderte native Rohberichte

Alle 31 JSONL-Dateien liegen unter `target/research-eval/`. G bezeichnet den
Grounding-Vergleich, O die unveränderte Streambeobachtung, R den Reparaturnachtest.
A=`FormatFieldOnly`, B=`RepeatSchemaInPrompt`; Fall ist `family:variant`.
Die Dateien sind lokale Messartefakte, keine eingecheckten oder nachträglich
bereinigten Modellantworten. Hashes beziehen sich auf ihre ursprünglichen Bytes.

| Stufe | Modell | Schema | Fall | Datei | SHA-256 |
| --- | --- | --- | --- | --- | --- |
| G | gemma-4-26b-a4b-it | B | 1:1 | `eval-1788871737521.jsonl` | `933139f282a9e51da575f7cf28b377f29a53cd23e5a293509c5da439805e8b46` |
| G | gemma-4-26b-a4b-it | A | 1:1 | `eval-1788871921725.jsonl` | `1b15af7d6ca823547247fe4011dbf5a13ddb624f4ca8558d6ccd954c6f4760e3` |
| G | qwen38-8k:latest | B | 1:1 | `eval-1788872068110.jsonl` | `749d042d297155e61394d47579531a07b2eb9f99998772cae200592e55a0e989` |
| G | qwen38-8k:latest | A | 1:1 | `eval-1788872096957.jsonl` | `75dd2fa5d9ad780309c515fdabd28de3218260400496ef5df9ca49e55e5c4d85` |
| G | gemma-4-26b-a4b-it | A | 1:1 | `eval-1788872104780.jsonl` | `25125cecba76fa12e2854f098025e1dfad65a36de368047a5e51f33f4e2e517f` |
| G | ornith-1.5:9b | B | 1:1 | `eval-1788872123277.jsonl` | `4b399943a2aa14fbaaf7a76a11e57843a8348f0a3b8b38eb01038d716201370b` |
| G | ornith-1.5:9b | A | 1:1 | `eval-1788872142624.jsonl` | `326ee557c7887864d81609e5dc2c8f4c407aa452eb749224283e099a56af131e` |
| G | gemma4:12b | B | 1:1 | `eval-1788872165213.jsonl` | `40b2cb7fb396d99f8d40ce7c587f756d7892eae993a7b7625cc969ffe453e521` |
| G | gemma4:12b | A | 1:1 | `eval-1788872194466.jsonl` | `c9974299750433ee173d45a0a6040147818acc22c8e3da598ae8652b4b8c7bad` |
| G | granite4.2:8b | B | 1:1 | `eval-1788872221751.jsonl` | `5c08006eb628eef3653948c33f4f0d577dea3079a02951a983fb2149dfb45c6b` |
| G | granite4.2:8b | A | 1:1 | `eval-1788872233953.jsonl` | `b5250488ebc183b1561dc56893e4342292c77cce5bd51b121bd98cbb65785f4b` |
| G | gpt-5.6-luna | B | 1:1 | `eval-1788872258197.jsonl` | `bf1c05df90ab59eb68768d268ae635b9fb6a4264429a20b38cb6eb1246fb6f80` |
| G | gpt-5.6-luna | A | 1:1 | `eval-1788872276814.jsonl` | `7ee65a0248ad939035e761161c4e73ca46869804ab06853cb2b650cf9c579689` |
| G | gemma-4-26b-a4b-it | B | 1:1 | `eval-1788872287351.jsonl` | `ba11206e9e12581ae213873cca30edbffe10bd9358d0378b180394caca7d7060` |
| G | gemini-3.8-flash | B | 1:1 | `eval-1788872294572.jsonl` | `12dbb1ebb777c72c853337e7b44f85bd43a9fd0410f58f9697d7d6ce0b3ed4e3` |
| G | gemini-3.8-flash | A | 1:1 | `eval-1788872318395.jsonl` | `5ebbc8b0ad0d584c7f3df972a90e013a389d3f1db4712ce1393e203b5558cd25` |
| O | gemma-4-26b-a4b-it | A | 1:1 | `eval-1788872397380.jsonl` | `b5e9ce02cfcaeeff6ef3a620b18cf6a77541e8508b2ba5e8ad9add22dba6bd24` |
| R | gemma-4-26b-a4b-it | A | 1:1 | `eval-1788872998508.jsonl` | `b8b32bef5c5e156f093869e4b9da077aa74b2f873975afe19a573934a3f691ea` |
| R | qwen38-8k:latest | A | 1:1 | `eval-1788873017529.jsonl` | `b0812591f4586c4199ff5623ab31111de00fcaf892b5f9cb482159e1bdcb2ce7` |
| R | qwen38-8k:latest | A | 3:0 | `eval-1788873044710.jsonl` | `61ac60b5b2b21c4ec8cd69705ab179cb65c38e1d0adc34696243b84e0763118f` |
| R | gpt-5.6-luna | A | 1:1 | `eval-1788873055848.jsonl` | `0f76059945a2db2b4b1ce1e7fb27426b15cdc4594c9a3e41abba656f07323446` |
| R | gpt-5.6-luna | A | 3:0 | `eval-1788873078109.jsonl` | `cc5fd35d430ccd01d299acaa65d9f485b7296e126fce396bb5960366e3a1ccb8` |
| R | ornith-1.5:9b | A | 1:1 | `eval-1788873081147.jsonl` | `1b648c58c6b6024fd13fb719c5d89667849b321c795191453f8567cf8a8462eb` |
| R | ornith-1.5:9b | A | 3:0 | `eval-1788873101527.jsonl` | `3f525ce82086d401dd9121bee912b9f40844d716b7376ca4638b109fae2a388e` |
| R | gemini-3.8-flash | A | 1:1 | `eval-1788873106222.jsonl` | `57b32b427d3f8da003fb79bc436cd69ebe157317698021fcc8d0c955a0584a04` |
| R | gemini-3.8-flash | A | 3:0 | `eval-1788873128975.jsonl` | `0c3518c713224874eb373bb2bb6141ade5fc61d94faf60a9f6e7b5e2f2147f41` |
| R | gemma4:12b | A | 1:1 | `eval-1788873133036.jsonl` | `a0e435d0949852d1383c51a168375838b677e8ea72079a8c0312ab76b2359b87` |
| R | gemma4:12b | A | 3:0 | `eval-1788873165543.jsonl` | `cac23ef948a5ca5eb2bccd5bbf6f0e99ad0c47688e1d106d3fdb40bcda79d17f` |
| R | gemma-4-26b-a4b-it | A | 3:0 | `eval-1788873179424.jsonl` | `8f83846a06442023e1a5b0af614e70287d8c03b7675b20be8f621f74ace34326` |
| R | granite4.2:8b | A | 1:1 | `eval-1788873200268.jsonl` | `134963d95f1c0443d42ce17e27278aa70195a4b0880abef114ecfe24da015e0e` |
| R | granite4.2:8b | A | 3:0 | `eval-1788873217148.jsonl` | `ce192fce669d19bdee1f3eb2705807043c6d9c8656740e26ff81074c52982441` |
