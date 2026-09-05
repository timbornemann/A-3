# ADR-0045: Funktionsabläufe und Wertflüsse im Fast Index

Status: Accepted

Datum: 2026-09-05

Entscheidung: Umsetzung des vom Nutzer bestätigten Fast-Index-Upgrade-Plans.

## Kontext

Der vorhandene Call-Graph unterscheidet weder Ausführungsreihenfolge noch lokale
Wertbindungen. Die zwei Aufrufe von C in `A → B → C` und `A → C` benötigen getrennte
Aufrufkontexte. Nutzer wollen diese Zusammenhänge in einem eigenen Bereich „Abläufe“
erkunden. Dieselben Belege sollen Ask, Plan und Agent zur Verfügung stehen.

## Entscheidung

- Die vorhandenen Rust-, JS/TS- und Python-Parser extrahieren eine begrenzte,
  sprachneutrale Ablaufanalyse. Sie gehört zu Parse, Link und demselben atomaren
  Publish des Fast Index. Es gibt keinen zweiten Index, Analysestart oder Dienst.
- Aufrufstellen, Kontrollbedingungen, lokale Bindungen und Wertabhängigkeiten
  behalten exakte Quellbereiche. Lokale Analysedaten erweitern das globale
  Symbolranking nicht. Funktionsaufrufe verwenden die gemeinsame Zielauflösung.
- Eindeutige Argument-/Parameter- und Rückgabezuordnungen sind aufrufbezogen.
  Abfragen komponieren begrenzte Pfade aus aktuellen Funktionsartefakten. Ein
  rekursiver, dynamischer oder nicht unterstützter Übergang bleibt eine sichtbare
  Grenze; ein statischer Pfad ist keine beobachtete Laufzeitspur.
- Die V1-Grenzen sind 4.096 Analyseelemente je Funktion, zwei Millionen je
  Indexlauf, 50 ausgegebene Schritte, acht Aufrufkontexte, 4.096 untersuchte
  Beziehungen und zwei Sekunden je Abfrage. Strukturelle Kürzung ist explizit;
  Cancellation oder fehlgeschlagener Gesamtpublish veröffentlicht keine Teildaten.
- Statische lokale Node-/Python-Skriptstarts und einfache Manifestketten werden
  nur bei belegbarem Ziel und Arbeitsverzeichnis verbunden. Analyse führt niemals
  ein Skript aus. Prozessstart, Warten und Kompilieren bleiben unterschiedliche
  Vorgänge. Shell und PowerShell erhalten keinen neuen Parser.
- Regenerierbare Analysedaten werden mit derselben IndexRunId und FileRevision in
  der bestehenden Knowledge-Datenbank gespeichert. Der normale PublishedIndex
  benötigt keine vollständige Rekonstruktion aller Funktionskörper. Alte
  Publikationen ohne Ablaufartefakte sind explizit nicht analysiert.
- Alle zusammengesetzten Belege behalten ihre vollständigen Quellrevisionen und
  die Analyseversion. Publication-Wechsel verwirft alte Cursor und Abfragecaches.
- Die neue lazy geladene Hauptfläche „Abläufe“ zeigt aufklappbare Schritte,
  Variablenverwendung und vorwärts/rückwärts verfolgbare Wertpfade. Technische
  Details erscheinen progressiv. Es gelten die bestehenden UI-Lifecycle-,
  Accessibility- und DOM-Grenzen aus ADR-0025.
- Eng typisierte Read-Models werden von UI und Agent gemeinsam benutzt. Neue
  Source-Auswahlen ergänzen ADR-0030/0031 unter unveränderten Safe-Read-Grenzen.
  Versionierte Read-only-Aktionen ergänzen das geschlossene Recherchewerkzeugset
  aus ADR-0038; dessen Aktions-, Reparatur-, Kontext- und Zeitbudgets bleiben gleich.

## Konsequenzen

Die Persistenz verwendet private, strikt dekodierte JSON-DTOs pro Funktion. Dafür
nutzt der Speicheradapter die bereits im Workspace gepinnten `serde`- und
`serde_json`-Versionen; die Standardbibliothek bietet keinen JSON-Codec. Das
vermeidet einen eigenen Binärcodec und hält normale Indexabfragen frei von den
Funktionskörpern. Die Domain bleibt ohne Serialisierungsabhängigkeit.

Der Agent kann konkrete Abläufe und Werte mit Quellen verfolgen. Nutzer können
zwei Aufrufstellen derselben Funktion unterscheiden, ohne den Quellcode zuerst zu
lesen. Zusätzliche Parser-, Storage-, IPC- und UI-Verträge sind erforderlich.
Unbekannte Alias-, Bibliotheks- und Nebenläufigkeitseffekte bleiben sichtbar;
fehlende Erkenntnis wird nie zu „keine Wirkung“ hochgestuft.

## Compliance

Mehrsprachige Golden-Fixtures prüfen Reihenfolge, Überschattung, Aufrufkontexte,
Argumente/Rückgaben, Skriptgrenzen, Kürzung und Determinismus. Reale Storage- und
Agententests prüfen Publish-Rollback, Bestand, Rebuild und vollständige
Invalidierung. UI-Tests prüfen Quellen, Tastatur, Projekt-/Publikationswechsel und
begrenzte Retention. Vorher-/Nachher-Messungen prüfen die bestehenden Fast-Index-
und Context-Budgets; keinerlei Geschwindigkeitsclaim ohne Messung.
