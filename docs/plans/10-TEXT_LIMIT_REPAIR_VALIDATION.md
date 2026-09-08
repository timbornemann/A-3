# Plan 10: Präzise Textlimit-Reparatur und echte Variantenabdeckung

Stand: 2026-09-08. Ausgangspunkt: `2f0a7e1`.

## Ziel und Grenze

Der [Originalbasis-Vergleich](10-DESIGN_BASIS_VALIDATION.md) belegte vollständige
Granite-Testentwürfe über der bestehenden 4096-Byte-Grenze. Der Decoder meldete
dafür denselben allgemeinen Wertfehler wie für ungültige Enums. Der vorhandene
Einzelrepair konnte deshalb die konkrete Ursache nicht benennen.

Work-Ergebnistext erhält jetzt `ResultTextTooLarge { bytes }`, gemessen nach dem
unveränderten Trimmen in UTF-8-Bytes. Der Desktop übernimmt diese typisierte
Information ohne Fehlerstring-Parsing. Der begrenzte phasenspezifische Repair
fordert eine kürzere vollständige Neuausgabe unter Erhalt des Originalauftrags
und der zugelassenen Voraussetzungen. Die Diagnose lautet
`research-v2/result-text-too-large`; verworfener Modellinhalt wird nicht gespiegelt.

Keine neuen Aufrufe, Reads, Budgets, Berechtigungen oder Abhängigkeiten. Die
4096-Byte-Ergebnisgrenze ist nicht das Modell-Kontextfenster. Originalversorgung,
kompletter rollen-gebundener Eingangskontext, Schema V7 und der einmalige Repair
bleiben unverändert. Vollständiges überlanges JSON ist kein Transportabbruch.
Die vorhandenen Architekturregeln für typisierte Fehler und einmalige Reparatur
gelten; dieser Bugfix trifft keine neue persistente oder Sicherheitsentscheidung.

## Offline-Nachweis und korrigierte Testabdeckung

Die neue Application-Regression war vor der Korrektur rot (`InvalidValue` statt
präziser Längendiagnose) und ist danach grün. Sie prüft Analyze,
SummarizeOriginals, Design und DesignTests mit ASCII sowie Mehrbytezeichen:
4096 Bytes passen, 4097 beziehungsweise 4098 nicht. Führende/abschließende
Whitespace-Normalisierung bleibt erhalten, leere und Kontrollzeichenwerte ungültig.
Eine Desktop-Regression prüft die reale Fehlerzuordnung, den Unterschied zu
Transporttrunkierung sowie alle sechs Phasen einschließlich maximaler Zahlenwerte
innerhalb von 768 Repairbytes.

Sechs getrennt benannte echte Git-/Index-/Reader-/libSQL-Tests prüfen Analyse,
Entwurf und Testentwurf jeweils mit einmalig und wiederholt überlangem Ergebnis.
Das konservative 8192/2048-Profil bleibt aktiv. Analyse läuft in Ask, Plan und
Agent-Vorbereitung; Entwurf/Testentwurf in den beiden passenden Modi. Der Repair
enthält das gesamte ursprüngliche rollen-gebundene Transkript bytegleich und
keinen Teil der verworfenen Fehlerantwort. Einmaliger Fehler endet nach genau
einem Repair; zweimaliger bleibt ohne Ergebnis und Analyse-Receipt offen. Keine
zusätzlichen Reads, keine Dateiänderung und derselbe dauerhafte Arbeitsstand beim
neu geöffneten Storage-Adapter. Wiederöffnung ist kein Prozesscrashtest.

Dabei wurde eine echte Lücke in zwei bestehenden Namens-Fixtures gefunden:
Die äußere Q2/Q3-Schleife lag vor der Windows-Isolationsgrenze. Deren Kindprozess
beendet sich nach der ersten Fixture; jede Wiederholung startete deshalb erneut
Q2. Die vorherige Aussage über reale Q3-Abdeckung war falsch. Jetzt existieren
vier getrennte Namen für Q2/Q3 und einmalig/wiederholt ungültige Befehlsnamen;
alle vier bestehen. Der [frühere Bericht](10-REQUEST_COMMAND_VALIDATION.md) trägt
eine ausdrückliche Korrektur. Die Application-Tests und bisherigen Live-Matrizen
(Schleifen innerhalb der Future beziehungsweise getrennte Shellaufrufe) sind
von dieser Lücke nicht betroffen. Die native Isolation, ihre Sicherheitsgrenzen
und ihre Retryregeln wurden nicht verändert; ein Kommentar dokumentiert die Falle.
Dies ist keine Aussage über eine vollständige Parametrisierungsprüfung aller Tests.

## Reproduzierbarer Livevergleich

Nur die öffentliche unveränderte Fixture `research-eval-v1`, kein privater
Quelltext oder mutierender Modelltest. Die lokale Batch läuft strikt sequenziell.
Die Cloud-Batch darf daneben laufen; Laufzeiten sind deshalb kein isolierter
Performancevergleich. Native Credentials bleiben in vorhandenen Slots, Settings
werden nicht geändert. Gemeinsame Selektoren:

```text
A3_RESEARCH_ANALYSIS_METHOD=joint
A3_RESEARCH_EVAL_REPETITIONS=1
A3_RESEARCH_EVAL_GROUNDING=format-only
--ignored research_approved_model_matrix --nocapture --test-threads=1
```

- A: vorherige Binärdatei
  `target/reports/research-design-basis-20260908/research-original-basis-v2-tests.exe`,
  SHA-256 `e4c06e4e158040b7f2518adc10702e683368a8a7b2405b1809edbe41fb52342e`.
- B: neue Längendiagnose und korrigierte Variantenabdeckung,
  `target/reports/research-text-limit-20260908/research-text-limit-tests.exe`,
  SHA-256 `2267be11e9b8fe0c0ba8b8279c310dd183814d9000d0aef291e3e4c2887e504`.

Granite wird auf `A3_RESEARCH_EVAL_CASE=3:0` und
`A3_RESEARCH_DESIGN_BASIS=originals` gegenläufig A/B/B/A geprüft. Das ist nur die
bereits freigegebene Stressvariante aus ADR-0105, keine Produktübernahme dieser
Basis. Weitere Nachtests mit B verwenden den unveränderten Standard
`interpretations`: je Ask `1:1` und Plan `3:0` für alle sieben Modelle.

Profile: Qwen 8192/2048, Luna 16384/2048, alle anderen 16384/4096 Kontext-/Output-
Tokens; Parallelität 1, Temperatur 0, frische Capability-Probe. Lokaler Selektor
`A3_LOCAL_RESEARCH_MODEL`; Cloud-Selektoren `A3_RESEARCH_EVAL_PROVIDER=openai|gemini`,
`A3_RESEARCH_EVAL_MODEL` und `A3_CONFIGURED_RESEARCH_CATALOG` auf dem bestehenden
nativen Settingskatalog. Kein direkter Schlüsselzugriff oder Profilwechsel in der App.

## Befund der Gegenprobe

Alle vier Granite-Stressfälle erreichen denselben überlangen primären Q3-Text:
6521 beobachtete rohe Textbytes, nach Trimmen korrekt 6520. Alle Streams enden mit
`Stop`, nicht `OutputLimit`. A meldet allgemeinen Wertfehler, B präzise Textlänge.
A repariert auf 396, B auf 874 Textbytes. Beide Varianten lassen dabei den
verlangten Befehlsnamen aus; die unabhängige Zulassung meldet
`research-v2/request-command-missing`. Je vier Aufrufe, genau ein Repair, Q3 offen,
keine automatische weitere Lesekette. Die Diagnose ist behoben, der fachliche
Reparaturerfolg dagegen **0/2 in beiden Varianten**. Die Eingangsdigests der drei
regulären Phasen sind zwischen A/B identisch; erst der Repairkontext ändert sich.

Eine vollständige Quelle und ein Hinweis auf Auftragsbedingungen garantieren
keine Einhaltung beim Kürzen. Die notwendige Namensprüfung darf nicht gelockert
werden, um daraus einen Abschluss zu machen. Breitere Konflikte zwischen
Auftragsbedingungen, Entwurf und Testplan bleiben ein eigener offener Schnitt.

## Reguläre Nachtests und fachliche Gegenprüfung

| Modell | Ask 1:1 | Plan 3:0 |
| --- | --- | --- |
| gpt-5.6-luna | Abschluss, 4 Aufrufe | Abschluss, 3 Aufrufe |
| gemini-3.8-flash | Abschluss, 3 Aufrufe | Abschluss, 3 Aufrufe |
| gemma-4-26b-a4b-it | offen, 3 Aufrufe | offen, 2 Aufrufe |
| granite4.2:8b | Abschluss, 3 Aufrufe | Abschluss, 4 Aufrufe |
| ornith-1.5:9b | Abschluss, 3 Aufrufe | Abschluss, 3 Aufrufe |
| qwen38-8k:latest | Abschluss, 3 Aufrufe | Abschluss, 3 Aufrufe |
| gemma4:12b | Abschluss, 4 Aufrufe | Abschluss, 3 Aufrufe |

Die reguläre Matrix schließt **12/14** ab, die unveränderte Wortrubrik v3 besteht
**6/14**. Luna repariert Ask-Initialisierung, lokales Gemma fehlende Quellenabdeckung,
Granite den Plan-Befehlsnamen jeweils einmal. Es treten keine identischen
adaptiven Readwiederholungen auf. Google Gemma scheitert in beiden Modi bei der
Bestandsanalyse und deren Einzelrepair an `OutputLimit`: Ask erreicht nach
erfolgreicher Initialisierung 20056/8481 Ausgabebytes, Plan 20137/11191; jeweils
4096 Ausgabetokens. Das ist nicht der hier korrigierte vollständige Textlimitfall.
Ein regulär erfolgreiches Probedokument beseitigt diesen Arbeitsfehler nicht.

Alle sechs abgeschlossenen Ask-Antworten nennen den schreibenden `_log`-Pfad,
Default, Arbeitsverzeichnisauflösung und Append-Verhalten. Dass das genaue Wort
`write` fehlt, belegt deshalb nicht allein einen fehlenden Writer. Die Rubrik
wird dennoch nicht nachträglich gelockert oder mit semantischer Abnahme verwechselt.
Originalvergleich zeigt weiterhin konkrete Fehler:

- Luna, Flash und mehrere lokale Antworten behaupten Speicherung durch
  `save_tasks`. Die beiden Fixture-Methoden geben nur ein Tupel zurück. Ein
  Aufrufname beweist keine Persistenz. Lokales Gemmas Ask vermeidet diese Behauptung,
  sein Plan verlangt dagegen eine tatsächliche Backend-Persistenzprüfung.
- Ornith erfindet in Ask Inline-Zeilenangaben, etwa den Dispatcher in Zeilen 15–17
  statt dessen tatsächlicher Methode in Zeilen 18–20. In der ersten Antwort
  verschiebt es die Pfadauflösung sprachlich zur Task-Erstellung; der Konstruktor
  berechnet den absoluten Pfad bereits früher.
- Ask liefert weiter zwei überlappende Teilantworten statt einer konsolidierten
  Erklärung. Formale Teilpflichtabdeckung ist kein Darstellungsqualitätsnachweis.
- Granites regulärer Q3-Text umfasst nur 401 Bytes und beschreibt pauschal Mocks,
  Erfolg/Fehler und übersprungene Zeilen; konkrete erwartete Werte fehlen.
  Qwen nennt konkrete Fälle, lässt aber unter anderem den UTF-8-Test im Q3-Text aus.
- Lokales Gemmas Test „Nicht vorhandene Datei“ nennt einen existierenden Pfad.
  Es fordert außerdem eine Prüfung gespeicherter Backenddaten, obwohl diese
  Fixture keine solche Persistenz implementiert. Überschriften und Worttreffer
  erkennen diesen Widerspruch nicht.
- Orniths Planbestandsaufnahme vermischt aktuelle APIs mit vorgeschlagenen
  Erweiterungen und behauptet weiterhin Speicherung der gesamten Aufgabenliste.
  Der Testentwurf überträgt diese Bestandsfehler nicht in verifizierte Fakten.

Der Fix ist eine präzise Fehlerklassifikation mit nachgewiesenen Sicherheits-
invarianten, **kein nachgewiesener allgemeiner Qualitätsgewinn des Modells**.
Keine produktive Strategieumstellung oder Freigabe der offenen Coding-Abnahme.

## Messwerte und Rohberichtzuordnung

Insgesamt **18 Fälle, 12 formale Abschlüsse, 6 Wortchecks, 60 Rechercheaufrufe,
288515 kumulative Kontext-UTF-8-Bytes, 16 adaptive Reads, 0 identische Wiederholungen**.
Reguläre Fälle: 44 Aufrufe, 197529 Bytes, 12 Reads; Stressfälle: 16 Aufrufe,
90986 Bytes, 4 Reads. Kumulative Bytes sind weder Tokens noch ein einzelnes
Kontextfenster. Capability-Proben werden nicht als Rechercheaufrufe gezählt.
Alle gestarteten Liveprozesse sind terminal; die Batch-Exitcodes bleiben wegen
der negativen Fälle 1 (lokal acht, Cloud vier nicht bestandene Wort-/Abschlussfälle).

Die drei regulären Granite-Eingabedigests sind in allen vier Stressfällen:
`caf4bea88dc6373eb523124493acd46347c00430aa782a1ab68deebc26692f69`,
`a4ebee8c2f8154e464069aaac133b4aaf58d1602e8c69bb89f169185b62ea61e`,
`f0beba00a16b85565f2f49cf741af36e729f43aedee237c1b9093431a241f14b`.
Repair A: `29d70c9e8d4d5cd0fac443a22c9497a6ac1d430c9f93e3f8e705c3c1ad6b7efc`;
B: `431250303426d22a0b76d78d9983f2a2da256c777f3b8848c1320eadafff217f`.
Dies sind Eingabetranskripte, keine Rohantwort-Hashes.

Der native Settingskatalog bleibt vor und nach den Livefällen unverändert:
SHA-256 `aecc7ef93abe29daa97747f2216e87f49aef99ce9490942fc11ee5c65c09ab32`.
Alle folgenden Dateien liegen in `target/research-eval/`.

| Bericht | Stand/Fall | Modell | Abschluss | Wortcheck | Aufrufe | Kontextbytes | ms | SHA-256 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| eval-1788879427028.jsonl | A Stress | granite4.2:8b | nein | nein | 4 | 22677 | 27434 | b5787974a417a0b74097a7dff92585a2d58c3d4f58e494db9f91ed0c224403a3 |
| eval-1788879437329.jsonl | B Ask | gpt-5.6-luna | ja | nein | 4 | 12131 | 23089 | 243d06083cc442f699f134d813db9bcd741ffd3f4ca4e51c60b8a619eb0fb9d0 |
| eval-1788879458785.jsonl | B Stress | granite4.2:8b | nein | nein | 4 | 22816 | 29739 | a2b3a831451aef1df146317ea3db020929f0feded732fe0852b7bd8dda9ad837 |
| eval-1788879463529.jsonl | B Plan | gpt-5.6-luna | ja | ja | 3 | 21347 | 36082 | 6d8530f9fc1bf4e479237f3c6d4eb6abeb39ddcb226920d6a969e9476e5ca3f9 |
| eval-1788879492498.jsonl | B Stress | granite4.2:8b | nein | nein | 4 | 22816 | 32805 | 8c409c38fa4eff17786df62ce86f59518cfd1ad47c4589ff0c09e1c41109abf7 |
| eval-1788879502443.jsonl | B Ask | gemini-3.8-flash | ja | nein | 3 | 9737 | 21626 | 8b8b298553315baa9d6eed61c17d22d92fa5f50165a11daecb13b1ec9953d64e |
| eval-1788879529320.jsonl | A Stress | granite4.2:8b | nein | nein | 4 | 22677 | 28592 | ecc9a257136a9cf7b1271debf97cc21939d9ab79f9921e973ded1d5e899042e0 |
| eval-1788879532454.jsonl | B Plan | gemini-3.8-flash | ja | ja | 3 | 18487 | 24186 | 53d92fc8e8d63d4fb7cb07458199cc246a088b8c733a5e1753f84496de0932e4 |
| eval-1788879558138.jsonl | B Ask | gemma-4-26b-a4b-it | nein | nein | 3 | 10392 | 178290 | b3b59a487f2e64a3bb6d78c3a9a672ed2d59fde7492419099f12cb2f25f7b76c |
| eval-1788879561649.jsonl | B Ask | granite4.2:8b | ja | nein | 3 | 9737 | 12655 | d443120ac828417ee7d1d37dea2fb75add403d248d8b11a8dfa5fa6e61252daa |
| eval-1788879578323.jsonl | B Plan | granite4.2:8b | ja | ja | 4 | 25340 | 17495 | 64c1340c59b1e16f6dc25e85d940639d395b8ab98d46e0153e3241e8be6f0a0e |
| eval-1788879603611.jsonl | B Ask | ornith-1.5:9b | ja | nein | 3 | 9737 | 14607 | 1f44dd526b4c650d5a881c57ba77dd3e2b2ac5050911bae3976829a26f02d71d |
| eval-1788879623043.jsonl | B Plan | ornith-1.5:9b | ja | ja | 3 | 20300 | 21730 | 561c35318b40306f9bcfa79aa9da983ddc637b52314d2f0fb9ebcb69216220fa |
| eval-1788879656550.jsonl | B Ask | qwen38-8k:latest | ja | nein | 3 | 8295 | 19308 | d9f0819b4a13be3cb79ccbc12aec843a96bd9da7dde8ea69611695621ff7fb62 |
| eval-1788879683634.jsonl | B Plan | qwen38-8k:latest | ja | ja | 3 | 10269 | 25757 | 9d7a4eb6b92265c0f116b0233b607f366abef7d88ee212ccc9fc5bd055a6af19 |
| eval-1788879718110.jsonl | B Ask | gemma4:12b | ja | nein | 4 | 14265 | 20346 | be7cb9a17de1b68f1327127ac1ae0f7def0e4273a12d72e75793186b9ab7cc67 |
| eval-1788879738564.jsonl | B Plan | gemma-4-26b-a4b-it | nein | nein | 2 | 9441 | 175644 | 78667a298e555dd69b1b9453355619b62dd9e472df73b6c26e725296706a6e31 |
| eval-1788879744055.jsonl | B Plan | gemma4:12b | ja | ja | 3 | 18051 | 26893 | 3a98bca1cda65cdb57b9e74c12da6d4c4fe77f533833ab632ace5f5048288ccf |

## Finale Nachweise

- `cargo fmt --all -- --check`: bestanden.
- `cargo test -p a3-application research_result_text_limit --offline --locked`:
  Application-Längengrenzen bestanden (zunächst Rot→Grün).
- `cargo test -p a3-desktop research_result_text_limit --offline --locked`:
  phasenspezifischer, begrenzter Repair und reale Fehlerzuordnung bestanden.
- `cargo test -p a3-desktop research_oversized_results --offline --locked -- --nocapture`:
  sechs getrennte reale Varianten bestanden.
- `cargo test -p a3-desktop research_command_names --offline --locked -- --nocapture`:
  vier getrennte reale Namensvarianten bestanden.
- `cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings`:
  bestanden; `target/reports/research-text-limit-20260908/clippy.log`.
- `cargo test --workspace --all-features --offline --locked`: **1315 bestanden,
  19 ignoriert, 0 fehlgeschlagen, 84 Suites**;
  `target/reports/research-text-limit-20260908/workspace.log`.
- `pnpm check:links`: 156 Markdown-Dateien und 661 lokale Links geprüft.
  Die bestehende Node-Enginewarnung (25.6.1 statt 24.14.0) bleibt; kein Frontendcode
  geändert. `git diff --check` und die abschließende Formatprüfung bestehen.

Die gewöhnlichen Offline-Harness-Gates umfassen auch
Mutation, Verifikation, Cancellation, Freshness und Replan. Es wurde kein neuer
live mutierender Modellprozess gestartet. Diese Nachweise ersetzen weder die
offene semantische Abnahme noch den separat gesperrten SourceGuided-Livestart.
