# Abläufe und Werte im Fast Index

Stand: 2026-09-05 · [ADR-0045](adrs/0045-fast-index-function-flow-analysis.md)

## Für Nutzer

Der Hauptbereich **Abläufe** sucht Funktionen, Tests und Skriptstarts. Eine Auswahl
zeigt aufklappbare Arbeitsschritte. „Hinein“ öffnet genau diese Aufrufstelle;
der Weg darüber führt zurück. „Woher?“ und „Wohin?“ verfolgen lokale Wertversionen
und eindeutig zuordenbare Argumente beziehungsweise Rückgaben. Die Quellansicht
öffnet den ausgewählten Schritt. Karte und Abläufe sind gegenseitig verlinkt.

Zwei Aufrufe derselben Funktion bleiben zwei Kontexte: `A → B → C` wird nicht mit
`A → C` vermischt. Die Anzeige erklärt statisch mögliche Abläufe, **keine gemessene
Ausführung**. Angezeigte Variablen sind Namen und Abhängigkeiten, keine beobachteten
Laufzeitwerte. Analyse und Navigation führen weder Code noch Modelle aus.

Die Analyse entsteht automatisch im normalen Fast Index. Änderungen verwenden
unveränderte Parse-Artefakte erneut und verknüpfen die aktuelle Gesamtpublikation.
Während einer Aktualisierung verschwinden alte Ablaufdetails. Alte Auswahlen sind
nach einem Run-/Projektwechsel ungültig. Deep Map muss für diese deterministischen
Ergebnisse nicht erneut ausgeführt werden; vorhandene Deep-Map-Claims behalten
ihre bisherigen eigenen Freshness-Regeln.

## Unterstützte statische Teilmenge

Rust, TypeScript/JavaScript und Python liefern einzelne Aufrufstellen in
Auswertungsreihenfolge, lokale Zuweisungen und Versionen, Bedingungen, alternative
Bereiche, Schleifen, explizite Abbrüche sowie Await-/Deferred-Markierungen.
Verschachtelte Funktionsdeklarationen werden nicht als ausgeführte Körper des
Elternaufrufs behandelt. Rust-Tail-Returns verwenden nur den letzten Ausdruck.
Try/Catch/Finally, Match, Switch und With bleiben ausdrücklich partiell: Bereiche
werden getrennt, Cleanup bleibt nach Return sichtbar, Dispatch und Overrides sind
nicht vollständig modelliert. Schleifen behaupten keine Iterationszahl.

Benannte ESM-Imports, Namespace-Imports, einfache CommonJS-Require-Bindungen,
Python-Imports und Rust-Use-Aliase nutzen deterministische lokale Auflösung.
Typinferenz, virtuelle Methoden, beliebige Reexports, dynamische Imports und
komplexe Alias-/Heap-Effekte sind nicht vollständig auflösbar. Rest-, Default-,
Destructuring- und Spread-Zuordnungen können eine unbekannte Grenze erzeugen.
Rekursion wird begrenzt aufgefaltet, nicht als endliche Laufzeit behauptet.

Node `execFileSync`/`spawnSync` und Python `subprocess.run`/`call`/`check_call`/
`check_output` markieren Warten; Node `spawn`/`execFile` und Python `Popen` markieren
nur Start. Auflösbare Ziele verlangen wörtliches Interpreter-/Skript-argv und
bekanntes dateirelatives cwd (`import.meta.dirname`, `.cjs`-`__dirname` oder Python
`os.path.dirname(__file__)`). Unbekannte Optionen, Shell, Überschattung und
Mutation verhindern sichere Zielbehauptungen. Bibliotheksidentität wird bei
konkurrierenden Bindungen im selben File bewusst konservativ behandelt.
`fork` wird als Prozessstart erkannt, sein Ziel bleibt derzeit unbekannt.

Einfache `package.json`-Skripte mit literalen Argumenten und `&&` verknüpfen lokale
Node-/Python-Dateien oder benannte npm/pnpm-Skripte. Folgeschritte hängen vom Erfolg
ab. `tsc`/`rustc` sind Kompilierung, keine Programmausführung. Shelloperatoren,
Substitution und Lifecycle-Hooks bleiben Lücken. Skriptargumente werden für
`process.argv[2+n]` und `sys.argv[1+n]` zugeordnet; Prozessresultate werden niemals
mit einem Return des Skripts gleichgesetzt.

## Daten, Versionierung und Grenzen

Die bestehenden sechs Phasen und dieselbe Knowledge-Datenbank bleiben erhalten.
Indexschema V6 und die `flow-v1`-Adapterrevisionen erzwingen die erneute Analyse
älterer Parse-Artefakte. Migration V34 ergänzt `index_function_flows`: immutable,
run-/symbolgebundene private JSON-V1-Daten, maximal 8 MiB je Zeile, strikte
Dekodierung und Domain-Revalidierung. Schreiben, Entfernen und Rebuild laufen in
derselben Fast-Index-Transaktion. Normale PublishedIndex-Reads laden diese Körper
nicht; einzelne Funktionen werden gezielt gelesen.

Grenzen: 4.096 Elemente/Funktion, 2.000.000/Run, 50 Ergebnisse/Seite, acht
Aufrufkontexte, 4.096 untersuchte Beziehungen und zwei Sekunden/Read. Begrenzung
erzeugt eine sichtbare Lücke beziehungsweise `truncated`. Cancellation und
fehlgeschlagene Publikation veröffentlichen keine halbfertigen Abläufe.

`ExploreFunctionFlows` ist der gemeinsame Application-Reader. Kompositionen
behalten jede beteiligte FileRevision, nicht nur das letzte Ziel. Er vergleicht
aktuellen Snapshot/Run erneut vor Rückgabe. Der pfadlose Desktop-Command
`query_function_flows` erlaubt ausschließlich Catalog, Inspect, Trace und Source;
die Source-Auswahl enthält nur Run, Root, Aufrufstellen und Schritt-ID. Privilegierte
Pfadauflösung und Live-Hash-Prüfung liegen im vorhandenen Safe Source Reader.

## Ask, Plan, Agent und Deep Map

Neu generierte Agent-Aktionen verwenden das geschlossene **AgentAction V4**.
`inspect.target.kind=function_flow` enthält `symbol_id`, höchstens sieben
`call_path`-IDs sowie eine View `steps`/`values` mit 50er-Offset oder `origins`/`uses`
mit Value-ID. Historische V1–V3-Schemas bleiben unverändert. Der kleine Systemvertrag
und alle bisherigen Controller-, Reparatur- und Mutationsregeln bleiben bestehen.
Agent-Reads prüfen die beteiligten Dateien zusätzlich live und übernehmen sämtliche
Evidence-Referenzen vor dem begrenzten Analysetext.

AskResearchDecision V4 ergänzt `inspectFunctionFlow` für eine schon ausgegebene
`S`-Quelle und denselben Aufrufpfad/View-Vertrag. Die Quelle muss ein eindeutiges
aktuelles Symbol bezeichnen. Ask und Plan verwenden denselben Researcher, dessen
Source-/Kontextbudgets weiter gelten. Kann er nicht alle Quellen sichern, wird das
ganze Ablaufresultat zurückgehalten. V3-Rechercheausgaben bleiben lesbar.

Deep Map erhält bei seiner bestehenden Inspect-Aktion für ein planbekanntes Symbol
die erste 50er-Schrittseite desselben Readers, mit bestehenden Evidence-IDs. Dies
erweitert nicht seine Zielautorität und führt keinen freien neuen Toolzugriff ein.
Die vollständigen interaktiven Pfad-/Wertabfragen stehen UI, Ask/Plan und Agent zur
Verfügung. Statische Möglichkeiten allein werden nicht zu verifizierten Claims.

## Reproduzierbare Prüfung

`function_flow_features`, `function_flow_publication` und der echte Read-only-
Agent-Controller prüfen Parser, getrennte Skriptkontexte, Reopen, Refresh/Rebuild
und einen Live-Edit vor Watcherbeobachtung. Codec-/Migrationstests prüfen beschädigte
Referenzen, Unbekanntfelder und Rollback. Die vorhandenen Source-, Publish-,
Controller- und Capability-Verträge gelten weiter.

Browserfixture: Desktop-Devserver starten und `/fixtures/function-flow.html`
öffnen. Es montiert die echte Komponente mit 50 Schritten, 50 Werten und 50
Trace-Knoten aus einem großen synthetischen Bestand; Messwerte zeigen DOM und
Long Tasks. Es enthält weder produktive IPC noch einen Prozessstart.
Messungen und Gate-Ergebnisse stehen im [Implementierungsplan](plans/08-FAST_INDEX_FLOW_ANALYSIS.md).
