# ADR-0047: Verbindlicher Recherchearbeitsstand und kontrollierter Abschluss

Status: Accepted\
Datum: 2026-09-06\
Entscheider: Tim Bornemann

Freigabe: Der Nutzer hat den vorgeschlagenen Implementierungsplan ausdrücklich
freigegeben und die damit verbundenen Architekturentscheidungen angenommen.

Supersedes: Freitext als aktive Recherchefrontier und alleinige Read-/Delivery-
Fortschrittsmessung aus ADR-0038/0043/0044/0046, ausschließlich revisionsbasierte
Rechercheübergaben sowie die kopierte Änderungsverifikation einer Replan-
Lokalisierungsaufgabe aus ADR-0042. Äußere Ressourcenlimits, Einzelrepair,
Policy, Verification, Worktree-Serialisierung und der Controller aus ADR-0010
bleiben unverändert.

## Kontext

Der TaskFlow-Export vom 2026-09-06 zeigt 22 CSV-Planrunden und nach Fortsetzung
weitere 19 Runden mit weitgehend gleichen offenen Fragen. Eine Audit-Recherche
blockiert an optionaler Pluginregistrierung, eine andere schließt trotz fehlendem
angefragtem Logziel ab. Der vorhandene Mehrdatei-Kontexttest besteht, liefert dem
Modellstub aber genau die benötigten nächsten Funktionsnamen. Er belegt keine
stabile Aufgabenabdeckung eines realen Modells.

## Entscheidung

- Ein begrenzter, revisionierter `ResearchWorkState` hält den ursprünglichen
  Auftrag, stabile Teilfragen, erforderliche/unterstützende/optionale Zwecke,
  Ergebnisse, Quellenabhängigkeiten und den aktuellen Prüfstand außerhalb des LLM.
- Modelloutput ist ein Vorschlag. Nur der Core erzeugt Identitäten, validiert
  Übergänge und entscheidet Abschluss. Eine neue Formulierung darf kein erledigtes
  Ziel wieder öffnen oder ein erforderliches Ziel entfernen.
- Quelle gelesen, Quelle ausgeliefert und Teilfrage bearbeitet sind getrennte
  Größen. Identische Ziel-/Revisions-/Bereichskombinationen und erneut gelieferte
  Suchtreffer sind kein neuer fachlicher Fortschritt. Versuch und Ergebnis werden
  getrennt gehalten; transiente Fehler bleiben innerhalb der bestehenden Retries.
- Die Core-Auswahl nutzt Cache, aktuelle exakte Pfad-/Symbolanker, vorhandene
  Fast-Index-Aufruf-/Import-/Wertbeziehungen, gezielte Suche und begrenzte
  Verzeichnissuche. Kein neuer Index, keine automatische Deep Map, keine neue
  Netzwerk- oder Ausführungsbefugnis entsteht.
- Der Auftrag und kompakte Prüfstand bleiben im Kontext. Eine aktive Teilfrage
  erhält zusammengehörige Originalbelege; große Untersuchungen werden aufgeteilt.
  Quelltext bleibt flüchtig. Ergebnisse behalten ursprüngliche Evidence-Anker,
  nicht Zusammenfassungen früherer Zusammenfassungen als Beweis.
- Deterministisch bestätigte Facts, quellengebundene Interpretationen,
  Entwurfsentscheidungen und Hypothesen bleiben getrennt. Quellenmitgliedschaft
  allein ist kein semantischer Wahrheitsbeweis. `sufficient` ist kein Abschlussrecht.
- Erforderliche Teilfragen benötigen eine zugeordnete Antwort oder eine durch
  aktuelle Belege und einen Core-bekannten begrenzten Untersuchungsweg erklärte
  Erkenntnisgrenze. Nicht untersucht ist nicht beantwortet. Optionale Vertiefungen
  dürfen einen ansonsten beantworteten Auftrag nicht blockieren. Neue Schnittstellen
  eines Plans sind Entwurfsentscheidungen, keine fehlenden Bestandsbelege.
- Zustand und Audit werden innerhalb der bestehenden projektlokalen Persistenz
  atomar fortgeschrieben. Historische Traces bleiben lesbar und erhalten keine
  nachträglich erfundenen Erledigtzustände. Ask/Plan erzeugen keinen ausführbaren Task.
- Der Handoff trägt Anforderungen und Ergebnisanker. Erst eine exakte Planfreigabe
  materialisiert Umsetzungskriterien im bestehenden Goal Contract und Task Ledger.
  Rechercheabschluss ist niemals Implementierungsverifikation.
- Betroffene Belege und abhängige Ergebnisse werden vor neuer Verwendung stale.
  Unveränderte Ergebnisse bleiben nutzbar. Gleiche Replan-Ursache bei unveränderten
  Belegen darf keine endlose Folge neu identifizierter gleicher Aufgaben erzeugen.
- Die UI zeigt eine Core-projizierte stabile Prüfliste und getrennte Beleg-/Arbeits-
  Fortschritte. Keine UI-eigene Steuerung oder Anzeige interner Gedankengänge.
- Phasenspezifisches Recherche-V5 und getrennte inhaltsfreie Stream-Unterursachen
  ersetzen den globalen Modell-Abschlussentscheid. Ungültige Dokumente bleiben
  unausführbar und erhalten höchstens einen Repair. Jeder Aufruf wird abgerechnet.

## Konsequenzen und Grenzen

Mehr expliziter Zustand und Migrations-/Vertragstests sind erforderlich. Dafür
werden Schleifen an unveränderten Belegen kontrollierbar und Ergebnisse über
Kontextfenster hinweg nachvollziehbar. Natürliche Sprache und fachliche
Interpretation bleiben modellabhängig; eine Garantie beliebiger korrekter
Antworten wird nicht behauptet. Sicherheitsablehnungen und echte fehlende
Nutzerentscheidungen bleiben Haltepunkte. Keine automatische Budgeterneuerung.

## Abnahme

Der [Umsetzungsplan](../plans/10-RESEARCH_WORK_STATE.md) verlangt adversariale
Offline-Verträge, reale Index-/Reader-/Storage-Pfade, Migration und Reopen,
Agent-Handoff/Replan, UI-Kohärenz und wiederholte ausdrücklich freigegebene lokale
Modelltests. Ein grüner kooperativer Modellstub allein genügt nicht.

## Referenzen

- [ADR-0010](0010-single-controller-state-machine.md)
- [ADR-0013](0013-goal-contract-ledger-and-event-journal.md)
- [ADR-0042](0042-adaptiver-agent-arbeitsplan.md)
- [ADR-0046](0046-progressive-recherche-und-getrennte-recoverybudgets.md)
- [Memory und Kontext](../MEMORY_AND_CONTEXT.md)
- [Security](../SECURITY_AND_EXECUTION.md)
- [Qualitätsgates](../QUALITY_GATES.md)
