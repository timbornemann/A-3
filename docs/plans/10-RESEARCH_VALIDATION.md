# Plan 10: Verifikationsprotokoll

Stand: 2026-09-06. Implementierung gegenüber `31e9db7`.
Die abschließenden 60er-Wiederholungsläufe sind ausgewertet; die inhaltliche lokale
Praxisabnahme bleibt wegen der unten dokumentierten Gegenbeispiele offen.
Dieses Protokoll trennt Steuerungsabschluss, notwendige Begriffe und inhaltliche Richtigkeit.

## Methode

Das [versionierte Fixture](../../fixtures/research-eval-v1/README.md) läuft durch den
echten Researcher, Fast Index, Safe Reader, libSQL und den freigegebenen Provider.
Kein Ausführen der Beispielanwendung, kein Ändern der Originaldateien. Vier Familien,
drei feste Formulierungen, fünf Wiederholungen ergeben 60 Fälle. Die Baseline verwendet
denselben eingefrorenen Testadapter, dasselbe Modell und dieselben Profilwerte.

Lokal: installiertes `ornith-1.5:9b`, 16.384 Kontext / 4.096 Output, Temperatur 0,
Parallelität 1, konservative UTF-8-Zählung. Konfiguriert: OpenAI `gpt-5.6-luna`,
16.384 Kontext / 2.048 Output, unverändertes read-only geladenes App-Profil.
Keine Installation, kein Modelldownload, keine neue Providerfreigabe oder Profiländerung.

Zeitwerte sind Summen der einzelnen Falllaufzeiten auf diesem Host. Sie enthalten weder
den Capability-Probe noch den Prozessstart. Nebenläufige Kompilierung kann sie beeinflussen.
Kontextbytes zählen den Transcript, nicht System-/Schemabytes oder abgerechnete Tokens.
Unterschiedliche Modelle sind kein isolierter Harness-Geschwindigkeitsvergleich.

## Vollständige frühere Messungen

Diese Stände wurden durch weitere Korrekturen abgelöst; sie sind keine Endabnahme.

| Stand / Modell | Fälle | Abgeschlossen | Begriffrubrik erfüllt | Modellaufrufe | Kontextbytes | Zeit (ms) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Baseline `31e9db7`, ornith 9B | 60 | 37 | 34 | 248 | 696.710 | 1.995.937 |
| ADR-0054, ornith 9B | 60 | 55 | 45 | 280 | 903.145 | 1.680.909 |
| ADR-0054, Luna | 60 | 57 | 56 | 275 | 882.998 | 1.296.720 |
| ADR-0056, ornith 9B, Smoke | 12 | 12 | 12 | 56 | 187.320 | 358.428 |
| ADR-0056, Luna, Smoke | 12 | 12 | 12 | 56 | 185.959 | 243.625 |

Rohberichte (JSONL unter dem jeweiligen Checkout, bewusst nicht als private Logdaten committed):

- Baseline: `target/research-baseline/target/research-eval/eval-1788686690039.jsonl`.
- ADR-0054 lokal: `target/research-eval/eval-1788689287786.jsonl`.
- ADR-0054 Luna: `target/research-eval/eval-1788689616101.jsonl`.
- ADR-0056 lokal, 12 Fälle: `target/research-eval/eval-1788691277199.jsonl`.
- ADR-0056 Luna, 12 Fälle: `target/research-eval/eval-1788691281757.jsonl`.

Die Baseline hat keine durable adaptive-Read-Metrik: fehlende Felder bedeuten unbekannt,
nicht null. Beide ADR-0056-Smokes hatten null adaptive Reads und keinen Nutzerhalt.
Der alte lokale ADR-0054-Lauf hatte einen zusätzlichen nativen Crash-Teilbericht
`eval-1788689242453.jsonl` mit einem Fall; er zählt nicht als vollständige Matrix und
wird nicht verschwiegen. Der anschließende vollständige Versuch hatte fünf blockierte
Storage-Fälle, fünf fehlende Audit-Schreibbegriffe und fünf unzureichende CSV-Pläne.

## Abgeschlossener Luna-Langzeittest

ADR-0056 mit vollständigen Originalpaketen: **60/60 abgeschlossen, 60/60 Begriffrubrik**,
null Nutzerhalte, null adaptive Reads, null identische adaptive Wiederholungen.
280 Modellaufrufe, 934.818 Kontextbytes, 1.332.441 ms Falllaufzeit.
Bericht: `target/research-eval/eval-1788691540088.jsonl`.
Alle 15 CSV-Planfälle benötigten jeweils drei Modellaufrufe und keinen adaptiven Read.

Das eingefrorene Binary dieses Langzeittests enthält noch nicht den nachfolgenden
Domain-Typabgleich und das Entfernen ungelöster Plan-Navigationshinweise.
Diese Korrekturen erhalten gesonderte aktuelle Regressionen und Live-Nachtests;
die verschiedenen Stände werden nicht als ein einziger Lauf ausgegeben.

Nach beiden Korrekturen besteht der aktuelle Luna-Nachtest ebenfalls: 12/12 abgeschlossen,
12/12 Begriffrubrik, null adaptive Reads/Nutzerhalte, 56 Modellaufrufe, 185.308 Kontextbytes,
263.355 ms. Bericht: `target/research-eval/eval-1788692907171.jsonl`.
Die drei CSV-Pläne enthalten Umsetzung, Fehlergrenzen und Tests; die irreführende
Verzeichnis-Suchpflicht erscheint nicht mehr als Bestandsvoraussetzung.
Zusätzlich besteht `research_configured_model_coherent_smoke` mit tatsächlichen
Ask-, Plan- und Agent-Vorbereitungsläufen über das zusammenhängende Mehrdateiprojekt
(drei Modellaufrufe pro Modus, kein Nutzerhalt, 49,55 s gesamter Testprozess).
Das nachgetestete Binary hat SHA-256
`54dce9fb577fc39d5f80f357cc46b53127cc06481f77e1b39aabcb24509402c1`.

## Abgeschlossener lokaler Vorher-/Nachher-Vergleich

Der vollständige ADR-0056-Lauf mit `ornith-1.5:9b` hat **60/60 Abschlüsse und 60/60
Begrifftreffer**, null Nutzerhalte, null adaptive Reads und null identische adaptive
Wiederholungen. Bericht: `target/research-eval/eval-1788691760808.jsonl`.
276 Modellaufrufe, 922.116 Kontextbytes und 1.708.552 ms Falllaufzeit.
Er enthält den Schutz neuer externer Lesefronten, aber noch nicht die späteren
Domain-/Plan-Navigationskorrekturen. Deren lokaler Nachtest bleibt gesondert.

| Familie (je 15 Fälle) | Abschlüsse vorher → nachher | Rubrik vorher → nachher | Zeit vorher → nachher (ms) |
| --- | --- | --- | --- |
| Storage | 15 → 15 | 15 → 15 | 275.267 → 424.185 |
| Audit | 8 → 15 | 5 → 15 | 389.263 → 480.507 |
| REST | 14 → 15 | 14 → 15 | 286.388 → 459.274 |
| CSV-Plan | 0 → 15 | 0 → 15 | 1.045.019 → 344.586 |

Damit: 37 → 60 Abschlüsse und 34 → 60 Rubrikerfolge unter demselben äußeren Profil.
Die gemessene Gesamtzeit sinkt um rund 14,4 %, allerdings steigen Modellaufrufe von
248 auf 276 und Kontextbytes von 696.710 auf 922.116. Die ausführlicher zerlegten
Ask-Aufgaben dauern länger; insbesondere die vorher erfolglose CSV-Planung wird kürzer.
Dies ist keine allgemeine Geschwindigkeitsgarantie und keine vollständige Inhaltsabnahme.

## Befunde und Korrekturen

- Der lokale Nachtest `eval-1788693531749.jsonl` (nach Domain-/Navigationskorrektur)
  schloss nur 11/12 Fälle ab, 10/12 bestanden die Begriffrubrik. CSV-Variante 1
  entwarf fälschlich Positionsargumente statt des geforderten CSV-Einlesens; Variante 2
  lieferte leeren Designfortschritt und löste zwölf adaptive Zugriffe ohne Abschluss aus.
  Dieser Fehlschlag wird nicht durch den früheren grünen 60er-Lauf ersetzt.
  [ADR-0057](../adrs/0057-leerer-entwurf-ist-kein-rechercheauftrag.md) schließt den leeren
  Designfortschritt im unabhängigen Decoder aus. Zwei reale Controllerregressionen
  für Plan/Agent waren rot und bestehen nach Einzelrepair ohne adaptive Reads.
- Drei getrennte Gemma-12B-Diagnosen (`eval-1788693922294.jsonl`,
  `eval-1788693977121.jsonl`, `eval-1788694017527.jsonl`) bestanden nicht:
  Storage/CSV scheiterten an Quellenabdeckung, Audit ließ den tatsächlichen `write`-Aufruf
  aus. Sie sind keine vollständige Modellmatrix. Die Analyse fand einen konkreten
  Core-Fehler im Pfadvergleich: ein vollständiger relativer Pfad wurde nur mit einem
  zusätzlichen führenden Slash verglichen. So konnte die eigene Quellenpflicht einer
  früheren Repositoryfrage fehlen und erst in einer späteren fremden Frage auffallen.
  Der neue Negativtest reproduziert dies; exakte relative Pfade und echte Pfadsegment-
  Suffixe werden nun gleichermaßen erkannt, fremde Dateipräfixe weiterhin ausgeschlossen.
- Nach ADR-0057 bestanden die gesonderten 12er-Nachtests erneut ohne adaptive Reads
  oder Nutzerhalt: ornith `eval-1788694670460.jsonl`, 57 Aufrufe, 190.507 Bytes,
  365.490 ms; Luna `eval-1788694675209.jsonl`, 55 Aufrufe, 182.601 Bytes, 254.312 ms.
  Luna bestand zusätzlich den kohärenten Ask-/Plan-/Agent-Smoke (59,65 s Testprozess).
  Diese Binaries enthielten noch nicht den anschließend gefundenen relativen Pfadfix.
- Nach dem relativen Pfadfix: Luna `eval-1788695097965.jsonl` bestand 12/12
  (55 Aufrufe, 182.598 Bytes, 252.402 ms) und den kohärenten Dreimodus-Smoke
  (59,27 s). Gemmas zuvor blockierter Storage-Fall bestand in
  `eval-1788695096465.jsonl` mit sieben Aufrufen und null adaptiven Reads.
  Gemmas CSV-Fall `eval-1788695162862.jsonl` scheiterte weiter an Quellenzuordnung.
  Ornith `eval-1788695189787.jsonl` bestand 11/12; die nun frühzeitige Quellenprüfung
  wies Storage-Variante 2 mit fehlenden Pflichtquellen korrekt ab (null adaptive Reads).
- Deshalb erhält ausschließlich der Quellenabdeckungs-Einzelrepair konkrete aktuelle
  E-Fenstergruppen pro benötigter Originalrevision. Diese Auswahl kommt aus Core-Zustand
  und tatsächlicher Auslieferung, nicht aus dem Modelltext. Ungelieferte oder fremde
  Quellen werden nicht ergänzt; der bestehende 768-Byte-/Einzelrepairrahmen bleibt.
  Die echte Plan-/Agent-Controllerregression war rot und besteht; ein unabhängiger Test
  prüft Originalgruppen, fehlende Anker, fremde Revisionen und das Größenlimit.
- Zu spät geprüfte Plan-Quellenabdeckung: Q1 muss alle benannten Originale belegen,
  bevor belegfreie Designphasen starten. Regression zunächst rot, danach grün.
- Unnötige Funktionsfragmentierung: Gemeinsam passende vollständige Originale behalten
  Imports und Initialisierung. Exakte Header-/UTF-8-Kosten, Read-Quittungen, expliziter
  Fokus, Überlauf und eine neue externe Lesefront werden unabhängig geprüft.
- Domain-Grenze: Repositoryinterpretation oder negative Suche dürfen weder direkt noch
  über Reopen einen Entwurfsauftrag abschließen. Gegenseitige Ergebnisartprüfung;
  neue Domain-Regression zunächst rot, danach grün.
- Irreführende Plannavigation: Nicht aufgelöste Namen künftiger Funktionen oder externer
  APIs werden im festen Core-Plan nicht mehr zusätzlich als Verzeichnis-Suchpflicht
  injiziert. Der vollständige Originalauftrag bleibt unverändert. Der zunächst rote
  Kontexttest besteht; freie Ask-Verträge und historische Navigation bleiben unverändert.
- Nativer Absturz: Der symbolisierte libSQL-Fehler und die gepinnte Quelle zeigen eine
  doppelte Connection-Freigabe. [ADR-0055](../adrs/0055-libsql-einmalige-verbindungsfreigabe.md)
  dokumentiert den minimalen Patch der vorhandenen Version. 1.024 native Lebensdauerzyklen
  ohne Crash-Retry bestehen. Der Lebensdauertest allein reproduzierte den vorherigen
  allocatorabhängigen Fehler nicht; er ersetzt nicht die Quell- und Crashanalyse.
- Schon zuvor korrigiert: Satzzeichen in Pfaden, unveränderliche Fragen/Originalziele,
  tatsächliche statt nur gecachte E-Fenster, persistierte Zugriffsschlüssel, phasengerechter
  Einzelrepair, vollständige Designübergabe, Replan-Originalmarker, V36/V37-Rollback und
  Budgeterhalt bei Fehlern/Cancellation. Die Einzelbelege stehen in [Plan 10](10-RESEARCH_WORK_STATE.md).

## Enge Kontexte: ADR-0058

Der freigegebene Qwen-8k-Einzelfall `eval-1788695666672.jsonl` stoppte vor der
Bestandsanalyse an ContextLimit. Ein neuer realer Git-/Index-/Reader-/Store-Vertrag
mit exakt 8.192 Kontext, 2.048 Output und konservativer UTF-8-Zählung reproduzierte
den Fehler. Zwischenkorrekturen beseitigten zunächst nur die Eintrittssperre,
verdrängten aber weiterhin notwendige Methodenkörper. Auch diese Versuche waren rot.

Die vollständige Korrektur reduziert redundante Systeminstruktionen, partitioniert
die Arbeitsansicht und liefert Originalfenster vor abgeleiteter Navigation.
Der unveränderte Mehrdateivertrag besteht jetzt für Ask, Plan und Agent-Vorbereitung:
je drei Modellstubaufrufe, alle fünf Originalmethodenkörper gemeinsam in der
Bestandsanalyse, unveränderte Dateien und Ziele. Die berechneten Evidence-Limits
sind 2.213 Bytes für Ask und 2.212 für Plan/Agent. Design verwendet anschließend
die bindenden Ergebnisse und benötigt nicht nochmals alle Originalkörper.

Die Systemtextgrößen ohne optionales Slash-Command-Profil, gemessen aus den
unveränderten ausgegebenen UTF-8-Strings vor/nach der Korrektur:

| Phase | Ask vorher → nachher (Bytes) | Plan vorher → nachher (Bytes) |
| --- | ---: | ---: |
| Initialize | 1.410 → 568 | 1.410 → 568 |
| Analyze | 2.178 → 641 | 2.178 → 641 |
| SummarizeOriginals | 1.586 → 649 | 1.586 → 649 |
| Design | 1.520 → 612 | 2.072 → 651 |
| Finalize | 1.157 → 560 | 1.215 → 560 |

Schema, Decoder, Quellenprüfung und äußere Budgets bleiben unverändert. Weniger
Systembytes sind kein Nachweis schnellerer oder richtiger Modellantworten.
Das für die neuen Live-Abnahmen eingefrorene Binary hat SHA-256
`f4893df466749f4841e7262a5011c149840492afb499499e1d2ef2ac8ddef353`.

Der reale 8k-Nachtest ist keine allgemeine Modellfreigabe: Qwen
`eval-1788696958896.jsonl` schloss 7/12 Fälle ab, 6/12 erfüllten die Begriffrubrik.
51 Aufrufe, 122.198 Kontextbytes, 592.395 ms, fünf Nutzerhalte, null adaptive Reads
und null identische adaptive Wiederholungen. Zwei Storage-Fälle scheiterten nach
dem Einzelrepair an Quellenabdeckung; die öffentliche Statusnotiz nennt im aktuellen
Paket fehlende `build_parser`-/`create_storage`-Definitionen. Ein abgeschlossener
Audit-Fall nennt den abschließenden `write`-Aufruf nicht. Alle drei CSV-Fälle erreichen
Q2, aber deren längere bindende Entwürfe passen für Q3 nicht mehr in das konservativ
berechnete 8k-Paket. Der Eintrittsfehler ist behoben, diese späteren Grenzen nicht.
Die vollständigen Entscheidungen bleiben gespeichert; sie werden weder still gekürzt
noch als erfolgreiche Pläne ausgegeben. Der Offlinevertrag mit kurzen Entwürfen
beweist deshalb ausdrücklich nicht, dass beliebige längere Entwürfe in 8k passen.

Der zusätzliche Gemma-12B-Einzelfall `eval-1788697559664.jsonl` scheitert weiterhin
an der ursprünglichen CSV-Quellenzuordnung: zwei Aufrufe, 7.742 Kontextbytes,
17.786 ms, ein Nutzerhalt, null adaptive Reads. Er ist kein vollständiger Modelltest.
Die unveränderte Quellenprüfung bleibt aktiv; ein größerer Modellname ersetzt keine
erfolgreiche strukturierte Antwort.

## ADR-0058-Langzeittest und Ankerdiagnose

Luna `eval-1788696957554.jsonl`: 60 Fälle, 59 Abschlüsse und Rubrikerfolge,
284 Aufrufe, 955.164 Kontextbytes, 1.370.207 ms, ein Nutzerhalt, drei adaptive Reads
und keine identische adaptive Wiederholung. REST-Variante 2, Wiederholung 3,
meldete in Q1 fehlende zulässige Originalanker; Q2/Q3 beantworteten die Methodenfolge
und Fehlerumwandlung. Der Core erklärte Q1 nicht aufgrund dieser fremden Ergebnisse
automatisch für erledigt. Alle 15 CSV-Fälle schlossen ab.

Der gesonderte unveränderte REST-Nachtest `eval-1788698345809.jsonl` bestand fünfmal
(20 Aufrufe, 54.200 Bytes, 87.441 ms, null adaptive Reads/Nutzerhalte); der anschließende
kohärente Ask-/Plan-/Agent-Smoke bestand mit je drei Aufrufen (52,27 s Testprozess).
Diese Erfolge entfernen den seltenen Fehlschlag nicht aus dem 60er-Bericht.

Der Ornith-ADR-0058-Lauf `eval-1788697584700.jsonl` wurde nach 32 protokollierten
Fällen gezielt für die Diagnose beendet: 26 Abschlüsse/Rubrikerfolge, sechs Fehler,
173 Aufrufe, 570.219 Bytes, 895.037 ms, null adaptive Reads. Die tatsächlich laufenden
beiden eigenen Testprozesse wurden anhand ihres vollständigen Binarypfads und
Testfilters überprüft und beendet. Keine Fixture-Datei oder Benutzerdatei wurde gelöscht.
Dieser erhaltene Teilbericht ist ausdrücklich keine vollständige lokale Abnahme.

Der neue bounded-shape-Testbericht enthält nur Größen, geschlossene Prüfbooleans und
numerische Fragepositionen, niemals Antworttext oder Providerpayloads. Ornith-Diagnose
`eval-1788698508191.jsonl` weist mehrfach identische gültige E-Anker als konkrete
InvalidValue-Ursache nach. Der Fall schloss mit einem Repair und acht Aufrufen ab.
Das Diagnosebinary (unveränderte ADR-0058-Produktion) hat SHA-256
`751d85479a9e56c72dded0746cf4a3b76e4034f554fe57a52c14bc9e6d4d5ed3`.

[ADR-0059](../adrs/0059-idempotente-originalanker-in-rechercheergebnissen.md)
beseitigt diesen Schema-/Decoderwiderspruch durch eine idempotente Ankerliste.
Die Decoderregression wurde mit InvalidValue rot; die echte Controllerregression
mit identischen gültigen Ankern blockierte ebenfalls. Das Eingabelimit gilt vor
Kanonisierung, jeder Anker wird einzeln validiert und aktuelle Originalzulassung
bleibt unabhängig. Die Korrektur erzeugt keine zusätzlichen Quellen oder Repairs.
Beide Regressionen bestehen nach der Korrektur; der Controllervertrag prüft Ask,
Plan und Agent mit jeweils drei Aufrufen, null adaptiven Reads und unveränderten
Originalen. Die vollständige Recherchegruppe besteht mit 83 Tests plus drei
ausdrücklich ignorierten Live-Tests, die Application-Gruppe mit 35 Tests.

Die erneuten vollständigen Modellmatrizen verwenden das eingefrorene ADR-0059-Binary
mit SHA-256 `59aebd62d6e9a8b1d0743331bc4a138e7f12283d3ed25ccfec32440b55c09f33`.

## Abschließende ADR-0059-Modellmatrizen

Beide Berichte enthalten alle vier Familien mit jeweils drei Formulierungen und fünf
Wiederholungen. Sämtliche Originaldateien bleiben bytegleich. Jeder der 15 CSV-Planfälle
benötigt pro Modell genau drei Aufrufe. Beide Modelle haben null Nutzerhalte, null
adaptive Reads und null identische adaptive Wiederholungen. Das ist ein Nachweis für
den Ablauf und die unveränderte notwendige Begriffrubrik, nicht für allgemeine Wahrheit.

| Modell | Fälle | Abgeschlossen | Begriffrubrik erfüllt | Modellaufrufe | Kontextbytes | Zeit (ms) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| ornith-1.5:9b | 60 | 60 | 60 | 276 | 923.353 | 1.602.396 |
| gpt-5.6-luna | 60 | 60 | 60 | 283 | 944.150 | 1.519.190 |

Rohberichte und SHA-256:

- `target/research-eval/eval-1788698871756.jsonl` (Ornith):
  `7b0fbb7be4de8f281703a0323155195a1fa220d4af29db54b6c0aaa45b513f5e`.
- `target/research-eval/eval-1788698878383.jsonl` (Luna):
  `6384c331fce6533d6750f9fd84e84a36827bba79c3c4b33b881c2773f067d2b8`.

Der abschließende lokale Vergleich bleibt beim identischen Ornith-Profil:

| Familie (je 15 Fälle) | Abschlüsse Baseline → ADR-0059 | Rubrik Baseline → ADR-0059 | Zeit Baseline → ADR-0059 (ms) |
| --- | --- | --- | --- |
| Storage | 15 → 15 | 15 → 15 | 275.267 → 463.289 |
| Audit | 8 → 15 | 5 → 15 | 389.263 → 359.255 |
| REST | 14 → 15 | 14 → 15 | 286.388 → 454.889 |
| CSV-Plan | 0 → 15 | 0 → 15 | 1.045.019 → 324.963 |

In Summe 37 → 60 Abschlüsse und 34 → 60 Rubrikerfolge. Die gemessene Gesamtzeit
sinkt um 19,7 %, während die Aufrufe von 248 auf 276 und die Kontextbytes von
696.710 auf 923.353 steigen. Storage und REST dauern länger; der größte Nutzen
liegt im Abschluss der zuvor gescheiterten Planung. Keine allgemeine Beschleunigungs-
oder Inhaltsgarantie; die unterschiedlichen Zwischenstände bleiben oben erhalten.
Qwen/Gemma wurden auf ADR-0058 geprüft, nicht nachträglich diesem Stand zugerechnet.

Ein zusätzlicher abschließender `research_configured_model_coherent_smoke` auf demselben
eingefrorenen ADR-0059-Binary besteht mit dem read-only geladenen Luna-Profil:
Ask vier Aufrufe einschließlich eines Einzelrepairs, Plan und Agent-Vorbereitung jeweils
drei Aufrufe, in allen drei Modi kein Nutzerhalt. Alle fünf zusammengehörigen
Methodenkörper liegen gleichzeitig im Analysepaket. Testprozess: 57,97 s, Exitcode 0.
Der begrenzte Repair bleibt sichtbar; dieser Smoke wird nicht als reparaturfrei bezeichnet.

## Inhaltliche Sichtprüfung und Grenze

Die unveränderte Begriffrubrik ist absichtlich nur eine notwendige Bedingung. Sie darf
nicht als Faktenprüfung bezeichnet werden. Die ADR-0056-Smokes liefern mit Luna konkrete
CSV-Änderungen und Tests mit benannten Fehlergrenzen; der zuvor beobachtete Q1-Nutzerhalt
trat dort nicht auf. Unterschiedliche neue Fehlerpolitiken sind erlaubt, sofern der
jeweilige Entwurf und seine Tests übereinstimmen.

Beim lokalen 9B-Modell bleiben trotz vollständiger Originale konkrete Inhaltsfehler:
Eine Audit-Antwort nennt zusätzlich `add_task` in `taskflow/plugins.py`, obwohl dort
keine solche Methode existiert. Ein CSV-Entwurf behauptet einen nicht belegten Ablageort
der erst künftig übergebenen CSV-Datei. Ein weiterer Entwurf vermischt propagierende Ausnahmen mit Weiterverarbeitung nach
Fehlern. Im ADR-0057-Nachtest traten außerdem unbegründete „kein Teilimport“-Zusagen
bei zeilenweisem `add_task` und ein falscher Manager-Konstruktorparameter auf.
Diese Antworten erfüllen Begriffe, sind aber **nicht inhaltlich abgenommen**.
Weitere Promptverlängerung oder ein zweites LLM-Wahrheitsurteil wären kein Beweis.

Auch im ADR-0059-Lauf `eval-1788698871756.jsonl` enthalten alle fünf Wiederholungen
von Audit-Variante 0 die konkrete falsche Behauptung, `add_task` in der Manager-Datei
rufe `add_task` in der Plugin-Datei auf. Diese Methode existiert dort nicht. Dieselben
Antworten nennen zunächst „absolut, nicht cwd-relativ“ und erklären später korrekt
die CWD-bezogene Auflösung des relativen Standardpfads. Die Ankerkanonisierung behebt
den Ablaufblocker, nicht diese widersprüchliche Interpretation. Die lokale inhaltliche
Praxisabnahme bleibt deshalb offen, trotz der 60 vollständigen Abschlüsse.

Die erste Wiederholung jeder CSV-Formulierung desselben abschließenden Ornith-Berichts
zeigt weitere konkrete Grenzen (Familie 3, Varianten und Wiederholungen nullbasiert):

- Variante 0, Wiederholung 0: Der Testentwurf zählt drei übersprungene Zeilen,
  obwohl sein Vierzeilenbeispiel zwei gültige und zwei ungültige Zeilen hat. Ein
  vorgeschlagener `chmod 000`-Test ist außerdem kein verlässlicher plattformübergreifender
  Nachweis für eine unlesbare Datei.
- Variante 1, Wiederholung 0: Q1 erkennt `project_id` und `title` als CSV-Felder;
  Q2 ersetzt das Projekt pro Zeile durch ein neues globales `--project-id` mit
  Inbox-Default. Dazu kommen ein nicht definierter `--file`-Schalter und die falsche
  Annahme, unbehandelte Importausnahmen hätten automatisch argparse-Exitcode 2.
  Q3 folgt teilweise diesem veränderten Auftrag statt der CSV-Spezifikation.
- Variante 2, Wiederholung 0: Der Entwurf behauptet „Keine Plugins für Import“,
  obwohl er `add_task` verwendet, das Plugins auslöst. Er ergänzt redundantes Speichern
  und vermischt Überspringen ungültiger Zeilen mit Tests auf propagierende Ausnahmen;
  auch die vorgeschlagenen Fehlerklassen für fehlende Dateien widersprechen sich.

Bei Luna wurden auf demselben Stand die ersten Wiederholungen aller drei CSV-Varianten
gegen Auftrag und Originale geprüft. Die Vorschläge trennen grundsätzlich Prüfung,
zeilenweise Anwendung und Fehlergrenzen. Variante 1 nennt vollständige Vorvalidierung
jedoch „atomar“, während sie zugleich Rollback bei Schreibfehlern ausdrücklich ausschließt.
Das ist keine Transaktionsgarantie und darf nicht als solche übernommen werden.
Die Sichtprüfung einzelner Antworten ist ausdrücklich keine semantische Vollabnahme
aller 60 Ausgaben. Die Fixture-Speicheradapter geben nur Tupel zurück; reale JSON-/SQLite-
Dateipersistenz einer künftig implementierten CSV-Funktion wurde damit nicht getestet.

Die Sicherheitsgrenze bleibt deshalb ausdrücklich bestehen: Ergebnisse sind
Interpretationen beziehungsweise Vorschläge, keine verifizierten Fakten. Eine
Planfreigabe ersetzt keine Implementierungs- und Testverifikation. Eine allgemeine
Garantie semantischer Richtigkeit beliebiger lokaler Modellausgaben ist nicht erreicht.
Der Abschlussstand muss diese Befunde offen ausweisen und darf aus einer grünen Matrix
keine pauschale Freigabe ableiten.

## Abschließende Gates

Der letzte vollständige Rust-Workspace inklusive Doc-Tests besteht nach allen Korrekturen
bis einschließlich ADR-0059. Die gezielte Recherchegruppe besteht mit 83 Tests und drei
ausdrücklich opt-in ignorierten Live-Tests. Clippy mit `-D warnings`, Formatierung, Linkprüfung,
`git diff --check` und der offline erzeugte Dependency-/Lizenzbericht bestehen ebenfalls.
Die exakten Befehle stehen im [Gate-Protokoll](10-RESEARCH_WORK_STATE.md#gate-protokoll-und-grenzen).
Die abschließenden Modellzahlen und die davon getrennten Inhaltsbefunde stehen oben.
Frontend: `pnpm ci:frontend` besteht (373 Tests, 14 vorhandene Skips, fünf Tooltests,
Formatter, Lint, Typecheck, Build). Node 25.6.1 statt festgelegtem 24.14.0 und bestehende
BigInt-Buildwarnungen bleiben sichtbar; keine Installation zur Umgehung.
Native Cross-Platform-WebView-/Releasegates bleiben CI-/Releaseaufgaben.
