# ADR-0095: Geführte Nachentscheidung nach angewendeten Änderungen

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Plan-10-Vorabannahme und expliziter Experimentauftrag)

## Befund und Ziel

Der gegenbalancierte ADR-0094-Vergleich verbessert Qwens kleines Coding-Fixture,
behebt aber nicht alle Ablaufentscheidungen. Granite und Ornith wenden Änderungen
an und fordern danach weitere Patches statt Verifikation an. Granites genauer
Nachlauf endet in `NoContentChange`; ein ungültiger Vorschlag darf niemals als
ausführbarer Prüfauftrag interpretiert werden.

Die bestehende ADR-0088-Quittung belegt eine tatsächliche Runmutation, nicht den
Erfolg oder die Vollständigkeit des aktuellen Schritts. Automatische Tests nach
jedem Teilpatch würden die offene Mehrdateiarbeit nicht berücksichtigen. Der Core
soll stattdessen die nächste kleine Arbeitsentscheidung vorgeben und bereits
bekannte Ausführungsdaten selbst ergänzen.

## Entscheidung

Eine dritte, ausschließlich im nativen Testaufbau gewählte Variante ergänzt
ADR-0094. Bestehende Baseline und zweistufiger Vergleich bleiben erhalten.

Die Application wählt eine vorgeschaltete `AfterChange V1`-Phase nur bei einer
aktuellen, passend gebundenen `PatchApplied`-Quittung aus ADR-0088 und einem
laufenden Schritt mit bereits vorhandenem operationalem Verifikationscommand.
Fehlende Quittungen, ProcessObserved, andere Verifikationsarten und Replan nutzen
keine solche Phase. Die Quittung bleibt ausdrücklich die letzte Runmutation;
sie wird nicht unbelegt zur Mutation des aktuellen Schritts umdeklariert.

Das Modell entscheidet ausschließlich zwischen drei nächsten Arbeiten:

- `verify`: fordert genau die geplante Verifikation des aktuellen Schritts an.
  Step-/Command-ID liefert der vorhandene Core-Selektor aus ADR-0079. Ein zweiter
  Modellaufruf für diese vollständig bekannten Argumente entfällt. Die vollständige
  V5-Run-Aktion muss weiterhin durch unabhängigen Decoder und normale
  Policy-/Approval-/Command-/Freshness-/Budget-/Mutationsgrenzen.
- `continue_change`: führt zur begrenzten Wahl einer Patchoperation oder eines
  begründeten Replan-/Blocker-Auftrags und dann zu deren variablen Argumenten.
- `need_evidence`: führt ausschließlich zur Wahl und Ausarbeitung eines Search-
  oder Inspect-Auftrags. Diese lokale Phasenauswahl erteilt keine neuen Leserechte.

Keine freie Selbsteinschätzung wird als Tatsache, Ergebnis oder Verifikation
persistiert. `verify` behauptet insbesondere nicht, dass eine Prüfung bestehen
wird. Ein roter Test durchläuft unverändert den vorhandenen Retry-/Replanpfad;
weitere Patches können unabhängig vom Modellurteil nötig sein. Keine Methode
beweist allein die semantische Vollständigkeit eines Plans.

Alle Phasen desselben logischen Turns erhalten denselben aktuellen Fachkontext
und teilen einen einzigen Repair, dieselbe Gesamtfrist sowie kumulative Budgets.
Die dritte Variante benötigt nach Patch maximal drei reguläre Modellaufrufe plus
einen Repair; `verify` benötigt nur einen regulären Aufruf. Ohne Nachentscheidung
bleibt die Grenze aus ADR-0094 bei zwei regulären Aufrufen plus einem Repair.
Vor Folgeaufrufen gelten erneuter Context Compile, Digestvergleich und Cancellation.
Alle verbrauchten Tokens bleiben auch bei Ablehnung gebucht. Ungültige Antworten
werden weder ausgeführt noch als zusätzliche Modellnachricht wiederverwendet.

Keine DB-Migration, neue Abhängigkeit, UI-Einstellung, Modellumschaltung oder
Änderung der produktiven Ask-/Plan-/Agent-Baseline entsteht. Die äußere
Controller-Zustandsmaschine bleibt unverändert. Der Vergleichspfad besitzt keinen
eigenen Tooladapter und keine vom bestehenden Harness getrennte Sicherheitsgrenze.

## Nachweis

Regressionen prüfen die drei Folgewege, falsche Auswahl und Felder, unerlaubten
Aktionswechsel, den gemeinsam verbrauchten Einzelrepair, Providerabbruch,
Cancellation, Freshness und Kosten. Der echte Harness muss eine aktuelle
Mutationsquittung nach Patch laden und ohne weitere Modellargumente die exakt
geplante Verifikation ausführen können; fehlende und rote Evidence bleiben offen.
Mehrdateiarbeit darf durch die Nachentscheidung nicht automatisch beendet werden.

Livevergleich: gleicher eingefrorener Stand, Profile, öffentliche Fixture und
Freigaben, gegenbalanciert wiederholt; lokale Modelle strikt nacheinander.
Zusätzliche unabhängige Eingaben und Mehrdateiaufgaben sind vor Produktübernahme
erforderlich. Vollständige lokale Qualitätsgates und ehrliche negative Ergebnisse
sind Pflicht. Das allgemeine Ziel der zuverlässigen Recherche und Umsetzung ist
durch diesen einzelnen Schnitt nicht abgeschlossen.

Referenzen: [Plan 10](../plans/10-RESEARCH_WORK_STATE.md),
[ADR-0094](0094-zweistufige-agentaktionen-im-kontrollierten-vergleich.md),
[ADR-0088](0088-journalgebundene-ausfuehrungsrueckmeldung.md),
[ADR-0079](0079-abschlussanforderung-verifiziert-zuerst-den-schritt.md).
