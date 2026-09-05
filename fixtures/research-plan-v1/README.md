# Plan-Recherche: Diagnose und Regression

Stand: 2026-09-06. Implementierungsbasis: `c604618`. Zugeordnet zu
[U13](../../docs/plans/06-DESKTOP_PRODUCT.md), innerhalb der angenommenen
ADR-0038, ADR-0042 und ADR-0046. Keine Budget-, Berechtigungs-, Index- oder
Persistenzschema-Erweiterung.

## Befund aus dem Nutzerlauf

Der bereitgestellte Verlauf zur Planung von `python main.py import-csv <filepath.csv>`
enthält sechs Rechercheabschnitte: den ersten Auftrag und fünf manuelle Fortsetzungen.
Es erschienen mehrfach Markdown-Zwischenstände mit `PLAN:`, aber kein abschließend
prüfbarer Plan. Die Quellenzahlen je Abschnitt lagen bei 70, 70, 40, 28, 39 und 61.
Das ist nicht die Zahl der tatsächlich gleichzeitig an das Modell gelieferten Ausschnitte.

Die sichtbaren Diagnosen belegen keine Erschöpfung der 48 Leseaktionen: etwa
`model=20/24; reads=13/48; repairs=1/6` im ersten und
`model=19/24; reads=8/48; repairs=0/6` im letzten Abschnitt.
Die UI-Bezeichnungen „Recherche-Runde“ umfassen auch Reparatur-/Diagnoseereignisse;
sie sind kein verlässlicher Zähler tatsächlicher Modellaufrufe.

Wiederholt als fehlend bezeichnet wurden zuvor bereits gelesene Dateien und sogar
zuvor korrekt benannte Schnittstellen (`TaskFlowManager.add_task`, CLI-Parser/Dispatch,
Task-Felder). Mehrfach wurden dieselben drei bis vier Dateien ab Zeile eins angefordert.
Zudem wurden CSV-Spalten und Importtests als fehlende Evidence behandelt, obwohl der
Import erst neu entworfen werden sollte. Die Diagnosecodes `value` und `shape` kamen
vor, erklären aber nicht die zahlreichen übrigen Runden ohne Repairverbrauch.

Der Verlauf enthält keine vollständigen Provideranfragen oder rohen JSON-Fehlausgaben.
Eine genaue Zuordnung jeder Modellentscheidung zu einem Parserfehler ist deshalb
nicht möglich. Der verkürzte Task-Lens-Suchtext in der Anzeige beweist für sich allein
keinen gekürzten Modellauftrag. Die folgenden Ursachen sind separat im Code und mit
Regressionen nachgewiesen; sie erklären das beobachtete Muster, ersetzen aber kein
Live-Replay mit demselben Modell.

## Ursachen und Korrektur

| Ursache im bisherigen Code | Korrektur |
| --- | --- |
| Ein einzelner Fokus wurde bei Batch-Reihenfolge, Cachefilter und Symbolhinweis wiederholt überschrieben. | Mehrere Dateifokusse bleiben gemeinsam erhalten; Symbolhinweise präzisieren nur ausgewählte Dateien. |
| Der erste große Quelltext verbrauchte fast das gesamte Fenster; 64 reservierte Bytes je Nachfolger reichten oft nicht einmal für Header und Kürzungshinweis. | Begrenzte gewichtete Verteilung der vollständigen Kosten; kurze vollständige Quellen geben freie Anteile zurück. |
| Wiederholte Seitenanfänge konnten nur innerhalb derselben langen Zeile fortsetzen. | Revisionsgebundene Fortsetzung auch über bereits gelieferte Zeilen; präzise innere Zeilen und `inspectSource` bleiben erneut betrachtbar. |
| Der Auftrag wurde pauschal auf ein Zwölftel des Fensters gekürzt. | Der vollständige Auftrag bleibt Pflichtinhalt; ein unzureichendes Fenster wird vor Modellaufrufen ausdrücklich gemeldet. |
| Die letzte, oft lange offene Frage stand vor den Befunden und verdrängte deren Text und Verweise. | Kompakte öffentliche Befunde mit ungeteilten Source-Referenzen; die offene Frage bleibt separat im Checkpoint/aktuellen Feedback. |
| Der Forschungsauftrag unterschied vorhandene Code-Fakten nicht ausreichend von neu zu treffenden Planungsentscheidungen. | Planreife statt Patchreife; sichere neue Verträge und Tests dürfen explizite Vorschläge/Annahmen sein. Bestehende APIs bleiben belegpflichtig. |
| Ein JSON-gültiger, aber nicht kompilierbarer Plan wurde erst bei Veröffentlichung als Nutzerfrage klassifiziert. | Planform, Quellen und vorhandener Work-Plan-Compiler werden schon an der Decision-Grenze geprüft; ein Formfehler nutzt den Einzelrepair. |
| Ein struktureller Repair konnte später noch eine zusätzliche Ziel-Zitatkorrektur erhalten. | Die bereits verbrauchte Einzelrepair-Befugnis bleibt bis zur Attribution erhalten. Kein zweiter Repair desselben Dokuments. |
| Auch explizite Nutzerfragen konnten zur Ziel-Zitatkorrektur gezwungen werden. | Echte `QUESTION:`-Antworten bleiben ohne erzwungene Zitate möglich. |
| Unvollständige Antworten ohne nächste Aktion zählten nicht zuverlässig zur Stagnation. | Neue Reads und tatsächliche Kontextabdeckung zählen einheitlich; zwei echte Nullrunden aktivieren die bestehende begrenzte Recovery. |

Die letzten beiden Punkte erhöhen weder die Autonomie bei Mutationen noch die
Macht von Modellbehauptungen. Der Core stuft `incomplete` niemals eigenständig
auf `sufficient` hoch. Planfreigabe und Agent-Ausführung bleiben getrennt.

## Reproduzierbarer Vergleich

[c604618_context.rs](c604618_context.rs) friert den alten Source-Packer nur für
Tests ein. Einzige strukturelle Anpassung: Der frühere einzelne zuletzt gesetzte
Fokus wird aus dem letzten Eintrag der neuen Fokusliste gelesen.
Der Test `plan_research_keeps_all_requested_interfaces_and_the_complete_goal`
erzeugt vier synthetische Dateien mit je einem späteren API-Anker und langem
Begleittext. Beide Packer erhalten identische gecachte Quellen und denselben
expliziten Vierdateibatch. Der alte Packer erhält sogar das gesamte Fenster für
Source; der neue muss darin zusätzlich Auftrag und Metadaten unterbringen.

| Gesamtfenster | Vorher sichtbare APIs | Nachher sichtbare APIs | Neues vollständiges Paket |
| --- | ---: | ---: | ---: |
| 2.048 Bytes | 1/4 | 4/4 | 1.772 Bytes |
| 4.096 Bytes | 2/4 | 4/4 | 3.800 Bytes |
| 8.192 Bytes | 3/4 | 4/4 | 7.912 Bytes |

Der Vergleich benötigt keine zusätzlichen adaptiven Reads. Er misst Kontextzugang,
nicht Laufzeit oder die Erfolgsquote eines echten Sprachmodells. Die ältere
[Storage-Fixture](../research-storage-v1/README.md) liefert weiterhin alle drei
relevanten Dateien vollständig innerhalb von 4.096 Bytes.

## Integrierter Planvertrag

`agent_research_plan_tests.rs` verwendet echte lokale Testrepositories, Python-Parser,
Fast Index, Safe Reader, Recherchecontroller, Scheduler und libSQL. Der Modellstub
hat keine Evidenzhistorie: Er antwortet erst, wenn alle vier Schnittstellen tatsächlich
im selben aktuellen Paket stehen. Nur ein Aufrufzähler und die Fehlerart bleiben im
Stub erhalten; keine `seen`-Map liefert unsichtbares Modellwissen.

Bei 2 KiB/Standard und 4 KiB/Gründlich benötigt dieser Vertrag jeweils drei
Modellaufrufe: Read-Anforderung, formal unvollständiger Plan, gültiger Einzelrepair.
Die produktive Plan-Zustandsauswahl und der echte atomare Research-Commit speichern
jeweils einen `Plan`-Eintrag mit Revision eins und `AwaitingPlanReview`. Es entsteht
kein Work Item und keine Ausführung. Der Test ersetzt weder die Tauri-UI noch den
realen Provider durch eine behauptete Live-Ende-zu-Ende-Abnahme.

Zusätzliche Fälle verlangen:

- echte Rückfrage ohne Zitatpflicht und ohne Planrevision;
- benannten Stagnationsabschluss vor Ausschöpfen sämtlicher Modellaufrufe;
- klares Context-Limit mit null Modellaufrufen;
- keinen zweiten Repair nach JSON- beziehungsweise Planformkorrektur;
- Weiterbestehen der negativen Verträge für Live-Edit, Cancellation, sichere Pfade,
  Trunkierung, geschlossene Reads und Profilbudgets;
- unveränderte Ask-, Plan-, Agent-Vorbereitungs- und Diagrammregressionen bei 1–8 KiB.

## Prüfungen und Grenzen

Reproduzierbare Befehle aus dem Repository-Root, ohne Installation/Providerzugriff:

```powershell
cargo fmt --all --check
cargo test --offline --locked -p a3-desktop --lib research_ -- --test-threads=1
cargo test --offline --locked -p a3-desktop --lib agent_session_manager -- --test-threads=1
cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
cargo test --workspace --all-features --release --offline --locked
pnpm check:links
git diff --check
```

Unter Windows benötigt das Gesamtgate einen frischen `TEMP`-/`TMP`-Ordner ohne Caret
für bestehende Junction-Fixtures. Release vermeidet den Dateilock einer laufenden
Debug-Nutzerinstanz. `RUST_TEST_NOCAPTURE=1` macht bei gezielter Diagnose auch die
Ausgabe der vorhandenen isolierten libSQL-Testprozesse sichtbar.

Die gezielte Recherche-Suite besteht mit 41 Tests, die umfassendere Session-Suite mit
58 Tests. Clippy mit Warnungen als Fehler, Rust-Formatierung, Linkprüfung (82 Dateien,
323 lokale Links) und `git diff --check` sind grün. Im ersten Gesamtgate erkannte die
vorhandene 320-Byte-Regression eine zu knappe Zielmetadatenreserve; die Korrektur gibt
ungenutztes Notizbudget frei und die unveränderte Assertion besteht wieder.
Das vollständige Release-Workspacegate ist auch nach der letzten Kontextkorrektur
bestanden, einschließlich 160 Desktop-, 111 Storage- und 228 Application-Unit-Tests
sowie der Integrationstests und Doc-Tests. Vorgesehene ignorierte Live-/Plattform- und
Performance-Smokes wurden nicht aktiviert. Die Linkprüfung lief unter lokalem Node
25.6.1 mit einer Engine-Warnung gegenüber dem Projektpin 24.14.0; Dependencies wurden
nicht geändert. Kein Live-Replay mit dem konkreten
Nutzermodell, keine Linux-/macOS-Abnahme, keine neue UI oder Schemaänderung in diesem
Change. Das Fenster bleibt endlich und enthält weiterhin höchstens acht Ausschnitte,
einen pro Revision. Semantische Planqualität ist nicht durch Formatprüfung beweisbar;
echte fehlende Nutzerentscheidungen, Policy-Ablehnungen und ausgeschöpfte Gesamtbudgets
bleiben begründete Haltepunkte.
