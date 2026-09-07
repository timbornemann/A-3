# ADR-0098: Quellengeführte Arbeitsentscheidung im Vergleich

Status: Accepted

Datum: 2026-09-08

Entscheider: Tim Bornemann (Plan-10-Vorabannahme und expliziter Experimentauftrag)

## Befund und begrenzter Schnitt

Der V8-Zwei-Modul-Test liefert beide Originale bereits vor dem ersten Codingturn.
Ein inhaltsfreier Nachlauf mit Qwen belegt trotzdem 26 Datei-Reads, davon 24
identische Anfragen mit identischem Ergebnis im selben Snapshot. Die bisherige
Aktionswahl und die erst nach Patch verfügbare Nachentscheidung lösen das nicht.
Mehr Kontext ist notwendig, aber kein Fortschrittsnachweis.

## Entscheidung

Der Context Compiler liefert zusätzlich zum unveränderten Prompt eine begrenzte
typisierte Projektion der tatsächlich eingepackten Originalseiten. Nur nach
Safe-Reader-Revalidierung, vollständiger Seitenvalidierung und erfolgreichem
Budgetpacking entsteht diese Projektion. Sie enthält Snapshot, Dateirevision,
Bereich und Fortsetzung, keinen zusätzlichen Quelltext. Höchstens zwei Seiten
bleiben an ihren aktuellen Compile gebunden. Preview, Suchtreffer, historische
Read-Erfolge und Modellbehauptungen erzeugen keine Lieferquittung. Replan bleibt
ausgenommen. Es entstehen keine dauerhafte Source-Kopie und keine DB-Migration.

Eine vierte native Vergleichsstrategie `SourceGuided` erhält die bisherigen
Produkt-, Staged- und Guided-Pfade unverändert. Sie nutzt die quittungsgebundene
Nachentscheidung aus ADR-0095 und davor, bei gelieferten nichtleeren Originalen
und einer geplanten operationalen Schrittprüfung, `SourceWork V1`:

- `change`: eine konkrete Änderung ausarbeiten;
- `need_evidence`: gezielt noch nicht geliefertes Material beschaffen;
- `verify`: die bereits geplante Prüfung anfordern, ohne Erfolg zu behaupten.

Die erste Wahl enthält keine Argumente oder freie Begründungen. `change` und
`need_evidence` führen in die vorhandenen engen Auswahl-/Argumentphasen; `verify`
verwendet ausschließlich bekannte Command-/Step-IDs und den unabhängigen
V5-Decoder. Gelieferte Seiten sind kein Beweis, dass alle benötigten Quellen
vorliegen oder die Aufgabe gelöst ist. Insbesondere erzwingt der Core keinen Patch.

Nur in dieser Vergleichsstrategie wird eine ansonsten gültige Dateiinspektion
vor der Toolgrenze darauf geprüft, ob ihr vollständiger angefragter Zeilenbereich
bereits im aktuellen Compile geliefert wurde. EOF und Fortsetzung kommen aus
dem Reader, nicht aus Promptparsing. Eine nur überlappende Anfrage, ein noch
fehlender Bereich oder eine andere Datei wird dadurch nicht gesperrt. Identische
Pfade allein reichen nicht. Der vorhandene einzige Repair kann die Argumente
derselben gesperrten Wahl korrigieren; ein erneuter Verstoß beendet den Turn ohne
Read. Er wird nicht als Änderung, erfolgreiche Recherche oder Verifikation
uminterpretiert. Diese enge Zulassung ist noch keine allgemeine Such-/Read-Frontier.

Alle Stufen verwenden denselben Fachkontext, dieselbe Gesamtfrist und kumulative
Tokenkosten. Höchstens drei reguläre Aufrufe plus ein gemeinsamer Repair sind
zulässig. Vor Folgeaufrufen werden sowohl Kontextdigest als auch die typisierten
Lieferquittungen erneut verglichen. Keine Modellantwort wird als neue Instruktion
reinjiziert. Policy, exakte Approval, Cancellation, Worktree-Lease, Hashprüfung,
Publish und echte Verifikation bleiben unverändert.

## Nachweis und Übernahmegrenze

Regressionen: Seitenbereich einschließlich Teilseite/EOF/CRLF, andere Datei oder
Revision, fehlendes Budget, Preview ohne Original, Replan, deterministische
Rekompilierung, gemeinsamer Repair, keine Toolwirkung bei abgelehnter Wiederholung,
weiterhin zulässige neue Reads, drei Arbeitswege und fehlende Voraussetzungen.

Live: derselbe eingefrorene Binärstand, unveränderte Zwei-Modul-Fixture und
unabhängiges Zusatzorakel, gegenbalancierter Vergleich insbesondere der belegten
Read-Schleifen. Lokale Modelle strikt nacheinander; tatsächlichen Taskerfolg,
Read-Identitäten, Kosten und negative Ergebnisse getrennt dokumentieren.
Vollständige lokale Qualitätsgates sind Pflicht. Eine kürzere Fehlerschleife ist
kein Taskerfolg. Produktübernahme, allgemeine Frontier und vollständige
Ask-/Plan-/Agent-Abnahme benötigen eigene Nachweise.

Referenzen: [Plan 10](../plans/10-RESEARCH_WORK_STATE.md),
[ADR-0093](0093-originalquellen-im-agentenkontext.md),
[ADR-0094](0094-zweistufige-agentaktionen-im-kontrollierten-vergleich.md),
[ADR-0095](0095-gefuehrte-nachentscheidung-nach-angewendeten-aenderungen.md).
