# Plan 10: Verifikationsprotokoll

## 2026-09-08: Größere Codeversorgung und unabhängige Live-Orakel (ADR-0096/0097)

Ausgangspunkt `957fc75`. [Live-Coding V2](../../fixtures/agent-live-coding-v2/README.md)
erweitert den echten nativen Modelllauf um die unveränderten öffentlichen
Zwei-Modul-Startdateien und ein separat ausgeführtes, nicht in den Modell-Worktree
kopiertes Orakel. Es prüft zusätzlich zwölf Increment-Werte beziehungsweise
49 Preis-/Rabattkombinationen und 35 Rechnungen einschließlich Helferdelegation.
Konstantantworten bestehen die alten sichtbaren Tests, scheitern aber jetzt
nachweislich an den unabhängigen Regressionen. `Done` bleibt von der zusätzlichen
physisch geprüften Korrektheit getrennt. Dies ist weiterhin kein voller
Conversation-Plan→Agent-Übergang und kein allgemeiner Korrektheitsbeweis.

Der erste eingefrorene V2-Build mit Context-Policy V7 scheitert bei Luna in allen
vier Zwei-Modul-Vorprüfungen bereits vor Coding: nur 256 CodeAndEvidence-Tokens,
kein aktueller Originalkörper. Keine Agentmutation oder Coding-Inferenz findet statt.
Ein gezielt roter Context-Regressionstest mit längerem Ziel, wiederholtem Schema
und offener Fehlerevidence bestätigt denselben Verdrängungsfehler.

V8 priorisiert aktuelle Evidence nach allen Pflichtankern vor optionalen
Projektmetadaten und Tool-Ergebnisanteilen. Der Nutzer erhöhte während der
Umsetzung das gewünschte Zielbudget: ADR-0097 ergänzt deshalb die zuerst
dokumentierte ADR-0096-Regel um 4096 optionale Referenztokens bei 16k, also 2048
bei 8k. Bestehende größere Codebereiche werden nicht begrenzt. Nur tatsächlich
verfügbarer Donorplatz wird übertragen; harte Gesamtgrenze, gezählte Pflichtbytes,
Mindestanteile und Output-/Sicherheitsreserve bleiben unverändert. Die halbe
Originalgrenze, höchstens zwei Safe-Reader-Seiten und alle Replan-Grenzen bleiben
erhalten. Die Regression ist nach der Korrektur grün.

Die aktualisierte ausdrückliche Modellauswahl ergänzt im geschlossenen nativen
Testselektor `gemini-3.8-flash`; ein unbekanntes oder falsch gepaartes Modell bleibt
abgelehnt. Kein Provider wird aktiviert, kein Katalog oder Profil gespeichert,
kein Schlüssel ausgegeben. Für die fortgesetzte Matrix ersetzen Google Flash und
lokales `gemma4:12b` die zuvor untersuchten Google-Gemma-/GPT-OSS-Fälle.

Die gegenbalancierten V8-Zwei-Modul-Läufe verwenden A=`staged`, B=`guided` in
Reihenfolge A1→B1→B2→A2. Luna bekommt jetzt beide Originalmodule und besteht
alle vier Läufe einschließlich unabhängigem Orakel und unveränderten geschützten
Dateien/Einstellungen. A benötigt jeweils vier, B jeweils drei Modellaufrufe.
Die vollständige Matrix und abschließenden lokalen Gates sind ausgewertet;
das gemischte Ergebnis rechtfertigt keine allgemeine Strategieübernahme.

Die abgeschlossenen lokalen Vergleiche zeigen die Grenzen der erweiterten Aufgabe:

- Qwen erhält beide Originalmodule, führt aber in allen vier Läufen 29–31
  Read-Aktionen ohne Mutation aus. Die vorhandene 120-Sekunden-Versuchsgrenze
  beendet den laufenden nächsten Turn. Ein größeres Kontextpaket verhindert
  diesen Ablauf nicht. Die vorhandenen Logs identifizieren nicht jede konkrete
  Leseanforderung; gleiche Schema-/Nachrichtengröße beweist keine identischen
  Pfad-/Bereichsidentitäten. Typisierte Fortschrittsdiagnostik bleibt notwendig.
- Granite: A1 und B1 ändern beide Dateien fehlerhaft, führen zwei rote Tests
  aus und scheitern an `InvalidReplanAnalysisAfterRepair(Decode(InvalidValue))`.
  A2 scheitert nach roten Tests an `RepeatedReplanRead`; B2 besteht vollständig.
  Kein roter Test beendet den Schritt als verifiziert.
- Ornith: A1 erzeugt nach acht Reads eine fehlerhafte Änderung und endet später
  in `NoContentChange`; A2 hat unabhängig korrekten Code, erreicht aber wegen
  derselben Ablehnung keinen verifizierten Runabschluss. B1 besteht; B2 erreicht
  `Done` mit grünen alten Tests, scheitert jedoch am Zusatzorakel.
- Gemma4 erreicht in allen vier Läufen `Done` mit grünen alten Tests. Nur B2
  besteht zusätzlich das unabhängige Orakel. A1, B1 und A2 sind deshalb
  ausdrücklich keine erfolgreichen Liveabnahmen.

`NoContentChange` vergleicht weiterhin den vorgeschlagenen Inhalt mit dem vom
Modell behaupteten Basishash, nicht automatisch mit aktuellen Datei-Bytes.
Eine solche Ablehnung darf nicht als ausführbarer Prüfauftrag interpretiert werden.

Damit sind bereits vier Done-Läufe mit unzureichender Verhaltensabdeckung
sichtbar geworden. Der Test-Goal-Contract besitzt bewusst weiterhin nur die
alte Testcommand-Verifikation; das Orakel ist ein zusätzlicher externer
Abnahmeschritt, kein nachträglich injizierter Agentbeleg. Die vollständige
Planübergabe mit ausreichend starken Kriterien ist weiter offen.
Die erste V2-Version verwendete für das negative Orakel eine Assertion im
besessenen Test-Worker; dessen Panic verursachte zusätzlich eine irreführende
Closed-Channel-Meldung. Nach den eingefrorenen Messungen gibt dieselbe
Erfolgsschranke einen normalen Fehler zurück. Die negative und positive
Offline-Orakelregression prüfen diesen Rückgabepfad; Erfolgskriterien und
Modellkontext ändern sich dadurch nicht.

Eingefrorene Artefakte:

- V2/V7-Vorprüfungen: `target/reports/agent-live-v2-20260908/agent-tests.exe`,
  SHA-256 `6de3dd7db328640d5026e4e14f1392872e298bbb137ec025a7330b6f6190ae18`.
- V2/V8 Luna: `target/reports/agent-live-v8-20260908/agent-tests.exe`,
  SHA-256 `d69635cc587865a444534f265d70bced717a22483485f074e06b8ea137847446`.
- V2/V8 mit zusätzlich zugelassenem Google-Flash-Testziel:
  `target/reports/agent-live-v8-20260908/agent-tests-flash.exe`,
  SHA-256 `5a98ab52a4219638ebca1868b8e74dff77499ef9fcbfbb1c37d4f6c0308ac23b`.
  Gegenüber dem vorherigen V8-Build sind ausschließlich der Testselektor und sein
  Scope-Test ergänzt; Harness, Budget, öffentliche Aufgaben und Profile identisch.
  Innerhalb jedes A/B-Vergleichs ist das Binary unverändert. Lokale Modelle laufen
  strikt nacheinander, während der Livevergleiche keine Builds.

### Abgeschlossene Zwei-Modul-Matrix

Abschließende Gates sind grün:

- `cargo fmt --all -- --check`
- `cargo test -p a3-domain context_pack --offline --locked --jobs 2` (7 Budgettests)
- `cargo test -p a3-context --offline --locked --jobs 2` (7 Unit-/24 Context-Tests)
- `cargo test -p a3-desktop --lib live_fixture::coding:: --offline --locked --jobs 2 -- --test-threads=1` (5 Offline-Regressionen; der Livefall bleibt opt-in)
- `cargo test -p a3-desktop --lib research_live_target --offline --locked --jobs 2 -- --test-threads=1` (2 Scope-/Providergrenzen)
- `cargo clippy --workspace --all-targets --all-features --offline --locked --jobs 2 -- -D warnings`
- `cargo test --workspace --all-features --offline --locked --jobs 2 -- --test-threads=1`
- `pnpm check:links` und `git diff --check`

Die Gesamtsuite enthält den tatsächlichen Ein-/Zwei-Datei-Harness, unveränderte
E9-Goldens, die Safe-Reader-/Live-Edit-Grenzen und den nativen
`connection_lifecycle`-Test ohne Crash-Retry. Keine Frontendänderung. Die bekannte
Node-25.6.1-statt-24.14.0-Warnung bleibt bestehen; der Linkcheck besteht mit
142 Markdown-Dateien/588 lokalen Links. Alle Cargo-Prozesse verwenden
`CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`.

- Clippylog `target/reports/agent-live-v8-clippy.log`, SHA-256
  `5bd6b8d054713a5670197904d2e9ac7089bd4b4392d0f8bb7f22a0818991f229`.
- Workspace-Log `target/reports/agent-live-v8-workspace.log`, SHA-256
  `3315c03b478339c1c64947dbf0dd2b1c082537c0358cbcbd55dad96aa7f104f2`.

Die vier anfänglich gescheiterten V7-Preflights bleiben unter
`target/reports/agent-live-v2-20260908/` erhalten:

| Log | SHA-256 |
| --- | --- |
| luna-two-A1.log | `633d1a2f981be3217c96710991ee69a30fe9c0c8def96ba0617b296cea84a4f8` |
| luna-two-A2.log | `51a84a74dc688faa27b9dc6a03b219c19bd490ffe3d9a3e3826d47eee31196e5` |
| luna-two-B1.log | `131a8940cb639bfc4b07dc7e563bd9741d4d5ae0bc8a10a67544101840e77660` |
| luna-two-B2.log | `3de15426e1d6298ecb1bd204a83413d209b3747201dde60aaaef57387bc29211` |

Alle 24 V8-Läufe bestehen den Original-Preflight mit beiden Modulen; alle bewahren
geschützte Dateien und native Einstellungen. Sieben Läufe bestehen die vollständige
Liveabnahme, elf erreichen den alten commandgebundenen Done-Status. Vier dieser elf
werden durch das Zusatzorakel abgelehnt. A (`staged`) besteht 2/12, B (`guided`)
5/12; die kleine Stichprobe erlaubt keine allgemeine Produktfreigabe. `baseline`
(`SingleAction`, produktiver Standard) ist nicht Gegenstand dieses A/B-Vergleichs.
Google Flash besteht die Capability-Vorbereitung, erhält beide Originale und endet
in allen vier Läufen nach 25–31 Reads ohne Mutation an der Versuchsfrist. Das ist
kein erneut aufgetretener Unsupported-/Unavailable-Befund. Die größere Codeversorgung
behebt nicht die weiter offene normale Read-Fortschrittsführung.

Profile: Luna 16384/2048 mit RepeatSchemaInPrompt, Qwen 8192/2048 mit FormatFieldOnly;
übrige Modelle 16384/4096 mit FormatFieldOnly. Aufrufe sind tatsächliche Startmeldungen
einschließlich abgebrochener Aufrufe. Tokens sind die letzte dauerhafte Runbuchung;
bei Abbruch können konservativ reservierte Kosten enthalten sein. Sekunden umfassen
auch Modellvorbereitung, Indexierung, reguläre Freigaben und unabhängige Nachprüfung.
Pro Modell war die Reihenfolge A1, B1, B2, A2; die Tabelle ist nach Dateiname sortiert.

| Log | Aufrufe | Prompt/Output gebucht | Aktionen/Repairs | Sekunden | Done | Abnahme |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| flash-two-A1.log | 64 | 236181/6573 | 31/0 | 122.96 | nein | nicht bestanden |
| flash-two-A2.log | 58 | 214035/6332 | 28/0 | 124.89 | nein | nicht bestanden |
| flash-two-B1.log | 51 | 183911/1973 | 25/0 | 124.71 | nein | nicht bestanden |
| flash-two-B2.log | 62 | 224518/2401 | 31/0 | 122.70 | nein | nicht bestanden |
| gemma4-two-A1.log | 4 | 12298/429 | 2/0 | 32.09 | ja | nicht bestanden |
| gemma4-two-A2.log | 4 | 12356/434 | 2/0 | 23.84 | ja | nicht bestanden |
| gemma4-two-B1.log | 3 | 9133/409 | 2/0 | 22.47 | ja | nicht bestanden |
| gemma4-two-B2.log | 3 | 9178/420 | 2/0 | 21.60 | ja | bestanden |
| granite-two-A1.log | 9 | 22685/827 | 4/1 | 25.79 | nein | nicht bestanden |
| granite-two-A2.log | 12 | 30423/951 | 5/1 | 26.92 | nein | nicht bestanden |
| granite-two-B1.log | 8 | 20100/788 | 4/1 | 22.49 | nein | nicht bestanden |
| granite-two-B2.log | 3 | 7172/512 | 2/0 | 15.27 | ja | bestanden |
| luna-two-A1.log | 4 | 6956/294 | 2/0 | 10.70 | ja | bestanden |
| luna-two-A2.log | 4 | 7000/294 | 2/0 | 9.76 | ja | bestanden |
| luna-two-B1.log | 3 | 5335/276 | 2/0 | 8.72 | ja | bestanden |
| luna-two-B2.log | 3 | 5314/276 | 2/0 | 9.23 | ja | bestanden |
| ornith-two-A1.log | 49 | 172618/2694 | 23/1 | 85.42 | nein | nicht bestanden |
| ornith-two-A2.log | 7 | 20791/1350 | 2/1 | 30.74 | nein | nicht bestanden |
| ornith-two-B1.log | 5 | 14679/412 | 3/0 | 19.55 | ja | bestanden |
| ornith-two-B2.log | 3 | 8752/495 | 2/0 | 18.10 | ja | nicht bestanden |
| qwen-two-A1.log | 62 | 153912/4393 | 30/0 | 131.54 | nein | nicht bestanden |
| qwen-two-A2.log | 59 | 147550/4260 | 29/0 | 126.67 | nein | nicht bestanden |
| qwen-two-B1.log | 63 | 157213/4406 | 31/0 | 126.63 | nein | nicht bestanden |
| qwen-two-B2.log | 62 | 154095/4355 | 30/0 | 126.51 | nein | nicht bestanden |

Die Rohlogs liegen unter `target/reports/agent-live-v8-20260908/`.
SHA-256 zur eindeutigen Zuordnung:

| Log | SHA-256 |
| --- | --- |
| flash-two-A1.log | `fb2102bde704f8f736038fe870c3513ca469cba1868bfc3163db41e9c41fd415` |
| flash-two-A2.log | `e0781631fcbb0709dbf7e9765a533a5ccd77714e2e0f1f1414da07a84d844ba6` |
| flash-two-B1.log | `2fa28744828987cf89e12c5d027a38557129afd5f3e52ad54055d65a82782eb7` |
| flash-two-B2.log | `bb667a0cbfbf09cb844bf10153afadcba4c58031477613eae27be0f710e41d97` |
| gemma4-two-A1.log | `38e876f1469b1edd01bd249231915535e379ff64f8957a95752f3b803b4c0594` |
| gemma4-two-A2.log | `02b3da099f77207a3cc174488f15c04b8d4cb149280f5c08d685c9a614ce35b1` |
| gemma4-two-B1.log | `1fe578668aa290156509704ebcc91f74f775a93963be3254c933fd40d1dada9f` |
| gemma4-two-B2.log | `f4bd8edcf556e263c00f0237e72296a84233314a4e533433878a0b8b422597e6` |
| granite-two-A1.log | `6aa0da06c4d1c80b903767a3cc5aab94b1cd5ae877ae7727fab58d328c7fdddb` |
| granite-two-A2.log | `ecfb94a390aeb3eb6187cd69da9cf8217e2fad4e0dde402cf3b153de7e0ca885` |
| granite-two-B1.log | `e7ed30714da1ee7d96138d1126384c4ab0ac25347997bb5ab3f932b5b6cc203c` |
| granite-two-B2.log | `1cffbafb9689f44ae5dd2b8dc39e64b0d6f3f035ad9d64b8d6e3f9fd8b76a709` |
| luna-two-A1.log | `6d99f59ce522fa5f2bd5a93ef951ef8da0205ba7f82df7bca3a5f9d3caadc07f` |
| luna-two-A2.log | `1bc7f77e02d1ad7b7d819aa19419cb2b53f65b96789f5dd08ce4d841915b42da` |
| luna-two-B1.log | `dbc18be1d9c5bc7705fc2098a1f92eb1d0bc4a7866636668561d17e0495d863c` |
| luna-two-B2.log | `83b12048d54afa6f3a9bc60fb41cf9da724dc3896abe56ec189e4f16c35d91c7` |
| ornith-two-A1.log | `a89e776650655f954991610acd53031490f606e13f8709cea895cb7955444106` |
| ornith-two-A2.log | `f06ac01f2b7ebbab9ed2d812cdf395a93d2ef70afa2bbebe9db476594bf2bd0e` |
| ornith-two-B1.log | `5628c58cf0e887fb4986b6c5be9f83cbdd0a754a424e3514b07b8977dfc6d6c1` |
| ornith-two-B2.log | `d23506df25d3169ae6337344c9d365ea8d5983d9988351ae1308b966f9e5b225` |
| qwen-two-A1.log | `291db20073aee0a7b0cc00337e3cb2b0fd093b02f6ed3deaca8408e6448738d2` |
| qwen-two-A2.log | `f4ff6575ce8f9dd6f81a86b2803df4e0e4b6a2f423c199cdaf6edcd91a034c3f` |
| qwen-two-B1.log | `37447cb3580d222e9ef5e0c4b8bb71f29aacbb7df740ae02e82e66121334a461` |
| qwen-two-B2.log | `c76ea168eb1cc612b439edef7ffffbb9940740970ae31275a2018483c35b055f` |

## 2026-09-07: Geführte Nachentscheidung nach Patch (ADR-0095)

Ausgangspunkt `ade289f`. Die dritte Vergleichsvariante `guided` ergänzt nach einer
aktuellen gebundenen Patchquittung und bei vorhandener operationaler Prüfung die
geschlossene Entscheidung `verify`, `continue_change` oder `need_evidence`.
Sie ist kein automatischer Test nach jedem Teilpatch und keine Selbstverifikation.
Bei `verify` erzeugt der Core allein den vorhandenen Run-Auftrag, der unabhängig
decodiert und anschließend durch denselben Mutationscontroller geprüft wird.
Ask/Plan/Replan und der Produktstandard bleiben unverändert.

28 gezielte Agent-Turn-Tests bestehen einschließlich der fünf neuen Regressionen:
bekannter Prüfauftrag ohne Argumentinferenz, aktuelle Patch-/Schrittvoraussetzungen,
getrennte Read-/Change-Wege mit gemeinsamem Einzelrepair, Kontextwechsel nach der
Nachentscheidung sowie Cancellation/OutputLimit vor einem möglichen Prüfauftrag.
Clippy über alle Workspace-Targets/-Features mit `-D warnings` besteht initial.

Der reale zusätzliche Coding-Harness besteht für Ein-Datei- und Zwei-Dateiänderung:
Patch/Approval, Index, dauerhafte Quittung, echte Context Compilation, eine
Modellentscheidung, tatsächlicher Python-Testprozess, Evidence und Acceptance.
Die Patchinhalte stammen in diesem Offlinevertrag weiterhin vom Testtreiber;
das Stubmodell liefert nur `verify`. Das ist kein Live-Nachweis einer selbständig
erarbeiteten Mehrdateiänderung. Die bestehende kleine Live-Fixture prüft nur
`increment(41) == 42`; breitere Orakel und volle Planübergaben bleiben offen.

Der abgeschlossene Livevergleich isoliert die neue Nachentscheidung: bisheriges
`staged` gegen `guided`, pro Modell A → B → B → A auf demselben eingefrorenen Build,
unveränderten Profilen und Freigaben. Modellreihenfolge: Granite, Luna, Qwen,
Ornith, GPT-OSS, Google. Lokale Modelle liefen strikt nacheinander; während der
Messungen liefen keine Builds. Die abschließenden lokalen Gates bestehen.

Geprüft wurden `cargo fmt --all -- --check`,
`cargo test -p a3-application --lib agent_turn::tests --offline --locked --jobs 2`,
`cargo test -p a3-agent-harness-tests --test coding_tasks guided:: --offline --locked --jobs 2 -- --test-threads=1`,
`cargo clippy --workspace --all-targets --all-features --offline --locked --jobs 2 -- -D warnings`
und `cargo test --workspace --all-features --offline --locked --jobs 2 -- --test-threads=1`.
Der unveränderte bisherige E9-Golden-Vertrag und die Storage-Verbindungsregression
bestehen auch in der Gesamtsuite. Pro Prozess gelten `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`. Keine Frontendänderung.
`pnpm check:links` und `git diff --check` bestehen; die bekannte Node-Abweichung
25.6.1 statt 24.14.0 bleibt als Warnung sichtbar. Alle Rust-Prüfungen betreffen
denselben Quellstand wie das eingefrorene Livebinary. Gateartefakte:

- `target/reports/guided-agent-clippy-initial.log`, SHA-256
  `a6e9e88134519f61c9a1707ce219d762a5b3c9b1fc433c47b2ab4ac437980b52`.
- `target/reports/guided-agent-workspace-final.log`, SHA-256
  `52f3c847afa31be9b9bfed97a7213190183e0730ebd296de7435b49e31f034e4`.

### Ergebnisse und Grenzen

| Modell | Staged: Done | Guided: Done | Befund |
| --- | ---: | ---: | --- |
| granite4.2:8b | 0/2 | 2/2 | Beide Gegenläufe NoContentChange nach Patch; guided jeweils Patch + echte Prüfung ohne Repair |
| gpt-5.6-luna | 2/2 | 2/2 | Ein guided-Lauf fordert zunächst einen weiteren Read an; keine allgemeine Tokenersparnis |
| qwen38-8k:latest | 2/2 | 2/2 | Guided jeweils drei statt vier Modellaufrufe, keine Repairs |
| ornith-1.5:9b | 0/2 | 2/2 | Gegenlauf 1: 42 Reads bis Zeitgrenze ohne Patch; Gegenlauf 2: ein Read, Patch, NoContentChange |
| gpt-oss:20b | 0/2 | 0/2 | Probe bestanden, jeweils InvalidResponse vor Patch; Nachentscheidung nie erreicht |
| gemma-4-26b-a4b-it | 0/1 Coding-Start | kein Coding-Start | Drei Versuche vor Coding nicht verfügbar; keine gepaarte Aussage zur Strategie möglich |

Beide erfolgreichen Ornith-Läufe führen nach dem Patch genau einen
AfterChange-Aufruf und danach die echte Prüfung aus. Davor benötigen sie
18 beziehungsweise null Reads. Die Nachentscheidung ist vor dem ersten Patch
noch inaktiv; diese Schwankung beweist ausdrücklich keine verbesserte
Vorrecherche. Im ersten Gegenlauf cancelt die vorhandene 120-Sekunden-Grenze des
Testversuchs einen laufenden Read nach 42 ausgeführten Reads; es gibt null
Mutationsquittungen und keine physische Änderung. Dieser Lauf ist **kein**
erneuter NoContentChange-Fall. Der zweite Gegenlauf belegt dagegen diese genaue
Fehlerklasse. Auch hier beweist NoContentChange nur die Übereinstimmung von
vorgeschlagenem Inhaltshash und vorgeschlagenem Basishash, nicht deren Aktualität.

Lunas zusätzlicher Read durchläuft den zugelassenen `need_evidence`-Zweig und
kehrt danach zur Nachentscheidung zurück. Über beide Versuche verbraucht guided
10795 gegenüber 9604 Prompttokens: weniger Aufrufe sind nicht universell belegt.
Qwen benötigt dagegen zusammen 13306 gegenüber 17850 Prompttokens, rund 25 %
weniger in diesem Fixture. Die Laufzeiten umfassen Probe, Modellladung und
Vorbereitung und sind keine allgemeine Geschwindigkeitsgarantie.

Googles erster Staged-Lauf führt zwei rote Testprozesse ohne Patch aus, liest
im Replan und endet bei dessen Analyse mit Unavailable. Die anderen drei
Versuche erreichen bereits die Modellvorbereitung nicht. Die Meldung belegt
keinen HTTP-Status, kein Rate-Limit und keine konkrete Schemaursache. Ebenso
beweist GPT-OSS' InvalidResponse weder pauschales Modellalter noch eine fehlende
Structured-Output-Fähigkeit; die vorgelagerte Probe besteht.

Alle 21 Versuche mit Coding-Start bewahren die geschützten Dateien. Alle zwölf
erfolgreichen Versuche bestätigen zusätzlich unveränderte native Einstellungen,
aktuelle Step-Verifikation und den unabhängigen gesperrten Fixturetest. Das bleibt
ein Ein-Eingabe-Test und kein vollständiger Plan→Agent-Handoff. Die Variante wird
wegen der reproduzierten Verbesserungen bei Granite und Ornith weiterverfolgt,
aber noch nicht als Produktstandard aktiviert. Produktive Evidenzfrontier,
Providerdiagnose, breitere Orakel und echte Mehrdatei-/Planübergaben bleiben offen.

| Lauf | Aufrufe | Gebuchte Prompt-/Outputtokens | Sekunden |
| --- | ---: | ---: | ---: |
| Granite A1 | 5 | 12846 / 582 | 18,10 |
| Granite B1 | 3 | 7569 / 181 | 11,94 |
| Granite B2 | 3 | 7595 / 181 | 11,71 |
| Granite A2 | 5 | 12875 / 528 | 15,64 |
| Luna A1 | 4 | 4818 / 164 | 11,75 |
| Luna B1 | 6 | 7164 / 215 | 12,40 |
| Luna B2 | 3 | 3631 / 135 | 8,81 |
| Luna A2 | 4 | 4786 / 153 | 11,00 |
| Qwen A1 | 4 | 8949 / 239 | 30,25 |
| Qwen B1 | 3 | 6664 / 263 | 22,38 |
| Qwen B2 | 3 | 6642 / 218 | 21,35 |
| Qwen A2 | 4 | 8901 / 289 | 24,85 |
| Ornith A1 | 86 | 326673 / 6589 | 127,94 |
| Ornith B1 | 39 | 143669 / 1157 | 66,15 |
| Ornith B2 | 3 | 9576 / 172 | 14,59 |
| Ornith A2 | 7 | 22715 / 564 | 22,68 |
| GPT-OSS A1 | 1 | 6932 / 4096 | 18,18 |
| GPT-OSS B1 | 1 | 6932 / 4096 | 11,90 |
| GPT-OSS B2 | 1 | 6932 / 4096 | 11,94 |
| GPT-OSS A2 | 1 | 6932 / 4096 | 11,93 |
| Google A1 | 6 | 22986 / 4181 | 8,73 |
| Google B1 | 0 | nicht verfügbar | 0,43 |
| Google B2 | 0 | nicht verfügbar | 0,41 |
| Google A2 | 0 | nicht verfügbar | 0,43 |

Aufrufe zählen den Controller einschließlich Replan/Repair, nicht die Probe.
Bei abgebrochenen Providerantworten enthalten die gebuchten Tokens die
konservative Reserve, nicht behauptete tatsächliche Providerabrechnung.

### Reproduzierbare Artefakte

Alle Logs liegen unter `target/reports/guided-agent-20260907/`.
Gemeinsames `agent-tests.exe`, SHA-256
`89dd6dc1b161284b3fc1908bdaf883b64505c21ad293b14c08acc99ef3829b7a`.

| Log | SHA-256 |
| --- | --- |
| granite-staged-1.log | `0089a3281c9b0e1a9c752426bcebf5255c5bb7b1729d27388158d63c1e8d0a08` |
| granite-guided-1.log | `70f3368726b5c082a5c7145ab5eb5ab9db4c0aa7d0dc28b02e1d0ff99fb32f9c` |
| granite-guided-2.log | `6311f523a353f81e7f45807025d7b70802cd577ca387647b24497ad8df104824` |
| granite-staged-2.log | `d704bdb82534e92c2cd816994aa42d266fd7623dfb4089089577d83ca7aa3ce1` |
| luna-staged-1.log | `1c5d36cad3ba34c15678bde3ff1c298f29f28c0757307ea4296c2313947da2fb` |
| luna-guided-1.log | `72480775fbf64956138294d7db5a0a1e5874cec392e28a74fc8573adb2903784` |
| luna-guided-2.log | `39bdcd60a019aaf5f320f85e0c80731c8d6e0c3b5c5224354df3427f9c10b75f` |
| luna-staged-2.log | `982e4a107eadc1d911511facaeec720b9efbb78b81a2a246861cad53c8579e2f` |
| qwen-staged-1.log | `5a3efca9a0af9b9b5eaf58a3617ed3a58534eb9925797e6a2f089e85e3eca2a3` |
| qwen-guided-1.log | `a4021d991aadddfb9f615cf20e37bfea027a0ffbb6de3812749e17d1c5d51aa0` |
| qwen-guided-2.log | `926d81935ec324e2c01c80c0ea482efcf7e98c4c7803d4db864774f5f809ba07` |
| qwen-staged-2.log | `a7240b2fdf0246e16e1947b3bee00017b95b5cb765fdc1c49b413e1586fb8b76` |
| ornith-staged-1.log | `34e204da3bf4d3f8921b5f276df0017197df29415df57233bfd51de7dbbd7392` |
| ornith-guided-1.log | `d47bd32e666ef5e3d1ad5ce800f23afee5762e052a9851ecff965535327dd710` |
| ornith-guided-2.log | `da4910e67ff08c6f23325b18a44bde52b5d5d1268cef209ea8f81a324212bed4` |
| ornith-staged-2.log | `fe0cf69dca3f7bd8296876ee336a3ad9e61f32d6316e312200a0f93de73dda7f` |
| gptoss-staged-1.log | `17b8b2c5e6382fc54574c1f0c64a7205f1834e4a0ac6ddb80588ac1d63b9da5a` |
| gptoss-guided-1.log | `ad5e9f956044877781fb249a89f27abc57296b4b79a95d920a40031fc389e0a3` |
| gptoss-guided-2.log | `9d0a83d8aafb6e4fa8b76091d1f866d6997ae36d33b8852d0c4937a06bb20a16` |
| gptoss-staged-2.log | `21149dcf659c642080a781f8d323d86ba936383ce0d8e0f3fd75d73add80f751` |
| google-staged-1.log | `8bc9fb50911b8a77d5b9a6a26dad2660fc4a9b56b666a07091a3c3e9da7da18f` |
| google-guided-1.log | `b67abe792986f54869dd320c691613b4bd15721f2be7db868e55e5dae021b3bb` |
| google-guided-2.log | `6714b263e97875055708c74d8e4ec00264c73e4a811867de58852065a877dffb` |
| google-staged-2.log | `da515d3dcc6161a649e010800b3577b96d167af65520e1c7539bbaa27282514c` |

## 2026-09-07: Auswahl und Argumente getrennt im echten Harness (ADR-0094)

Ausgangspunkt `62b8fb3`. Der neue Application-Vergleichspfad führt zuerst
ActionChoice V1 und anschließend ausschließlich die Argumente der festgelegten
Operation aus. Bekannte Controllerkonstanten ergänzt der Core; aktuelle Dateien,
Hashes, Policy, Approval und tatsächliche Verifikation bleiben unabhängig.
Ask/Plan/Replan und der Produktstandard wurden nicht umgestellt. Eine optionale
Modellselbstbewertung wurde nicht als neuer Verifikationsweg eingebaut.

Das unveränderte öffentliche `small-local-bugfix`-Fixture verlangt ausschließlich
die Korrektur von `increment.py`, unveränderte geschützte Tests und den exakten
offline ausgeführten Befehl `python -m pytest`. Nach jeder Mutation kontrolliert
der normale Produktionspfad Publish und Verifikation; die Fixture führt außerdem
einen unabhängigen physischen Test aus. Das ist weiterhin ein bestätigter
Ein-Schritt-Plan, **kein** vollständiger Conversation-Plan→Agent-Handoff.
Der bestehende Test prüft nur `increment(41) == 42`. „Grün“ beziehungsweise
„physisch richtig“ bezeichnet hier genau diesen unveränderten Fixturetest,
nicht den Beweis korrekten Verhaltens für sämtliche Eingaben. Vor einer breiten
Praxisfreigabe sind unabhängige weitere Eingaben und komplexere Aufgaben nötig.

### Gegenbalancierter Vergleich

Je Modell wurden Baseline 1 → Staged 1 → Staged 2 → Baseline 2 auf demselben
eingefrorenen Binary ausgeführt. Alle lokalen Modelle liefen nacheinander.
Profile unverändert: Luna 16k/2048 mit Schemawiederholung, Qwen 8k/2048;
Google Gemma und die übrigen lokalen Modelle 16k/4096 ohne Schemawiederholung.
Keine native Einstellung wurde für eine Variante umgeschaltet.

| Modell | Baseline: verifiziertes Done | Staged: verifiziertes Done | Zusätzlicher Befund |
| --- | ---: | ---: | --- |
| gpt-5.6-luna | 2/2 | 2/2 | Staged halbiert nicht die Aufrufe, sondern verkleinert ihre Verträge |
| gemma-4-26b-a4b-it | 0/2 Versuche | 0/2 Versuche | Providerfehler; zweiter Baselineversuch scheitert bereits an der Probe |
| ornith-1.5:9b | 0/2 | 0/2 | Staged korrigiert beide Dateien physisch richtig, aber viele Reads und erneuter Patch statt Abschluss |
| qwen38-8k:latest | 0/2 | 2/2 | Staged jeweils Patch + erfolgreicher Test + verifiziertes Done ohne Repair |
| gpt-oss:20b | 0/2 | 0/2 | InvalidResponse auch bei kleiner Auswahl; nicht als pauschaler Alters-/Fähigkeitsbeweis interpretieren |
| granite4.2:8b | 0/2 | 0/2 | Staged korrigiert beide Dateien physisch richtig, scheitert anschließend am erneuten Patch |

Ein physisch grüner Test bei nicht abgeschlossenem Run zählt ausdrücklich nicht
als Taskerfolg. In allen 23 Versuchen mit tatsächlichem Coding-Start bleiben die
geschützten Dateien unverändert. Google Baseline 2 hat keinen Coding-Start und
keinen solchen Fixture-Nachweis; es ist kein regulärer gepaarter Strategietest.
Google-Unavailable allein beweist weder einen HTTP-Status noch eine Schemaursache.

| Lauf | Modellaufrufe | Gebuchte Prompt-/Outputtokens | Sekunden | Physischer Test / Done |
| --- | ---: | ---: | ---: | --- |
| Luna B1 | 2 | 8238 / 356 | 10,47 | grün / ja |
| Luna S1 | 4 | 4792 / 153 | 10,52 | grün / ja |
| Luna S2 | 4 | 4778 / 155 | 9,20 | grün / ja |
| Luna B2 | 2 | 8255 / 360 | 9,07 | grün / ja |
| Google B1 | 5 | 21282 / 4248 | 57,27 | rot / nein |
| Google S1 | 1 | 6931 / 4096 | 2,34 | rot / nein |
| Google S2 | 6 | 22956 / 4171 | 8,68 | rot / nein |
| Google B2 | 0 | nicht verfügbar | 0,44 | kein Coding-Start |
| Ornith B1 | 1 | 3129 / 4096 | 46,95 | rot / nein |
| Ornith S1 | 83 | 310639 / 3275 | 118,69 | grün / nein |
| Ornith S2 | 63 | 233897 / 2577 | 96,10 | grün / nein |
| Ornith B2 | 3 | 9827 / 743 | 19,26 | rot / nein |
| Qwen B1 | 11 | 26463 / 1426 | 63,98 | rot / nein |
| Qwen S1 | 4 | 8911 / 247 | 24,32 | grün / ja |
| Qwen S2 | 4 | 8925 / 248 | 24,35 | grün / ja |
| Qwen B2 | 8 | 18832 / 1297 | 50,39 | rot / nein |
| GPT-OSS B1 | 2 | 9264 / 4208 | 20,82 | rot / nein |
| GPT-OSS S1 | 1 | 6931 / 4096 | 11,98 | rot / nein |
| GPT-OSS S2 | 1 | 6931 / 4096 | 11,93 | rot / nein |
| GPT-OSS B2 | 2 | 9252 / 4211 | 14,54 | rot / nein |
| Granite B1 | 5 | 12508 / 411 | 17,55 | rot / nein |
| Granite S1 | 5 | 12867 / 549 | 15,92 | grün / nein |
| Granite S2 | 5 | 12916 / 516 | 15,62 | grün / nein |
| Granite B2 | 5 | 12417 / 435 | 15,37 | rot / nein |

Modellaufrufe betreffen den Controller einschließlich Replan/Repair, nicht die
vorherige Capability-Probe. Gebuchte Tokens enthalten bei Streamfehlern die
konservative Reserve und sind dann keine behauptete Providerabrechnung.
Sekunden sind der gesamte isolierte Test einschließlich Probe und Vorbereitung;
inhaltsfreie Completion-Zeilen bewahren daneben die gemeldete Provider-Usage und
Anfragelaufzeit. Keine parallelen Builds liefen während der Modellmessungen.

Für die zwei erfolgreichen Luna-Paare sinken die gesamten Prompttokens von
16493 auf 9570 (rund 42 %) und Outputtokens von 716 auf 308 (rund 57 %), bei
nahezu gleicher Gesamtzeit: 19,54 gegenüber 19,72 Sekunden. Dies ist nur ein
gemessener Fixturebefund, kein allgemeiner Performance- oder Zuverlässigkeitsclaim.
Bei Ornith steigen dagegen Aufrufzahl und Verbrauch stark; gültige Struktur
verhindert keine semantische Lese-/Aktionsschleife. Die Logs belegen viele Reads,
nicht deren vollständige paarweise inhaltliche Identität.

### Artefakte und Folgerung

Alle 24 Logs liegen unter `target/reports/staged-agent-20260907/`, jeweils
`{luna,google,ornith,qwen,gptoss,granite}-{baseline,staged}-{1,2}.log`.
Gemeinsames Binary `agent-tests.exe`, SHA-256
`637026d9e1cd43ef92f57cba0257d3520a7ecfef0ab29fda5f34fbb2707cda72`.
Die Variante bleibt als kontrollierter Vergleichspfad erhalten, wird aber
aufgrund der gemischten Ergebnisse nicht automatisch Produktstandard.

Die Folgearbeit wird anhand dieser tatsächlichen Befunde ausgerichtet:

- bereits gelieferte/erledigte Evidenz und produktive nächste Schritte genauer
  Core-seitig führen, statt lediglich mehr gültige Auswahlantworten zu erzeugen;
- den Übergang nach einer angewendeten Änderung zur geplanten Verifikation
  untersuchen, ohne ungültige Modellaktionen in ausführbare Eingaben umzudeuten;
- die genaue Post-Patch-Ablehnung inhaltsfrei erhalten; ursprüngliches V1 meldet
  nur `Staged(InvalidArguments)`, nicht deren genaue Decoderunterursache;
- Providerfehler von Google/GPT-OSS getrennt untersuchen; nicht durch größere
  Budgets oder umgangene Sicherheitsprüfungen verdecken;
- erfolgreiche kleine Fälle auf weitere Aufgaben und echte Planübergaben erweitern.

Der Codebefund zum Post-Patch-Übergang ist enger als ein allgemeines
„Verifikationsproblem“: `MutatingAgentController` liefert bei einer erfolgreichen
Änderung mit einer anderen Verifikationsmethode als `DiffInvariant` über
`request_next_execution` einen frisch kompilierten Kontext zurück.
`ProductionAgentRunExecutor` startet danach erneut die freie Modell-Aktionswahl.
Erst ein passender Finish-/Result-Wunsch wird bereits heute durch
`verification_command_for_request` auf die Core-gebundene geplante Prüfung
abgebildet. Ein selbständiger Übergang nach gültiger Mutationsquittung ist daher
ein eigener nächster Schnitt, keine Lockerung des Patchdecoders. Dabei müssen
unvollständige Mehrdateiänderungen, exakte Freigaben, Freshness, Wiederanlauf und
bereits verbrauchte Prüfversuche ausdrücklich berücksichtigt werden.

Der initiale vollständige Workspace-Gate und Clippy bestanden. Eine zusätzliche
panikfreie Prüfung des Compiler-Schemaarms wurde vor der Matrix gezielt geprüft.
Danach bewahrt V2 des Testbinarys zusätzlich die geschlossene Decoderursache und
korrigiert die Schema-Anzeigenamen auf A^3. Kein Ablauf, Repairhint oder
Sicherheitsgate wurde aufgrund eines negativen Modellresultats gelockert.

Frozen V2: `agent-tests-v2.exe`, SHA-256
`583e53177372c247f02f67276c022077f9540e0a054fc19ab6606876f39bd1e5`.
Ein gesonderter Granite-Nachlauf bestätigt nach angewendetem Patch und grünem
Fixturetest die exakte terminale Klasse
`InvalidAction(InvalidPatchOperation(NoContentChange))`. Dabei stimmt der Hash des
vorgeschlagenen Inhalts mit dem ebenfalls vom Modell vorgeschlagenen Basishash
überein. Diese frühe Konstruktorprüfung erreicht die spätere Zulassung gegen den
aktuellen Snapshot nicht: Sie beweist allein nicht, dass der Vorschlag dieselben
Bytes wie die aktuelle Datei enthält. Belegt ist der erneute Patchvorschlag nach
angewendeter Änderung und vor der geplanten Verifikation. Laufzeit 15,08 Sekunden,
5 Modellaufrufe, 12896/536 gebuchte Tokens. Log `granite-diagnostic-v2.log`, SHA-256
`ca1b070eb1664d854407518070f3270c269277c5bfab9b8cbf0238f2bfe97fdd`.
Das ist keine rückwirkende genaue Klassifikation aller Ornith-Ablehnungen.
Qwens V2-Nachlauf erreicht erneut Done mit vier Aufrufen, 8905/247 Tokens,
keinem Repair und 31,05 Sekunden einschließlich erneuter Modellladung/Probe.
Diese Nachläufe werden nicht nachträglich in die gegenbalancierten Paare gemischt.
Lunas V2-Nachlauf erreicht ebenfalls Done: vier Aufrufe, 4794/153 Tokens,
kein Repair, 11,01 Sekunden. Beide erfolgreichen V2-Nachläufe bestätigen die
unveränderten geschützten Dateien und nativen Einstellungen. Loghashes:
`qwen-final-v2.log` =
`3a4b6629fe6f9a753d7dda67f6abd9ce9183d4ae60102f90b238cdc585dda35f`,
`luna-final-v2.log` =
`f871a0f2240aefcabcbd4954c0ee43baa8b326d54c6f844d346815c8386df868`.

Auf dem endgültigen Quellstand bestehen die 23 gezielten Agent-Turn-Tests,
`cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --all-features --offline --locked --jobs 2 -- -D warnings`
und `cargo test --workspace --all-features --offline --locked --jobs 2 -- --test-threads=1`.
Die vollständige Suite umfasst den echten Freshness-Harness zwischen Modellstufen
und die Storage-Verbindungsregressionen. Das Abschlussreview ergänzt gegenüber
dem V2-Livebinary ausschließlich zwei Tests: gemeinsame verbleibende Anfragefrist
einschließlich bereits abgelaufenem Run sowie projizierbare Verträge für alle
16 angebotenen Auswahlwerte. Der produktive V2-Code bleibt dabei unverändert.
Pro Prozess gelten `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0` und `CARGO_PROFILE_TEST_DEBUG=0`. Keine Frontend-Änderung.
`pnpm check:links` und `git diff --check` bestehen; die bekannte Node-Warnung
25.6.1 statt 24.14.0 bleibt sichtbar. Finale Gateartefakte:

- `target/reports/staged-agent-clippy-review.log`, SHA-256
  `2acb9aca1551bb2c67e1daefa7829f2f34e821bd4cc928e98020e95ef8a9efbe`.
- `target/reports/staged-agent-workspace-review.log`, SHA-256
  `d1810e0eecfaf4e167a1affb6d68e74d08ea0ee215be167020fe2fa7e9788548`.

| Logdatei | SHA-256 |
| --- | --- |
| google-baseline-1.log | `59199ce7dc19f6abe53c2c3a1622f0a17f3d0cd50ba192a4c11ace700de3f559` |
| google-baseline-2.log | `967049627a25bdc74a4540bc43a8d276864d3e3f0c81f718dea9297d91e80c3d` |
| google-staged-1.log | `34c4ad3b196f8001c38192b53745ad1bb8ce4bc464e1c0f2961353ca0a3984d4` |
| google-staged-2.log | `dfc7f195019efc5842a86eb50a6061de368ea14a1ab457d02a5b923a4a9200ff` |
| gptoss-baseline-1.log | `5eb286fddfbab2806135181fa6369b72f86bab76e86e0021c8ab6cdb029b6197` |
| gptoss-baseline-2.log | `70811cedf7ed0b103b71743aa89144d6ea1e018d5c7ed2d41e926f34b40f55b3` |
| gptoss-staged-1.log | `3075b110c464ada9da7a8cb48d479f694eab4243a30941794bc14897dd5a9ea3` |
| gptoss-staged-2.log | `b9077b2ae22a5998c2b2694d71a9b559228ab3454122cf810dbd902c3a5d5c6f` |
| granite-baseline-1.log | `73b05ec6f0994919b9782d8e289f799bd55d86ff72932e92537a9d931fc67055` |
| granite-baseline-2.log | `15c270b2310382717d3732a2d1320957042c7e0211a45d5732846e0992428b37` |
| granite-staged-1.log | `1b2b42eb7b38469b992a772445ca6ef0363b6a890e8a8fbcb7b95b9b7c2e9922` |
| granite-staged-2.log | `fd1b94954ce2e87191c1da9d707afa60dd525c67eb9db4a6d6bf2b6976237df1` |
| luna-baseline-1.log | `d7db8444f19afb90f4ddbec3814d6971eb9c8e28a182fd698f6034873edb4f5c` |
| luna-baseline-2.log | `4729c32ecd7ea8d11640ce97a3583e1d85c6e5989fda56a54e2f7aee6b6267c7` |
| luna-staged-1.log | `d9bd240843f352519203bd8a9eb1ce60177fdd4a36a3273065ce467a6aed8505` |
| luna-staged-2.log | `8d00e8e5a9d09beffdd35b077f7b83e6494a8426749f211fb0bac968aede2bb1` |
| ornith-baseline-1.log | `605286a5dbc69d7149bd036d0cbbebfd6cc6114eb53722e3ec86e9201dcf31f4` |
| ornith-baseline-2.log | `a4120b4d3227111cca83e7018df5dea5e74b7cfcde3ab26492f9abe63401feef` |
| ornith-staged-1.log | `9a468fa41b0a478cfe658de3a26980bbe1101c144e90db4d4da58c9ed9bab019` |
| ornith-staged-2.log | `c0ede5c441938c475e00642ce1491779d4ab245f6e06b7a276aa93976656d80e` |
| qwen-baseline-1.log | `873dff709f569cb907849221ca7821f7fda2b16afe81113d7d13a2d4fb864789` |
| qwen-baseline-2.log | `aa631681a09f3e8f848626f075d59b26947097f5fe4287bcc9e11a7484fb5fdf` |
| qwen-staged-1.log | `e792cc484a2cd3353b489fbbb3f378674db198c716fdb7d570ef0f72b4893dd9` |
| qwen-staged-2.log | `b7a58e51681991c0de4e07a02893dc57273b46838841401495b09fa296f6131e` |


## 2026-09-07: Aktuelle Originalquellen im normalen Agenten (ADR-0093)

Ausgangspunkt `6cbbd97`. Die Codeinspektion belegt den bisherigen reinen
Metadatenpfad der normalen Task Lens. Die erste zusätzliche Live-Vorprüfung
war jedoch selbst fehlerhaft: Sie suchte eine nicht typisierte Funktionssignatur,
während die öffentliche Fixture `def increment(value: int) -> int:` enthält.
Ihre roten Logs (`original-context-preflight-red-visible.log` und der erste
`original-context-20260907/luna-coding.log`) sind deshalb **kein unabhängiger
Nachweis** einer fehlenden Originalversorgung. Der erste Log ohne
`RUST_TEST_NOCAPTURE` enthält zusätzlich nur den isolierten Prozessfehler.
Die korrigierte Vorprüfung vergleicht mit den tatsächlichen unveränderten
Fixture-Bytes; ein eigener Regressionstest verhindert die veraltete Erwartung.
Die Produktionshydration wurde für diese Testkorrektur nicht verändert.

Context-Policy V7 injiziert den vorhandenen Safe Reader verpflichtend in den
Compiler. Höchstens zwei aktuelle Originalseiten werden innerhalb des bisherigen
Budgets materialisiert; typisierte Originalmarker früherer Reads erhalten Vorrang,
ihre alten Vorschauen dagegen keine Originalautorität. Replan erhält null
automatische Zusatzreads. Keine neue Datenbankmigration, Dependency, Mutations-
befugnis, Repair-Runde oder persistierte Source-Bytes.

Die gezielten Compiler-Tests prüfen Originalbytes, Digest, genaue Tokenrechnung,
8k-/16k-Profile, Vorrang vor optionaler Historie, übergroße Seiten, zwei Quellen
mit jüngsten tatsächlichen Lesemarkern, Deduplication, Marker nach Entnahme der
Quellseite, nicht-originale Suchspans, Fehlerklassen und Replan ohne zusätzliche
Reads. Der Deadline-Test unterscheidet kooperative Frist von Nutzerabbruch ohne
Sleep. Der reale multilingual-indizierte Harness prüft tatsächlichen Code,
unveränderten Worktree und Ablehnung eines anschließenden Live-Edits.

Vorhandene synthetische Metadata-/Rankingtests injizieren ausdrücklich einen
nicht verfügbaren Reader; sie behaupten keine Dateisystemmessung. Alle echten
Harness- und Desktoppfade verwenden den WorkspaceAgentSourceReader.

Die finalen Gates bestehen: `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --all-features --offline --locked --jobs 2 -- -D warnings`,
`cargo test --workspace --all-features --offline --locked --jobs 2 -- --test-threads=1`
einschließlich `connection_lifecycle`, sowie `pnpm check:links`.
Pro Prozess gelten unverändert `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`. Keine Frontend-Änderung.
Die bekannte Node-Versionabweichung 25.6.1 statt 24.14.0 bleibt beim Linkcheck sichtbar.
Die ersten Clippy-Logs bewahren die korrigierten Test-`expect_err`-/`unwrap`-
und Variablenscopefehler; die Regeln wurden nicht abgeschwächt.
Finale Logs: `target/reports/original-context-clippy-final3.log` (SHA256
`c3a28eb151c6e97c00aaf0f1ac001ad2901b870dd8ce589bc4d60b28a3ba03e1`)
und `target/reports/original-context-workspace-final.log` (SHA256
`b3be57803f56b8df56cb67103ba07676c9f902a9521a9979c32b63cb90f2cc74`).

Eingefrorener korrigierter Live-Teststand:
`target/reports/original-context-20260907/agent-tests-v2.exe`, SHA256
`02c3a1c6f8dfc9cc74f7fe435f00b3d4ed310e8bd462cc36a473ab2d0f09b76c`.
Der erste Build mit fehlerhafter Vorprüfung bleibt separat erhalten; er wird
nicht als Coding-Modellergebnis gezählt. Alle folgenden Lives verwenden v2,
dieselbe öffentliche unveränderte Fixture und nur die vorher fest freigegebene
Dateiänderung plus Testausführung. Es ist weiterhin kein vollständiger
Plan→Agent-Handoff-Test. Lokale Modelle liefen ausschließlich nacheinander.

| Modell | Sekunden | Tatsächliches Ergebnis |
| --- | ---: | --- |
| gpt-5.6-luna | 17,48 | Original geliefert; Done, Sequenz 23, genau ein Patch und ein erfolgreicher Test; aktive Verifikation und unabhängiger Test grün, geschützte Dateien und Settings unverändert |
| Google gemma-4-26b-a4b-it | 10,99 | Original geliefert; ModelFailed(Unavailable), kein Repair, Failed/16, keine Mutation |
| ornith-1.5:9b | 44,02 | Original geliefert; IncompleteModelOutput(OutputLimit), kein Repair, Failed/6, keine Mutation |
| qwen38-8k:latest | 55,20 | Original geliefert; ein Test mit Exit 1, kein Patch; RepeatedReplanRead nach Einzelrepair, Failed/21 |
| gpt-oss:20b | 19,58 | Fähigkeitstest bestanden, Original geliefert; ModelFailed(InvalidResponse), kein Repair, Failed/6, keine Mutation |
| granite4.2:8b | 17,85 | Original geliefert; zwei Tests mit Exit 1, kein Patch; Replan-V7 Decode(InvalidValue) nach Einzelrepair, Failed/23 |

Die Dateien `*-coding-v2.log` liegen beim eingefrorenen Build. SHA256:

- Luna: `13b38d25b4a1ea2b3586fceb5cdaeabc2783f4e237fa375314aad21f88fa5b84`
- Google: `a044e786b130154677d50999a0a6636ec9cd74f96479ec47e11b4b197ee33363`
- Ornith: `b568bd2e7c4f9d8acc3ebb03da5a4e99d5f333da7f8f44e5b0e1e41f5a366cae`
- Qwen: `231fdfa99762dd8201f768707780f1864787d80dc70f67f879e594350070304d`
- GPT-OSS: `e5b5a38484fc797a83abc90a56cc5882e386d1fa92fccc2667a90832e9193b5d`
- Granite: `83c2c2dbd8b69aae33a5b460d1767b7393a9768474a80ba00af89743874e27e2`

Alle fehlgeschlagenen Coding-Lives bewahrten die geschützten Dateien und
bestanden den unabhängigen Test nicht. Process/Applied bedeutet ausdrücklich
nicht Test bestanden. Googles konkrete Transport-/HTTP-Ursache und Granites
ungültiges V7-Feld bleiben unbekannt; die geschlossenen Fehlerklassen belegen
weder einen 400-Schemafehler noch ein zu altes Modell. Die Zeiten sind einzelne
Gesamtläufe, keine isolierte Vorher-/Nachher-Leistungsmessung. Belegt ist die
Originalversorgung, nicht eine allgemein verbesserte Erfolgsquote.

Die vollständige Luna-Ask-/Plan-Matrix (vier Familien × drei Varianten, eine
Wiederholung, kein Fallfilter) beendet 12/12 Fälle mit WorkReady und null
Nutzerhalten beziehungsweise adaptiven Reads. Rubrik V3 besteht 10/12;
Audit 1:1 und 1:2 fehlen jeweils `write`. Manuell gegengeprüft: 1:0 liefert die
konkrete Aufrufkette einschließlich `output.write` korrekt, dupliziert aber große
Teile der Antwort. 1:1 und 1:2 behaupten eine Speicherung durch `save_tasks`,
obwohl die Fixture-Implementierungen in `storage.py` nur Tupel zurückgeben.
Der konkret schreibende File-Handle beziehungsweise `.write` wird nicht in
beiden Antworten vollständig benannt. Alle drei Planvarianten liefern Pläne.
Bericht: `target/research-eval/eval-1788809883739.jsonl`, SHA256
`5d2fc3a70b5553dfb01d04e7ed4e8fa946506d458d1f4519b4493260decf5bcb`.
Gesamtlauf 141,65 Sekunden; Exit 101 wegen der zwei Rubrikfehler, nicht wegen
einer Endlosschleife. Ungeprüfte Callee-Effekte bleiben eine offene Inhaltslücke.

Auch die vollständige Ornith-Matrix beendet 12/12 mit WorkReady und null
Nutzerhalten beziehungsweise adaptiven Reads. Rubrik V3: 10/12; Audit 1:0
und 1:1 fehlen `write`. Manuelle Kontrolle verhindert eine falsche Abnahme:
1:1/1:2 behaupten Persistenz, 1:2 erfindet einen Dateinamenbezug von `Task.title`
und nennt unter anderem `output.write` in Zeile 25 einer tatsächlich nur
20-zeiligen `plugins.py`. Diese freien E-/S-/Zeilenbehauptungen sind keine vom
Core zugeordneten Zitate. 1:0 nennt zunächst drei Methoden, zählt dann nur zwei
auf und enthält ebenfalls fehlerhafte freie Zeilenangaben. Die bereits bekannte
Inhaltslücke bleibt trotz des Rubrikerfolgs von 1:2 erhalten.

Plan 3:0 benötigt den bestehenden Einzelrepair für einen 4558-Byte-Entwurf;
3:2 nutzt ihn nach OutputTruncated. Beide liefern anschließend einen Plan,
ohne zusätzliche Reads oder Nutzerhalt. Bericht:
`target/research-eval/eval-1788810081373.jsonl`, SHA256
`e2ebc202b9d00d0f3adb2f106fa7348fcc43c7e7b6f7ec2ebe9f9d6c100658c6`.
Gesamtlauf 187,74 Sekunden, Exit 101 wegen der zwei Rubrikfehler.

Die Originalversorgung ist als vertikaler Schnitt verifiziert. Die insgesamt
offene Nutzbarkeitsabnahme wird weder durch die grünen Offline-Gates noch durch
die technisch abgeschlossenen Recherchefälle ersetzt. Nächste Arbeit muss
konkret die Aktionswahl vor einer Änderung, die bislang groben Provider-/
V7-Wertfehler und unbelegte Callee-Effekte beziehungsweise freie Quellenkoordinaten
adressieren; eine weitere pauschale Budgeterhöhung ist durch diese Daten nicht belegt.

## 2026-09-07: Dauerhafter Replan-Belegbedarf (ADR-0092)

Ausgangspunkt `15242b0`. Neue Replan-Analysen verwenden die eingeschränkte V7-Union
Interpretation/EvidenceNeed. Der bisher verlorene Bedarf wird als Navigation an
Originalpaket, Q1 und Analysequittung gebunden, im bestehenden Checkpoint (Knowledge
V38) gespeichert und nach Safe-Reader-Hydration erneut geprüft. Kein neuer Index,
keine Modellnotiz als Fakt, kein zusätzlicher Repair und kein erneuertes Lesebudget.

Nachweise: 16 gezielte Application-Replan-Tests; reale Context-/Gemini-HTTP-Verträge;
strikter Need-Codec; Storage-Vertrag mit falschem Hash, unmarkiertem Suchspan,
fehlendem Analyseversuch, verbotener Bedarfsumschreibung, Rollback des Journals,
Reopen und weiterem echten Read. Die reale UTF-8-Safe-Reader-Probe prüft erhaltene
Zähler, identische Pakete, ungebundene Literale und bearbeitete Dateien. V38 erhält
V37-Payloads ohne Backfill, Update-/Delete-Guards sowie atomaren Migrationsrollback.
Die zunächst fehlende V37-Zeile der vollständigen Upgrade-Testmatrix wurde ergänzt;
deren Vollständigkeitsprüfung bleibt unverändert streng.

Bestanden: `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --all-features --offline --locked --jobs 2 -- -D warnings`,
`cargo test --workspace --all-features --offline --locked --jobs 2 -- --test-threads=1`
(einschließlich `connection_lifecycle`) und `pnpm check:links`.
Cargo lief pro Prozess mit `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0` und
`CARGO_PROFILE_TEST_DEBUG=0`. Keine persistenten Profiländerungen. Der Linkcheck
meldet weiterhin die vorhandene Node-Abweichung 25.6.1 statt 24.14.0; sein Check
besteht. Kein Frontend-Code wurde verändert.
Logs: `target/reports/replan-need-{workspace,workspace-final,clippy}.log`.
Der erste Workspace-Log enthält den beschriebenen roten Matrix-Vollständigkeitstest,
der finale Log ist grün (SHA256
`14d7b138c645a6a5b5ea9bf83dcd89e976ab3483e43d846d14e0078d54d9672c`).

Eingefrorener Desktop-Teststand:
`target/reports/replan-need-20260907/agent-tests.exe`, SHA256
`ad0b4dae4bc472deb464a50124c2d877f86488fe7304038e5d9f778248b09220`.
Alle folgenden Coding-Lives verwenden denselben unveränderten öffentlichen
Ein-Schritt-Fixture und unabhängigen Test. Dies ist kein Plan→Agent-Handoff-Test.
Die Modelle liefen nacheinander; Zeiten sind Testgesamtdauern, kein isolierter
Geschwindigkeitsvergleich. Geschützte Fixture-Dateien blieben überall unverändert.

| Modell | Dauer (s) | Tatsächliches Ergebnis |
| --- | ---: | --- |
| gpt-5.6-luna | 16,99 | Done/Sequenz 25, ein Patch und ein Prozess, Test 0, aktiver Schritt verifiziert, unabhängig grün, Settings unverändert |
| gemma-4-26b-a4b-it / Google | 11,14 | Unavailable ohne Repair, Failed/16, keine Mutation |
| ornith-1.5:9b | 21,43 | Failed/12, ein Test mit Exit 1, kein Patch, aktiver Schritt Blocked |
| qwen38-8k:latest | 56,78 | RepeatedReplanRead im Einzelrepair, Failed/21, ein Test mit Exit 1, kein Patch |
| gpt-oss:20b | 18,67 | Capability bestanden, InvalidResponse ohne Repair, Failed/6, keine Mutation |
| granite4.2:8b | 27,16 | V7-Replan-Analyse: Decode(InvalidValue) nach Einzelrepair, Failed/23, zwei Tests mit Exit 1, kein Patch |

Coding-Log-SHA256 (Dateien im selben eingefrorenen Verzeichnis):

- `luna-coding.log`: `d756cf207e333ea6bfcd6120459acbf1445360125a6494b12149aa6dadf6fc3f`
- `google-coding.log`: `0ee032919ce7eef4dc88930c2d9c2a2d941872789f59fa214aa1d93d1bbabcf7`
- `ornith-coding.log`: `9fe44baddcfcb7b1075c90866b4ef3bc49726e890513a9a891ee5f84aed99ab0`
- `qwen-coding.log`: `dc86333415a0cbedaac37492c4c263ad8daf7354119a26846b011717939ff7ba`
- `gpt-oss-coding.log`: `2cdf9df0f81c3cc828d04b233140bfc113bc8038ddc5440de0210a016811d8b6`
- `granite-coding.log`: `3a548c8e3815a85e11c71547c2452a122765748dfe4981bd2a8544fb6eaeb89a`

Lunas vollständige Ask-/Plan-Matrix (vier Familien × drei Formulierungen, eine
Wiederholung) endet 12/12, WorkReady 12/12, Nutzerhalte und adaptive Reads jeweils 0.
Rubrik V3 besteht 11/12; Audit 1:2 fehlt der konkrete `write`-Aufruf. Unabhängig
davon behaupten Audit 1:1 und 1:2 eine Speicherung durch `save_tasks`, obwohl die
Fixture nur ein Tupel zurückgibt. In 1:2 steht außerdem die Konstruktor-Pfadauflösung
innerhalb der als zeitlich bezeichneten späteren Callback-Kette. Das ist keine
vollständige inhaltliche Abnahme. Bericht:
`target/research-eval/eval-1788806418304.jsonl`, SHA256
`d73978e98533866b6254ab1c45e8f94e733c12f715bcad204e6f38c40aa5eeb9`.
Die Matrix endet wegen des Rubrikfehlers korrekt mit Fehlerstatus. Ask/Plan wurden
in diesem Schnitt nicht auf eine neue Grammatik umgestellt.

Orniths vollständige identische Matrix endet ebenfalls 12/12 und WorkReady 12/12,
ohne Nutzerhalte oder adaptive Reads. Rubrik V3 besteht 10/12; Audit 1:0 und 1:1
lassen `write` aus. 1:1 und 1:2 behaupten fälschlich Persistenz; 1:2 bezeichnet
zusätzlich den Task-Titel als Dateinamenbezug und enthält erfundene E-/Zeilenangaben
(etwa `output.write` auf Zeile 25 einer tatsächlich 20-zeiligen Datei). Diese
freien Angaben sind nicht die vom Core verifizierten Quellenanker. Ein bestandener
Begriffstest in 1:2 ist deshalb ausdrücklich kein semantischer Erfolg. Plan 3:0
repariert einen zu großen Entwurf (4.558 Textbytes) und Plan 3:2 eine abgeschnittene
Antwort innerhalb des vorhandenen Einzelrepairs; beide liefern anschließend Pläne
ohne Nutzerhalt. Bericht `target/research-eval/eval-1788806630075.jsonl`, SHA256
`6e1439cddecf4ef5ba4e03084f193486c67f862f9ec0278a843c84e4d5e07576`.
Auch diese Matrix endet wegen der Rubrikfehler korrekt mit Fehlerstatus.

Offen bleiben insbesondere die Originalversorgung im normalen Agentkontext,
vorzeitige Blockierungen ohne Korrekturversuch, lokale Read-Wiederholungen sowie
der genaue V7-Wertfehler von Granite. Googles `Unavailable` bedeutet im aktuellen
Adapter einen transienten HTTP-/Transportfehler, nicht einen nachgewiesenen
Schemafehler: HTTP 400 wird dort separat als `Rejected` klassifiziert. Der genaue
Live-Status ist bislang nicht belegt. Die Änderung beseitigt den konkreten Verlust
des Belegbedarfs, nicht sämtliche Modell- und Inhaltsfehler.

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

## Fortsetzung 2026-09-07: Mehrprovider und GPT-OSS

Ausgangsstand ist `21a37b1` einschließlich der zwischenzeitlichen Mehrprovider-
Einstellungen (`93aaf5e`, ADR-0066). Die explizite Fixture-Auswahl verwendet
unveränderte gespeicherte, aktivierte Providerslots, native origin-gebundene Keys,
frischen Modellkatalog und echte Capability-Probe. Sie schreibt keine Rollen,
Profile, Credentials oder Benutzerkataloge. Für lokale Tests wurden `/api/tags`
und `/api/show` geprüft: Ornith 9,0B Q4_K_M, Qwen 27,3B IQ3_M und GPT-OSS
20,9B MXFP4 sind installiert, ohne Remote-Modell/-Host. Lokale Läufe sind sequenziell.

Zwischenstände, ausdrücklich **keine inhaltliche Endabnahme**:

| Modell | Bericht | Rückgabe `completed` | Begriffrubrik | Nutzerhalte | Calls / Bytes / ms |
| --- | --- | ---: | ---: | ---: | --- |
| gpt-5.6-luna | eval-1788769943354.jsonl | 12/12 | 12/12 | 0 | 35 / 126978 / 213736 |
| gemma-4-26b-a4b-it (Google) | eval-1788769864966.jsonl | 0/12 | 0/12 | 12 | 14 / 37777 / 449022 |
| gpt-oss:20b mit ADR-0067 | eval-1788770459162.jsonl | 12/12 | 11/12 | 1 | 35 / 137301 / 221813 |

`completed` bedeutet hier nur Rückgabe des Researchers, nicht zwingend einen
fertigen Plan: GPT-OSS fragt in CSV 3:2 unnötig nach der Bestätigung der erst zu
entwerfenden Tests. Die unabhängig ausgewiesenen Nutzerhalte und Rubrik bleiben
daher verbindlich. Der vollständige Inhalt dieses Gegenbeispiels bleibt im Bericht.
Auch erfolgreiche Audit-Antworten enthalten unbewiesene Persistenzbehauptungen;
Luna CSV 3:0 widerspricht sich zwischen verpflichtendem Header und erfolgreicher
leerer Datei. Die frühere inhaltliche Abnahme bleibt offen.

Die ersten beiden Berichte verwenden das eingefrorene Binary
`target/research-eval/multiprovider-20260907/research-tests.exe`, SHA-256
`45474578263b004a67a263ec26307418ce1c9698e623a158b3ee7996b7784dad`.
GPT-OSS verwendet `target/research-eval/oss-low-20260907/research-tests.exe`, SHA-256
`551a2a8beb21fba1798bf5818fa5bfa5d34a218dfd140214aebc1634c6901c64`.
Berichtshashes in obiger Reihenfolge:

- `a649814fae0f6b17a6f763daf7f0734acd9142550d01be4aaa71ef7f75f1f7e6`
- `b0a7a28d9621d50924f6d4d3570e3d6dc56a2d452b8eaa25fa04b172c230ee85`
- `8bf2e9ef30ac8730f51ba96df61220dfe61fabee2accc886c08043db438e1de7`

### Eng belegte Adapterkorrektur

GPT-OSS scheiterte zunächst vor Erstellung eines Matrixberichts an der Capability-
Probe (36,68 s Gesamtversuch). Der lokale A/B-Vergleich liefert mit `think: false`
HTTP 200/Stop, aber null sichtbare Bytes – sowohl bei 32 als auch 256 Output-Tokens.
Mit `think: "low"` liefert derselbe 32-Token-Request das exakte 17-Byte-Probeobjekt.
ADR-0067 korrigiert ausschließlich dieses Adapter-Wiremapping, nicht die Budgets.
Der neue HTTP-Regressionstest war vorher rot (`false` statt `low`) und danach grün;
er prüft Probe, Stream, unveränderte Output-Limits und ausgeschlossenen Thinking-Kanal.
Alle Provider-Verträge bestehen: 24 Einheiten, 13 Gemini-, 14 Ollama-, 10 OpenAI-
Vertragstests. Der externe OpenAI-Live-Test bleibt absichtlich ignoriert.

### Noch offene Ursachen

Google Gemma besteht die reale Probe und einfache JSON-/Union-/Nullarray-/numerische
Const-Diagnosen. Das tatsächliche Initialize-Schema wird zurückgewiesen; Typisierung
von schema_version oder Inlining der Referenzen hilft nicht. Kleinere verschachtelte
Array-Maxima werden angenommen, erzeugen aber im 30-Sekunden-Diagnosefenster noch
keinen sauberen Abschluss. Ein akzeptierter Wire-Request ist kein valides Ergebnis.
Die Diagnose verändert weder Produktionsschema noch Decoder oder Zugriffsrechte.
Gezielte weitere Schemaanalyse und die unnötige GPT-OSS-Testplanfrage bleiben offen.

Der volle Rust-Workspace samt All-Features-, Offline-, Locked- und seriellen Tests
besteht auf ADR-0067 einschließlich realer Patch-/Index-/Verification-Grenzen.
Weitere Diagnoseänderungen, Clippy, verbleibende Modellnachtests und Live-Agent-
Implementierung benötigen weiterhin eigene abgeschlossene Nachweise.

Der anschließende Diagnose-A/B-Lauf (`schema-bounds-20260907/research-tests.exe`,
SHA-256 `5b6fd851987d1cad040eee27272b5c3a4f5f4a2737f164155d15cc3fad572656`)
entfernt nur im öffentlichen Testrequest Array-Maxima größer eins. Das bisher
zurückgewiesene Initialize liefert jetzt Stop und 673 sichtbare Bytes. Der Inhalt
enthält weiterhin ungültige Quellen/Abhängigkeiten und wird dadurch nicht fachlich
oder durch den Core zugelassen. Das belegt einen Wire-Unterschied, keine richtige
Recherche. Clippy mit `--workspace --all-targets --all-features --offline --locked
-- -D warnings`, erneute drei Fixture-Auswahltests, Formatierung und Linkprüfung
(111 Dateien / 423 lokale Links) bestehen. Keine Frontenddatei wurde geändert.

## Fortsetzung: Provider-Wire, Core-Testentwurf und belastbare Messung

Ausgangspunkt ist Sicherungscommit `b1f9791`. Alle hier genannten Berichte liegen
unter `target/research-eval/`; historische Berichte werden nicht umgeschrieben.
Granite wurde vor dem Start als lokal installiert (8,8B Q4_K_M, kein Remote-Host)
geprüft. Lokale Modelle liefen weiterhin ausschließlich nacheinander.

| Modell / Stand | Bericht | Rückgabe `completed` | Rubrik v1 | Nutzerhalte | Calls / Bytes / ms |
| --- | --- | ---: | ---: | ---: | --- |
| Ornith, ADR-0067 | eval-1788770705617.jsonl | 12/12 | 11/12 | 0 | 35 / 127892 / 245005 |
| Qwen 8k, ADR-0067 | eval-1788771015049.jsonl | 12/12 | 11/12 | 0 | 34 / 103628 / 411583 |
| Granite, ADR-0068 | eval-1788771571188.jsonl | 11/12 | 9/12 | 1 | 36 / 127858 / 241267 |
| GPT-OSS, ADR-0069, nur CSV 3:2 fünfmal | eval-1788772145252.jsonl | 5/5 | 2/5 | 0 | 15 / 85461 / 129698 |
| Luna, ADR-0069 | eval-1788772147463.jsonl | 12/12 | 12/12 | 0 | 34 / 123903 / 223820 |
| Google Gemma, ADR-0070, nur Storage 0:0 | eval-1788772487134.jsonl | 0/1 | 0/1 | 1 | 4 / 13498 / 201320 |

Die fünf GPT-OSS-Nachtests beantworten den konkreten Core-Testauftrag ohne
Bestätigungsfrage, zusätzlichen Repair oder adaptiven Read. Die drei Rubrikfehler
sind bei Sichtprüfung **keine fehlende UTF-8-Nennung**: das Modell schreibt `UTF‑8`
mit U+2011 statt ASCII-Bindestrich. Der gespeicherte v1-Fehlbericht bleibt unverändert.
Die Auswertung wurde deshalb rot→grün auf typografische U+2010/U+2011-Bindestriche
geprüft. Ab `rubric_version=2` verlangt `passed` zusätzlich ausdrücklich keinen
Nutzerhalt und `work_ready=true` aus dem dauerhaften Core-Arbeitsstand. Ein
`QUESTION:` mit sämtlichen Rubrikbegriffen oder fehlender/unfertiger Arbeitsstand
kann nicht mehr bestehen. Diese Änderung betrifft nur die Messung, nicht
Ergebniszulassung, Dateizugriff oder Produktionsprompts.

Inhaltlich bleiben echte Gegenbeispiele: GPT-OSS verwendet im CSV-Testentwurf
`click.testing.CliRunner` für den tatsächlich gelesenen argparse-Einstiegspunkt
und patcht den importierten Manager im falschen Modul. Luna kombiniert weiterhin
fehlende Pflichtheader als Fehler mit leerer Datei als Erfolg, ohne diese Ausnahme
eindeutig aufzulösen. Ornith und Qwen lassen im Audit-Fall 1:0 den Writer-Begriff aus.
Granite lässt zusätzlich beide Storage-Dateinamen aus und scheitert einmal nach
überlangem Erstresultat und fehlender Quellenabdeckung im Einzelrepair. Ein
abgeschlossener Pflichtstand oder Begrifftreffer beweist keine semantische Wahrheit.

### Gemini: getrennte Ursachen, keine vorgetäuschte Abhilfe

ADR-0068 projiziert ausschließlich variable Array-Maxima größer eins aus dem
Wire-Schema. Ein unabhängiger HTTP-Vertrag weist eine vom Provider akzeptierte
33-Fragen-Antwort weiterhin am unveränderten Decoder ab; 0/1-Grenzen und feste
Tuple-Arity bleiben erhalten. Die reale Nachmatrix `eval-1788771282213.jsonl`
ist bei dieser Zwischenprotokollierung noch aktiv und liefert weiterhin zahlreiche
OutputLimit-Abbrüche. Sie wird noch nicht als abgeschlossene Matrix gewertet.

ADR-0070 setzt ausschließlich für die zwei dokumentierten gehosteten Gemma-4-IDs
explizit `thinkingLevel=minimal`, mit identischem Wirewert in Probe und Stream.
Der neue HTTP-Vertrag war mit fehlendem Feld rot, danach grün; unveränderte
256-/2048-Tokenlimits, ausgeschlossene Thought-Ausgabe und unveränderte andere
Gemini-Modelle sind geprüft. Der kontrollierte echte Storage-0:0-Nachtest nutzt
weiter 16k/4k, dieselbe Frage und dieselbe öffentliche Fixture. Er repariert einmal
eine ungültige Initialize-Statusnotiz und erreicht die Analyse, aber beide
Analyseversuche enden weiter mit OutputLimit. Das ist **keine erfolgreiche
Recherche und kein nachgewiesener Geschwindigkeitsgewinn**. Die Annahme, Thinking
allein erkläre die Abbrüche, ist damit nicht bestätigt.

### Reproduktionsanker

Ornith/Qwen nutzen das oben dokumentierte `oss-low-20260907`-Binary. Granite und
die ADR-0068-Google-Matrix nutzen `gemini-bounds-20260907/research-tests.exe`, SHA-256
`7584dd382c03726f47aa0d8692845fe53d5d402c9e9f8100355afc7cf06c2341`.
GPT-OSS fünfmal und Luna nutzen `core-test-design-20260907/research-tests.exe`, SHA-256
`f372f103fa407b05055467f0d77d845ff0a219fbd09dc5a781412aa5efa60472`.
Google mit minimalem Thinking nutzt `gemma-minimal-20260907/research-tests.exe`, SHA-256
`0a165a1a4031374fc6e0fc7478f52233f5d0edc2f31c68d54bf4d91df136782d`.
Berichtshashes in Reihenfolge der obigen Tabelle:

- `036412b4356fe5575863a3dc95ff4efc62f571160eb169a0e9d2c64ae0927361`
- `9fdd391727aaa5ae356dbbf946c4008c7e4d724aae4a17862c90af580975df2d`
- `41560a00f080564031ad8b8f8a9dca8252f1046575222ba3e1e3e700e358d874`
- `d7c7d955223d42ba923d7050266927001da54d74de9a788e13a37328c6647b08`
- `828da5cdb209242a6ea370044d8dbb6b05b0a2edb687e1f06e43cad148cc1d10`
- `2938f511ef43589028e14d2c721d2f980ebd1367d26f6a01905ca60653617770`

Die Rubrik-v2-Nachtests nutzen `rubric-v2-20260907/research-tests.exe`, SHA-256
`a61247fa1738e7d9d4dd8b32612809ea89418a31631bb3623829897537b76b32`.
Ihre Ergebnisse und der noch laufende Workspace-Gesamtgate benötigen einen
eigenen terminalen Nachweis. Gezielte Prüfungen bestehen bereits: 39 Application-
Recherchetests, 101 Desktop-Recherchetests (vier Live-Tests ignoriert), danach
26 Provider-Einheiten, 15 Gemini-, 14 Ollama-, zehn OpenAI-HTTP-Verträge und fünf
Matrix-Diagnose-/Rubriktests. Linkprüfung: 114 Markdown-Dateien, 435 lokale Links;
`git diff --check` grün. Keine Frontenddateien, Benutzerkataloge oder Profile geändert.

### Terminaler Gate- und GPT-OSS-v2-Nachweis

Der volle `cargo test --workspace --all-features --offline --locked -- --test-threads=1`
und `cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings`
enden auf ADR-0068 bis ADR-0070 einschließlich Rubrik v2 mit Exit 0. Echte lokale
Patch-, Process-, Index-, Recovery-, Acceptance- und Storage-Verträge laufen mit;
die Live-Agent-Implementierungsabnahme wird damit nicht vorgetäuscht.

GPT-OSS `eval-1788772713023.jsonl` besteht mit Rubrik v2: 12/12 Rückgaben, 12/12
`work_ready`, 12/12 notwendige Begriffe, null Nutzerhalte und adaptive Reads,
34 Calls / 127123 Kontextbytes / 207217 ms. SHA-256:
`f4399675bc2f366d86e46d651a8426b2af3b2ae47afe68ae58e3cd8c53b11f4b`.
Der anschließende zusammenhängende Ask-/Plan-/Agent-Vorbereitungs-Smoke besteht
ebenfalls (68,61 s, eigener Test). Die Analysepakete enthalten die fünf benötigten
Methodenkörper gleichzeitig. Sichtprüfung findet trotzdem unbewiesene Persistenz,
unscharfe Konstruktor-CWD-Angaben und im vorgeschlagenen Audit-Test eine JSON-
Behauptung, obwohl die Fixture ein Python-Dict formatiert. Solche Inhaltsdefekte
werden nicht durch grüne Ablauf- oder Begriffsmetriken geschlossen.

## V6: Core-Status, konkreter Belegbedarf und lange Helfer

Ausgangscommit `e214c0c`; ADR-0071 bis ADR-0074 sind Implementierungsschnitte,
keine automatische Praxisfreigabe. V6 entfernt Modell-Statusnotizen. Ein zuerst
roter realer Persistenztest belegte deren falsche Wiederaufnahme als Befund;
Core-Status wird jetzt nur als Audit gespeichert. Der echte Fortsetzungsweg
revalidiert Quellen, erhält die letzte Suchrichtung und übernimmt null Core-
Statusangaben in Findings oder Gaps. V3–V5 bleiben separat geprüft.

Die neue lange Helferfixture enthält entry.py, zeta.py und kappa.py, zwei
190-Zeilen-Originalkörper sowie 32 unbeteiligte Dateien. Vor der Kontextkorrektur
verliert Ask beim zweiten Helfer die vorherige Rückgabestelle und endet nach
sieben Calls ohne Ergebnis (neun Read-Aktionen). Das Modell erfindet dabei keine
Erkenntnissammlung: Es bewertet ausschließlich das jeweilige aktuelle Paket.
Originalgebundene kurze Navigationsstellen und das Freigeben des alten breiten
Cursors erhalten den Zusammenhang. Nach ADR-0073/0074 bestehen Ask, Plan und
Agent-Vorbereitung mit jeweils neun Calls, acht Read-Aktionen und unverändert
4096 Evidence-Bytes. Der negative erfundene Helfer erhält einen Einzelrepair,
null adaptive Reads und null Abschlüsse. Kleine Legacy-Pakete, expliziter Fokus
und Wiederaufnahme wurden nach einem zwischenzeitlich gefundenen Legacy-
Fokusregress erneut gezielt geprüft. Das ist kein Live-Agent-Patchnachweis.

Neutrale Schema-UTF-8-Bytes V5 → V6, einschließlich konkretem Belegbedarf:
Initialize 1845 → 1148; Analyze 2215 → 2028; SummarizeOriginals 1949 → 1820;
Design 2280 → 1537; DesignTests 1985 → 1288; Finalize 1943 → 1246.
Das ist eine reproduzierbare Größenmessung, kein Laufzeit- oder Qualitätsbeweis.
Schema- und unabhängige Decoderprüfungen erhalten die Ergebnisgrenzen; der
V6-SummarizeOriginals-Schemaarm kann für evidenceNeed null Ergebnisse darstellen,
leerer progress bleibt durch Decoder und Work-Admission verboten.

### Abgeschlossene Modellnachtests und offene Ursachen

Der zuvor noch aktive ADR-0068-Google-Bericht `eval-1788771282213.jsonl` endet
mit 0/12 Abschlüssen, zwölf Nutzerhalten, 22 Calls, 58300 Kontextbytes und
1782054 ms. SHA-256 `550bb8b8d184e949909ba920e84b4acd9051c54d85aa870e97ffb3ded74a5026`.
Die historische Zwischenprotokollierung oben bleibt als solche erhalten.

Das eingefrorene `v6-core-20260907/research-tests.exe` (nur ADR-0071/0072,
vor Kontextkorrektur und Plan-Erweiterung) hat SHA-256
`8091fa45be8581c6254a32dd2db481bf29d51cbd771321d23f0be4031d6f3712`.

- Luna `eval-1788775227603.jsonl`: 12/12 completed, work_ready und Rubrik v2,
  null Nutzerhalte/Reads, 35 Calls, 129371 Kontextbytes, 159870 ms. SHA-256
  `e620b3152ae5a45808c764a652422fb4ee3f09d57bcc62e55bd251bc11ca8313`.
- Google Gemma Storage 0:0 `eval-1788775215289.jsonl`: 0/1 Abschluss,
  ein Nutzerhalt, vier Calls, 13538 Kontextbytes, 188740 ms. SHA-256
  `063bc9703b4547b0543c04aee4d994152efa9abec36ba763b0ed055636f1b3ec`.
  Initialize bleibt zunächst ungültig; nach Repair endet Analyze zweimal am
  Ausgabelimit. Weglassen der Statusnotiz allein löst diesen Modellfall nicht.

Die Nachtests einschließlich ADR-0073/0074 und Reopen verwenden
`v6-navigation-20260907/research-tests.exe`, SHA-256
`11d22281d3f78ab05fae603df4caf5c18983db0ceb89ee82d8b4b82cc80c3de5`.
Lokale Modelle laufen strikt nacheinander; externe Provider verwenden weiterhin
nur die freigegebenen nativen Profil-/Credential-Bindungen. Während der Messung
laufen teilweise lokale Buildgates: Laufzeiten sind daher kein isolierter
Performancevergleich.

Granite antwortet im REST-Fall 2:0 nach gültigem Q1 auf Q2 nur mit leerem progress,
obwohl API und Manager im aktuellen Paket stehen. Die Deduplizierung verhindert
eine Endlosschleife, liefert aber noch keinen Abschluss. Google Gemma liefert im
CSV-Plan 3:2 zweimal denselben 97-Byte-Leerfortschritt; er wird korrekt abgewiesen,
null Reads, null beantwortete Pflichten. Diese Grenzen und ausgelassene tatsächliche
Writer in einzelnen Luna-/Granite-Antworten bleiben offen. Ein gültiger Decoderlauf,
eine schöne Prüfliste oder vollständige Begriffrubrik ersetzt keine Inhaltsabnahme.

Gezielt bestehen sechs V6-Desktoptests, drei V6-Applicationtests, beide Provider-
Phasenübersetzungen und der wiederhergestellte Legacy-Aufrufkettenvertrag. Der
Workspace-Gesamtgate läuft bei dieser Zwischenprotokollierung noch. Clippy fand
eine durch den neuen optionalen Bedarf vergrößerte Enum-Variante; der seltene Bedarf
wird nun wie der vorhandene Work-Vorschlag indirekt gespeichert. Der erneute Gate
und die restlichen lokalen Matrixenden benötigen eigene terminale Nachweise.

### Terminale V6-Navigationsmatrix und Qualitätsgates

Alle vier lokalen Modelle liefen nacheinander mit demselben eingefrorenen Binary.
Die folgenden Zahlen sind vollständige Matrixenden, keine Hochrechnung:

| Modell / Bericht | completed / ready / Rubrik v2 | Halte / Reads | Calls / Kontextbytes / ms |
| --- | --- | --- | --- |
| Granite / `eval-1788775949362.jsonl` | 11 / 11 / 9 von 12 | 1 / 1 | 36 / 128567 / 180870 |
| Luna / `eval-1788775996818.jsonl` | 12 / 12 / 11 von 12 | 0 / 0 | 34 / 123886 / 150548 |
| GPT-OSS / `eval-1788776142807.jsonl` | 8 / 8 / 8 von 12 | 4 / 0 | 39 / 144382 / 177579 |
| Ornith / `eval-1788776328699.jsonl` | 11 / 11 / 10 von 12 | 1 / 3 | 37 / 132440 / 208713 |
| Qwen 8k / `eval-1788776549940.jsonl` | 12 / 12 / 11 von 12 | 0 / 0 | 35 / 107011 / 233711 |

SHA-256 in derselben Reihenfolge:

- `f304921df9c145fbd70545a7a191697ef6085a3fc7b6f79d7792f6cbb0b348fa`
- `e9337e1d699297f29b560be888153da8e9e67240ee1b41a793f6a65e0eed2245`
- `d475ac228a394859f18949d27dc98dff054b111b390cb399c0cffa5f35d4b13f`
- `8b470c19d46b421c2c6b738b2e5f12d8d891d223d3c4ff58a91cc9de50455647`
- `2c191a4bfcd422552e839b0bfd37a6cc9f7844e0993edc839882bfc1094c09a6`

Google CSV 3:2 `eval-1788775954643.jsonl` endet mit 0/1 completed/ready/Rubrik,
einem Halt, null Reads, zwei Calls, 7661 Kontextbytes und 2723 ms; SHA-256
`d17bd68d1adafb77876047f09af1f6557f88a4d9eca21c82f913a4fd402ed724`.
Der GPT-OSS-Regress besteht aus leeren sichtbaren Stream-Dokumenten in vier Fällen,
nicht aus einer erneut gescheiterten Capability-Probe. Alter oder Modellgröße sind
damit nicht als Ursache belegt. Ornith bleibt einmal nach leerer Bestandsanalyse
stehen. Alle Modelle außer GPT-OSS lassen im Audit-Fall 1:0 den tatsächlichen
Writer aus; eine vorhandene Quelldatei garantiert keine vollständige Interpretation.

Der volle Workspace-Test endet mit Exit 0 (`v6-workspace-tests-final.log`), inklusive
Storage-/Reopen-, Provider-, echter Patch-/Process-/Acceptance- und Doc-Tests.
Clippy mit allen Targets/Features und `-D warnings` endet ebenfalls mit Exit 0
(`v6-clippy-verified.log`). Die Testfixture verwendet keine Panic-Ausweichstelle.
Nach den reinen Teststilkorrekturen bestehen die sechs V6-Desktopregressionen erneut.
Formatter, `git diff --check` und Linkprüfung (118 Dateien, 453 lokale Links) bestehen;
die bekannte lokale Node-Versionsabweichung bleibt sichtbar. Kein Frontend geändert.

Der Endstand stellt außerdem die zwischenzeitlich verlorene explizite Analysepflicht
für ACTIVE Q wieder her. Die eingefrorene Matrix oben enthält diese Promptkorrektur
noch nicht; ein gezielter neuer Live-Lauf ist deshalb erforderlich. Ein öffentlicher
Wire-Diagnosetest ergänzt denselben trivialen Analyseauftrag unter V5 und V6, um
Stream- und Schemafehler getrennt von Repository-Semantik untersuchen zu können.
Die Gesamtpraxisabnahme, Inhaltskorrekturen und Live-Agent-Umsetzung bleiben offen.

## V7: Disjunkte Antworten und der live gefundene Union-Regress

V6-Sicherungscommit: `263353f`. Das zuvor eingefrorene `v6-verified-20260907/research-tests.exe`
hat SHA-256 `7509255a9a276b31adb28179916fa9f500a0aaadb02af586a21e12d29a4ebded`.
Mit der wiederhergestellten ACTIVE-Q-Instruktion besteht Granite REST 2:0 in drei
Wiederholungen: `eval-1788777001278.jsonl`, 3/3 completed/ready/Rubrik, null Halte/Reads,
je fünf Calls und 14036 Kontextbytes, 23800/15416/15030 ms. SHA-256
`ec6cb01ca910f1a1bf91fa80b9daf2b334f527b9e6da88af63e62d5746a40030`.
Das schließt nicht die anderen Inhaltsbefunde.

ADR-0075 beseitigt die getrennten nullable Work-/Decision-Felder. Der erste
V7-Ergebnistest scheitert vor Implementierung an UnsupportedVersion und besteht
danach einschließlich negativer Felder-/Phasen-/Anker-/Bytefälle. Die echte
Mehrmodus-Navigation mit Reopen und ein Repair auf injizierten Status bestehen.
Der breitere Lauf findet eine alte Provider-Schemaassertion; nach Anpassung an
den konkreten V7-Root besteht auch der geschützte echte Paketvertrag. Der volle
Workspace und Clippy auf diesem flachen V7-Stand enden mit Exit 0
(`v7-workspace-tests.log`, `v7-clippy.log`). Trotzdem scheitert die reale Matrix:

Binary `v7-disjoint-20260907/research-tests.exe`, SHA-256
`ad109a32abd35cc1d4cb4f315453051fd959d26c209daf2d5ba3d64792844e08`.
Alle folgenden Berichte haben null completed, ready und Rubrik-Erfolge:

| Modell / Bericht | Fälle / Halte / Reads | Calls / Kontextbytes / ms |
| --- | --- | --- |
| Google, CSV 3:2 / `eval-1788777743423.jsonl` | 2 / 2 / 24 | 2 / 7148 / 5533 |
| Luna / `eval-1788777745716.jsonl` | 12 / 12 / 88 | 32 / 99366 / 71371 |
| GPT-OSS / `eval-1788777753588.jsonl` | 12 / 12 / 88 | 33 / 102279 / 78376 |
| Ornith / `eval-1788777841237.jsonl` | 12 / 12 / 64 | 31 / 95545 / 58488 |
| Qwen 8k / `eval-1788777912190.jsonl` | 12 / 12 / 64 | 34 / 97625 / 96580 |
| Granite / `eval-1788778014996.jsonl` | 12 / 12 / 76 | 32 / 99946 / 51505 |

SHA-256 in derselben Reihenfolge:

- `b4cee3af61304d5752aa59df7305a3cf9cba466a5aa755cf7997d8b266e9cc35`
- `bbb016a99519ca5675245ef39da7f9161f57b727fda2cc96b3c4e7d04e30e4cd`
- `9159a5c1f1776120844360e0d731fee54aef4cdb1312884a80e50cf4386efe94`
- `fb38b668448bd52e8f1906b3b537541f28cf3e4c913637f50c42aa79324ef357`
- `88fb0252dfb65b5363b4eabf46b4073ca49b18351c4034c50737c82e60c68cb0`
- `c6ea63d439e6241dd80e6f10530b6049c1bea4dba00fcbfc002537f283151051`

Die Nullabschlüsse sind ein Schema-/Modell-Kompatibilitätsregress, kein bestandenes
Autonomiegate. Viele Antworten weichen auf Belegbedarf aus; der Core führt begrenzte
deduplizierte Zugriffe durch und hält ungelöste Pflichten offen. Die kürzeren Laufzeiten
sind kein Geschwindigkeitsgewinn, sondern folgen aus dem vorzeitigen Scheitern.

### Direkter öffentlicher Wirevergleich und ADR-0076

`v7-wire-shape-20260907/research-tests.exe`, SHA-256
`6ed5472c6f6147c68c464de06fa1c4f5a3ab437f631747e1d125a716ce03ab5d`, vergleicht
denselben minimalen Auftrag mit `def helper(): return 7` ohne Repository oder Tools.
Luna gibt unter der flachen Union zuvor nur eine Rückfrage aus (111 Bytes); Google
liefert Bedarf samt Whitespace (12486 Bytes). In der Ergebnis-only-Variante antworten
Luna, Google und GPT-OSS jeweils korrekt mit E1 (135/160/160 Bytes). In der Union mit
vorangestelltem `kind` und verschachteltem `result` bleiben sämtliche Alternativen
erhalten; alle drei liefern ebenfalls die richtige Antwort mit E1 (146/171/158 Bytes).
Die übrigen Providerbudgets und Profile bleiben unverändert. Ein Test-Exit 0 des
Wirewerkzeugs bedeutet nur ausgeführte Diagnose, nicht automatisch gültiges Ergebnis.

ADR-0076 übernimmt diese Ergebnisform. Der neue verschachtelte Decodertest wird
zunächst mit UnknownOrMissingField rot und danach grün. Der Core validiert den
äußeren epistemischen Typ und das enge Payload separat; flache V7-Felder sind keine
Fallback-Eingabe. Schema-UTF-8-Bytes V6 → korrigiertes V7: Initialize 1148 → 968,
Analyze 2028 → 1773, SummarizeOriginals 1820 → 1539, Design 1537 → 1032,
DesignTests 1288 → 786, Finalize 1246 → 969. Das ist nur eine Größenmessung.
Die erneuten Gesamtgates und vollständigen Modellnachtests auf diesem korrigierten
Stand sind bei dieser Zwischenprotokollierung noch offen.

### Terminaler Nachtest der verschachtelten V7-Antwort

`v7-nested-20260907/research-tests.exe`, SHA-256
`a4bce99a404b00c9e3368052254e081c223092d5ecf882ca0598eef45855521b`:
vollständige Offline-Workspace-Tests (alle Features, seriell) und Workspace-Clippy
(alle Targets/Features, Warnungen als Fehler) bestehen. Die vier V7-Applicationtests,
drei V7-Desktopregressionen und begrenzten Repair-Hints bestehen ebenfalls.
Lokale Modelle liefen ausschließlich nacheinander. Alle Berichte haben null
adaptive Reads und null wiederholte Reads; das ist für diese bereits vollständig
gelieferten Originalpakete kein Nachweis zusätzlicher Suchfähigkeit.

| Modell / Bericht | Fälle / Abschlüsse / Rubrik bestanden / Halte | Calls / Kontextbytes / ms |
| --- | --- | --- |
| Luna / `eval-1788778564676.jsonl` | 12 / 12 / 12 / 0 | 35 / 129527 / 168445 |
| Google, CSV 3:2 / `eval-1788778563626.jsonl` | 2 / 0 / 0 / 2 | 2 / 7148 / 176025 |
| GPT-OSS / `eval-1788778573966.jsonl` | 12 / 11 / 10 / 1 | 40 / 144804 / 161998 |
| Ornith / `eval-1788778744534.jsonl` | 12 / 12 / 11 / 0 | 36 / 136913 / 186190 |
| Qwen 8k / `eval-1788778943199.jsonl` | 12 / 12 / 11 / 0 | 34 / 100902 / 210690 |
| Granite / `eval-1788779160101.jsonl` | 12 / 11 / 10 / 1 | 36 / 131714 / 161695 |

SHA-256 in derselben Reihenfolge:

- `7ba951cb37698fc364ca8d45b03d99964748ec605bba9b41acf2cc9657b5655a`
- `b7052bf2d26d1053863f84de869cbd1c318b3e9d61eb6adf35cdb502e7ee0ed3`
- `271aecef607a44d4302da655cd35e211512f21de8d2ee2a20fe2caef667198ed`
- `5b626912bc94abcafd8e132fbf23117e6a1fdc98b963c8f2e2636b010dfbe537`
- `0410d5b707bc110188899c41e1334db392becad1b83660ae4df342c864ce9f22`
- `094c40761fe2fb677a3e6786eafb0531db1ae7c228d56312ee93f6dbe1d7780a`

Offene Befunde: GPT-OSS erhält im REST-Fall 2:1 wiederholt leere Dokumente;
Granites CSV-Testentwurf 3:2 überschreitet auch nach der einzigen Reparatur die
4096-Byte-Textgrenze. Google endet zweimal nach etwa 88 Sekunden als ModelRejected;
der genaue Providergrund ist noch nicht belegt. GPT-OSS, Ornith und Qwen lassen
im Audit-Fall 1:0 den eigentlichen Writer aus. Luna benennt ihn jetzt korrekt,
behauptet im CSV-Plan aber weiterhin unbelegt Persistenz durch `save_tasks`:
die vollständig geprüfte Fixture implementiert dort nur eine Tupelrückgabe.
Die grüne Keywordrubrik ist daher ausdrücklich keine semantische Gesamtabnahme.
Ein tatsächlicher mutierender Live-Agent-Abschluss bleibt separat nachzuweisen.

### Tatsächlich mutierender Agent: erste Produktionsabnahme (2026-09-07)

Der ignorierte Desktop-Test `agent_approved_live_coding_fixture` verwendet jetzt den
echten ProductionAgentRunExecutor, Git, Fast Index, libSQL, Safe Reader, Patch- und
Prozessadapter sowie das vorhandene native Providerprofil. Nur die öffentliche
`small-local-bugfix`-Fixture wird in einem eigenen temporären Repository verarbeitet.
Der feste bereits bestätigte Ein-Schritt-Plan beginnt in Execute; diese Prüfung ist
kein Live-Nachweis des Planerzeugers. Der Agent muss `increment.py` tatsächlich ändern.
Test, Runner, Manifest und fremde Sentineldatei bleiben bytegleich. Ein physischer
unabhängiger Test ist vorher rot und muss nachher grün sein; außerdem werden echte
Step-Evidence und Run=Done verlangt. Modellprosa oder ein beendeter Versuch reichen nicht.
Die Fixture bestätigt nur exakt diese einzelne Update-Datei oder den bereits bestätigten
manifestbelegten `python -m pytest`-Befehl; Scope-Negativtests lehnen andere Pfade,
Operationen, CWD, argv, Shell- und Netzwerkrechte ab. Nutzereinstellungen bleiben read-only.

Mit dem unveränderten Luna-Profil (16384 Kontext, 2048 Output,
ConservativeUtf8BytesV1, RepeatSchemaInPrompt) wurden nacheinander folgende Ursachen
isoliert und mit Regressionen korrigiert:

- Der Compiler verlangte fälschlich eine Outputkapazität von mindestens 3605. Die
  reservierte Menge und die tatsächliche Outputcap sind nun getrennt (ADR-0077).
- 823 Bytes statischer Prompt plus 6681 Bytes Schema passten nicht in starre Bereiche,
  obwohl gesamter optionaler Platz vorhanden war (ADR-0078). Profil und Schema-Grounding
  bleiben unverändert, ebenso die Gesamtgrenze und Sicherheits-/Outputreserve.
- Worktree-/Run-ID sowie der tatsächliche Originaldateihash fehlten in den gelieferten
  Patchinformationen. Sie werden nun aus den ohnehin geprüften Core-Ankern geliefert.
- Vorzeitiges Finish versuchte die Gesamtabnahme vor der aktuellen Step-Verifikation.
  Für operationale Specs wählt der Core nun exakt deren vorhandenen Verifikationsbefehl
  durch den normalen Policy-/Freigabepfad (ADR-0079).
- Offene Fehlereinträge und Hypothesen konnten durch die Pflichtbudget-Verteilung aus
  Code/Evidence verdrängt werden (`AnchorTooLarge`). Ein separater roter Repeat-Schema-Test
  wird durch deren vollständige Vorreservierung grün (ADR-0080).
- Ein größeres Goal plus Pflichtmemory konnte anschließend den verpflichtenden L0-Anker
  verdrängen (`InvalidPack`). Auch dieser Fall ist separat rot/grün reproduziert; sein
  tatsächlicher Aufwand samt Framing wird vor dem Packing berücksichtigt.
- Entdeckte Commands verlangten gleichzeitig TEMP, TMP und TMPDIR. Auf dem geprüften
  Windows-Host fehlt TMPDIR, wodurch der Prozessadapter vor dem Start Denied zurückgibt.
  Nur die drei portablen Tempvarianten dürfen jetzt fehlen; andere angeforderte fehlende
  Werte bleiben Denied (ADR-0081). Der echte Prozessvertrag war zuvor rot und besteht nun.

Die neuen Live-Fixture-Fehler durch doppelte Fortschrittsverwendung sind getrennt behoben:
Ein Diagnosecompile markiert keinen Ausführungsjob abgeschlossen; jede Approval-Fortsetzung
bekommt wie die Anwendung einen eigenen besessenen, abbrechbaren Job. Das sind Testgerüst-
Korrekturen, keine behaupteten Produktverbesserungen.

Eingefrorene ausführbare Diagnosebestände unter `target/research-eval`:

| Bestand | SHA-256 | Terminaler Befund |
| --- | --- | --- |
| `agent-v5-verify-20260907/agent-tests.exe` | `b113c43a552c6a7b28534351e07ea7967a55f5396dad798644568c5dafa073d4` | Patch physisch korrekt; später InvalidAfterRepair, kein Done |
| `agent-v5-diagnostics-20260907/agent-tests.exe` | `0cd7486b8f956ec50a971a4a6440523d9aa78dddbb49dce1d9c251301f2c3822` | Lauf 1: Mutationsanker abgewiesen; Lauf 2: Patch physisch korrekt, danach InvalidActionAfterRepair(InvalidValue), kein Done |

SHA-256 der drei Logs in Tabellenreihenfolge (`luna-live-1.log`, danach
`luna-live-1.log` und `luna-live-2.log`):

- `320dc2c9883ddf495e9af2f81caec8c5bcae2b5dc6adfc3f1d7834183570b1b0`
- `e7c09064948849ef3ff2a730539690e18a16e645dd669dd58059beb75e7f0d86`
- `ce25d5cf28d3a5992dd812b3b16effc48234fe5fbbe2c80b65bc2374b3b42a15`

`v5` in diesen Verzeichnisnamen bezeichnet die Context-Policy, nicht das weiterhin
unveränderte AgentAction-V4-Wireschema. Die neue inhaltsfreie Fehlerklassifikation
trennt Decoderfehler von InvalidPublicNote; der jüngste konkrete Befund ist InvalidValue,
nicht nachgewiesenermaßen eine falsche Präsentationsnotiz. Alle Grenzen halten geschlossen.
Es wird weder ein verifizierter Live-Abschluss noch eine vollständige Behebung aller
Modellfehler behauptet. Die anschließende Überprüfung der Action-/Ankerübergabe bleibt offen.

Lokale Gates dieses Korrekturschnitts: `cargo fmt --all --check`,
`cargo test --workspace --all-features --offline --locked` und
`cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings`
bestehen, ebenso `node scripts/check-markdown-links.mjs` (125 Markdown-Dateien,
484 lokale Links) und `git diff --check`. Logs: `agent-v5-current-workspace-tests-2.log`
und `agent-v5-current-clippy-2.log` unter `target/research-eval`.
Der erste Workspace-Durchlauf fand eine noch auf die alte generische Fehlerklasse
prüfende Integrationserwartung; der reale Invalid-Output-Vertrag prüft jetzt zusätzlich
`malformed_json`, weiterhin genau zwei Provideraufrufe, null Reads, null Toolversuche
und den unveränderten InvalidModelOutput-Journaleintrag. Der gezielte Nachtest und das
erneute vollständige Gate bestehen. Diese Windows-Gates ersetzen weder andere native
Plattformläufe noch die weiterhin offene echte Modellabnahme.

Der anschließend allein laufende lokale Qwen-Nachtest auf demselben
`agent-v5-diagnostics`-Binary verwendet unverändert 8192 Kontext, 2048 Output und
FormatFieldOnly. Der Compile gelingt; die erste Patchvorschau endet jedoch mit
Conflict, ohne erfolgreiche Änderung oder Done (Runsequenz 5). Geschützte Dateien
bleiben gleich. Die genaue Konfliktart ist durch diese bisher generische Diagnose
noch nicht belegt. `qwen-live-1.log`, SHA-256
`8f2980c279fac5f7f931dc053b2a14a883a6ba2f1e3d197e0ca6ce8a1323753c`.

### Ankergebundener Einzelrepair und erster echter Done-Nachweis

ADR-0082 bindet Primärdecoder und denselben einzigen Repair an die aktuellen fünf
Patchanker sowie die aktuelle Step-ID von Run-/Ledgeraktionen. Die 14 Varianten
(sieben unterschiedliche falsche Anker, jeweils korrekte oder nochmals falsche
Korrektur) waren vor der Änderung rot und bestehen danach. Die Tests verlangen
genau zwei Modellaufrufe, null Toolaufrufe vor Übergabe und terminale Ablehnung einer
erneut falschen Korrektur. Die Mutationsgrenze revalidiert alle Anker weiterhin selbst.

Eingefroren: `agent-anchor-repair-20260907/agent-tests.exe`, SHA-256
`a614cb86af1bd0690a36fbb826ac2caff52002ae61bf7424f6c16501f69e9145`.
Mit demselben unveränderten Luna-Profil erreicht Lauf 1 nach 25,92 Sekunden erstmals
Run=Done (Sequenz 23), Step=Completed, echte erfolgreiche Command-Evidence und
`python -m pytest` mit Exitcode 0. Der unabhängige physische Test besteht ebenfalls;
gesperrte Dateien und Benutzerkatalog bleiben gleich. Die Wiederholung auf exakt
demselben Binary scheitert nach 30,18 Sekunden mit
InvalidActionAfterRepair(InvalidValue), obwohl die korrigierte Datei physisch besteht.
Das ist ein echter einzelner Funktionsnachweis, keine behauptete stabile Erfolgsrate.

Logs unter demselben Verzeichnis:

- `luna-live-1.log`: `e8bf1910611824a4f4abd65999746837caf98e6f13a182107d917240283e9814`
- `luna-live-2.log`: `b55bc4735b18530ac31fc6b3df30177b56c1bf10cf22acd561fdf76fcef8739e`

Alle 68 gezielten Agent-Unit-Tests, das erneute vollständige Offline-Workspace-Gate
(alle Features), Workspace-Clippy (alle Targets/Features, -D warnings), Formatierung,
Diffprüfung und 488 lokale Links in 126 Markdown-Dateien bestehen. Gesamtlogs:
`agent-anchor-repair-workspace.log` und `agent-anchor-repair-clippy.log`.
Die abgehakten Implementierungsschnitte sind damit lokal verifiziert; die getrennte
fortgesetzte Modell-/Inhaltsabnahme bleibt offen. Insbesondere werden die übrigen
ungültigen Aktionswerte und Qwens Patchvorschau-Konflikt nicht als behoben ausgewiesen.

### Bekannte Identitäten als V4-Schemakonstanten (ADR-0083)

Der aktuelle Vertrag setzt nur die fünf bekannten Step-/Worktree-/Spec-Felder sowie
bei laufendem Versuch dessen Run-ID und den operationalen Verifikationscommand fest.
Der Kontexttest war vor Implementation rot; Pending/InProgress und beide Grounding-
Modi prüfen danach das gesamte exakte Schema, unveränderte übrige Felder, tatsächliche
Bytes, deterministischen Digest und unverändertes Profil. Die Turnmatrix prüft nun
16 Fälle einschließlich eines abweichenden Command im Primär- und Einzelrepair.

Eingefroren: `agent-step-constants-20260907/agent-tests.exe`, SHA-256
`430fdae06636362cd7fec70d38319657b63f2500fe7e2e07484d4aa6005c568f`.
Lunas tatsächliches wiederholtes Schema wächst von 6681 auf 7045 Bytes; dieser
Mehrbedarf wird im unveränderten 16384/2048-Profil vollständig gezählt.
Alle fünf nachfolgenden Preflights gelingen. Die lokalen Modelle liefen strikt
nacheinander; Einstellungen, Providerorigin und öffentliche Fixture blieben gleich.

| Modell | Beobachteter echter Agent-Nachtest | Dauer |
| --- | --- | --- |
| Luna | Patch physisch korrekt, danach InvalidActionAfterRepair(InvalidValue), Failed Sequenz 24 | 34,55 s |
| Qwen 8k | PatchPreview Conflict vor Änderung, Execute Sequenz 5 | 35,90 s |
| Ornith | AwaitApproval Sequenz 7; Fixture verweigert Aktion außerhalb des exakt erlaubten Testumfangs | 31,33 s |
| GPT-OSS | Zwei echte Tests Exit 1, dann ModelFailed(InvalidResponse), Failed Sequenz 21 | 29,59 s |
| Granite | Zwei echte Tests Exit 1, dann InvalidReadResult, Failed Sequenz 26 | 36,19 s |

Kein neuer Done-Nachweis. Luna, Qwen, GPT-OSS und Granite bestätigen zusätzlich
physisch unveränderte geschützte Dateien; Ornith wird bereits vor der nicht erlaubten
Freigabe gestoppt. Keine dieser Fehlerklassen beweist allein ihre genaue Unterursache.
Die Schemakorrektur wird deshalb nicht als Behebung dieser Restfehler bewertet.
Logs im eingefrorenen Verzeichnis, SHA-256:

- `luna-live-1.log`: `9d9257dff4632439cc75d169286631d4b51c2704103654a4f1bd119ada519d02`
- `qwen-live-1.log`: `5bb702ac8b2be1b9a8c99e14beb372d75fbe7ea00ff23e3026665be4f2bfb0bb`
- `ornith-live-1.log`: `75fce45b71901ff57f50ed58c91b90d2f02f102ce1f26e4fa5c97a6cdc8abe66`
- `gptoss-live-1.log`: `53411f0c33e21e568e58b3302a58ebec4c5cfd83820aa233ef51318e191098cf`
- `granite-live-1.log`: `4ed0370688e22e969a02b1f639d19769047c977e2692afd8e3c745b65777d15f`

Lokale Gates bestehen: sechs Context-Units, 13 Context-Integrationen, 68 gezielte
Agent-Units (16 Ankerfälle), vollständiger Workspace mit allen Features offline/locked,
Workspace-Clippy mit allen Targets/Features und -D warnings, Formatierung und
Diff-/Markdown-Linkprüfung. Gesamtlogs: `agent-step-constants-workspace.log` und
`agent-step-constants-clippy.log`. Keine neue Abhängigkeit, DB- oder Frontendänderung.

### Präzise Ablehnungsgründe und zweite fehlende Prüfanforderung

Der neue Decoder-Regressionstest war mit dem generischen `invalid_value` rot.
Danach erhalten ungültige 64-Zeichen-Identitäten, No-Content-Change und Same-Move-Path
geschlossene Einzelrepaircodes; wiederholte ungültige Dokumente bleiben terminal,
und die Diagnose enthält weder Quelltext, Pfad noch Hash. 69 gezielte Agent-Units
und sieben echte Patchverträge bestehen, einschließlich fünf unabhängig erzeugter
Vorschaukonflikte mit unveränderten Dateien und erhaltener Symlink-/Hash-/Apply-Grenze.

Die damalige ausführbare Datei `agent-error-reasons-20260907/agent-tests.exe` hatte
SHA-256 `353ebcc2c8423907a1e7554155779104281c36431a34c8fbff081e449ab71cc8`.
Die in den Toolausgaben bestätigten Live-Ergebnisse:

- Luna 1: 25,15 s, Done Sequenz 23, Completed/verified, echter Test Exit 0,
  unabhängiger physischer Test erfolgreich, geschützte Dateien und Settings gleich.
- Qwen 1: 43,76 s, Vorschau `Conflict(TargetAlreadyExists)`, Execute Sequenz 5,
  keine erfolgreiche Änderung, geschützte Dateien gleich. Damit ist ein bereits
  vorhandenes Ziel belegt; nicht eine bestimmte Datei oder ein angeblich alter Hash.
- Luna 2: 22,09 s, physisch korrekter Patch, danach Run=Verify Sequenz 20 und
  Step=Verifying ohne Prozessnachweis. Acceptance verweigert zu Recht das unvollständige
  Ledger. Der zugehörige Code lässt `record_result` mit bloßer Read-Evidence die
  operationale Verifikation vorbereiten. Dies wird separat nach ADR-0084 korrigiert.

Das erste Gesamtgate dieses Diagnose-Schnitts scheiterte beim parallelen Kompilieren
mit Windows OS-Fehler 1455 (Auslagerungsdatei zu klein, mmap-/Allokationsfehler),
nicht an einer abgeschlossenen Testassertion. Der Nachlauf wurde auf zwei Buildjobs
begrenzt. Währenddessen verschwand das gesamte lokale `target`-Verzeichnis zwischen
zwei Checks; in dieser Arbeitsfolge wurde kein Löschbefehl ausgeführt. Der Nachlauf
endete ebenfalls erfolglos. Quellen und Git-Diff blieben erhalten. Alle historischen
`target/research-eval`-Pfade sind deshalb aktuell keine lokal verfügbaren Artefaktlinks;
die oben getrennt aufgezeichneten Toolausgaben ersetzen kein neues vollständiges Gate.
Die abschließende Abnahme wird aus erhaltenen Quellen neu aufgebaut. Keine
Speicher-/Systemeinstellung wurde geändert und keine neue Abhängigkeit installiert.

Der Nutzer bestätigte anschließend, `target` wegen mehr als 160 GB SSD-Verbrauch
selbst gelöscht zu haben. Der noch laufende Debug-Neuaufbau wurde daraufhin über
seine eigene Exec-Session abgebrochen. Weitere Gates verwenden pro Prozess
`CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0` und
`--jobs 2`; Debugassertionen, Overflowchecks und Testumfang bleiben unverändert.
Keine globale Cargo-/Systemkonfiguration und kein Repositoryprofil werden dafür
geändert. Weitere Artefakte oder Nutzerdateien wurden nicht gelöscht.

### RecordResult prüft den geplanten Schritt (ADR-0084)

Der erweiterte Selektor-/Ledger-Test reproduziert zuerst rot, dass aktuelle Read-
Evidence eine operationale Spec in Verifying versetzen konnte. Die korrigierte
Application-Grenze verweigert dies atomar für alle fünf operationalen Zielarten;
die neun Agent-Action-Tests bestehen. Finish und aktuelle Result-Notiz wählen bei
Command/Test/Diagnostic dieselbe exakte vorhandene Command-ID, fremde Steps,
Blocker, Replan, andere Aktionen und nicht ausführbare Zustände dagegen keinen Run.
Diff/UserConfirm erhalten keine Prozessfreigabe; Legacy-Read-Verifikation bleibt bestehen.

Neu aufgebaut und eingefroren: `agent-result-request-20260907/agent-tests.exe`, SHA-256
`5fbe644b68b7744f0ea01f5d35739fc2e10017058b110b31baa27b45f0017290`.
Luna besteht drei aufeinanderfolgende echte Läufe auf diesem unveränderten Binary
und Profil (16384/2048, RepeatSchemaInPrompt; tatsächliches Grounding 7045 Bytes):

| Lauf | Dauer | Runsequenz bei Done |
| --- | --- | --- |
| 1 | 35,85 s | 27 |
| 2 | 21,24 s | 23 |
| 3 | 16,11 s | 23 |

Alle drei erreichen echte Testausführung mit Exit 0, Step=Completed/verified und
Run=Done. Die unabhängige physische Verifikation besteht; geschützte Dateien und
native Settings bleiben gleich. Dies belegt Wiederholbarkeit dieser einzelnen
öffentlichen Aufgabe, nicht allgemeine Fehlerfreiheit oder einen Nachweis aller Modelle.
Logs unter dem eingefrorenen Verzeichnis, SHA-256:

- `luna-live-1.log`: `eeda80de880474c7225e0143f159155eb0365e480f5fa380b403ade2ebdfecf9`
- `luna-live-2.log`: `d7aa0e9461ab0b1e6cb480e4bc5308f9066428869426090ab96734f49cf74774`
- `luna-live-3.log`: `953bf7b587355106dec2302380b17322f1ccbd4736c78212e5eca515ac53b15a`

Der ergänzende Google-Gemma-Agentlauf auf demselben Binary erreicht mit seinem
unveränderten 16384/4096-FormatFieldOnly-Profil den Context-Preflight, wird aber beim
ersten Modellturn als ModelFailed(Rejected) beendet (2,42 s, Failed Sequenz 6).
Die physische Prüfung bleibt rot und die geschützten Dateien bleiben unverändert.
Der konkrete Provider-Ablehnungsgrund ist weiterhin nicht belegt; kein Schema- oder
Safetygrund wird aus der generischen Klasse abgeleitet. `google-live-1.log`, SHA-256
`1baeb89c99011e972ad823908a60dd41bf4c9961eb7f9feace8507972e7bcba2`.

Das abschließende neu aufgebaute Gate dieses Schnitts besteht vollständig:
`cargo test --workspace --all-features --offline --locked --jobs 2`,
`cargo clippy --workspace --all-targets --all-features --offline --locked --jobs 2 -- -D warnings`,
`cargo fmt --all --check`, `node scripts/check-markdown-links.mjs` (128 Dateien,
498 lokale Links) und `git diff --check`. Test-/Clippylogs sind
`agent-result-request-workspace.log` und `agent-result-request-clippy.log`.
Builds verwenden die oben dokumentierten prozesslokalen Debug-/Incremental-Einstellungen;
der Nutzer erlaubt inzwischen ausdrücklich erneutes Wachstum von `target`. Kein
Testumfang oder Sicherheitscheck wurde dafür reduziert. Alle Befunde zu den anderen
Modellen, Vorschaufehlern und fortgesetzter Rechercheabnahme bleiben offen.

### Reine Patch-Snapshotzulassung (ADR-0085)

Der zuerst rote Konfliktfall besteht nach Bindung derselben immutable Publikation
an Primär- und Repairdecoder. 71 gezielte Agententests bestehen, darin 30 Kombinationen
aus fünf bekannten Konflikten und wiederholt falscher Antwort, gültigem Update,
Add, Move, Delete oder Inspect. Genau zwei Antworten werden verbraucht; abgewiesene
Patches führen kein Tool aus. Nur der gültige Inspect-Korrekturvorschlag erzeugt
genau einen journalisierten Read. Falscher Snapshot oder Index-Run stoppt bereits
vor dem Provider. Schema-only-Replay bleibt unverändert.

Eingefrorenes Binary `agent-patch-snapshot-20260907/agent-tests.exe`, SHA-256
`7edbbd473b08ea706ccf20c010f5d503c92f6e7bc9264bd36c3833808a4f66d8`:

- Qwen: 179,06 s einschließlich Probe, Failed Sequenz 6. Der Einzelrepair endet
  mit `PatchConflict(TargetAlreadyExists)`; kein Patch wird ausgeführt, unabhängige
  Prüfung weiter rot, geschützte Dateien unverändert. Der Schutz wirkt, aber der
  geschlossene Code allein ermöglicht diesem Modell noch keine erfolgreiche Korrektur.
- Luna: 17,92 s, Done Sequenz 23, tatsächlicher Test Exit 0, Completed/verified,
  unabhängige physische Prüfung grün und geschützte Dateien/Settings unverändert.

Logs desselben Verzeichnisses, SHA-256:
`qwen-live-1.log` = `aa962c9238a819d43323670733331687a55a2f27e5366a286d194c1f3f12afa9`,
`luna-live-1.log` = `93eddbee1deaa694aaf38bb0a6e2a2fcb289bbaf4f3986f0645b320946b94045`.
Dies ist keine Abnahme der gesamten Modellmatrix. Insbesondere Qwens wiederholte
falsche Operation und Googles noch nicht lokalisierte Ablehnung bleiben offen.

Vollständige Gates auf diesem Stand erfolgreich: `cargo test --workspace --all-features
--offline --locked --jobs 2`, `cargo clippy --workspace --all-targets --all-features
--offline --locked --jobs 2 -- -D warnings`, `cargo fmt --all --check`,
`node scripts/check-markdown-links.mjs` (129 Dateien/506 lokale Links) und
`git diff --check`. Test-/Clippylogs: `agent-patch-snapshot-workspace.log` und
`agent-patch-snapshot-clippy.log`. Dieselben prozesslokalen kompakten Buildprofile;
keine neuen Abhängigkeiten, Frontend-, Settings- oder Datenbankschemaänderungen.

Weitere strikt nacheinander ausgeführte lokale Läufe auf demselben
`agent-patch-snapshot-20260907`-Binary erreichen den Kontext-Preflight, aber keinen
verifizierten Abschluss. Jeweils Failed Sequenz 6, unabhängiger Test rot und
geschützte Dateien unverändert:

| Modell | Dauer | Terminaler Befund |
| --- | --- | --- |
| Ornith 9B | 49,63 s | IncompleteModelOutput |
| GPT-OSS 20B | 19,36 s | ModelFailed(InvalidResponse) |
| Granite 8B | 19,88 s | InvalidPatchOperation(SameMovePath) nach Einzelrepair |

SHA-256: `ornith-live-1.log` =
`6d6ce69ba86dc66ea7440045b801d7a2ecfeb6a19e80b2fd30bbb6a81a0386cb`,
`gptoss-live-1.log` = `17ee72b56b8c95de143a43380d43540aa5cbf37c4eb2abfe0c16f4a487ba00a5`,
`granite-live-1.log` = `8151a1bd47c916df04ee319cb3ec36394efbc62a07e690ab196259557f934376`.
Weder eine erfolgreiche Probe noch das Modellalter erklären allein diese Fehler.

### Vollständiges Agent-V4-Schema im Gemini-Adapter (ADR-0086)

Die bisherige lokale Providerprüfung testete Agent V1–V3, nicht V4. Dessen Flow-
Offset trägt `multipleOf:50`; der explizite Adapter lehnt es vor jeder HTTP-Anfrage
ab. Der neue vollständige V4-Test reproduziert zuerst `Error: Rejected`. Nach der
engen Wire-Projektion bestehen 27 Provider-Unit- und 16 Gemini-HTTP-Verträge.
Unveränderte Core-Decodierung akzeptiert 0/50/4050 und lehnt 1/49/4051/4100/-50 ab.
Const-/Enum-Literale bleiben gleich; unbekannte Keywords bleiben abgewiesen.
Der HTTP-Gegentest liefert absichtlich Offset 1 als STOP: der Decoder verweigert
ihn trotz erfolgreicher Übertragung des vollständigen Schemas.

Eingefroren: `agent-gemini-v4-20260907/agent-tests.exe`, SHA-256
`5d932abe234688da391ce48469b03aa4cdf0ab70c0c766064e0383bf23188441`.
Google Gemma liefert nun tatsächliche Modellausgabe statt lokaler Schemaablehnung,
scheitert nach Einzelrepair aber an `InvalidPublicNote` (10,57 s, Failed Sequenz 6).
Die unabhängige Prüfung bleibt rot, geschützte Dateien unverändert. Das unveränderte
Profil bleibt 16384/4096, FormatFieldOnly. `google-live-1.log`, SHA-256
`ec512ac273e14ffece9dbf7aafe2cb753262f0004a385585a199272812a95065`.
Damit ist die lokale Übersetzungsursache behoben, nicht der gesamte Google-Agent.
Der streng vor der Action gelesene Präsentationsblock verhindert aktuell schon die
Prüfung der eigentlichen Aktion; deren Gültigkeit wird deshalb nicht behauptet.

Vollständige lokale Gates bestehen auch nach ADR-0086: `cargo test --workspace
--all-features --offline --locked --jobs 2`, `cargo clippy --workspace --all-targets
--all-features --offline --locked --jobs 2 -- -D warnings`, `cargo fmt --all --check`,
`node scripts/check-markdown-links.mjs` (130 Dateien/512 lokale Links) und
`git diff --check`. Logs: `agent-gemini-v4-workspace.log` und
`agent-gemini-v4-clippy.log`. Prozesslokale Buildprofile bleiben unverändert.

### AgentAction V5 ohne redundante Statusnotiz (ADR-0087)

Der neue V5-Regressionsfall scheitert zuerst mit UnknownOrMissingField. Nach der
versionierten Umstellung bestehen 73 gezielte Agententests, einschließlich strikter
V3/V4-Notizen, verbotener V5-Notizinjektion, genau eines Repairs, aller aktuellen
Anker-/Snapshotkonflikte und unveränderter Replan-V4-/Research-V5-Grenzen.
Das gesamte ausführbare V5-Subschema entspricht exakt V4 ohne öffentliche Notiz.
Die bisherige AgentTurnExecution-Notiz wird im produktiven Executor nicht konsumiert;
Fortschritt stammt unverändert aus tatsächlichem Journal und Ledger.

Das gezielte Paketgate besteht: 1 Coding-Eval, 8 mutierende und 3 read-only Harness-
Verträge, 6 Context-Units/13 Context-Contracts, 27 Provider-Units und HTTP-Verträge
für Gemini (16), Ollama (14) und OpenAI (10). Live-Tests bleiben dort opt-in.
Log: `agent-v5-targeted.log`. Keine Profile, Budgets oder Sicherheitsprüfungen erweitert.

Eingefrorenes Binary `agent-v5-core-status-20260907/agent-tests.exe`, SHA-256
`b523c2a72abe5aead0d04c1cd07cf917d5e99e203c39dbd7b817486ab3334152`.
Der tatsächliche Systemtext sinkt von 823 auf 809 Bytes, Lunas vollständiges
Schema-Grounding von 7045 auf 6382 Bytes bei unverändertem 16384/2048-Profil.
Dies ist eine Vertragsgrößenmessung, kein Geschwindigkeits- oder Qualitätsbeweis.

| Modell/Lauf | Dauer | Nachweis |
| --- | --- | --- |
| Luna 1 | 23,80 s | Patch physisch korrekt, danach wiederholter NoContentChange im Repair; Failed Sequenz 22 |
| Luna 2 | 16,97 s | Done Sequenz 23, echter Test Exit 0, Step Completed/verified, unabhängige Prüfung grün |
| Google Gemma 1 | 10,76 s | Mehrere Turns, danach ModelFailed(Unavailable), Failed Sequenz 16 |
| Google Gemma 2 | 11,16 s | Erneut ModelFailed(Unavailable), Failed Sequenz 16 |
| Qwen 8k | 38,43 s | PatchConflict(TargetAlreadyExists) nach Einzelrepair, Failed Sequenz 6 |

Geschützte Dateien bleiben in allen Läufen unverändert. Nur Luna 2 erreicht die
vollständige Implementierungsverifikation und bestätigt außerdem unveränderte
native Settings. Die übrigen physischen Tests bleiben bis auf den bereits korrekten
Patch in Luna 1 rot. Googles Schema-/Notizabbruch ist überwunden; aus Unavailable
wird ohne weitere Evidenz kein bestimmter HTTP-Status oder Quotenfehler abgeleitet.
Lunas Wiederholung nach korrektem Patch und Qwens Operationswahl bleiben offen.

SHA-256 der Logs im selben Verzeichnis:

- `google-live-1.log`: `c8d924cfa5ed1923ef2289f471b50722fd15d598a8489852ceb833291b9681ce`
- `google-live-2.log`: `2051dd26af0268322907b8e8c5a6180877a1e15f34e4ddf436c021c5a3ced458`
- `luna-live-1.log`: `e00dffed36ca50b541b34386cf2decc250ff5f7d898fda5297507f30224d13a0`
- `luna-live-2.log`: `248947ba9e678093eae028fb08cab9552768181cbb5c1f5ea8e1437a022e52ac`
- `qwen-live-1.log`: `de9b2e797ba17c3de0738e278486c7542deef3a0ea47d67a7c6d63bf7ed23f9b`

Vollständige Gates bestehen auf dem finalen V5-Stand: `cargo test --workspace
--all-features --offline --locked --jobs 2`, `cargo clippy --workspace --all-targets
--all-features --offline --locked --jobs 2 -- -D warnings`, `cargo fmt --all --check`,
`node scripts/check-markdown-links.mjs` (131 Dateien/521 lokale Links) und
`git diff --check`. Nach Präzisierung des Debug-Redaktionstests wurden Workspace
und Clippy erneut vollständig ausgeführt; abschließende Logs sind
`agent-v5-core-status-final-workspace.log` und `agent-v5-core-status-final-clippy.log`.
Die letzte Änderung betrifft nur diese Testassertion; die Live-Binaries enthalten
denselben Produktionscode. Keine neuen Abhängigkeiten, UI- oder Storage-Migrationen.

### Abschluss der V5-Lokalserie vor ADR-0088

Dasselbe eingefrorene V5-Binary wurde anschließend weiter strikt sequentiell geprüft:

| Modell | Dauer | Nachweis |
| --- | --- | --- |
| Ornith 9B | 22,36 s | InvalidActionAfterRepair InvalidValue, Failed Sequenz 6 |
| GPT-OSS 20B | 18,48 s | ModelFailed(InvalidResponse), Failed Sequenz 6 |
| Granite 8B | 23,37 s | Zwei echte Testresultate Exit 1, danach InvalidReadResult, Failed Sequenz 26 |

Alle drei unabhängigen physischen Prüfungen bleiben rot, geschützte Dateien bytegleich.
Diese Fehler werden nicht durch das Modellalter erklärt; eine konkrete Ursache muss
separat aus dem jeweiligen Vertrag nachgewiesen werden.

SHA-256 im Verzeichnis `agent-v5-core-status-20260907`:

- `ornith-live-1.log`: `27d99180f44c7272f0072af4a0c6acd203ea08de7cd29f7bea52fd1056eb437b`
- `gptoss-live-1.log`: `86bd675a8f7d8cd746f27b339d4e60ca6fa04d9826596c75a9c57855e3058932`
- `granite-live-1.log`: `8edb68458d08544a84fe33115d072e4c549e894064cc963a8d44591d2e4f8f0f`

### ADR-0088: Ausführungsrückmeldung

Der echte `patch_waits_for_approval_then_reindexes_before_compiling_context`-Test
scheiterte vor der Korrektur mit `fresh patch context lost the durable execution receipt`.
Nach der Korrektur besteht er einschließlich Rekonstruktion aus separat geöffnetem
libSQL-Store und identischem Kontextdigest. Die gesamten acht mutierenden, drei
read-only und der Coding-Eval-Vertrag bestehen. Die bisherigen 13 Context-Verträge
bleiben grün; drei neue prüfen die wirklichen Kontext-/Budget-/Freshness-/History-Grenzen.
Zwei Application-Tests prüfen fehlende Laufzeit, Cancellation und Timeout mit
nachweislichem Drop des besessenen ausstehenden Reads. Kein Timeout wird umgangen.

Der Harness-Testtreiber nutzt jetzt direkt die bereits vorhandene Tokio-Workspace-
Version als Dev-Abhängigkeit, damit auch diese Tests dieselbe Timer-Voraussetzung
wie der Produktionslauf erfüllen. Keine neue Bibliotheksversion, Produktabhängigkeit,
Migration, Provider-/Profiländerung oder Freigabeerweiterung.

Die reproduzierbare Context-Fixture misst vor/nach Zuschaltung der Rückmeldung
2283→2712 Packbytes (8k/2k) und 2502→2931 (16k/4k). Der vollständig gezählte
Pflichtanker wächst jeweils um 429 konservative Einheiten; Output- und Safety-
Reserve bleiben gleich. Der zusätzliche echte Wiederaufbau aus dem neu geöffneten
Store benötigt im einzelnen separaten Messlauf 11275 µs. Das ist zusätzlicher
lokaler Aufwand, kein Geschwindigkeitsgewinn; weder ein Einzellauf noch concurrent
Buildzeiten sind portable Benchmarks. Reproduktion: die beiden Tests mit
`execution_receipt_is_mandatory` beziehungsweise
`patch_waits_for_approval_then_reindexes_before_compiling_context` und `--nocapture`.
Messlogs: `agent-execution-checkpoint-context-measure.log` und
`agent-execution-checkpoint-reopen-measure.log`.

Eingefrorenes Binary `agent-execution-checkpoint-20260907/agent-tests.exe`, SHA-256
`434fca74ec0359a7e0ad8cd5eeb1ad14f05cfde5fb3cb2eab279681bf19943bc`.

| Modell/Lauf | Dauer | Tatsächliches Ergebnis |
| --- | --- | --- |
| Luna 1 | 14,16 s | Done 21, echter Test Exit 0, Completed/verified |
| Luna 2 | 12,05 s | Done 21, echter Test Exit 0, Completed/verified |
| Luna 3 | 12,20 s | Done 21, echter Test Exit 0, Completed/verified |
| Google Gemma | 12,06 s | ModelFailed(Unavailable), Failed 16 |
| Qwen 8k | 189,34 s inkl. Probe | Modellaufruf durch Versuchstimeout beendet, Cancelled 6 |
| Ornith 9B | 138,18 s inkl. Probe | Modellaufruf durch Versuchstimeout beendet, Cancelled 6 |
| GPT-OSS 20B | 74,28 s | Test Exit 1, anschließend InvalidResponse, Failed 12 |
| Granite 8B | 25,91 s | Zwei Tests Exit 1, danach InvalidAfterRepair, Failed 23 |

Alle drei Luna-Läufe bestätigen zusätzlich unabhängige physische Verifikation,
unveränderte geschützte Dateien und unveränderte Settings. Alle übrigen physischen
Prüfungen bleiben rot, geschützte Dateien unverändert. Lokale Modelle liefen strikt
nacheinander. Keine zusätzlichen Repairs, Modellwechsel oder Profileinstellungen
haben einen fehlgeschlagenen Fall in einen Erfolg umgewandelt. Die neue Rückmeldung
beweist keine Lösung der separaten Provider-/Aktions-/Replanfehler und drei Luna-
Erfolge sind keine allgemeine Agentabnahme.

SHA-256 der Live-Logs im selben Verzeichnis:

- `luna-live-1.log`: `1e5dc6e4528860bef7f5886286f6b653307ef72e6fcdbb0f77447c07fec3209e`
- `luna-live-2.log`: `46863826fc488503adc0216abaa87fca6fc58a8d92f2c4b46c81cc17ec734acc`
- `luna-live-3.log`: `53dbb52f5affe5470ea21112357ee6c3ea4af1c150ff9f2a8c50fdc09aa466bf`
- `google-live-1.log`: `84788c7d8c565c4b257960d7851eddf4749b1e85ff8218beafa98e3d46ac2356`
- `qwen-live-1.log`: `90d5e958f66b9494afcf3cc6f6ec68be6bc6ef3024f976cb79e6e7cdcc95001f`
- `ornith-live-1.log`: `cbe97fd60aa1869a150996fd2b5762e52076d111f25ef739185bbf9c111b2a64`
- `gptoss-live-1.log`: `f9064f41f98df4707bc55daecafd7266f287d23f4bcfeeeda596af3cef7f19ce`
- `granite-live-1.log`: `76c6223c6d9d72e675bcdba5c02e00f63da79340e9384690e4f4dea69431687c`

### Erneute Luna-Ask-/Plan-Matrix auf dem ADR-0088-Binary

Eine vollständige Zwölfermatrix endet in 161,00 s mit 12/12 abgeschlossenen,
durabel fertigen Fällen, null Nutzerhalten und null adaptiven Reads. Die bestehende
Rubrik v2 meldet 12/12 Treffer. Originale bleiben unverändert. Report
`eval-1788793090398.jsonl`, SHA-256
`b699407429a34c8c35f929829c5cbfb04d6fc87f24c4031697056f3f41740f15`;
Log `agent-execution-checkpoint-20260907/luna-research-matrix.log`, SHA-256
`92b5832e978af046fbe8d2e6af8caa603fe1fc4048719745db67e80b209668fe`.

Die Sichtprüfung widerspricht einer vollständigen Inhaltsabnahme: Audit 1:1 nennt
Speichern der Liste; Plan 3:0 behauptet in der Recherchegrundlage, jeder erfolgreiche
add_task-Aufruf persistiere. `JsonStorage.save_tasks` und `SQLiteStorage.save_tasks`
geben im tatsächlichen Original aber lediglich ein Tupel zurück. Manager ruft sie
auf; Aufrufnachweis ist kein Persistenznachweis. Außerdem akzeptiert v2 das Wort
„Writer“ als Teilstringtreffer für die gesonderte Methode `write`. Dieser Rubrikfehler
und die nicht belegten Nebenwirkungen bleiben ausdrücklich offen. Ein höherer
Abschlusszähler schließt diese Gegenbeispiele nicht.

ADR-0088 ist lokal vollständig verifiziert: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets --all-features --offline --locked --jobs 2
-- -D warnings`, `cargo test --workspace --all-features --offline --locked --jobs 2
-- --test-threads=1`, `node scripts/check-markdown-links.mjs` (132 Dateien/526 lokale
Links) und `git diff --check`. Abschließende Rust-Logs sind
`agent-execution-checkpoint-complete-clippy.log` und
`agent-execution-checkpoint-complete-workspace.log`; beide enden mit Exit 0.
Zuvor wurden eine veraltete Policy-Versionsassertion und zwei nicht erlaubte
explizite Panic-Wächter sowie die komplexe Fixture-Tupelsignatur korrigiert, ohne
Produktionsverhalten oder Prüfumfang zu lockern. Danach liefen beide Gates erneut
vollständig. Das eingefrorene Live-Binary enthält denselben Produktionscode;
die letzten Änderungen betreffen ausschließlich Tests. Keine Frontend-Änderung,
keine plattformübergreifende UI-/Releaseabnahme behauptet.

### Rubrik v3: vollständige Methodenbezeichner, keine umgeschriebenen Altreports

Der neue Regressionstest scheitert vor der Korrektur an `Writer` als vermeintlichem
`write`-Nachweis. Nach der Korrektur bestehen alle sechs `research_matrix_`-Tests;
zusätzlich werden `audit_log.txt` statt `_log`, `batch_add_task` statt `add_task`
und `get_task_response` statt `get_task` abgewiesen. Qualifizierte echte Aufrufe
bleiben gültig. Produktionsprompts, Quelle, Fragen, Budgets und Berechtigungen
sind unverändert. Neue JSONL-Zeilen tragen `rubric_version: 3`; alte v1/v2-Berichte
werden nicht geändert. Der Worttest ist weiterhin kein Beweis für Aufrufreihenfolge
oder behauptete Nebenwirkungen.

Eingefrorenes Binary `research-rubric-v3-20260907/research-tests.exe`, SHA-256
`c59a91062f373a24960e9fe5f71579a544c8b9385c7f5586f2d3386995f8d855`.

| Modell | Dauer | Abschluss / Work ready | Rubrik v3 | Nutzerhalte / adaptive Reads |
| --- | --- | --- | --- | --- |
| Luna | 167,72 s | 12/12 | 10/12 | 0 / 0 |
| Qwen 8k | 215,37 s | 12/12 | 9/12 | 0 / 0 |

Luna lässt `write` in Audit 1:1 und 1:2 aus, Qwen in allen drei Audit-Varianten.
Die Sichtprüfung bestätigt die Auslassungen; die Antworten beschreiben das Schreiben
in Prosa, nennen aber nicht die verlangte vollständige Methodenkette. Luna 1:2
bezeichnet zudem `open(...)` selbst als Writer. Die unbelegte Behauptung, der
Aufruf von `save_tasks` speichere tatsächlich die Aufgabenliste, tritt in Luna 1:1
und 1:2 erneut auf. Beide Live-Matrixtests enden deshalb korrekt rot (Exit 101),
nicht wegen Rechercheabbruch oder Nutzerfrage. Die Quellen bleiben bytegleich.

SHA-256:

- `eval-1788794346926.jsonl`: `22bf0b8dfad1810b356d6e5b91f147261ffa3c98ff5dafb9474173f3c5c6305f`
- `eval-1788794362960.jsonl`: `858b7e44b9d8850978004f36258bd90b928e068f8c9baf986c8c6874afbbe28f`
- `research-rubric-v3-20260907/luna-matrix.log`: `7b92028a7d7daf81769688833a885c53b92754315afcec43b4ed7b8589bdde63`
- `research-rubric-v3-20260907/qwen-matrix.log`: `9a2a5c43d52af9c23f853b9a25ef7cadda572b8f1d3a65370bb71f3ad720ea75`

Lokale Gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
--all-features --offline --locked --jobs 2 -- -D warnings` und `cargo test --workspace
--all-features --offline --locked --jobs 2 -- --test-threads=1` enden mit Exit 0.
Logs: `research-rubric-v3-red.log`, `research-rubric-v3-targeted.log`,
`research-rubric-v3-clippy.log`, `research-rubric-v3-workspace.log`.
Die verschärfte Auswertung behebt den Messfehler, nicht die jetzt sichtbaren
Inhaltsdefekte. Diese bleiben Teil der offenen Modellabnahme.

### Replan-Zulassung: konkrete Fehler statt verschluckter Ursachen

`analyze_replan` reduzierte bisher JSON-, Decoder-, Packet- und Originalzulassungs-
Fehler mit `.ok()?` auf denselben Leerwert. Der einzige Repair enthielt deshalb nur
die allgemeine Aufforderung, das Schema zu erfüllen. Ein neuer echter Turntest
scheitert vor der Korrektur mit `InvalidAfterRepair` statt der erwarteten Klassifikation
`Admission(UndeliveredQuote)`; Log `replan-admission-diagnostics-red.log`.

Die pure Zulassung behält jetzt geschlossene Fehlerklassen. Nur daraus und aus festen
Core-Texten entsteht der höchstens 512-Byte-Repairhinweis. Der Turntest prüft
Primärerfolg, weiterhin ungelöste leere Ergebnisse, zwei abgewiesene Antworten und
eine gültige Korrektur nach abgewiesenem Anker: höchstens zwei Requests, identischer
ursprünglicher Kontext und Schema, exakt ein Repair, null Tools, unverändertes
Eingangscheckpoint und weiterhin Interpretation statt Implementierungsverifikation.
Zwei reine Tests prüfen falsche Entscheidungen, fehlende/zusätzliche Felder,
erfundene Anker, private Sentinels und die feste Feedbackgrenze.

Erster eingefrorener Stand (noch ohne getrennte Replan-Read-Ursachen):
`replan-admission-diagnostics-20260907/agent-tests.exe`, SHA-256
`f2f9620285d81fca3ff3c9f270878daf7ef4cfe302cc1704a9c62a2477517762`.

| Modell | Dauer | Tatsächliches Ergebnis |
| --- | --- | --- |
| Luna | 14,32 s | Done 23, echter Test Exit 0, unabhängig geprüft, Completed/verified |
| Google Gemma | 13,75 s | Weiterhin Unavailable, Failed 16 |
| Granite | 25,16 s | Zwei Tests Exit 1, danach InvalidReadResult, Failed 26 |

Geschützte Dateien bleiben unverändert; Luna bestätigt außerdem unveränderte native
Settings. Granite erreicht diesmal einen anderen terminalen Pfad; ohne genauere
Ursache ist das weder eine Replan-Abnahme noch ein Nachweis, dass das neue Feedback
den früheren Fehler ursächlich behoben hat.

SHA-256 im selben Verzeichnis:

- `luna-live.log`: `db2d085502c7caa63385b99bbddcc4f92e9a7d9601b0768dc071cd6a4258546f`
- `google-live.log`: `3c639886d7a65d6776fe2ae63367983cba1aa231e40983dfbc55b44bfefdd3d2`
- `granite-live.log`: `8e40dcba776068119f2c5d634e3cb5389c4e06a5b6571aa9eb7432ed04635315`

Der folgende Schnitt unterscheidet zusätzlich den bereits vor dem Tool verweigerten
Lesevorschlag (Duplikat, vier Reads ausgeschöpft, falsche Aktionsklasse) von einem
tatsächlich fehlerhaften Toolresultat. Die bisherigen Grenzen und der terminale
Verlauf sind unverändert. Eine Wiederholung wird nicht zu einem weiteren Read.

Der finale Diagnose-Stand ist in `replan-read-diagnostics-20260907/agent-tests.exe`
eingefroren, SHA-256
`d0671e15b2925178d6a9f82f1ebfe296302abca58e2193c4cb24e39891592316`.
Granite endet nach 22,43 s mit `ReplanReadRejected(RepeatedRead)`, Failed 26, zweimal
Test Exit 1, unabhängiger Prüfung rot und unveränderten geschützten Dateien. Log
`replan-read-diagnostics-20260907/granite-live.log`, SHA-256
`6390d199d5377946fec80082dcd94f0d7eb19cec186adabc8bede31cec03d74b`.
Damit ist dieser konkrete terminale Ablehnungszweig nachgewiesen; noch kein Erfolg
bei der Fehlerbehebung der öffentlichen Coding-Aufgabe.

Der neue vollständige Turn-Grenztest unterscheidet verweigertes Duplikat,
ausgeschöpftes Budget und tatsächlich erlaubten neuen Read. Abgewiesene Vorschläge
erzeugen null Tool-/Recovery-Aufrufe und keine zusätzliche Reparatur; ein neuer
Read erreicht genau einen Toolaufruf. Bestehende Journalcodes bleiben erhalten.
Die gezielte finale Serie umfasst neun Application-, drei Context-, drei Desktop-
sowie je einen Domain- und libSQL-Migrations-Replantest. `cargo fmt --all --check`, die vollständige
Workspace-Clippy-Prüfung mit `-D warnings` und die vollständige Workspace-Testsuite
(`--all-features --offline --locked --jobs 2 -- --test-threads=1`) bestehen.
Finale Logs: `replan-admission-diagnostics-serial-targeted.log`,
`replan-admission-diagnostics-serial-clippy.log` und
`replan-admission-diagnostics-serial-workspace.log`.

Ein vorher parallel zum laufenden Workspace-Test gestarteter Wiederaufbau scheiterte
mit Windows LNK1104 an der noch geöffneten `index_repository_contract`-Testdatei.
Nach Ende der alten Suite wurden alle obigen Gates seriell neu ausgeführt; keine
Datei wurde dafür gelöscht und keine Prüfung ausgelassen. Die zuvor grüne Suite
allein wurde nicht als finale Abnahme des ergänzten Read-Diagnosecodes verwendet.

Bei der weiteren Codeprüfung fiel getrennt auf: `read_key` verwendet für Claim-
Inspektionen Debug-Text, während `ModuleCardClaimId` dort absichtlich redigiert wird.
Dadurch kollidieren unterschiedliche Claim-Ziele. Das ist nicht als Ursache des
Granite-Fixtures nachgewiesen und wird separat mit dauerhafter Legacy-Behandlung
bearbeitet; es darf nicht durch Lockerung des Duplikatschutzes verdeckt werden.

### ADR-0089: Replan-Leseduplikate im bestehenden Einzelrepair

Der neue Turntest scheitert vor der Korrektur daran, dass ein bereits bekannter
Read sofort terminal wird und null statt eines Repairs verbraucht. Log
`replan-duplicate-repair-red.log`. Danach besteht die gesamte Kombination aus
Duplikat→anderem Read (genau ein Tool-/Recovery-Aufruf), Duplikat→Duplikat und
Strukturfehler→Duplikat (null Toolwirkung, genau zwei Modellantworten und kein
dritter Versuch). Der ursprüngliche Request samt Schema bleibt im Repair erhalten;
der feste Hinweis enthält weder Query noch Pfad. Ein ausgeschöpftes Readbudget
öffnet weiterhin keinen Duplikatrepair. Fertige Replan-Untersuchungen und Turns
ohne Replan behalten ihre normale Zulassung.

Der tatsächliche Storage-Vertrag öffnet den Store erneut, rekonstruiert dieselbe
Quittung und prüft einen weiterhin gesperrten identischen File-Read bei unverändertem
Readzähler. Alle bestehenden Anker-, Snapshot-, Mutations-, Context-, Provider-,
Storage- und Mehrmodusverträge bestehen. Keine Migration oder Profiländerung.

Eingefrorenes Binary `replan-duplicate-repair-20260907/agent-tests.exe`, SHA-256
`ecab19018a650ea07f8fc4624e98b13cb6a62f2f19d5c4760260e6be1ffae34d`.
Es enthält den finalen Produktionscode; danach wurde nur der zusätzliche
Unit-Grenztest für fertige und Nicht-Replan-Kontexte ergänzt.

| Modell | Dauer | Tatsächliches Ergebnis |
| --- | --- | --- |
| Luna | 15,72 s | Done 23, echter Test Exit 0, Completed/verified, unabhängige Prüfung grün |
| Granite 8B | 52,95 s | Sechs Tests Exit 1, später RepeatedReplanRead nach Einzelrepair, Failed 62 |
| Qwen 8k | 39,04 s | TargetAlreadyExists nach Einzelrepair, Failed 6 |

Die lokalen Modelle liefen strikt nacheinander. Geschützte Dateien bleiben überall
bytegleich; Luna bestätigt zusätzlich unveränderte native Settings. Der spätere
Granite-Abbruch ist kein Erfolg und kein Geschwindigkeitsgewinn. Auch eine technisch
nutzbare Korrekturmöglichkeit garantiert nicht, dass das Modell den anderen Read
zielgerichtet auswählt oder den eigentlichen Programmfehler behebt. Wiederholte
erfolglose Verifikationen und Qwens falsche Patch-Operationswahl bleiben offen.

SHA-256 im selben Verzeichnis:

- `granite-live.log`: `dd564cac5093a4e1fd0e869dea4efccfaf6d4d0c5c7b3209b0900fd25ade67ca`
- `qwen-live.log`: `197c297bede325c1759865c64da2c41e4e2396d7af1d3cc56dfb10fac02121d8`
- `luna-live.log`: `d2945185b29809e2da070782a8db9c1f80ac840973da8fb9e8368afa7c25641b`

Finale lokale Gates: `cargo fmt --all --check`, `cargo clippy --workspace
--all-targets --all-features --offline --locked --jobs 2 -- -D warnings`,
`cargo test --workspace --all-features --offline --locked --jobs 2 -- --test-threads=1`
und lokale Markdown-Link-/Diff-Prüfung. Alle Rust-Gates enden mit Exit 0;
Logs `replan-duplicate-repair-final-targeted.log`, `replan-duplicate-repair-clippy.log`
und `replan-duplicate-repair-workspace.log`. Die finale Serie wurde nach der letzten
Unit-Testergänzung vollständig neu gebaut und seriell geprüft. Keine plattformfremde
oder allgemeine Modellabnahme wird daraus abgeleitet.

### ADR-0090: Wertgebundene Claim-Leseidentitäten

Der neue Regressionstest scheitert zunächst an zwei verschiedenen Claim-IDs mit
demselben bisherigen Debug-basierten Hash (`replan-claim-key-red.log`). Neue
Claim-Reads verwenden nun die kanonischen ID-Bytes in einer getrennten Hashdomäne.
Vier verschiedene IDs sind vier echte Reads; dieselbe ID bleibt auch nach einem
erfolglosen Read gesperrt. Andere Readschlüssel ändern sich nicht.

Eine eingefrorene V1-Quittung prüft die verlorene historische Zielidentität unabhängig
vom heutigen Debug-Format. Laufende, erfolgreiche und erfolglose alte Versuche behalten
ihren Platz. Ein anderer Readtyp ist innerhalb des vorhandenen Limits möglich;
eine weitere Claim-ID ist kein Umgehen der Mehrdeutigkeit. Der echte gemeinsame
libSQL-Vertrag speichert die alte Quittung zusätzlich zu einem File-Read, öffnet einen
weiteren Store und prüft beide Einschränkungen bei unverändertem Zähler zwei.
Der erste neue Storage-Test vergaß den erforderlichen `begin_agent_tool_attempt`.
Die korrekte Ablehnung wurde nicht gelockert; nach Vervollständigung des Test-Lifecycles
besteht `replan-claim-key-storage-corrected.log`. Die früheren Fehlberichte bleiben erhalten.

Primär-/Repairtests prüfen Legacy-Claim→Search, Legacy-Claim→andere Claim-ID und
Strukturfehler→Legacy-Claim: jeweils genau ein Repair, höchstens ein tatsächlich
zugelassener Toolaufruf, keine dritte Modellantwort und keine privaten IDs im Hinweis.
Die gezielte Replan-Serie steht in `replan-claim-key-targeted.log`.

Eingefrorenes Binary `replan-claim-key-20260907/agent-tests.exe`, SHA-256
`1902d805aa6f0bd7343efa8ee4d17e026b17c59e3d08e6d99c944123c4120d01`.
Es enthält den finalen Produktionscode; danach wurde nur der Storage-Testaufbau korrigiert.

| Modell | Dauer | Tatsächliches Ergebnis |
| --- | --- | --- |
| Luna | 13,31 s | Done 21, Test Exit 0, Completed/verified, unabhängige Prüfung grün |
| Granite 8B | 22,98 s | Zwei Tests Exit 1, WrongDecision nach Replan-Einzelrepair, Failed 23 |
| Qwen 8k | 39,77 s | TargetAlreadyExists nach Einzelrepair, Failed 6 |

Lokale Modelle liefen nacheinander. Geschützte Dateien sind bytegleich; Luna bestätigt
zusätzlich unveränderte native Settings. Diese Coding-Fixture enthält keinen
nachgewiesenen Claim-Read: Sie prüft mögliche Nebenwirkungen, nicht die Claim-Korrektur
selbst. Die beiden lokalen Implementierungsfehler bleiben offen.

SHA-256 im selben Verzeichnis:

- `luna-live.log`: `cfd83950ebe92d2d32c79363bb7f170efbc2f17c0fbfac5748e73308a2f1d6f6`
- `granite-live.log`: `dcc259a11807f584ac6da0ca7f274d1a745031b9800f139fc6aab1fe76722fe4`
- `qwen-live.log`: `9a50a1abe45d73aae722d3847569c191dcfc4b9daf9dd602a05c066f4a8be0b6`

Finale Gates bestanden: `cargo fmt --all --check`, `cargo clippy --workspace
--all-targets --all-features --offline --locked --jobs 2 -- -D warnings` und
`cargo test --workspace --all-features --offline --locked --jobs 2 -- --test-threads=1`.
Clippy und Tests enden jeweils mit Exit 0 (`replan-claim-key-clippy.log`,
`replan-claim-key-workspace.log`), einschließlich des echten Storage-Vertrags.
Lokale Markdown-Links und `git diff --check` bestehen ebenfalls. Keine neue
Abhängigkeit, Migration, Profiländerung oder zusätzliche Reparaturrunde.

### Replan-Schema ohne anschließend verbotene Nutzerfragen

Nach ADR-0090 lieferte Granite `WrongDecision`. Die separate Codeprüfung belegt:
`prepare_replan_analysis` verbietet Nutzerfragen im Systemtext, verwendet aber das
allgemeine V5-Analyze-Schema mit `questionDecision`, den `replan_analysis::admit`
anschließend unabhängig abweist. Die konkrete damalige Modellentscheidung wurde
nicht aufgezeichnet; sie wird deshalb nicht rückwirkend als Nutzerfrage ausgegeben.

Der neue Test ist zunächst rot (`replan-analysis-schema-red.log`). Nur der aktuelle
Replan-Request beschränkt jetzt `decision` auf Progress und entfernt unbenutzte
Definitionen. Die unveränderten allgemeinen Analyze-/Design-Schemata behalten echte
Nutzerfragen. Das kanonische Replan-Schema sinkt von 2.215 auf 1.923 UTF-8-Bytes;
dies ist eine Größenmessung derselben Fixture, kein Geschwindigkeitsnachweis.
Ein leeres Ergebnis bei fehlenden Originalen kann weiterhin nicht abschließen.

Die gezielte Serie besteht: 14 Application-, drei Context-, drei Desktop-, ein Domain-,
ein echter Gemini-HTTP- und ein libSQL-Migrationstest. Der neue HTTP-Vertrag besitzt
beide Futures mit fünf Sekunden Deadline und bestätigt, dass der unzulässige Frage-Arm
auch nach Providerübersetzung fehlt. Der echte Context-Compile erhält dieselbe
Projektion, Originalfenster, Tokenabrechnung und deterministischen Digest.
Der Turnvertrag liefert absichtlich eine im allgemeinen Ask gültige Frage trotz engerem
Replan-Schema: einmalige Korrektur oder terminale Ablehnung, null Toolwirkung, kein
Rohtext im Feedback. Die bisherigen Quellen- und Leerergebnisfälle bleiben bestehen.

Der erste umfassendere Build scheiterte an einem neuen Context-Test, der direkt ein
Macro einer dort nicht vorhandenen Abhängigkeit verwendete. Nach Prüfung über die
bereits vorhandene Value-API besteht `replan-analysis-schema-final-targeted.log`.
Keine Abhängigkeit wurde ergänzt. Danach bestehen erneut Formatierung, vollständiges
Workspace-Clippy mit `-D warnings` und die gesamte Workspace-Testsuite, jeweils mit
`--all-features --offline --locked --jobs 2`, Tests seriell mit `--test-threads=1`.
Logs: `replan-analysis-schema-clippy.log`, `replan-analysis-schema-workspace.log`.
Lokale Markdown-Link- und Diff-Prüfungen bestehen ebenfalls.

Eingefrorenes Binary `replan-analysis-schema-20260907/agent-tests.exe`, SHA-256
`4d5f7ef371534236dd0585cf29e87a8fd9af981589979380e38de1c486e97d72`.

| Modell | Dauer | Tatsächliches Coding-Ergebnis |
| --- | --- | --- |
| Luna | 16,21 s | Done 23, Test Exit 0, Completed/verified, unabhängige Prüfung grün |
| Google Gemma | 14,97 s | ModelFailed(Unavailable), Failed 16 |
| Granite 8B | 27,64 s | Zwei Tests Exit 1, RepeatedReplanRead nach Einzelrepair, Failed 26 |
| Ornith 9B | 26,98 s | Test Exit 1, SameMovePath nach Einzelrepair, Failed 12 |
| GPT-OSS 20B | 21,28 s | Test Exit 1, ModelFailed(InvalidResponse), Failed 12 |
| Qwen 8k | 38,37 s | TargetAlreadyExists nach Einzelrepair, Failed 6 |

Alle lokalen Modelle liefen nacheinander. Geschützte Dateien bleiben bytegleich;
Luna bestätigt zusätzlich unveränderte native Settings. Andere terminale Pfade sind
weder Implementierungserfolg noch Ursachenbeweis für die früheren Modellfehler.

SHA-256 der Logs im selben Verzeichnis:

- `luna-live.log`: `1a25230c80d3ced8a0386f5a9ff7f628c1f176da3b8b339b835aed9647405400`
- `google-live.log`: `61f91f7e19bcf4589f8703a02988dc6c5eba4c789dc7c36a47b0b4433a68c4fe`
- `granite-live.log`: `35a6252f27478e8c4f3488fed59b9dcca6d4350d1e1eb1822599f98c5f991bd9`
- `ornith-live.log`: `ddeabd813605949b213ad631cb2143fa751083aacb1bea447ab3b5960d089042`
- `gpt-oss-live.log`: `8ccce5d9e07fa7255d79b096690d412904e1de10b39c32ca34b9671d0baf0672`
- `qwen-live.log`: `0fb766a7f216c01d9798432c168f146abc709c8dc80126b4701d6261245b2a4b`

Die vollständige Luna-Research-Matrix endet nach 159,44 s mit zwölf abgeschlossenen,
arbeitsstandbereiten Fällen ohne Nutzerhalt oder adaptive Reads. Rubrik v3 besteht
10/12; in Audit 1:1 und 1:2 fehlt weiterhin `write`. Die Sichtprüfung bestätigt in
1:2 erneut die falsche Gleichsetzung von Writer und `open`-Kontextmanager, in 1:1
die nicht durch den Callee belegte Speicherung der Aufgaben. Die Fragen und Originale
blieben unverändert. Dieser Inhaltsbefund wird nicht durch den Ablaufabschluss ersetzt.
Bericht `eval-1788799066038.jsonl`, SHA-256
`59feb56047e7673d35306257a6008cfc329feb5681339f9dd5a9879d9f513200`;
Log `replan-analysis-schema-20260907/luna-research-matrix.log`, SHA-256
`397a8c1469814acd74a5f6c098d278e09062fda073b9ba58bb8415462a692078`.

### Inhaltsfreie Operationshilfe im vorhandenen Patch-Einzelrepair

Qwen wiederholte mehrfach `TargetAlreadyExists`, Ornith `SameMovePath`. Der bisherige
Repair nannte zwar den geschlossenen Fehlercode, erläuterte aber nicht die betroffenen
Operationsregeln. Der neue Hinweis-Test ist zunächst rot (`patch-operation-repair-red.log`).
Für genau diese beiden Fehler erklärt fester Core-Text jetzt Add, Move und Update:
Nur ein beabsichtigtes In-place-Edit soll `update` mit den gelieferten Ankern verwenden;
bei Unklarheit bleibt Inspect möglich. Löschen oder erfundene Ziele sind kein Ausweg.
Der Core schreibt keine Operation oder Modellwerte um und gibt nichts zusätzlich frei.

Die vollständigen V5-Reparaturhinweise umfassen 420 beziehungsweise 406 UTF-8-Bytes,
bleiben unter der bestehenden 512-Byte-Testgrenze und passen auch innerhalb der
450-Einheiten-Sicherheitsreserve des 8k-Profils. Die Primärinstruktion und Modellprofile
bleiben unverändert. Es wurde keine zusätzliche Reparatur- oder Ausgaberunde eingeführt.

Der bestehende vollständige Turnvertrag prüft nun 36 Kombinationen: sechs Fehler
einschließlich des bereits strukturell ungültigen SameMovePath, jeweils mit wiederholtem
Fehler oder Update/Add/Move/Delete/Inspect. Wiederholte Fehler haben keine Toolwirkung,
gültige Vorschläge bleiben unmodifiziert und benötigen normale weitere Autorisierung.
Genau zwei Requests, ein Repair, unveränderte Originalnachrichten und Schema sowie
eine ungenutzte dritte Antwort werden geprüft; die Hinweise enthalten keine Fixturepfade
oder Modellprosa. Gezielte Serie: `patch-operation-repair-targeted.log`.

Formatierung, vollständiges Workspace-Clippy mit `-D warnings` und vollständige
Workspace-Tests bestehen; Rust mit `--all-features --offline --locked --jobs 2`,
Tests seriell mit `--test-threads=1`. Logs `patch-operation-repair-clippy.log` und
`patch-operation-repair-workspace.log`, jeweils Exit 0. Link- und Diff-Prüfung bestehen.

Eingefrorenes Binary `patch-operation-repair-20260907/agent-tests.exe`, SHA-256
`b4988c4977804e140023d257cd901b405e790e4f7d94851504b6957410638b00`.

| Modell / Lauf | Dauer | Tatsächliches Coding-Ergebnis |
| --- | --- | --- |
| Qwen 8k / 1 | 85,54 s | Done 34, zuletzt Test Exit 0, Completed/verified, unabhängig grün |
| Qwen 8k / 2 | 77,88 s | Done 37, zuletzt Test Exit 0, Completed/verified, unabhängig grün |
| Qwen 8k / 3 | 115,18 s | IncompleteModelOutput, Failed 21, unabhängig rot |
| Ornith 9B | 22,76 s | SameMovePath nach Einzelrepair, Failed 6, unabhängig rot |
| Luna | 13,11 s | Done 23, Test Exit 0, Completed/verified, unabhängig grün |

Lokale Modelle liefen strikt nacheinander, jede Wiederholung auf einer neuen Kopie
derselben öffentlichen Aufgabe mit denselben Schutzregeln. Geschützte Dateien bleiben
überall bytegleich; die erfolgreichen Fälle bestätigen zusätzlich unveränderte native
Settings. Das sind zwei echte lokale Umsetzungserfolge, keine allgemeine Zuverlässigkeits-
oder Geschwindigkeitsgarantie: Qwens dritter Lauf und Ornith bleiben ausdrücklich rot.
Die früheren fehlgeschlagenen Testereignisse werden auch in erfolgreichen Runs nicht gelöscht.

SHA-256 der Logs im selben Verzeichnis:

- `qwen-live.log`: `7fe4917e84dcab781ee6781b52cd83a48362d85b6101e35dd5741cfa6c70da8d`
- `qwen-live-2.log`: `71917ced8cb6cf5b1fb322706111a13b009bb87903d1cfc5870f9e840c6f8c04`
- `qwen-live-3.log`: `d8625463ae4cd6752401f6ab00cb327307ea393032a7f5fa42135c4039fdeec2`
- `ornith-live.log`: `199d8ad581b4af01d030b70f10150dfa04bd966d7b1f9766c3158136407625d0`
- `luna-live.log`: `09e26dcd8f3ff6c59daadcb74c3734b965d445fc98e0f74dc85ea2b0cafc8fec`

### Geschlossener Beendigungsgrund statt ununterscheidbarem Modellabbruch

`IncompleteModelOutput` behält jetzt den bereits normalisierten Providergrund
`OutputLimit` beziehungsweise `Other`. Keine Ausführungs-, Reparatur- oder
Budgetregel wurde geändert. Der Regressionstest ist zunächst rot
(`model-output-termination-red.log`), danach grün (`model-output-termination-targeted.log`).
Er prüft beide Gründe im Primär- und Repairrequest, auch bei vollständig aussehendem
Aktions-JSON: keine Toolwirkung, keine Recoveryanlage, keine weitere Antwort,
unveränderte Abrechnung und redigierter Journalevent. Private Querybytes bleiben
außerhalb der Fehlerdiagnose.

Formatierung, Workspace-Clippy mit `-D warnings` und die vollständige Testsuite
bestehen mit `--all-features --offline --locked --jobs 2`, Tests zusätzlich seriell
mit `--test-threads=1`. Logs: `model-output-termination-clippy.log` und
`model-output-termination-workspace.log`, Exit 0. Link- und Diff-Prüfung bestehen.

Eingefrorenes Binary `model-output-termination-20260907/agent-tests.exe`, SHA-256
`763e9410e4b16b0d42a4d51ac9b1862f821b499e4b9c3cfda24a99892f0f3e1d`.

| Modell / Lauf | Dauer | Tatsächliches Coding-Ergebnis |
| --- | --- | --- |
| Qwen 8k / 1 | 94,54 s | InvalidValue nach Einzelrepair, Failed 21, unabhängig rot |
| Qwen 8k / 2 | 72,84 s | Done 37, zuletzt Test Exit 0, Completed/verified, unabhängig grün |
| Qwen 8k / 3 | 74,83 s | Done 37, zuletzt Test Exit 0, Completed/verified, unabhängig grün |
| Luna | 23,23 s | Done 27, Test Exit 0, Completed/verified, unabhängig grün |

Die lokalen Wiederholungen liefen sequenziell. Geschützte Dateien bleiben bytegleich;
erfolgreiche Läufe bestätigen unveränderte native Settings. Der vorherige unklare
Modellabbruch wurde hier nicht reproduziert; weder OutputLimit noch Other ist damit
als dessen Ursache bewiesen. Zwei lokale Erfolge aus drei Läufen sind weiterhin
keine zuverlässige Modellabnahme und die Diagnose selbst ist kein Verhaltensfix.

SHA-256 der Logs im selben Verzeichnis:

- `qwen-live-1.log`: `d4c9a0a9df65675f302808fc0e0a276709ef93bd0ce2af5b48eabe96db8bf4d1`
- `qwen-live-2.log`: `f1f19ab28aebedabc964907fe9eff925eae7478fd0c4cc60703818b7aef536cb`
- `qwen-live-3.log`: `ba6e5f481f9c4d7c5154572f7b241f5f3284dee816f7085c0133392356bdc247`
- `luna-live.log`: `328d4a0d01bb8fe557fcf00274f3daca060a538d46a2ae38787025eac6c2748e`

### ADR-0091: Statusfreie Replan-Leseturns

Der Promptregressionstest ist zuerst rot (`replan-statusless-red.log`): Der neue
Leseturn fordert noch V4 statt V5. Prompt und unabhängiger Primär-/Repairdecoder
verwenden jetzt den bestehenden V5-Umschlag, weiterhin ausschließlich Search/Inspect.
Die expliziten historischen Decoder und der getrennte Research-V5-Analysevertrag
bleiben erhalten. Die kanonische, gleichartig beschnittene Schema-Fixture misst
3.663 Byte für V4 gegenüber 3.000 Byte für V5. Das ist eine Umfangsmessung, kein
Nachweis besserer Modellqualität oder Geschwindigkeit.

Gezielte Verträge bestehen (`replan-statusless-targeted.log` und
`replan-statusless-all-targeted.log`). Der vollständige Turntest wurde anschließend
um den explizit gültigen historischen V4-Read erweitert und prüft insgesamt 14
Kombinationen: vier außerhalb Replan gültige Nicht-Leseaktionen, Zusatznotiz,
falsche Version und historischer Read, jeweils wiederholt oder durch einen gültigen
V5-Read korrigiert. Genau ein Repair, unveränderte Requests/Schema, keine Nutzung
der dritten Antwort, keine Toolwirkung bei Ablehnung und keine übernommene Notiz
werden unabhängig geprüft. Vorhandene Duplikat-, Legacy-Claim-, Budget- und
Originalregressionen bleiben bestehen. Der echte Gemini-HTTPvertrag deckt jetzt
beide Replan-Phasen ab; der reale Contextvertrag prüft Digest und vollständige
Nachrichtenabrechnung für das aktuelle Leseschema.

Die abschließende Formatprüfung, vollständiges Workspace-Clippy mit `-D warnings`
und die vollständige Workspace-Testsuite bestehen, einschließlich der ergänzten
historischen Turnkombination. Rust-Gates mit `--all-features --offline --locked
--jobs 2`, Tests zusätzlich `--test-threads=1`. Logs: `replan-statusless-clippy.log`
und `replan-statusless-workspace.log`, Exit 0. Markdown-Link- und Diff-Prüfung bestehen.

Eingefrorenes Binary `replan-statusless-20260907/agent-tests.exe`, SHA-256
`6a1262b456e42ce80f7bd0d4d589a919cf9e1f6d5ff8aad71cb3ff6ec827ed25`.
Es enthält den geänderten Produktionscode; die letzte zusätzliche historische
Turntestkombination und die kosmetische Formatierung des HTTPtests kamen erst
danach hinzu und werden durch die abschließende Workspace-Suite geprüft.

| Modell | Dauer | Tatsächliches Coding-Ergebnis |
| --- | --- | --- |
| Luna | 14,71 s | Done 21, Test Exit 0, Completed/verified, unabhängig grün |
| Google Gemma | 11,52 s | ModelFailed(Unavailable), Failed 16, unabhängig rot |
| Qwen 8k | 114,54 s | InvalidState, Failed 43, vier Tests Exit 1, unabhängig rot |
| Granite 8B | 21,07 s | RepeatedReplanRead nach Einzelrepair, Failed 24, unabhängig rot |
| Ornith 9B | 66,09 s | IncompleteModelOutput(OutputLimit), Failed 24, unabhängig rot |
| GPT-OSS 20B | 20,68 s | ModelFailed(InvalidResponse), Failed 12, unabhängig rot |

Lokale Modelle liefen nacheinander; alle geschützten Dateien bleiben bytegleich.
Luna bestätigt außerdem unveränderte native Settings. Mehrere Testfehler allein
beweisen nicht, ob dazwischen eine Patchmutation stattfand. Orniths Ausgabelimit
ist jetzt erstmals genau klassifiziert; daraus folgt nicht rückwirkend derselbe
Grund für Qwens älteren unklassifizierten Abbruch. Die Änderung beseitigt die
unnötige Statuspflicht, aber die lokale Nutzbarkeit ist damit nicht abgenommen.

SHA-256 der Logs im selben Verzeichnis:

- `luna-live.log`: `f67300bdef5d219f1afdcaee9aca7df038c5f7785c3404d2240248b03c85130e`
- `google-live.log`: `4bc7acbeda48e19688024b83f86b94cff51a2a477ce656357c534e971352b15d`
- `qwen-live.log`: `9a5c03d9dffa082d3df7b90d608f8644271b01b56c5aefda7608071e97f8341f`
- `granite-live.log`: `f794ec1b9d187a4aed679d653499225c66f5d90bedf9e872084c35c9e776f55c`
- `ornith-live.log`: `8943c711233ebd483e58dfd288c0c15ce2acdb0e7245cb59197e280ccd6694ee`
- `gpt-oss-live.log`: `2e5bb2579d3899e497fd3f2e9cde9a7fb689fd3948aaf3e73cf63cc8443ae7e3`

### Live-Quittungen trennen tatsächliche Mutation, Test und aktiven Schritt

Test-only-Instrumentierung liest die bereits begrenzte durable Mutationshistorie
und druckt höchstens 32 inhaltsfreie Einträge plus Gesamt-/Auslassungszahl. Die
Eintragsnummer beschreibt die Speicherreihenfolge, **nicht** die Ausführungszeit.
`Process/Succeeded/Applied` belegt Prozessanwendung, nicht Test-Erfolg. Der separate
Prozessbefund enthält weiterhin Exitcode und Erfolg. Tatsächliche Source- und
Snapshotänderungen werden als unabhängige Bits, Ledger-Schritte mit Aktivstatus
ausgegeben. Die Ablehnungsdiagnose nennt nun auch den Repairverbrauch dieses Turns.
Ein terminaler Failed-Lauf wird nicht mehr irreführend als fehlende Freigabe eines
noch nicht terminalen Laufs bezeichnet. Produktionsverhalten, Modellrequests,
Read-/Repair-/Zeitbudgets und Sicherheitsgrenzen wurden nicht verändert.

Die zwei bestehenden Scope-Tests bestehen (`live-coding-receipts-targeted.log`);
die Lives bleiben ausdrücklich opt-in. Formatprüfung, vollständiges Workspace-
Clippy mit `-D warnings` und vollständige Workspace-Tests bestehen mit
`--all-features --offline --locked --jobs 2`, Tests zusätzlich `--test-threads=1`.
Logs: `live-coding-receipts-clippy.log`, `live-coding-receipts-workspace.log`, Exit 0.
Link- und Diff-Prüfung bestehen. Keine neue Produktionslogik oder Adaptergrenze.

Eingefrorenes Binary `live-coding-receipts-20260907/agent-tests.exe`, SHA-256
`99d8358c96eeb0616a29fb6375f3968de8c18ce480e1230b55f6aede7c105785`.

| Modell / Lauf | Dauer | Unabhängig beobachteter Befund |
| --- | --- | --- |
| Luna | 15,56 s | Done 25; Patch und Process angewendet, Source/Snapshot geändert, Test Exit 0, aktiver Schritt verifiziert, unabhängig grün |
| Ornith | 20,73 s | Failed 6, aktiver Schritt Blocked, keine Mutationsquittung, Source/Snapshot unverändert, unabhängig rot |
| Qwen / 1 | 41,31 s | SameMovePath, Repair One, Failed 6, keine Mutationsquittung, Source/Snapshot unverändert, unabhängig rot |
| Qwen / 2 | 88,38 s | SameMovePath, Repair One, Failed 21, genau ein angewendeter Process mit Test Exit 1, kein Patch, Source/Snapshot unverändert, unabhängig rot |

Alle lokalen Läufe waren sequenziell, geschützte Dateien bleiben bytegleich;
Luna bestätigt unveränderte native Settings. Bei Qwen / 2 ist der alte Blocked-
Schritt inaktiv und der neue InProgress-Schritt aktiv. Der fehlgeschlagene Test
darf trotz angewendetem Prozess nicht als Erfolg erscheinen. Orniths früheres
OutputLimit wurde in diesem Nachtest nicht reproduziert; dessen Primär-/Repairphase
bleibt offen. Die Instrumentierung selbst verbessert keine Modellentscheidung.

SHA-256 der Logs im selben Verzeichnis:

- `luna-live.log`: `f0f5b3bbcdaa42a882008d05076b61b28d9ff2d6f4c3620e0d2a4ded601ab31e`
- `ornith-live.log`: `57b712535f2ac9fef088f140d9abad81d177099bef37fcfefc3ff90262f6a570`
- `qwen-live.log`: `cb6d011040d3f55a1a2f6c0f406efc7795ce596b7e830cdb4e691241f1f4440b`
- `qwen-live-2.log`: `a2b011d7c3aadf1149a9657fbd5b63aa0f0d22ec370c710af8dadc1b53f5a30a`

### Kontrollierter nicht ausführender Vergleich der Agent-Aktionsform

Der neue opt-in Wirevergleich verwendet ausschließlich öffentliche Konstanten:
eine vorhandene zweizeilige `increment.py`, tatsächlicher BLAKE3-Hash, feste
gültige Identitäten und das bekannte Sollverhalten. Derselbe 832-Byte-Auftrag
und dieselben beiden Nachrichten werden für beide Formen verwendet. Normale
Agent-Kontextpakete und wiederholtes Prompt-Schema werden hier bewusst nicht
simuliert. Der bestehende volle V5-Aktionsumfang einschließlich aller vier
Patchoperationen bleibt in beiden Schemaformen erhalten. Identitäten werden in
beiden gleich eingeschränkt; weder Update noch überhaupt ein Patch wird erzwungen.

Die experimentelle Form verlagert nur Aktions- und Operationsparameter unter
`parameters`, nach dem gemeinsamen `kind`. Der Produktionsvertrag bleibt unverändert.
Eine ausschließlich testlokale strikte Entpackung weist gemischte/zusätzliche
Umschläge zurück; anschließend prüft der bestehende vollständige V5-Decoder.
Kein normalisiertes Dokument erreicht einen Executor, Freigabepfad oder Prozess.
`current_in_place_update` verlangt Update, genau eine Operation, korrekten Pfad,
Hash und alle aktuellen Identitäten; es beweist ausdrücklich nicht die Semantik
des neuen Inhalts oder erfolgreiche Implementierung. Rohantworten werden nicht geloggt.

Die Sequenz ist AB/BA/AB mit sechs Vergleichsaufrufen nach der vorhandenen
Fähigkeitsprüfung, ohne Retry/Repair. Ein Modell läuft höchstens 300 s, jeder
Aufruf hat 30 s Providertimeout und 35 s äußere besitzende Deadline; das maximale
gesammelte Ergebnis beträgt 16 KiB. Lokale Modelle liefen nacheinander. Ein
bestandener Diagnosetest bedeutet vollständige Beobachtung, nicht Modell- oder
Task-Erfolg. Der finale Logmarker stellt diese Trennung ausdrücklich klar.

Die Schema-/Entpackungs-/Redaktionsprüfungen einschließlich aller Patchoperationen
bestehen (`agent-action-shape-final-targeted.log`). Gegenüber dem flachen Schema
mit 6.393 Bytes benötigt die verschachtelte Form 7.876 Bytes. Keines der Profile
wurde dafür vergrößert. Der initiale Stand besteht vollständiges Clippy und die
Workspace-Suite (`agent-action-shape-clippy.log`, `agent-action-shape-workspace.log`).

Eingefrorenes Binary `agent-action-shape-20260907/agent-tests.exe`, SHA-256
`bb1825838fb1b9d334b90d940d644cc1f268225824bbd6539cec6fc9f5780f43`.

| Modell | Flach | Kind zuerst | Beobachtung |
| --- | --- | --- | --- |
| Luna | 3/3 aktuelle Updates | 3/3 aktuelle Updates | Reguläres Stop, strikter V5-Decode nach jeweiliger Entpackung |
| Ornith | 3/3 aktuelle Updates | 3/3 aktuelle Updates | Reguläres Stop, gleiche beschränkte Aktionsmetrik |
| Qwen | 3/3 Provider-/Ausgabegrenzfehler | 3/3 Provider-/Ausgabegrenzfehler | Kein auswertbares vollständiges Dokument |

Gesamtdauern der Testprozesse einschließlich Probe: Luna 20,70 s, Ornith 44,65 s,
Qwen 20,02 s. Die drei Beobachtungen je Form erlauben keine allgemeine
Zuverlässigkeitsgarantie. Dieser Vergleich liefert insbesondere **keinen** Nachweis,
dass ein größerer Kind-zuerst-Vertrag die Fehler des echten Agenten behebt. Deshalb
wird weder ein produktives AgentAction V6 noch eine großzügigere Zulassung eingeführt.

SHA-256 der initialen Logs im selben Verzeichnis:

- `luna-wire.log`: `513171df391ebd35e724571c89f3b2aeb7262cf2a885bd439e36c30093edfd3b`
- `ornith-wire.log`: `11bdf1efa86a8cb79f6bac14a03bfe02642c3efc1bd33673a428c638dcd90143`
- `qwen-wire.log`: `5866f1895159e7a01fb0eb6d63fe81ba11431a5ec1c49abe18488210a52a1d83`

Weil die initiale Probe den Qwen-Fehler zu grob zusammenfasste, wurde die Diagnose
um geschlossene Provider-/Ausgabebound-Codes ergänzt. Der gezielte finale Test
besteht (`agent-action-shape-classified-targeted.log`). Derselbe Qwen-Vergleich
bestätigt in allen sechs Aufrufen konkret `provider_InvalidResponse`, ohne
verwertbares Dokument; kein Timeout oder Kontextlimit ist damit bewiesen.
Das operative Ollama-Kontextminimum beträgt 16.384, begrenzt durch das Profil:
Qwens 8.192 werden in diesem Pfad also nicht auf 4.096 verkleinert.

Finales Binary `agent-action-shape-classified-20260907/agent-tests.exe`, SHA-256
`375ddd8079d94fe09065498e31384d0a4c7ad6154efaa266f9e17353dda61859`;
Qwen-Nachtest 25,29 s, Log `qwen-wire.log` im selben Verzeichnis, SHA-256
`26694ad8ce129822775dfc5dbc616d3384419083274b1251fd37ceeb3c63c6eb`.

Auch der abschließend klassifizierte Stand besteht Formatierung, vollständiges
Workspace-Clippy mit `-D warnings` und die gesamte Workspace-Testsuite, jeweils
`--all-features --offline --locked --jobs 2`, Tests zusätzlich `--test-threads=1`.
Logs `agent-action-shape-classified-clippy.log` und
`agent-action-shape-classified-workspace.log`, Exit 0. Link- und Diff-Prüfung bestehen.

Nächster Untersuchungsgegenstand ist die Quelle des InvalidResponse und der
Unterschied zum produktiven Kontext. `render_lens_entry` liefert für File/Span
lediglich Pfad, Hash und Range, `render_symbol` Signaturen: solche Metadaten
ersetzen keine Originalbytes. Die Toy-Fixture liefert dagegen den ganzen
Funktionskörper. Dieser Unterschied ist belegt, aber allein noch kein kausaler
Nachweis für die im echten Agenten beobachtete falsche Patchwahl.
