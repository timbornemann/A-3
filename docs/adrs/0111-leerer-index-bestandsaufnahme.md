# ADR-0111: Leerer veröffentlichter Index ist eine Core-Bestandsaufnahme

Status: Accepted

Datum: 2026-09-16

Entscheider: Tim Bornemann

Freigabe: ausdrücklicher Auftrag, den Agenten auf einem neu angelegten leeren
Git-Projekt arbeitsfähig zu machen, statt die Recherche an fehlenden Originaldateien
scheitern zu lassen.

Ergänzt: [ADR-0047](0047-verbindlicher-recherchearbeitsstand.md),
[ADR-0054](0054-vollstaendig-gelieferte-planbestandsaufnahme.md) und
[ADR-0110](0110-leeres-projekt-anlegen.md). Interpretation bleibt quellengebunden.
Dieses ADR ersetzt nicht die Pflicht, vorhandene Indexdateien zu belegen.

## Kontext

Nach [ADR-0110](0110-leeres-projekt-anlegen.md) kann A^3 einen leeren Unborn-Worktree
anlegen. Plan und Agent erzeugen denselben dreiteiligen Core-Planvertrag. Q1 verlangt
bestehende Einstiegspunkte anhand benannter Originale. Interpretation ohne Quellen
ist unzulässig. Die bisherige Untersuchungsgrenze verlangt Literalsuche ohne Treffer
und Inspect/Verzeichnis-Belege, die ein Follow-up aus einem Graph ohne Dateien nicht
erzeugt.

Folge: 0 von 3 Teilfragen bleiben offen, `evidenceNeed` wiederholt sich, Fortsetzung
bindet denselben leeren Stand neu. Design (Q2/Q3) startet nie. Der Agent materialisiert
keinen Arbeitsplan und schreibt keine Dateien. Ein Task-Lens-Treffer von null in einem
**nicht** leeren Graph ist ein anderer Fall und bleibt eine offene Recherche.

## Entscheidung

- Ein **veröffentlichter Index ohne Dateien** ist eine Core-Bestandsaufnahme, kein
  unvollständiger Evidence-Stand. Der Core löst die aktive Repository-Frage als
  `BoundedUnknown` mit dem Publikations-Scope, ohne Modell-Interpretation und ohne
  erfundene Originale.
- Die Mindestabdeckung aus ADR-0054 gilt vacuous: ohne benannte aktuelle Revisionen
  gibt es nichts zu zitieren, bevor Design beginnt.
- Ein Graph mit mindestens einer Datei darf diesen Abschluss nicht nutzen, auch wenn
  die Task Lens null Dateien ausgewählt hat.
- Designfragen bleiben `DesignDecision` ohne Pflichtquellen. Fehlende Originale der
  leeren Bestandsaufnahme sind keine ContextLimit für die Entwurfsphase.
- Ein vollständiger Core-Plan darf genau diese aktuelle leere Bestandsaufnahme als
  Plan-Grounding ohne Source-Zitate verwenden. Der Nachweis ist an vollständigen
  Core-Planvertrag, Publikations-Scope und einen typisierten Core-Inventory-Receipt
  gebunden; ein quellenloser Plan in einem nicht leeren Graph bleibt gesperrt.
- Recherche, `AgentWorkPlan`, Policy, Approval und Verification werden nicht
  übersprungen. Der Agent erzeugt Dateien weiterhin nur über die bestehenden
  Patch-Werkzeuge.
- Geordnete Top-Level-Ergebnisse aus den beiden Designphasen werden verlustfrei in
  mehrere atomare `AgentWorkPlan`-Schritte kompiliert. Freier Prosaentwurf bleibt aus
  Kompatibilitätsgründen genau ein Schritt und erhält keine Markdown-Heading-Autorität.

## Konsequenzen

### Positiv

- Greenfield-Aufträge auf leeren Worktrees erreichen Design und Materialisierung.
- Der Core bleibt Eigentümer der negativen Bestandsaufnahme; das Modell erfindet
  keine vorhandenen APIs.

### Negativ

- Ein tatsächlich leerer Index kann keine Integrationsgrenzen aus Code ableiten.
  Der Entwurf stützt sich auf den Nutzerauftrag und reversible Annahmen.

### Risiken und Gegenmaßnahmen

- Verwechslung mit „Task Lens fand nichts“ — nur `graph.files().is_empty()` schließt
  die Bestandsaufnahme.
- Veralteter leerer Stand nach ersten Dateien — Revalidation am neuen Publikations-
  Scope macht die begrenzte Unbekanntheit stale.

## Verworfene Alternativen

- Recherche überspringen — widerspricht ADR-0042/0047.
- Interpretation ohne Quellen — verletzt die Domain-Invariante.
- `open_project`/`create_project` mit Scaffold — widerspricht ADR-0110.

## Compliance

- Desktop-Regression: leerer `PublishedIndex` löst Q1, ein dateihaltiger Index nicht.
- Design nach leerer Bestandsaufnahme gilt als original-geliefert, wenn keine Quellen
  nachzuweisen sind.
- Der Core-Planvertrag kann mit begrenzter Q1-Unbekanntheit plus zwei
  Designentscheidungen in einen `AgentWorkPlan` übersetzt werden.
- Ein real indizierter leerer Git-Worktree erreicht in Plan und Agent ohne Quellenzitat
  einen abgeschlossenen Plan; derselbe Test weist mehrere atomare Änderungs- und
  Testschritte nach.

## Referenzen

- [ADR-0042](0042-adaptiver-agent-arbeitsplan.md)
- [ADR-0047](0047-verbindlicher-recherchearbeitsstand.md)
- [ADR-0054](0054-vollstaendig-gelieferte-planbestandsaufnahme.md)
- [ADR-0110](0110-leeres-projekt-anlegen.md)
- [Produktanforderungen](../PRODUCT_REQUIREMENTS.md)
- [Memory und Kontext](../MEMORY_AND_CONTEXT.md)
