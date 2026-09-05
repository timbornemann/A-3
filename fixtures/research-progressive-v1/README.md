# Progressive Recherche: Offline-Abnahme

Stand: 2026-09-05. Umsetzung von
[ADR-0046](../../docs/adrs/0046-progressive-recherche-und-getrennte-recoverybudgets.md)
in [Plan 06 / U13](../../docs/plans/06-DESKTOP_PRODUCT.md).

## Befund und Änderung

Der gemeldete TaskFlow-Verlauf zeigte frühe Reparaturenden und mehrfach verschobene
Reads derselben Manager-Datei. Der Export enthält keine rohen Modellantworten;
die konkrete historische JSON-Verletzung ist deshalb nicht nachweisbar.
Im bisherigen Code waren dagegen folgende Ursachen nachvollziehbar:

- Dateireads und tatsächlich ausgelieferter Kontext waren nicht getrennt. Bereits
  gelesener später Code konnte hinter einem wiederholt gekürzten Dateianfang fehlen.
- Diagramme reservierten Entscheidungen aus dem ohnehin kleinen Recherchebudget.
- Reparaturen konnten aktuellen Kontext verdrängen; unterschiedliche Fehler und
  unabhängige Dokumente waren nicht hinreichend getrennt.
- Die Datenbank begrenzte Fortschrittssequenzen auf 64. Ein längerer Lauf hätte
  unabhängig vom verbleibenden Recherchebudget beim nächsten Ereignis scheitern können.

Der Core führt jetzt revisionsgebundene Read-/Delivery-Intervalle, fokussiert
vorhandenen Code ohne erneuten adaptiven Read und verwendet phasengenaue Schemas.
Reparaturhinweise verdrängen nicht das aktuelle Frage-/Evidence-Paket. Geschlossene
Fehlercodes und verbrauchte Zähler bleiben ohne rohe Modellantworten nachvollziehbar.

| Obergrenze pro Abschnitt | Standard | Gründlich |
| --- | ---: | ---: |
| Recherche-Modellaufrufe einschließlich Repair/Retry | 12 | 24 |
| Neue adaptive Read-Aktionen | 24 | 48 |
| Repairs unabhängiger Dokumente, je Dokument höchstens einer | 3 | 6 |
| Transiente Modellwiederholungen, aus dem Aufrufbudget | 2 | 4 |
| Separate Diagrammaufrufe einschließlich eines zweiten Versuchs | 2 | 2 |
| Gesamtzeit einschließlich Diagramm | 5 Minuten | 15 Minuten |

Genau eine Core-Recovery darf nach zwei Nullrunden oder einem fehlgeschlagenen
Einzelrepair aktuelle bekannte Quellen neu fokussieren und gegebenenfalls bis zu
vier neue sichere Reads aus dem Restbudget wählen. Keine automatische Fortsetzung
startet neue Budgets. Ungültige Modellaktionen werden nie ausgeführt. Bei gescheiterter
Diagrammformatierung bleibt die validierte Textantwort erhalten.

Knowledge V35 übernimmt alle Ereignisse, Notizen und Quellenverknüpfungen atomar,
erweitert die Sequenzgrenze auf 1024 und lässt die Anzeige bei den neuesten 64
Ereignissen. Fremdschlüssel und Unveränderlichkeit bleiben aktiv. Tests erzwingen
auch einen Fehler nach dem Tabellenaustausch und prüfen den vollständigen Rollback.
Ein bereits migriertes Projekt benötigt eine V35-kompatible Programmversion;
es gibt keinen stillen Schema-Downgrade. Keine private Nutzer-Datenbank wurde für
diese Abnahme manuell verändert.

## Reproduzierbarer Vorher-/Nachher-Vergleich

Die synthetische [Manager-Fixture](taskflow/manager.py) hat 143 Zeilen und einen
langen Initialisierungsbereich vor `add_task` ab Zeile 130. Der Ablauf führt über
Speichern zum [Dispatcher](taskflow/plugins/base.py) und zum
[Audit-Plugin](taskflow/plugins/audit_log_plugin.py).
[legacy_context.rs](legacy_context.rs) friert ausschließlich für den Test den
vorherigen Packer aus Commit `ee85cbe` ein; Produktionscode verwendet ihn nicht.

Der Test `progressive_cache_delivers_late_code_with_exact_utf8_ranges_without_new_reads`
vergleicht denselben gecachten Inhalt und den angeforderten späteren Bereich:

| Evidence-Fenster | Alter Packer: später Dispatch-Aufruf sichtbar | Neuer Packer: sichtbar | Zusätzliche adaptive Reads |
| --- | ---: | ---: | ---: |
| 1 KiB | nein | ja | 0 |
| 2 KiB | nein | ja | 0 |
| 4 KiB | nein | ja | 0 |
| 8 KiB | nein | ja | 0 |

Alle Fenster halten ihre Bytegrenze ein. Überlappende Quellen derselben Revision
erhöhen die Read-Abdeckung nicht erneut. Überlange UTF-8-Zeilen werden an gültigen
Bytepositionen fortgesetzt. Dies misst Evidence-Zugang und Read-Verbrauch,
keine Laufzeitbeschleunigung oder Live-Modell-Erfolgsquote.

Weitere Nachweise:

- 24 Szenarien: vier Fenstergrößen mal Ask, Plan, Agent-Vorbereitung, `/diagram`,
  fehlgeschlagener Einzelrepair und zwei Nullrunden. Reale Parser, Indexpublikation,
  libSQL, Safe Reader und Scheduler; nur die Modellantworten sind skriptiert.
  Die Mehrdateikette schließt mit drei Quellen ohne Fortsetzungsklick ab.
- 48 aufgezeichnete Provider-Anfragen über den produktiven Nachrichtenaufbau und
  Stream-Collector: vier Größen, zwei Schema-Grounding-Profile, drei Phasen und
  jeweils Primär-/Reparaturpaket. Vollständiger aktueller Auftrag, Evidence,
  tatsächliches Phasenschema und Gesamtkontextgrenze werden geprüft.
- Negative Verträge für zweite ungültige Ausgabe, geschlossene Reads, globale
  Retrygrenzen, Trunkierung, Live-Edit, Cancellation und Symlink-/Junction-Escape.
- 100 tatsächlich gespeicherte Ereignisse mit genau den neuesten 64 in der
  Projektion; V34→V35-Datenerhalt, Upgrade-Ausgangsversionen und Fehlerrollback.
- Die Anzeige übernimmt weitergerückte 64er-Fenster anhand ihrer überlappenden
  Ereignisse und erhält die DOM-Zeilen des gemeinsamen Abschnitts. Vollständig
  verpasste Fenster werden nur bei durchgängig späteren Zeitstempeln übernommen;
  diese Präsentationsprüfung ist keine Evidence- oder Ausführungsautorität.
  Drei zusätzliche Komponentenszenarien prüfen beide APIs, veraltete Polls,
  verpasste Fenster und den sichtbaren Abschluss nach mehr als 100 Ereignissen.
- Separater Test-Synchronisationsnachweis für den früher sporadischen Start/Pause-Test:
  `Running` wird vom Scheduler vor dem Executor-Callback veröffentlicht. Der Test
  wartet jetzt über einen begrenzten Kanal auf Start 1 beziehungsweise 2 und prüft
  weiterhin die exakten Zähler, Pause, Ownership, Resume-Anker und Cancellation.
  Nur der Testadapter ist geändert; kein Produktionszustand und keine Assertion
  wurden entfernt oder abgeschwächt.
  Der exakte Test bestand außerdem in 25 von 25 frischen Testprozessen.

## Prüfungen

Aus dem Repository-Root, ohne Providerzugriff oder Installation:

```powershell
cargo fmt --all --check
cargo test -p a3-application --offline --locked agent_research_controller
cargo test -p a3-desktop --lib --offline --locked research -- --test-threads=1
cargo test -p a3-desktop --lib --offline --locked agent_run_manager::tests
cargo test -p a3-desktop --lib --offline --locked agent_run_manager::tests::explicit_start_pause_and_cancel_require_owned_terminal_work -- --exact
cargo test -p a3-storage-libsql --offline --locked knowledge_v35
cargo test -p a3-storage-libsql --offline --locked long_research_keeps_complete_storage
cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
cargo test --workspace --all-features --release --offline --locked
pnpm format:check
pnpm lint
pnpm typecheck
pnpm --filter @a3/desktop test -- AgentAskResearch.test.ts
pnpm test -- --run
pnpm check:links
git diff --check
```

Die Rechercheprüfung besteht mit 35 Tests; das Controllerpaket mit 12 gezielten
Tests. Der vollständige Release-Workspace-Lauf besteht einschließlich 153 Desktop-
und 111 Storage-Unit-Tests sowie der Integrationstests. Der Run-Manager-Vertrag
besteht mit sieben Tests. Clippy mit Warnungen als Fehler ist grün. Frontend:
371 bestanden, 14 bereits vorgesehene Tests übersprungen; zusätzlich fünf
Script-Tests bestanden. Formatter, Lint und Svelte-Typecheck sind grün
(0 Fehler, 0 Warnungen im Typecheck). Die gezielte Recherche-Komponente besteht
mit 29 Tests. Markdown-Linkprüfung und `git diff --check` sind ebenfalls grün.

Unter Windows wurde für den Gesamtlauf ein frischer Ordner unter dem Standard-
Tempverzeichnis als `TEMP` und `TMP` verwendet. Ein Caret im Temp-Pfad stört
bestehende Junction-Fixtures. Release vermeidet den Dateilock der laufenden
Debug-Nutzerinstanz; diese wurde nicht beendet. Bereits ausdrücklich ignorierte
plattform-/providergebundene Tests wurden nicht eingeschaltet.

## Verbleibende Grenzen

Kein Live-Replay mit dem konkreten Nutzermodell und keine Linux-/macOS-Ausführung
in dieser Abnahme. Die lokale Node-Version 25.6.1 weicht vom Projektpin 24.14.0 ab;
die genannten Frontend-Prüfungen bestanden mit einer Engine-Warnung. Es wurden
keine Dependencies, UI-Capabilities oder Freigaben erweitert. Echte fehlende
Nutzerentscheidungen, Sicherheitsablehnungen und ausgeschöpfte Gesamtbudgets
bleiben bewusste Haltepunkte. Schwierige Aufträge können durch die größeren
endlichen Budgets mehr Modellzeit und gegebenenfalls Kosten verursachen.
