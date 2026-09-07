# ADR-0094: Zweistufige Agentaktionen im kontrollierten Vergleich

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Plan-10-Vorabannahme und expliziter Experimentauftrag)

## Kontext und Grenze

Aktuelle Originale im Kontext lösen die lokalen Aktionswahl- und Strukturfehler
nicht allein. Der bisherige Wirevergleich prüfte zwei Umschläge in einem Aufruf,
nicht die vom Nutzer vorgeschlagene Trennung von Auswahl und Ausarbeitung.
Der normale AgentAction-V5-Pfad bleibt die Produktbaseline. Ask, Plan und Replan
werden in diesem ersten Versuch nicht umgestellt; Modellselbstbewertungen sind
weiterhin weder Fakten noch Verifikation.

## Entscheidung

Ein explizit im nativen Testaufbau gewählter Application-Pfad führt denselben
echten Context Compiler, Controller und dieselben Werkzeuge zweistufig aus:

1. ActionChoice V1 wählt nur eine geschlossene Aktionsart einschließlich
   Inspect-Zielart, Ledger-Operation oder Patch-Operation. Keine Argumente und
   keine freie Statusnotiz werden zugelassen. Die Wahl hat keine Toolwirkung.
2. ActionArguments V1 verlangt ausschließlich die variablen Argumente der
   festgelegten Aktion. Der Core projiziert dafür den aktuellen V5-Schemaarm,
   bindet die bereits bekannten Run-/Worktree-/Snapshot-/Step-/Verification-
   Identitäten und lässt weder einen Aktionswechsel noch zusätzliche Felder zu.
   Nur solche bekannten Konstanten werden ergänzt. Dateihash, Pfad und Inhalt
   bleiben Modellvorschläge mit unabhängiger Prüfung, keine geratenen Core-Werte.

Beide Stufen erhalten bytegleich denselben aktuellen fachlichen Kontext, aber
getrennte kurze Systemverträge und das jeweils passende Schema. Vor jedem
Folgeaufruf wird der ursprüngliche Kontext erneut kompiliert; ein abweichender
Digest oder ein nicht mehr sicher lieferbarer Kontext sperrt die gesamte Wahl.
Die endgültige rekonstruierte V5-Aktion passiert den unveränderten unabhängigen
Decoder einschließlich Snapshot-Patchzulassung. Policy, exakte Approval,
Worktree-Lease, Live-Hashprüfung, Publish und echte Verifikation bleiben Pflicht.

Die zusätzliche reguläre Inferenz ist keine Reparatur. Beide Stufen teilen
genau einen Repair, also maximal drei Aufrufe je logischem Controllerturn.
Ungültige Daten werden nicht ausgeführt oder in den Folgeprompt übernommen.
Jeder Aufruf wird gezählt und alle tatsächlichen beziehungsweise konservativ
reservierten Tokenkosten werden gemeinsam journalisiert, auch bei Ablehnung.
Vor Folgeaufrufen gelten die unveränderten kumulativen Run-Token-/Repairgrenzen;
der gesamte zweistufige Austausch teilt eine Modellanfragefrist. Cancellation
bleibt kooperativ und besessen. Kein unbeschränkter Chatloop entsteht.

Es gibt keine produktive Einstellung, automatische Modellumschaltung, neue
Persistenz, Provider-Orchestrierung, Index- oder Sicherheitsgrenze. Nur der
Test-Composition-Root aktiviert den Versuch ausdrücklich. Die Application
implementiert beide Strategien am selben Zulassungspfad. Eine Übernahme als
Produktstandard benötigt zuerst eigene belastbare Abnahme und dokumentierte
Entscheidung; ein negatives Ergebnis belässt den bestehenden Produktpfad.

## Nachweis und Übernahmekriterium

Offline: unabhängige Schema-/Decoderprüfung, falsche/zusätzliche Felder,
Aktionswechsel, ein gemeinsamer Repair, abgebrochene Providerantworten,
Cancellation, Kontextwechsel zwischen Stufen, Token-/Zeitgrenzen und keine
Toolwirkung vor erfolgreicher Zulassung. Replan bleibt unverändert.

Live: gleiches öffentliches Coding-Fixture mit unveränderten geschützten Tests,
gleichem Modellprofil und derselben exakten Freigabe. Baseline und Versuch
werden gegenbalanciert wiederholt. Bericht: echter Taskerfolg, unabhängiges
Testergebnis, geschützte Dateien, Modellaufrufe, Tokens, Laufzeit, Repairs und
Fehlerklassen. Lokale Modelle laufen ausschließlich nacheinander. Ein einzelnes
grünes JSON oder ein einzelner Task ersetzt weder Wiederholbarkeit noch die
gesamte Ask-/Plan-/Agent-Abnahme. Vollständige lokale Qualitätsgates sind Pflicht.

Referenzen: [Plan 10](../plans/10-RESEARCH_WORK_STATE.md),
[ADR-0087](0087-agentaktionen-ohne-modellstatusnotiz.md),
[ADR-0093](0093-originalquellen-im-agentenkontext.md).
