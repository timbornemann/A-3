# Plan 10: Vergleich der Entwurfsbasis

Stand: 2026-09-08. Ausgangspunkt: `27953a3`.

## Ziel und Grenze

[ADR-0105](../adrs/0105-originalgebundene-entwurfsbasis-im-vergleich.md) prüft,
ob die erneute Übergabe freier Bestandsinterpretationen an nachfolgende Entwürfe
Fehler verstärkt. `interpretations` bleibt der Produktstandard. Die native
Variante `originals` ersetzt nur diese Prosa durch die vollständig gelieferten,
aktuellen Originalbereiche ihrer Belege. Vollständiger Originalauftrag, aktive
Pflicht, vollständige Designentscheidungen und der dauerhafte Prüfstand bleiben.
Keine selbstbewerteten Fakten, zusätzlichen Rechte, Reads oder Repairs entstehen.

## Offline-Befund und Korrektur

Die erste Umsetzung nutzte beim Phasenwechsel die gewöhnliche Quellenauswahl.
Der echte 8k-Mehrdateitest stoppte schon vor Q2, weil deren Auswahl nicht alle
vorher belegten Bereiche erneut lieferte. Der Vergleich materialisiert deshalb
exakt diese Bereiche aus revisionsgleichen, tatsächlich gelesenen Cacheausschnitten.
Ein Bereich muss vollständig in einen vorhandenen Ausschnitt passen; Lücken
werden weder aus Indextext noch Modellprosa aufgefüllt. Die endgültige
Lieferprüfung kontrolliert Byte- und Positionsbereich im tatsächlichen Paket.

Der erste Qwen-Livefall zeigte anschließend einen zweiten Packfehler: Die Wahl
zwischen vollständigem und kompaktem Arbeitsstand reservierte nur die bisherige
256-Byte-Rahmung, nicht die nun verpflichtenden Originalbereiche. Eine kompakte
Darstellung wurde dadurch zu spät erwogen. Die Korrektur reserviert die
Originalbytes mit konservativen Headerkosten bereits bei dieser Auswahl.
Sie hebt weder das 8192/2048-Profil noch die äußeren Grenzen an.

Vier neue Regressionen prüfen den geschlossenen Selektor, vollständige aktuelle
Lieferung mit UTF-8, abweichende Revisionen, fehlenden Lesebeleg, gekürzten Cache,
atomar verworfene Überfüllung, getrennte Analyseidentität, unveränderten dauerhaften
Stand und die rechtzeitige Kompaktwahl. Die echte Git-/Index-/Reader-/libSQL-
Fixture prüft Plan und Agent-Vorbereitung mit tatsächlichem konservativem
8192/2048-Profil: passende Originale erreichen alle drei Aufrufe ohne Zusatzread;
bei zu langem verbindlichem Entwurf bleibt Q3 vor dem ersten Versuch offen.
Das Öffnen eines neuen Storage-Adapters lädt denselben vollständigen Entwurf und
dieselben offenen Pflichten. Das ist kein Prozesscrash-Test. Dateien bleiben
bytegleich; Ask und die erste Bestandsphase werden nicht umgestellt.

## Protokoll

Alle Livefälle verwenden die öffentliche `research-eval-v1`-Fixture, dieselbe
Planaufgabe `3:0` (`python main.py import-csv <filepath>`, UTF-8, `csv.DictReader`,
`project_id,title`, jede gültige Zeile über `Manager.add_task`, Fehler und Tests,
noch keine Implementierung), `joint`, `format-only` und Rubrik V3. Frische
Capability-Proben, Temperatur 0 und Parallelismus 1 bleiben. Qwen: 8192/2048;
Luna: 16384/2048; übrige Modelle: 16384/4096. Lokale Modelle laufen ausschließlich
nacheinander, Cloudfälle in einer getrennten seriellen Reihe. Gespeicherte Profile
und Credential-Slots werden nicht geändert.

Der erste eingefrorene Teststand liegt in
`target/reports/research-design-basis-20260908/research-original-basis-tests.exe`,
SHA-256 `b012d26e61f55ffce180ba4a9b81fda885031e8a8b5b09ac4d00ed79f02f4d54`.
Beide Varianten laufen aus demselben Binary. Granite und Ornith jeweils A/B/B/A;
Qwen und lokales Gemma A/B; Cloudmodelle jeweils B/A. Das ist ein begrenzter
gegenläufiger Vergleich, keine randomisierte statistische Allgemeinfreigabe.

## Inhaltliche Gegenprüfung

- Granite: A korrigiert den falschen Namen in Q2 mit einem Repair und schließt
  zweimal ab; der Testtext bleibt sehr allgemein. B erfindet `--command` und
  `--command import-csv`. Q3 überschreitet mit 6521 UTF-8-Textbytes die bestehende
  Grenze; der reparierte kurze Text erfüllt nicht mehr alle Zulassungsbedingungen.
  Beide B-Läufe bleiben offen. Der Decoder meldet die Textüberschreitung derzeit
  nur als allgemeinen ungültigen Wert; die gezieltere Fehlerklassifikation ist
  ein separat zu prüfender Folgeschritt, keine bereits behobene Ursache.
- Ornith: A schließt zweimal ab, B zweimal nicht. In B endet der erste Q3-Aufruf
  am Ausgabelimit, der einmalige Repair bleibt unzulässig. Der zugelassene Entwurf
  bezeichnet alle Fehler als `SystemExit(1)`, fordert bei unbekanntem Projekt aber
  gleichzeitig Überspringen/Fortsetzen. Originale allein verhindern diese
  widersprüchliche Regel nicht.
- Luna und Flash schließen beide Varianten ab, behaupten aber weiterhin
  Speicherung/Persistenz, die der Fixture-Code nicht ausführt. Die Storage-
  Implementierungen geben lediglich Tupel zurück. Flash B enthält zusätzlich
  Prozent-Escapes wie `Testf%4lle` im Testtext. Ein bestandener Wortcheck ist daher
  keine inhaltliche Abnahme.
- Wichtige Vergleichsgrenze: Luna, Flash und Ornith zitieren in Q1 nur E1/E2,
  obwohl E3 ebenfalls geliefert wurde. B übergibt danach nur die zitierten
  Bereiche; das nicht zitierte Storage-Original fällt weg. Der Vergleich ändert
  somit nicht nur Prosa, sondern teilweise auch Quellenversorgung. Daraus lässt
  sich kein isolierter kausaler Effekt der Interpretationstexte ableiten.
- Google Gemma scheitert in beiden Varianten bereits vor der Entwurfsübergabe.
  Dieser Fehler kann durch die neue Designbasis nicht behoben werden.

Die Vergleichsvariante wird nicht zum Produktstandard. Ursprüngliche
Interpretationen bleiben sichtbar, aber unverifiziert; es gibt keine allgemeine
semantische Freigabe oder Freigabe realer mutierender Modellläufe aus diesen Tests.

## Messergebnisse und Abschlussnachweise

Der erste Stand umfasst 18 Fälle: A schließt 8/9 ab, B 3/9. Formaler Abschluss
und unveränderte Wortprüfung stimmen in diesen Fällen überein, nicht aber mit
semantischer Abnahme.

| Modell | A: bisherige Interpretation | B: Originalbereiche (erster Stand) |
| --- | --- | --- |
| gpt-5.6-luna | 1/1 Abschluss, 3 Aufrufe | 1/1 Abschluss, 3 Aufrufe |
| gemini-3.8-flash | 1/1 Abschluss, 3 Aufrufe | 1/1 Abschluss, 3 Aufrufe |
| gemma-4-26b-a4b-it | 0/1, 2 Aufrufe, vor Design | 0/1, 2 Aufrufe, vor Design |
| granite4.2:8b | 2/2 Abschlüsse, je 4 Aufrufe | 0/2, je 4 Aufrufe |
| ornith-1.5:9b | 2/2 Abschlüsse, je 3 Aufrufe | 0/2, je 4 Aufrufe |
| qwen38-8k:latest | 1/1 Abschluss, 3 Aufrufe | 0/1, 1 Aufruf, vor Q2 |
| gemma4:12b | 1/1 Abschluss, 3 Aufrufe | 1/1 Abschluss, 3 Aufrufe |

Der korrigierte Stand ist separat eingefroren als
`target/reports/research-design-basis-20260908/research-original-basis-v2-tests.exe`,
SHA-256 `e4c06e4e158040b7f2518adc10702e683368a8a7b2405b1809edbe41fb52342e`.
Qwen wurde damit gegenläufig B/A/A/B nachgeprüft. B erreicht jetzt in beiden Fällen
einen zugelassenen Q2-Entwurf mit 1126 Textbytes (zwei Aufrufe), stoppt aber vor Q3
am konservativen Pflichtpaketlimit; A schließt beide Male mit drei Aufrufen ab.
Die vollständigen Entscheidungen und Originalbereiche werden nicht abgeschnitten,
um diesen Vergleich erfolgreich erscheinen zu lassen. Die Korrektur beseitigt
die vorzeitige Voll-/Kompaktentscheidung, nicht sämtliche Kontextgrenzen.

Insgesamt: **22 Fälle, 13 formale Abschlüsse/Worttreffer, 66 Modellaufrufe,
344427 kumulative Kontext-UTF-8-Bytes, 16 adaptive Reads, 0 identische Readwiederholungen**.
A: 10/11 Abschlüsse; B einschließlich korrigiertem Qwen-Nachtest: 3/11.
Bytes sind keine Tokens und keine Größe eines einzelnen Modellfensters.
Die Matrix zählt nur die aufgezeichneten Rechercheaufrufe, nicht Capability-Proben.
Zeiten unten sind native `elapsed_ms`, nicht die komplette Shelllaufzeit.
Parallele Cloud-/Offlinearbeit beeinflusst Laufzeiten; daraus folgt kein
allgemeiner Geschwindigkeitsnachweis.

Der native Settingskatalog hat vor und nach allen Livefällen unverändert SHA-256
`aecc7ef93abe29daa97747f2216e87f49aef99ce9490942fc11ee5c65c09ab32`.
Für die erste Bestandsphase haben die erfolgreich dekodierten Nicht-Qwen-Fälle
denselben Eingabedigest `caf4bea88dc6373eb523124493acd46347c00430aa782a1ab68deebc26692f69`;
Qwen `cfdacfea5a728cc6e7b6b0271c5430dd2809d3f8904ad0d686e73c287ae4dd36`.
Die Google-Gemma-Transportfehler liefern dort keinen solchen Dekodierdigest;
diese fehlende Beobachtung wird nicht ergänzt.

### Exakte Rohberichtzuordnung

Alle Dateien liegen in `target/research-eval/`. Stand 1 bezeichnet den ersten
eingefrorenen Vergleich, Stand 2 den korrigierten Qwen-Nachtest.

| Stand | Bericht | Modell | Basis | Abschluss | Aufrufe | Kontextbytes | ms | SHA-256 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | eval-1788877078202.jsonl | granite4.2:8b | A | ja | 4 | 25340 | 17885 | 02353443202919428af95dfa7739e55410745d4e2bbe1e2904689937ccd5c048 |
| 1 | eval-1788877079565.jsonl | gpt-5.6-luna | B | ja | 3 | 15030 | 30039 | 6bf84fadf932100ceb0a10994859cabd5584b6a5e04308804e014039a10ff5d8 |
| 1 | eval-1788877100118.jsonl | granite4.2:8b | B | nein | 4 | 22677 | 26998 | 3ba0425449dc67e41c5e485472787a8159682fd7824d71ddc20b50cec6943aa4 |
| 1 | eval-1788877112839.jsonl | gpt-5.6-luna | A | ja | 3 | 20850 | 35382 | a598cb73b965b265d02ed42e5ed1b3a6c4b32ca53a40bc4f4211ac074403273f |
| 1 | eval-1788877130768.jsonl | granite4.2:8b | B | nein | 4 | 22677 | 27109 | b169312995f3384a85d59b0bc101a4e0d9ba572df6822652847e729c4e66c7f3 |
| 1 | eval-1788877152189.jsonl | gemini-3.8-flash | B | ja | 3 | 15077 | 18397 | c80fa26466720ac7457892843ccb6dab9bf8946cb2b3252feef5fd3c9fe9c9c4 |
| 1 | eval-1788877161839.jsonl | granite4.2:8b | A | ja | 4 | 25340 | 17324 | cb6e9c287adeaa8537f188d0f594e554c16e89c344eb5c1b5f80c7211b92a756 |
| 1 | eval-1788877172622.jsonl | gemini-3.8-flash | A | ja | 3 | 18226 | 20031 | 4fc4a1924dd3229c61cefc75d6468c2febdaa809c8264f6e2cd6b59800695a4f |
| 1 | eval-1788877186653.jsonl | ornith-1.5:9b | A | ja | 3 | 20300 | 22182 | 0a57cae8a4a6fb5c7cea86a1c09c9574ca37c3727ec28bb3484dfa52f6e2f238 |
| 1 | eval-1788877194241.jsonl | gemma-4-26b-a4b-it | B | nein | 2 | 9441 | 174947 | ccf3d2376e6a59860000509b4ab0654df902928bebce3985b66afb6d60eb6d8f |
| 1 | eval-1788877213610.jsonl | ornith-1.5:9b | B | nein | 4 | 21209 | 52938 | 5ba133fcf1c0fd37c77bbb1a2aa129735f7ef939a5159f4d611bc109b1cec6e2 |
| 1 | eval-1788877273980.jsonl | ornith-1.5:9b | B | nein | 4 | 21209 | 56970 | 9dd84a414f442632e36649974846b2aa8dbe93fe2b597a26869012409078652f |
| 1 | eval-1788877340255.jsonl | ornith-1.5:9b | A | ja | 3 | 20300 | 22440 | 26be02e14e617a865f111dd36c58465a9a10689fc8d90820407ac440fd3d0078 |
| 1 | eval-1788877374640.jsonl | qwen38-8k:latest | A | ja | 3 | 10269 | 26160 | faa8f5ec5a23a55da7729d28c669a22d99ad6ad2ad1e39b5a812ee2f2fa65c36 |
| 1 | eval-1788877376727.jsonl | gemma-4-26b-a4b-it | A | nein | 2 | 9441 | 175024 | cf3e32972ee74145319d6d1e1a31416e70254f4b4012ac6e4cc364cdee6ff1b2 |
| 1 | eval-1788877407529.jsonl | qwen38-8k:latest | B | nein | 1 | 2698 | 13359 | 7c6648f88b7fd03f62787f12c17fae8a0fefd8cb1a015290a371e929b443752a |
| 1 | eval-1788877429259.jsonl | gemma4:12b | A | ja | 3 | 18051 | 27646 | 8b08cb8128a5b18f1b5be97c800d0829bf8855778f3cc732180e50367be06086 |
| 1 | eval-1788877462528.jsonl | gemma4:12b | B | ja | 3 | 14296 | 25376 | 1251406e80fcbf667cc02726050331379a6d2853a3f4dc164fe6628482be1b2c |
| 2 | eval-1788877779212.jsonl | qwen38-8k:latest | B | nein | 2 | 5729 | 20779 | 76df7ae40d991df5ef7c270d6060c82e222f48374e91b457786daeeeb6c36049 |
| 2 | eval-1788877807372.jsonl | qwen38-8k:latest | A | ja | 3 | 10269 | 25880 | 513dc6cfb560fd34bc3298573719fc932040a033339e44b72053be03346f7978 |
| 2 | eval-1788877844652.jsonl | qwen38-8k:latest | A | ja | 3 | 10269 | 26364 | 83d6542b0003d08f2bef0108ebf78fa86ab3290f5498620a4b93a320279cecd0 |
| 2 | eval-1788877877523.jsonl | qwen38-8k:latest | B | nein | 2 | 5729 | 20155 | 207350734f3350ad1bc35f921565b54d76730897dfbfc95bee43e0bc36f03690 |

### Finale Gates

- `cargo fmt --all -- --check`: bestanden.
- `cargo test -p a3-desktop research_original_design_basis --offline --locked`:
  alle drei Materialisierungs-/Mehrmodus-/Wiederöffnungstests bestanden.
- `cargo test -p a3-desktop research_design_basis_selection --offline --locked`:
  geschlossener Selektor bestanden.
- `cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings`:
  bestanden, `target/reports/research-design-basis-final-clippy.log`.
- `cargo test --workspace --all-features --offline --locked`: **1305 bestanden,
  19 ignoriert, 0 fehlgeschlagen, 84 Suites**;
  `target/reports/research-design-basis-final-workspace.log`.
  Auch der frühere Zwischenstand ohne `--all-features` bestand 1305/19.
- `pnpm check:links` und `git diff --check`: bestanden. Die vorhandene
  Node-Enginewarnung (25.6.1 statt 24.14.0) bleibt; kein Frontendcode geändert.

Das Workspace-Gate umfasst auch die unveränderten Offline-Verträge für Ask,
Replan, mutierende Agenten, Verifikation, Cancellation und Freshness. Es ersetzt
keinen freigegebenen neuen Live-Codinglauf. Alle gestarteten Live- und Gateprozesse
sind terminal. Die übergeordnete semantische und mutierende Modellabnahme bleibt offen.
