# Plan 10: Operationshinweise – Vergleich und Gegenbefund

Stand: 2026-09-08. Ausgangspunkt: `d6d28a7`.
[ADR-0102](../adrs/0102-originalgebundene-operationshinweise.md) ergänzt den
[SourceReview-Vergleich](10-SOURCE_REVIEW_VALIDATION.md).

## Ergebnis

Die zusätzliche Operationsprojektion erreicht das Modell, verbessert die hier
geprüfte Auditantwort aber nicht. Alle acht Versuche mit Operationshinweisen
verfehlen die unveränderte Begriffrubrik; sieben schließen ab, Granite hält nach
mehrdeutigen Zitaten in Primärantwort und Einzelrepair. Luna und Ornith zeigen im
gegenbalancierten Vergleich keinen behobenen Writer-/Persistenzfehler.

Der Befund spricht gegen die Übernahme dieses Zusatzes. Die experimentelle
Implementierung wird für ihre Reproduzierbarkeit in der Git-Historie gesichert
und anschließend aus dem aktuellen Laufzeitpfad entfernt. Produktstandard war
durchgehend `joint`. Der vorherige native `source-local`-Vergleich bleibt ohne
diese Operationszusätze erhalten. Die konkreten Inhaltsfehler sind nicht behoben.

Sinnvoll erhalten bleiben der echte Parsernachweis und eine geschlossene,
inhaltsfreie Diagnose auch vor vollständigem Modelloutput. Die native Matrix
erhält außerdem den bestehenden Flow-Reader wie die Anwendung: Ein ausdrücklich
gewähltes vorhandenes `inspectFunctionFlow` muss nicht künstlich an einem
fehlenden injizierten Port scheitern. Das führt nicht automatisch weitere Reads aus.

## Hypothese und geprüfter Aufbau

Hypothese: Der Core kann dem Modell bereits vorhandene Aufruf-/Rückgabeschritte
als eng gebundene Indexinformation bereitstellen, damit es weniger aus Namen rät.
Der unveränderte Python-Parser belegt für die öffentliche Fixture:

- `_log`: Aufrufe `open` und `output.write`; Handler-/Branch-Schritte und
  Unsupported-/Dynamic-Grenzen bleiben erhalten.
- Beide `save_tasks`: jeweils ein `Return` mit dem originalen Tupelausdruck,
  keine Call-Schritte, aber weiterhin Dynamic-Grenzen.
- Das ist Syntaxinformation. Fehlende Calls beweisen bei beliebiger dynamischer
  Attributauswertung oder unbekannten Callees keine allgemeine Wirkungslosigkeit.

Der versuchte Researcher-Pfad wählte höchstens acht vollständig gelieferte
Funktionen je aktuellem Originalfenster aus einem 4096-Symbol-Präfix. Der echte
`ExploreFunctionFlows`-Reader lieferte nur dieselbe Revision und keine expandierten
Callpfade. Der gemeinsame Zwei-Sekunden-Umschlag je Fenster, äußere Recherchefrist,
Cancellation und Safe-Reader-Prüfung vor/nach der Vorbereitung blieben erhalten.
Ganze Funktionszeilen mit höchstens 16 Schritten nutzten maximal 1024 tatsächlich
freie Paketbytes. Originale, Aufgabe und Einzelrepair wurden nicht gekürzt.

Die bestehenden echten Mehrmodus-Fixtures wurden mit diesem Reader ausgeführt:
Ask, Plan und Agent-Vorbereitung bei 3409, 4096 und 8192 Paketbytes. Der
3409-Byte-Fall entspricht dem geprüften FormatFieldOnly-8192/2048-Profil; die
anderen beiden Werte sind Paketbytes, nicht Kontexttokens. Der konkrete Writer
und die Return-/Dynamic-Hinweise mussten im jeweiligen gelieferten Inventar stehen.
Reparatur, transiente Retryabrechnung, Live-Edit und Cancellation bestanden weiter.

Die Hinweise wurden nur dem Einzelquellenmodell gegeben. Die anschließende
Bestandsanalyse erhielt weiterhin die Originale und die kurze Interpretation,
nicht das zusätzliche Operationsinventar. Die gespeicherten Interpretationen
zeigen, dass konkrete Operationen dabei nicht zuverlässig erhalten blieben.
Dies ist ein belegter Engpass, keine Garantie, dass eine andere Platzierung allein
das Inhaltsproblem lösen würde.

## Reale Modelle und Kosten

17 abgeschlossene Testprozesse, unveränderte öffentliche Auditfrage Familie 1,
Variante 1 aus `fixtures/research-eval-v1`. Kein Coding, keine mutierende Agentaktion.
Alle vier lokalen Modelle liefen strikt nacheinander. Luna und Google nutzten
ausschließlich bereits freigegebene native Credential-Slots. Keine Installation,
Profiländerung oder zusätzliche externe Datenquelle.

Für Luna und Ornith: jeweils A/B/B/A mit A=SourceReview V2 ohne Operationshinweise,
B=derselbe Vergleich mit Operationshinweisen. Vier weitere B-Nachtests prüfen
Qwen, Flash, lokales Gemma und Granite; sie sind keine weiteren AB/BA-Reihen.
Google Gemma erhält getrennt vier Ausgangsläufe joint/source-local/source-local/joint
sowie einen gezielt instrumentierten joint-Nachtest.

| Modell / Vergleich | Abschluss | Wortprüfung v3 | Modellaufrufe | Transkriptbytes gesamt |
| --- | --- | --- | --- | --- |
| Luna A, A | 2/2 | 0/2 | 6, 6 | 14411, 14397 |
| Luna B, B | 2/2 | 0/2 | 6, 6 | 16987, 16945 |
| Ornith A, A | 2/2 | 0/2 | 6, 6 | 14783, 14783 |
| Ornith B, B | 2/2 | 0/2 | 7, 7 | 19682, 19682 |
| Qwen B | 1/1 | 0/1 | 5 | 13214 |
| Flash B | 1/1 | 0/1 | 6 | 17133 |
| Gemma lokal B | 1/1 | 0/1 | 8 | 21589 |
| Granite B | 0/1 | 0/1 | 5 | 10484 |
| Google Gemma Ausgangsvergleich | 0/4 | 0/4 | 3, 5, 5, 3 | 10267, 10254, 10254, 10267 |
| Google Gemma instrumentiert, joint | 0/1 | 0/1 | 3 | 10267 |

Bytes summieren Transkripte über alle Modellaufrufe, einschließlich Repair;
sie sind weder die Größe eines Kontextfensters noch Provider-Tokenmessungen.
Unveränderte Profile: Qwen 8192/2048, Luna 16384/2048, übrige 16384/4096.
Zusatzreads: Qwen null, sonst je eins; identische adaptive Reads in allen
17 Fällen null. Es gibt wegen gleichzeitiger CPU-Gates keinen Latenzclaim.

### Inhaltsbefunde

Luna lässt auch mit den tatsächlich gelieferten Operationshinweisen
`output.write` aus und verwendet weiter allgemeine Speicherformulierungen.
Orniths beide B-Antworten behaupten ausdrücklich Persistenz; der konkrete Writer
fehlt. Seine erste Manager-Interpretation enthält trotz statischem Inventar
bereits die erfundene Wirkung, die Storage-Interpretation wird zu einer allgemeinen
„task persistence“-Einordnung. Der zusätzliche Einzelrepair repariert ein Zitat,
nicht diese Deutung.

Qwen, Flash und lokales Gemma lassen ebenfalls den Writer aus. Flash beschreibt
nur den Dateikontext und die formatierte Zeile; Gemma bezeichnet `_log` als
Schreiboperation ohne den konkreten Aufruf. Granite beendet den Vergleich nach
zweimal `source-review/Ambiguous`. Die frühere einzelne V2-Probe ohne den Zusatz
schloss für Granite ab; daraus wird keine allgemeine statistische Kausalität abgeleitet.

Weder mehr Quellenanker noch das Vorhandensein des Wortes „Writer“ beweisen
fachliche Vollständigkeit. Die Rubrik wurde nicht zum Bestehen angepasst.
Doppelte Teilantworten und abweichende Planliterale bleiben gesondert offen.

### Google Gemma: bisher verdeckter Ausgabefehler

`gemma-4-26b-a4b-it` besteht die bestehende Capability-Probe und liefert im
Source-local-Ausgangsvergleich drei gültige kleine Einzelquellendokumente.
Das beweist nicht seine Eignung für den größeren aktuellen Bestandsanalysevertrag.

Im gezielten Nachtest `eval-1788870203107.jsonl` protokolliert die neue Diagnose
zweimal **OutputTruncated** in Analyze(Q1), bei 3805 beziehungsweise 4358
Transkriptbytes. Die Initialisierung gelingt; Primäranalyse und ihr Einzelrepair
enden am Provider-Ausgabelimit. Es sind keine erfolgreichen Antworten, die nur
ein Wort verfehlen. Ein höheres Read-Budget würde diesen beobachteten Fehler nicht
reparieren. Das ist kein Nachweis für ein grundsätzlich zu altes Modell oder
fehlende allgemeine JSON-Unterstützung.

Das eingefrorene Diagnosebinary besitzt noch ein aus einer Promptvorsilbe
abgeleitetes `repair=false`-Feld. Dieses Feld ist **nicht autoritativ**: Der
Core-Repairtext beginnt in dieser Phase nicht mit dieser Vorsilbe. Es wird deshalb
im verbleibenden Code durch die objektive Anzahl der Transkriptnachrichten ersetzt;
keine Ablaufentscheidung wird aus der Textschreibweise abgeleitet.
Alte Rohberichte werden nicht umgeschrieben.

## Nachweise und Wiederholung

Baselinebinary:
`target/reports/research-source-local-v2-20260908/research-tests.exe`,
SHA-256 `2b0361741681d7444c6908417fb6cb7a0858adea24446e416f1d13aadd84a348`.

Operationsbinary:
`target/reports/research-operations-20260908/research-tests.exe`,
SHA-256 `5d03dd94daaf71613e89918d9e89d7a97624046da2a989121d79face7c6261ea`.
Die spätere reine Test-Clippy-Korrektur und Diagnosefeldkorrektur verändern keine
dieser bereits abgeschlossenen Modellversuche.

Aufruf: `research-tests.exe --ignored research_approved_model_matrix --nocapture --test-threads=1`.
`A3_RESEARCH_EVAL_CASE=1:1`, `A3_RESEARCH_EVAL_REPETITIONS=1` und ausdrücklich
gewählte Analyse-/Modellparameter wie im vorherigen Verifikationsbericht.
Für neue externe Läufe ist weiterhin explizite Providerfreigabe erforderlich;
Credentials dürfen nicht als Shellvariablen, Argumente oder Logs kopiert werden.

Enge Offline-Prüfungen:

- `cargo test -p a3-repo-index --test function_flow_features research_fixture_flow --offline --locked --jobs 2 -- --nocapture`
- `cargo test -p a3-desktop --all-features --lib source_review --offline --locked --jobs 2 -- --test-threads=1`: acht bestanden, mit echten Operationspaketen.
- `cargo test -p a3-desktop --all-features --lib source_operations --offline --locked --jobs 2 -- --test-threads=1`: drei bestanden.
- Vollständige experimentelle Gates: `cargo clippy --workspace --all-targets --all-features --offline --locked --jobs 2 -- -D warnings` und `cargo test --workspace --all-features --offline --locked --jobs 2 -- --test-threads=1`; Logs unter `target/reports/research-operations-clippy-final.log` und `research-operations-workspace.log`.
- Finale Gates nach Rücknahme werden getrennt dokumentiert. Eine bestandene
  Offline-Strukturprüfung ersetzt ausdrücklich nicht den negativen Modellbefund.

Die Prozesse aller 17 Modelltests melden wegen fehlender Begriffe oder
Recherchehalt Fehler. Die gespeichert abgeschlossenen Recherchen bleiben davon
getrennt: insgesamt 11/17 Completed, **kein** bestandener Wortcheck.
Rohartefakte liegen unter `target` und werden nicht eingecheckt; der Git-Verlauf
bewahrt den Versuchsaufbau. Der normale SourceReview-V2-Nachweis wird nicht rückwirkend
umgedeutet.

## Vollständiges Fallmanifest

Alle Dateien unter `target/research-eval`. B steht für tatsächlich übergebene
Operationshinweise, A für V2 ohne diese; Google-joint bleibt eigens bezeichnet.

| Modell | Variante | JSONL | SHA-256 |
| --- | --- | --- | --- |
| gemma-4-26b-a4b-it | joint | `eval-1788869448562.jsonl` | `6cf05abb7a28b497b0c41e50308097ffe2b567b868e35ceddc2b42212ec2fed4` |
| gemma-4-26b-a4b-it | A | `eval-1788869630202.jsonl` | `e4d268456d61162993546d259159b8f5233976d75628e42207cb7429ee0223cb` |
| gemma-4-26b-a4b-it | A | `eval-1788869735641.jsonl` | `9a197d8e33d097a4b8fae1f8bf0da260eae24983da3469890199d2751c0a0257` |
| gemma-4-26b-a4b-it | joint | `eval-1788869842029.jsonl` | `0c7367ca1a68c6ae20a97b53bfe6e10ce1766fa39a659aae04257fea795141c5` |
| gpt-5.6-luna | A | `eval-1788870013570.jsonl` | `f8c841fb37c40d31b45e03176e93c58190f5a420e9b8e9263dc6de0e664c2943` |
| ornith-1.5:9b | A | `eval-1788870027217.jsonl` | `3248e6d6318ef6ba32a3a112da160e642ef6344faab50cdc514d6cdab38c7e1d` |
| gpt-5.6-luna | B | `eval-1788870038943.jsonl` | `a0fe29ccbe01996f27d7b49e0971df762c5a953553b75a36f79b9eba06bfb1ba` |
| ornith-1.5:9b | B | `eval-1788870053489.jsonl` | `3b7c81bc8e6f4655b339466db685d678833b9c746858e583b2798094ff457f96` |
| gpt-5.6-luna | B | `eval-1788870064577.jsonl` | `83e450a80e52701665451e09e4e3152aa52a14517d76421c2b11b3d9a4056550` |
| ornith-1.5:9b | B | `eval-1788870078818.jsonl` | `07452cb0086af19930e7ff704625dbe6ad6e92d70a733f4b5ef2252220a3b1cd` |
| gpt-5.6-luna | A | `eval-1788870088158.jsonl` | `09cdaa2602b601d2dcaed3c0c9dd3ade39531067b31dbf0720844c805ed27e92` |
| ornith-1.5:9b | A | `eval-1788870103938.jsonl` | `626959832e5062e9d54191a10f0518234e7427c8a6b410f21f76cd6d5b272736` |
| qwen38-8k:latest | B | `eval-1788870173514.jsonl` | `1cf9a9ad13e4b09efd51b3f2bfbc4546124fe02d777508dba9dc1cfb72bb97ca` |
| gemini-3.8-flash | B | `eval-1788870174106.jsonl` | `9ed69f6322e634413efa7bd97224b2c820a98c44c6966e719c6dde5144145885` |
| gemma-4-26b-a4b-it | joint | `eval-1788870203107.jsonl` | `19f312153256dc8d5289161cde14c56cde6cf79771d3297c62254f87783d5b11` |
| gemma4:12b | B | `eval-1788870209042.jsonl` | `2e95249e76fe18b4b0bb4d01552b6b10a02ba80fe061f968d367a5e57d6a60de` |
| granite4.2:8b | B | `eval-1788870247139.jsonl` | `f6542072811ad3998cc2d719f6d1045947c2ee5354c1354df66dd54af7a98f9c` |

## Nächste überprüfbare Arbeit

Den bei Google Gemma belegten Analyze-Ausgabeabbruch anhand des tatsächlich
übertragenen Phasenvertrags isolieren. Zugleich prüfen, ob deterministische
Originaloperationen direkt bei der abschließenden Bestandsanalyse helfen, statt
erneut durch eine freie, verlustbehaftete Modellzusammenfassung zu laufen.
Das ist ein neuer zu prüfender Ansatz, keine behauptete Abhilfe oder Produktfreigabe.
