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

## Fortsetzung nach Sicherungscommit 35630d6

Am 2026-09-06 wurden auf Nutzerauftrag sämtliche vorhandenen Änderungen mit
`35630d6` gesichert. Die folgenden Korrekturen und Messungen sind nachfolgende
Arbeit. Keine Modellinstallation, Profiländerung oder Tests an privaten Projekten.
Alle Liveberichte verwenden dieselbe öffentliche Fixture und bleiben einschließlich
Fehlschlägen erhalten. Die neuen Matrizen haben eine Wiederholung je Formulierung
(zwölf Fälle), nicht die vorherigen fünf Wiederholungen. Lokale Modelle werden
nacheinander ausgeführt; der ausdrücklich freigegebene konfigurierte Provider kann
parallel laufen. Kontextgrößen bleiben Qwen 8.192/2.048, Ornith/Gemma 16.384/4.096
und Luna 16.384/2.048.

### ADR-0060: Tatsächlich verfügbaren aktuellen Kontext nutzen

Der unveränderte ADR-0059-Stand reproduziert in Qwen CSV 3:0 den Kontextabbruch
nach zwei erfolgreichen Ergebnissen vor Q3: `eval-1788705505032.jsonl`, zwei Aufrufe,
5.100 Kontextbytes, 34.308 ms, null adaptive Reads. Der letzte bindende Entwurf
passte nicht in die künstlich nochmals um ein Drittel verkleinerte Paketgrenze.
Der finale Message-Packer räumte dem aktuellen Paket bereits unabhängig Vorrang
vor optionalem altem Dialog ein. ADR-0060 entfernt ausschließlich diese doppelte
Historienreserve; sämtliche tatsächlichen Modell-, Output-, Schema-, Sicherheits-
und Repairgrenzen bleiben bestehen.

Rot→Grün: Budgetregression (8k/2k und 650 Systembytes: 2.212 → 3.702 Bytes)
und realer Plan-/Agent-Controller mit langem Q2-Entwurf. Alle späten Entscheidungen
einschließlich Unicode müssen ungekürzt Q3 erreichen. Ein zusätzlicher
CapturingProvider prüft die tatsächlich gesendeten Nachrichten für alle Modi und
Phasen, 8k/16k und beide Schema-Groundings mit maximalem Repairhinweis und
überfüllter optionaler Historie. Echte Überläufe bleiben abgewiesen.
Recherchegruppe: 86 bestanden, drei explizite Live-Tests ignoriert.

### ADR-0061: Leere Navigationshinweise sind keine ungültigen Ergebnisse

Luna liefert im ADR-0060-Nachtest mehrfach ein leeres Initialize-`note.gap`.
Das bisherige Schema und der Decoder verlangen beide nichtleeren Text; ihre
Lockerung ist bewusst auf die nichtautoritativen V5-Statushinweise begrenzt.
Neutrale Core-Texte ersetzen ausschließlich leere `gap`/`next_step`-Angaben;
fehlende Ergebnisse, Quellen oder Nutzerentscheidungen werden nicht ergänzt.
Historische Antwort-/Recherche-Notizen behalten ihr striktes eigenes Schema.

Decoder- und echter Mehrmodusvertrag wurden rot→grün geprüft: Ask, Plan und
Agent-Vorbereitung benötigen mit leeren Statushinweisen keine zusätzlichen
Aufrufe oder Reads. Null, falsche Typen, Übergröße, Steuerzeichen sowie leere
Ziele/Befunde bleiben abgewiesen. Application-Recherche: 36 Tests; Desktop-
Recherche: 87 bestanden, drei explizite Live-Tests ignoriert.

### Neue Zwölfer-Matrizen und Inhaltsprüfung

| Stand / Modell | Fälle | Abschluss | Begriffe | Aufrufe | Kontextbytes | Zeit (ms) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| ADR-0059 Luna | 12 | 12 | 12 | 55 | 185.305 | 263.833 |
| ADR-0060 Qwen 8k | 12 | 12 | 11 | 55 | 174.553 | 562.957 |
| ADR-0060 Luna | 12 | 12 | 12 | 58 | 193.119 | 269.839 |
| ADR-0061 Luna | 12 | 12 | 12 | 55 | 187.550 | 272.551 |
| ADR-0061 Ornith | 12 | 12 | 12 | 56 | 187.481 | 332.023 |

Diese vollständigen Matrizen haben null Nutzerhalte und null adaptive Reads.
Keine allgemeine Geschwindigkeitsbehauptung aus einzelnen stochastischen Läufen.
Die frozen Desktop-Testbinaries wurden vor weiteren Builds separat gesichert:

- ADR-0060: `target/research-eval/adr60/research-tests.exe`, SHA-256
  `229693db51e4b7e26d1ebf51c69ae5df98f8b74c707874b68b03838b160eb8cd`.
- ADR-0061: `target/research-eval/adr61/research-tests.exe`, SHA-256
  `d9c0beb915d9f313503ba6451619b657f91629ce81a8cc2c19dd8fd704bea977`.

Berichte in `target/research-eval/`, SHA-256:

- ADR-0059 Qwen-Einzelfehler `eval-1788705505032.jsonl`:
  `d6f056c54b91aa4b93c2717762fde7a0681315d0c0d8fde343776e51a6e3f216`.
- ADR-0059 Luna `eval-1788705505388.jsonl`:
  `764172bcfbdad8726fd4bcdfb6d014e4a7033ab7a71c4f9b3c833bc045cda00d`.
- ADR-0060 Qwen `eval-1788706145063.jsonl`:
  `f371d93aaba8df925ba80fa2902ea2666de2ab5530ac6b179695d734e6ff2c6e`.
- ADR-0060 Luna `eval-1788706150645.jsonl`:
  `4b1a2118a7f25cdb90a10f43c885c1114e36db82ee16bce3cb2feb9802b64cfa`.
- ADR-0061 Luna `eval-1788706862661.jsonl`:
  `43009a046c21d49e9c4665d773ec1330b405cf9d920d62e97e51f4b29813ee35`.
- ADR-0061 Ornith `eval-1788706894825.jsonl`:
  `abd37b98072a1aefe71244625f0135b01891d4e6b8ee9a4bdfb90854a586d485`.

Zusätzlicher Luna-Mehrmodus-Smoke besteht auf ADR-0060 (54,40 s) und ADR-0061
(71,71 s). Auf ADR-0061 schließt Ask mit drei Aufrufen ohne Einzelrepair ab;
alle fünf zusammengehörigen Methodenkörper liegen gleichzeitig im Analysepaket.
Diese Tests prüfen Plan und Agent-Vorbereitung read-only, keine ausgeführte Änderung.

Der ADR-0060-Qwen-Auditfall 1:0 scheitert an der unveränderten Begriffrubrik
wegen fehlendem wörtlichem `write`. Die gelesene Antwort beschreibt die echte
Methodenkette, CWD-Auflösung und Append-Modus korrekt, nennt aber den letzten
Aufruf nicht ausdrücklich. Dieser Rubrikfehler ist vom früheren erfundenen
`add_task` in der Plugin-Datei zu unterscheiden; die Rubrik wird nicht nachträglich
angepasst, um den Bericht grün zu machen.

Qwen CSV 3:0 desselben Berichts kopiert hingegen die gesamte Q3-Testpflicht als
Ergebnis. Das ist ein echter falscher Abschluss trotz bestandener Begriffrubrik.
CSV 3:1 ersetzt außerdem `project_id` pro CSV-Zeile durch ein neues Positionsargument
und nur noch eine `title`-Spalte. CSV 3:2 vermischt abgefangene Ausnahmen mit Tests
auf geworfene Ausnahmen. Solche Widersprüche sind nicht durch Kontextabschluss
beseitigt und bleiben getrennte Inhaltsbefunde.

### ADR-0062: Kopierter Core-Auftrag erhält keinen Abschluss

Der reale Qwen-Gegenfall motiviert eine enge deterministische Prüfung vor der
Ergebniszulassung im Core-Planvertrag, keine sprachabhängige Wahrheitsheuristik.
Der direkte Guard-Test und der reale Controller-Test wurden zunächst rot:
die kopierte Pflicht wurde zugelassen und Q3 schloss ohne Repair ab.
Die neue typisierte Fehlerkategorie verwendet den vorhandenen einmaligen Repair.
Ein wiederholtes Echo kann weder Leserunden noch Paketquittung oder Erfolg erzeugen.
Bei Reopen ohne neu zugeordnete Originale müssen auch die früheren beleggebundenen
Pflichten wieder geöffnet werden; mit gültiger Zuordnung bleibt Q3 der nächste Schritt.

Diese drei Regressionen bestehen anschließend zusammen mit der vollständigen
Desktop-Recherchegruppe (90 Tests, drei explizite Live-Tests ignoriert). Das
eingefrorene ADR-0062-Binary hat SHA-256
`6312737c2151b02c39f022ac7a51da3bfc197603e2c0c995651da524da990e86`.
Luna schließt damit erneut alle zwölf Fälle ab:
`eval-1788707479638.jsonl`, 55 Aufrufe, 185.234 Kontextbytes, 287.570 ms,
SHA-256 `d534c4748d837486c719ad21ee93918975c220e6893a009f8f70669af369bcfb`.

### Konkretes Antwortfeld statt Fragetitel

Gemma auf ADR-0061 liefert in Storage 0:0/0:1 fast ausschließlich kopierte
Fragetitel mit Quellen. Audit 1:0 wiederholt ebenfalls bloß seine beiden Pflichten;
REST 2:0 bleibt unvollständig. CSV 3:2 scheitert an fehlender Originalabdeckung
auch nach seinem einzelnen Repair. Der vollständige Bericht
`eval-1788707299178.jsonl` enthält elf Abschlüsse, sechs Begrifferfolge,
52 Aufrufe, 172.719 Kontextbytes, 396.406 ms und null adaptive Reads.
SHA-256: `cbe058680fb4495150446c431ce4a6525d8da93b998c9055dd81b80de9c388df`.

Der bestehende kompakte V5-Vertrag benennt nun das konkrete Feld
`work.results[].text` als Antwort statt Kopie von ACTIVE Q oder Outcome.
Dies präzisiert die bestehende Ausgabeaufgabe des Modells, ersetzt keine
unabhängige Core-Prüfung und enthält keine Fixture-Antworten. Der Systemtext
wächst um 63 UTF-8-Bytes; diese werden konservativ mitgezählt. Die realen
Providerpaket- und langen 8k-Designregressionen bestehen weiterhin. Der tatsächliche
aktuelle Paketrahmen beträgt nun Plan/Agent 3.640, Ask 3.642 Bytes.

Das separate Binary `target/research-eval/answer-contract/research-tests.exe`
hat SHA-256 `91a1c52057a1ddb46abbffdbd72fb870bc371348f47460dc53587c89b83fb9c0`.

| Modell | Fälle | Abschluss | Begriffe | Aufrufe | Kontextbytes | Zeit (ms) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Gemma nach Feldpräzisierung | 12 | 12 | 11 | 57 | 190.263 | 425.579 |
| Luna nach Feldpräzisierung | 12 | 12 | 12 | 55 | 185.511 | 284.334 |

Beide haben null Nutzerhalte und null adaptive Reads. Gemmas Storage-Antworten
sind im Nachtest tatsächlich ausgearbeitet; die erste wurde vollständig mit
den Originalen verglichen. Audit 1:0 lässt weiterhin das letzte wörtliche
`write` aus und erfüllt daher die unveränderte Rubrik nicht. Mehr Aufrufe und
längere Laufzeit sind sichtbar: Das ist eine beobachtete Qualitätsverbesserung,
keine Geschwindigkeitsbehauptung oder allgemeine semantische Abnahme.

- Gemma `eval-1788707786335.jsonl`, SHA-256
  `1745d45178d28b51097b70c43d077e41388487ddaf8ac78ecde9d38a2e7a38aa`.
- Luna `eval-1788707878602.jsonl`, SHA-256
  `bc92c4809c86e36fcee6f317b635694c92be9eadbc5f1605036cb6fae97a9b4f`.

Zusätzlicher Luna-Mehrmodus-Smoke: bestanden, 57,84 s; Ask drei Aufrufe ohne
Repair, sämtliche fünf Methodenkörper gleichzeitig in den Analysepaketen.
Plan und Agent-Vorbereitung schließen ebenfalls ab; keine Mutation ausgeführt.
Im vorangehenden Ornith-ADR-0061-Bericht bleiben die erfundene Plugin-`add_task`-
Methode und die CSV-Abweichung auf globales `--project-id` erhalten.

### ADR-0063: Wiederholte Statusquellen ohne unnötigen Repair

Der Nachtest mit präzisiertem Antwortfeld belegt einen weiteren reinen
Darstellungsfehler: Gemma Storage 0:2 hat kanonische Ergebnisanker, aber doppelte
`finding_source_refs`. Der Decoder verwirft deshalb das ganze Dokument.
Die neue V5-Statuskanonisierung beseitigt nur gültige Wiederholungen nach Prüfung
der ursprünglichen Eingabegrenze. Legacy-Notizen und Ergebnisbelege bleiben getrennt.

Beide neuen Regressionen wurden zunächst rot: unabhängiger Decoder mit InvalidValue
und echter Mehrmoduscontroller mit Nutzerhalt. Nach Korrektur schließen die
kontrollierten Fälle ohne zusätzlichen Repair oder Read ab. Ein ergänzender
unabhängiger Negativtest zeigte außerdem, dass Rusts Zahlparser `S+1` entgegen
der bisherigen S-Schemagrammatik akzeptierte. Der gemeinsame Decoder fordert nun
vor dem Zahlenparse reine ASCII-Dezimalziffern; S1 und S200 bleiben gültig.
Die Application-Recherchegruppe besteht mit 38 Tests, die Desktop-Gruppe mit
91 Tests plus drei explizit ignorierten Live-Tests.

Eingefrorenes Nachtestbinary `target/research-eval/adr63/research-tests.exe`,
SHA-256 `d0520ba98669bfceb0c5611731e3099d3ca602f5f8a0f26549a28ab5cb101f6e`.

Ein zwischenzeitlicher Workspace-Lauf auf ADR-0062 scheiterte einmal im isolierten
Invalid-Analysis-Test mit Exitcode 1 vor dem Abschlussmarker; die Unterursache war
wegen gecaptureter Ausgabe nicht sichtbar. Derselbe eingefrorene Test besteht
ungecaptured direkt, danach auch die komplette Recherchegruppe und der volle
Workspace mit sichtbarer Ausgabe. Es gab weder einen Access-Violation-Retry noch
eine Lockerung dieses Tests. Die Ursache dieses Einzelfehlers ist damit nicht
bewiesen; insbesondere wird er nicht als behobener Produktionsfehler verbucht.
Clippy fand anschließend ein `unwrap` im neuen Repairhint-Test; es wurde durch
normale Fehlerweitergabe ersetzt. Ein abschließendes Gate auf ADR-0063 bleibt
von diesen Zwischenständen zu unterscheiden.

Auf dem korrigierten ADR-0063-Stand bestehen anschließend Formatierung, der
vollständige Rust-Workspace mit allen Features einschließlich Doc-Tests, nativem
Lebensdauertest und echten Mutations-/Replanverträgen sowie Clippy mit `-D warnings`.
Auch `pnpm ci:frontend` besteht erneut: Formatter, Lint, Typecheck (null Fehler/
Warnungen), 373 Tests mit 14 vorhandenen Skips, fünf Tooltests und Build.
Die bekannte Node-Versionsabweichung und BigInt-Transformwarnungen bleiben sichtbar.
`pnpm check:links` prüft 106 Markdown-Dateien/406 Links; `git diff --check` besteht.
Die Befehle verwenden weiterhin `--offline --locked`, Tests `--test-threads=1`;
sichtbare Testausgabe wurde für die vollständige Fehlereinordnung aktiviert.

Luna auf dem eingefrorenen ADR-0063-Stand: zwölf von zwölf Abschlüssen und
Begrifferfolgen, 55 Aufrufe, 186.460 Kontextbytes, 264.241 ms, null Nutzerhalte
oder adaptive Reads. Bericht `eval-1788708411083.jsonl`, SHA-256
`ceac4051692019f5f489d860e10cf3154646b3374dcc062b414364cfc5f018e0`.
Die gesichtete CSV-Variante 3:1 verwendet weiterhin „atomar“ zu weitgehend:
Vorvalidierung allein garantiert keinen Rollback späterer Storage-/Pluginfehler.
Die im Testteil selbst ausgeschlossene Rückabwicklung macht daraus keine
Transaktion. Dieser Inhaltsbefund bleibt trotz formal grüner Matrix offen.

Qwen auf ADR-0063: zwölf Abschlüsse und Begrifferfolge, 55 Aufrufe,
174.187 Kontextbytes, 570.574 ms, null Nutzerhalte oder adaptive Reads.
`eval-1788708412343.jsonl`, SHA-256
`f163c61a422338c01ea8185369b2ebc40fb563b7809b4fbeabd3c3845e4faa54`.
CSV 3:0 enthält jetzt konkrete Testfälle statt der kopierten Q3-Pflicht.
Der Entwurf widerspricht sich jedoch bei leeren Pflichtfeldern (Validierung versus
stilles Überspringen); das ist kein vollständiger semantischer Erfolg.
Zusätzlicher Mehrmodus-Smoke besteht in 126,23 s: jeweils drei Aufrufe in Ask,
Plan und Agent-Vorbereitung; keine Mutation. In Plan/Agent kennt Q1 den Audit-Pfad,
Q2 erklärt ihn jedoch als nicht angegeben. Genau diese verlorene Kontextübergabe
wird nachfolgend untersucht, nicht als sichere Modellkonvergenz verbucht.

Ornith auf ADR-0063: elf Abschlüsse und Begrifferfolge aus zwölf Fällen,
57 Aufrufe, 192.006 Kontextbytes, 385.177 ms, ein Nutzerhalt und drei adaptive
Reads bei Storage 0:1. `eval-1788709222744.jsonl`, SHA-256
`c626f55b01db4d62730ec313ee24ae9afaa821332b5d114ed58fb748bfdbd8f7`.
Die isolierte Pflicht `Umgebungsvariable,` bleibt offen, obwohl andere Antworten
dieselbe Variable bereits korrekt erklären. Keine identischen adaptiven Reads.
Audit 1:0 widerspricht sich weiterhin beim CWD-Bezug von `abspath`.
Der zusätzliche Mehrmodus-Smoke besteht in 90,23 s mit 4/5/4 Aufrufen;
Ask erfindet dabei `Manager.trigger_task_created`, Plan/Agent zusätzliche nicht
angeforderte Fehlerpolitiken. Diese formalen Abschlüsse sind keine Inhaltsabnahme.

### ADR-0064: Tatsächliche Bestandsdetails in abhängigen Entwürfen

Der Qwen-Mehrmodustest hat den Audit-Pfad in Q1 korrekt, aber erst hinter der
384-Byte-Vorschau. Q2 erhält trotz bereits zugelassener Voraussetzung nur deren
Anfang; sein optionaler Originalteil enthält den Audit-Körper nicht. Die Korrektur
erhält passende Bestandsvoraussetzungen und kennzeichnet budgetbedingt verbleibende
Auszüge. Vollständige Designentscheidungen bleiben vorrangig, gespeicherte Texte
und epistemische Arten unverändert.

Direkter Packing-Test und echter 8k/2k-Mehrmoduscontroller scheiterten zuerst an
der fehlenden späten Bestandsangabe. Nach Änderung bestehen beide: Plan und Agent
jeweils drei Aufrufe, keine adaptiven Reads, bytegleiche Originale. Der direkte
Überlauffall behält einen langen vollständigen Entwurf neben dem expliziten
Bestandsauszug; Wiederholung ist deterministisch und verändert den Prüfstand nicht.
Anschließend bestehen alle 93 Desktop-Recherchetests, drei Live-Tests bleiben opt-in.

### ADR-0065: Listenpflicht bleibt beim zugehörigen Auftrag

Der erneute Ornith-Halt macht einen zweiten deterministischen Fehler sichtbar:
nach der ersten Satzgrenze behandelt der Core jedes weitere Komma als Pflichtgrenze.
So entstehen aus einer Liste mehrere isolierte, teilweise überlappende Fragen.
Die neue Regression mit der unveränderten Storage-Formulierung scheitert zunächst
mit fünf statt drei Pflichten. Ein echter Audit-Mehrmodusvertrag hält wegen des
zusätzlich abgespaltenen `und wohin wird das Audit-Log geschrieben?`.
Komma und Doppelpunkt entfallen als Trennzeichen; die Liste einschließlich Einleitung
bleibt wörtlich erhalten. Kein Ergebnis wird dadurch automatisch zugelassen.

Nach Änderung bestehen beide Regressionen und die ganze Desktop-Recherchegruppe:
95 Tests, drei opt-in Live-Tests ignoriert. Der Audit-Listenvertrag schließt Ask,
Plan und Agent-Vorbereitung im echten 8k/2k-Profil mit je drei Aufrufen ab.
Eingefrorenes Binary `target/research-eval/adr65/research-tests.exe`, SHA-256
`f8a36eb2b66d028244f8e6c7e4ea9559e8881fae56d90a501c5b32d28c3332ee`.
Dieses enthält ADR-0064 und ADR-0065; seine Livewerte isolieren die beiden Änderungen
nicht voneinander. Die unveränderte Fixture, Modellprofile und Rubrik bleiben erhalten.

Luna im ersten ADR-0065-Nachtest: 12/12 Abschlüsse und Begriffe, 35 Modellaufrufe,
126.183 Kontextbytes, 190.071 ms, null Nutzerhalte oder adaptive Reads.
Bericht `eval-1788709858635.jsonl`, SHA-256
`6abf270f96ae949558c5ba9bd59ddcddb271f12ec53466c6c5d451f27972a8f5`.
Gegenüber dem einzelnen ADR-0063-Lauf sind es 35 statt 55 Aufrufe und
126.183 statt 186.460 Kontextbytes. Dies ist eine gemessene Beobachtung dieser
Matrix, keine modellübergreifende Laufzeitgarantie. Der zusätzliche Mehrmodus-Smoke
besteht in 51,83 s mit je drei Aufrufen; Originale bleiben bytegleich.
Die gesichtete Audit-Antwort erklärt jetzt `output.write`, Konstruktor-CWD und Append
korrekt. Im Agent-Vorbereitungs-Smoke bleiben vorhandene Fehlerweitergabe und die
fehlende Rückabwicklung erhalten. CSV 3:1 legt nun ausdrücklich keinen atomaren Import
fest; sein Testteil fordert bei ungültigem UTF-8 jedoch pauschal null `add_task`-Aufrufe,
obwohl Streaming mit bereits verarbeiteten früheren Zeilen vereinbart ist. Eine
derartige Garantie braucht einen begrenzten Testinput oder eine Vorvalidierung;
sie folgt nicht allgemein aus dem Entwurf. Inhaltsabnahme bleibt daher getrennt offen.

Gemma auf ADR-0065: elf Abschlüsse, neun Begrifferfolge aus zwölf Fällen,
35 Aufrufe, 116.901 Kontextbytes, 285.119 ms, ein Nutzerhalt, null adaptive Reads.
`eval-1788709874954.jsonl`, SHA-256
`067274607aae249887b7dfd52701dcac318fddd11115a3e736297a742c4109ba`.
Audit 1:0 lässt weiterhin `output.write` aus. CSV 3:0 vergisst UTF-8 und ersetzt
das verlangte Positionsargument durch `--file`; seine Annahmen über echte
Backendpersistenz sind in der Fixture nicht belegt. CSV 3:1 scheitert vor jeder
Designphase zweimal an Originalabdeckung; der Einzelrepair wird nicht erweitert.
ADR-0064 ändert nur abhängige Designansichten und ADR-0065 neue Ask-Zerlegungen:
dieser Plan-Q1-Halt liegt nicht im geänderten Phasenpfad und ist kein Beleg, dass
mehr Bestandskontext ihn verursacht hat. Er bleibt ein gesonderter Modellfehlschlag.
Der zusätzliche Mehrmodus-Smoke besteht formal in 86,35 s mit je drei Aufrufen;
Gemma wiederholt in Q3 jedoch die Codebeschreibung statt konkreter Tests. Das ist
ein Inhaltsfehler trotz Schemaerfolg, kein erfolgreicher Verifikationsentwurf.

Auf ADR-0065 bestehen Formatierung, alle Rust-Workspace-/All-Feature-/Doc-Tests,
die echten Mutations-, Crash-/Replan-, Read-only-, Freshness-, Prozess-, Pfad- und
Storagegrenzen sowie Clippy mit `-D warnings`. Dazu `pnpm check:links` mit
108 Markdown-Dateien/412 lokalen Links, `git diff --check` und der mit
`CARGO_NET_OFFLINE=true` erzeugte Abhängigkeitsbericht. Die Befehle bleiben
`--offline --locked`, Tests seriell mit sichtbarer Ausgabe. Frontendcode ist seit
dem bereits vollständig grünen ADR-0063-Frontendgate unverändert.

Qwen auf ADR-0065: 12/12 Abschlüsse, elf Begrifferfolge, 34 Aufrufe,
103.628 Kontextbytes, 383.455 ms, null Nutzerhalte oder adaptive Reads.
`eval-1788710257738.jsonl`, SHA-256
`e37bd9d01b13b0fb89b3434ec686867e995dfdd2d1d09dc2ed9bab6bd2afab7d`.
Audit 1:0 lässt den wörtlichen letzten Schreibaufruf aus. CSV 3:1 führt weiterhin
ein zusätzliches globales Projektargument ein und wechselt zwischen Positionsargument
im Entwurf und `--project-id` im Test. Q1 formuliert dort Zukunftsplanung statt der
geforderten Bestandsanalyse: Quellenzuordnung allein beweist auch den Inhalt dieser
Phase nicht. Der Mehrmodus-Smoke besteht in 115,41 s mit je drei Aufrufen. Gegenüber
ADR-0063 ist das Logziel jetzt in Q2 erhalten; Q3 nennt konkrete Erfolgs-, Fehler- und
Reihenfolgetests. Die ursprüngliche Informationsverlustregression ist damit auch
live nachgetestet, ohne Qwens übrige Inhaltsmängel als behoben zu erklären.

### Begrenzte Entscheidungsdiagnostik für verbleibende Modellfehler

Nur der opt-in Testadapter erhält zusätzliche `decision_diagnostics`: Phasen,
rollen-/längengefasster Transkript-Hash, numerische Originalfenster-/Fixture-Datei-
Zuordnung und unabhängig decodierte Ergebnisanker. So lassen sich fehlende
Quellenzuordnungen und identische Eingaben untersuchen, ohne ein weiteres allgemeines
Prompt- oder Budgetexperiment daraus abzuleiten. Keine Produktionsinstrumentierung,
keine beliebigen Pfade, Quelltexte oder freien Modellausgaben in diesen neuen Feldern.
Bestehende öffentliche Fixture-Antworten bleiben gesondert erhalten.

Die drei Matrix-Unit-Tests bestehen einschließlich des neuen Negativvertrags für
ausgeblendete Sentineltexte/fremde Pfade und des stabilen, bei Byteänderung wechselnden
Hashes. Separates Binary `target/research-eval/adr65-diagnostics/research-tests.exe`,
SHA-256 `cc2cdd066d60335f6d12418a219aa9e66562068b92732e5b9424679827a1eaf6`.
Die Produktionslogik entspricht ADR-0065; lediglich Testdiagnostik wurde ergänzt.

### Wiederholungsserie und verbleibende Grenzen auf ADR-0065

Luna besteht anschließend die vollständigen fünf Wiederholungen: 60/60 Abschlüsse
und Begrifferfolge, 171 Modellaufrufe, 619.543 Kontextbytes, 900.279 ms, null
Nutzerhalte, adaptive Reads oder identische adaptive Wiederholungen.
`eval-1788710118977.jsonl`, SHA-256
`a7d626bce3ea8257c3f6ffc698e3526d8cb521bea4c27d877a3929ff07d870da`.
Gegenüber der vollständigen ADR-0059-Luna-Serie sind es 171 statt 283 Aufrufe,
619.543 statt 944.150 Kontextbytes und 900.279 statt 1.519.190 ms auf derselben
Fixture und demselben Profil. Die Änderung umfasst mehrere Korrekturen; parallele
lokale Tests/Builds und Providerlatenzen verhindern eine isolierte Laufzeitkausalität.
Die Stichprobe Storage 0:2/Repeat 4 erklärt CLI-Vorprüfung und Tuple-Adapter korrekt;
CSV 3:2/Repeat 2 hält pro Zeile die Schnittstelle sowie erste-Fehler-/Teilimportpolitik
zwischen Entwurf und Tests konsistent. Das hebt frühere Gegenbeispiele nicht auf.

Ornith auf ADR-0065: 12/12 Abschlüsse, elf Begrifferfolge, 35 Aufrufe,
127.756 Kontextbytes, 228.565 ms, null Nutzerhalte oder adaptive Reads.
`eval-1788710763716.jsonl`, SHA-256
`0e8916e06d822b62f81cfa21971c4783062e3f44ce4d83dd19f6b7722b69eb54`.
Storage 0:1 schließt die zuvor isolierte Listenpflicht jetzt ab. Audit 1:0 lässt
den letzten wörtlichen Schreibaufruf aus. Der Mehrmodus-Smoke besteht formal in
86,58 s mit 4/4/4 Aufrufen, enthält aber weiter die erfundene Methode
`Manager.trigger_task_created`; Q3 ersetzt Tests durch einen Registrierungsbedarf
und eine nicht belegte `add_plugin`-Annahme. Die Korrekturen an Ablauf und Packing
beweisen deshalb keine vollständige lokale Modellqualität.

Gemma CSV 3:1 wird auf dem separaten Diagnosebinary fünfmal gezielt wiederholt:
0/5 Abschlüsse, zehn Aufrufe, 39.010 Kontextbytes, 77.284 ms; null Reads.
`eval-1788711137118.jsonl`, SHA-256
`e425a9a64031fe89fddab0ef0f0066e96a6b5120bc243ab64c871b134edc21ec`.
Alle ersten Pakete haben denselben Transkript-Hash, alle Repairpakete ebenfalls.
Jedes liefert E1 für main.py und E2 für manager.py; jede Antwort referenziert nur E1,
auch nach dem Repair. JSON und Phasenschema sind gültig, die Originalabdeckung nicht.
Der Befund ist damit konkret wiederholbar: kein fehlender Read, keine Kontextgrenze,
keine Parserreparatur, sondern ausgelassene Quellenzuordnung trotz Gruppenhinweis.

Ein enger Nachtest ergänzt den vorhandenen Repair um ein direkt nutzbares
`result.evidence`-Array ausschließlich bei genau einem tatsächlichen Fenster je
benötigter Originaldatei und nur, wenn der gesamte Hinweis weiter in 768 Bytes passt.
Bei mehreren möglichen Fenstern gibt es keine willkürliche Auswahl. Die Ausgabe
wird nicht automatisch verändert; das Modell muss weiter selbst das Ergebnis und
seine Belege liefern. Keine neue Phase, kein weiterer Repair oder Read. Die direkte
Hint-Regressionsprüfung wurde zunächst rot, bevor der Hinweis geändert wurde.

Der konkrete Repair-Nachtest (`concrete-repair/research-tests.exe`, SHA-256
`2aefa69001bdcb9a5dfa9423f6d4a80a4d1c3b2d6615339547ce495f50ca7a59`)
schließt 5/5 Fälle ab: 17 Aufrufe, 87.854 Kontextbytes, 189.165 ms,
keine Nutzerhalte oder Reads. Bericht `eval-1788711497459.jsonl`, SHA-256
`c9550082effbee266af7b9ecc460bc2d087dcdcf129b43b1b21bad34bb59028b`.
Die beiden tatsächlich nötigen Repairs liefern jetzt E1 und E2; drei weitere
Durchläufe liefern beide schon im ersten Versuch. Der Hash des ersten Pakets ist
gegenüber dem Fehllauf identisch. Die Wirkung des geänderten Repairtexts ist deshalb
an den beiden Repairs nachgewiesen; die drei unterschiedlichen Primärantworten sind
kein kausaler Reparaturnachweis, sondern beobachtete Modellvariabilität trotz
Temperatur null. Alle fünf Antworten wurden als öffentliches Fixture-Ergebnis
aufbewahrt. Die gesichteten Entwürfe/Testteile enthalten weiterhin unbelegte
Backendpersistenzannahmen; diese Quellenkorrektur ist keine semantische Gesamtprüfung.

Die Recherchegruppe besteht mit 96 Tests und drei opt-in Live-Skips. Das anschließende
vollständige Build-Gate findet einen Fehler dieser Ergänzung: `serde_json` ist im
Desktop-Crate nur als Dev-Abhängigkeit vorhanden. Die feste Arraydarstellung nutzt
deshalb jetzt ausschließlich formatierte, bereits typgeprüfte numerische E-IDs;
keine zusätzliche Produktionsabhängigkeit oder freie String-Interpolation.
Die erweiterte Regression prüft bytegenau E1/E2, alternative Fenster ohne willkürliche
Auswahl, fehlende Originale, 768 Bytes und maximal acht Gruppen. Sie besteht erneut.
Separates finales Binary `final-repair/research-tests.exe`, SHA-256
`d7d9e8821bcf829cb2a5ebada5e372e8ce0492104fd4943a8bd3ae2074c89465`.
Die erzeugte E1/E2-Reparaturdarstellung bleibt bytegleich. Der erste Buildfehler und
die früheren Fehlberichte bleiben im Prüfverlauf sichtbar; gezielte Tests allein
hatten diese Produktionsgrenze nicht geprüft.

Die vollständigen Zwölfermatrizen auf `concrete-repair` ergeben für Gemma 12/12
Abschlüsse, elf Begrifferfolge, 37 Aufrufe, 130.053 Kontextbytes und 308.087 ms;
für Luna 12/12 Abschlüsse und Begrifferfolge, 34 Aufrufe, 125.821 Kontextbytes
und 228.834 ms. Beide haben null Nutzerhalte, adaptive oder wiederholte Reads.
Berichte `eval-1788711877859.jsonl` (Gemma), SHA-256
`6550a750c93fed738e87f480b4a1da80efc13c6c10100f302445afbf778faa6f`,
und `eval-1788711883484.jsonl` (Luna), SHA-256
`a49dc591776a8c730dbff7f376fe4645c0ee45e29784579f29fa0abff3297492`.
Gemmas Audit 1:0 nennt alle vier Projektmethoden, lässt aber `output.write` aus;
Q1 besteht dort lediglich aus dem Einstiegssatz. Die Begriffrubrik bleibt deshalb
rot. CSV 3:0 behält das angeforderte Positionsargument und die Skip-Politik zwischen
Entwurf und Tests, behauptet aber tatsächliche Persistenz und antwortet überwiegend
auf Englisch. Call-/Quellenabdeckung ersetzt keine Inhalts- oder Sprachprüfung.

Lunas zusätzlicher Mehrmodus-Smoke besteht mit je drei Aufrufen in insgesamt
68,40 s. Die gesichteten Plan-/Agent-Testentscheidungen nennen konkrete Inputs,
Spies, Reihenfolgen, unbekannte Projekte, Speicher-/Plugin-/Dateifehler und
fehlende Rückabwicklung. Die Originale bleiben bytegleich; Agent bezeichnet
hier weiterhin ausschließlich die Recherche-/Vorbereitungsphase, nicht einen
live mutierenden End-to-End-Lauf.

Gemmas Mehrmodus-Smoke auf `concrete-repair` besteht formal mit 3/3/3 Aufrufen
in 87,20 s. Die Texte wurden geprüft: Q3 wiederholt in Plan und Agent weiterhin
die Codebeschreibung statt Tests; Ask lässt unter anderem die Writer-Methode aus.
Dieser Smoke beweist den technischen read-only Ablauf, ausdrücklich nicht die
vollständige Beantwortung seiner inhaltlichen Pflichten.

Auf `final-repair` bestehen anschließend erneut der erweiterte gezielte
Coverage-Repair-Test, `cargo test --workspace --all-features --offline --locked
-- --test-threads=1 --quiet`, `cargo clippy --workspace --all-targets --all-features
--offline --locked -- -D warnings` und `cargo fmt --all --check`.
`RUST_TEST_NOCAPTURE=1` hält dabei Kindprozessfehler sichtbar. Die echten Mutations-,
Read-only-, Replan-/Crash-, Freshness-, Pfad-, Prozess-, Storage- und Migrationsverträge
bleiben grün; Modellnetzwerkaufrufe sind in diesem Gate weiterhin opt-in ignoriert.
Das ersetzt keinen nativen plattformübergreifenden Release-/UI-Test.

Auch `pnpm ci:frontend` wurde auf dem finalen Stand erneut vollständig ausgeführt:
Formatter, Lint ohne Warnungen, Svelte-/TypeScript-Check mit null Fehlern/Warnungen,
373 erfolgreiche Tests bei 14 bestehenden Skips, fünf Tooltests und Build bestehen.
`pnpm check:links` prüft 108 Markdown-Dateien/412 lokale Links; `git diff --check`
und der offline erzeugte Abhängigkeitsbericht bestehen ebenfalls. Die bereits
vorhandene Node-25.6.1-Abweichung von 24.14.0 und tolerierte BigInt-Buildwarnungen
sind weiterhin sichtbar; keine Änderung an Toolchain, Abhängigkeiten oder UI-Code.

Luna auf dem finalen Binary: 12/12 Abschlüsse und Begrifferfolge, 34 Aufrufe,
123.838 Kontextbytes, 195.203 ms, null Nutzerhalte oder adaptive Reads.
`eval-1788712243531.jsonl`, SHA-256
`c6da72daf93c7c8d5717896a381b33ade4f9c02d9228b71b77b910a840996f74`.
Die geprüfte CSV-Variante 3:2 hält das Positionsargument und konkretisiert
Vorvalidierung und Testinputs. Sie nennt den Import aber pauschal dateiatomar,
während sie im selben Entwurf Rollback bei Storage-/Plugin-Fehlern ausschließt.
Die Tests sichern nur Vorvalidierung ab. Diese Einschränkung behebt die anfängliche
Atomaritätsbehauptung nicht: auch bei Luna bleibt inhaltliche Abnahme offen,
obwohl Abschluss- und Begriffszähler grün sind.

Der zusätzliche Gemma-Fünfernachtest auf demselben finalen Binary bestätigt
5/5 Abschlüsse und Begrifferfolge, 17 Aufrufe, 87.854 Kontextbytes, 201.181 ms,
null Nutzerhalte oder Reads. `eval-1788712330760.jsonl`, SHA-256
`e257e0975f7da7e66865cf66881f6c42ae0ada1c5533d24bb3edb45f64ed41f3`.
Auch hier benötigen genau zwei Fälle den erfolgreichen E1/E2-Repair; drei liefern
beide Anker primär. Die vorherige `serde_json`-Darstellung und die nun ausschließlich
numerische Formatierung erzeugen dieselben Hintbytes und denselben Aufruf-/Byteumfang.
Die Produktionskorrekturen, ADR-0060 bis ADR-0065 und Regressionen sind nach den
vollständigen Gates in `030f2f1` gesichert; `35630d6` bleibt der anfängliche
ausdrücklich beauftragte Sicherungscommit. Kein Push oder Release.

Qwen auf `030f2f1`/`final-repair` mit unverändert 8.192/2.048: 12/12 Abschlüsse,
elf Begrifferfolge, 34 Aufrufe, 103.628 Kontextbytes, 405.890 ms, null Nutzerhalte
oder adaptive Reads. `eval-1788712543768.jsonl`, SHA-256
`6293ad5519eacb89eb5e404ef6ba7def920f3e5b809d00380208272f15b4b602`.
Storage 0:1 hält die Auswahlpriorität und die zusammenhängende Liste korrekt;
REST 2:0 erklärt Router, Handler, Manager und 200/404 anhand der tatsächlichen
Fixture. Audit 1:0 lässt weiterhin `output.write` aus. CSV 3:0 liefert konkrete
Tests statt des historischen Core-Auftragsechos, bleibt jedoch Englisch und
plant weitgehend unbehandelte Exceptions. CSV 3:1 ersetzt die Bestandsanalyse
wieder durch einen Zukunftsentwurf und wechselt von einem zusätzlichen
Positionsargument `project_id` im Entwurf zu `--project-id` im Test. Die
Originaldateien sind in allen drei Paketen vorhanden, die Entwurfsübergabe ist
vollständig. Diese Fehler sind keine nachgewiesene fehlende Leserunde oder
Kontextabschneidung und werden nicht durch weitere Reads oder eine größere
Repairzahl als behoben erklärt.

Qwens finaler Mehrmodus-Smoke schließt mit 3/3/3 Aufrufen in 122,53 s ab. Die
geprüften Entwürfe behalten `audit_log.txt` als Ziel; Q3 nennt konkrete Erfolgs-,
Fehler-, Reihenfolge- und Integrationstests. Gegenüber dem historischen 8k-Abbruch
und der abgeschnittenen Zielangabe liegt damit erneut ein technischer und ein
konkreter inhaltlicher Nachtest vor. Die teilweise englische Ausgabe und die
separaten CSV-Widersprüche bleiben bestehen. Originale bleiben bytegleich.

Ornith auf `030f2f1`/`final-repair`: 12/12 Abschlüsse, elf Begrifferfolge,
35 Aufrufe, 127.892 Kontextbytes, 241.915 ms, null Nutzerhalte oder adaptive Reads.
`eval-1788713079847.jsonl`, SHA-256
`c2c7640fc367fb0589a4b11cdb17734c6831c91ff24d7038253858e177504332`.
Storage 0:1 schließt erneut ab; die gesichtete REST-Variante 2:1 unterscheidet
Router-, Handler- und Managerverhalten korrekt. Audit 1:0 lässt wie bei Gemma/Qwen
`output.write` aus. CSV 3:0 legt konkrete Tests vor, lässt aber einzelne
Fehlerentscheidungen bis zu den Tests offen, behauptet tatsächliche Persistenz und
vermischt einen direkten `main(argv)`-Aufruf mit `SystemExit` beim Programmstart.
Die Fixture gibt bei direktem erfolgreichem `main` eine Zahl zurück; nur der
`__main__`-Block erzeugt daraus `SystemExit`. Diese Testverwechslung bleibt ein
konkreter Inhaltsbefund trotz formal gültigem Entwurf.

Orniths finaler Mehrmodus-Smoke besteht formal mit 4/4/4 Aufrufen in 89,51 s.
Ask nennt weiterhin die nicht vorhandene Methode `Manager.trigger_task_created`.
Plan und Agent erhalten vollständige Originalrümpfe, ersetzen die Testpflicht Q3
aber erneut durch vermeintlich nötige Registrierungsnachweise und eine nicht
belegte `add_plugin`-Annahme. Auch ein erfolgreicher Einzelrepair der Quellenliste
macht diesen anschließenden Inhalt nicht richtig. Alle Originale bleiben bytegleich.

### Abschluss dieses Korrektur- und Nachtestpakets

Die letzte vollständige Zwölfermatrix je Modell (Binary-Zuordnung und Hashes oben):

| Modell | Abschlüsse | Begriffrubrik | Nutzerhalte | Adaptive Reads |
| --- | ---: | ---: | ---: | ---: |
| gpt-5.6-luna | 12/12 | 12/12 | 0 | 0 |
| gemma4:12b | 12/12 | 11/12 | 0 | 0 |
| qwen38-8k:latest | 12/12 | 11/12 | 0 | 0 |
| ornith-1.5:9b | 12/12 | 11/12 | 0 | 0 |

Die lokalen Matrixprozesse enden wegen der fehlenden Writer-Nennung absichtlich
mit Fehlerstatus; dieser wird weder verschluckt noch als Storagecrash umgedeutet.
Der jeweilige anschließende Mehrmodus-Smoke ist ein separater technischer Nachweis.
Alle gestarteten Modell- und Gate-Prozesse sind beendet. Kein autonomer Hintergrundlauf,
keine Provider-/Profiländerung und kein zusätzlicher Repair wurden eingerichtet.

Verifiziert abgeschlossen sind die konkreten Harness-Korrekturen, die neuen
Regressions- und Diagnoseverträge sowie die zugeordneten Nachtests. Die inhaltliche
Praxisabnahme bleibt offen: falsche oder unvollständige Methodenketten, freie
Codebeschreibung statt Tests, wechselnde Schnittstellen, unbewiesene Persistenz
und widersprüchliche Atomaritätsgarantien sind weiterhin Gegenbeispiele. Ihre
Beseitigung wird nicht aus vorhandenen Quellenankern, Begriffstreffern oder dem
eigenen Urteil eines weiteren LLM abgeleitet. Weitere Leserunden sind für die hier
vollständig gelieferten Originale kein nachgewiesenes Heilmittel.

Die abschließende reine Protokoll-/Checklistenänderung besteht erneut
`pnpm check:links` (108 Markdown-Dateien, jetzt 413 lokale Links) und
`git diff --check`. Seit `030f2f1` wurde keine Produktions- oder Testlogik geändert.
