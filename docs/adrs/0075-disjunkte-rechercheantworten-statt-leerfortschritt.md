# ADR-0075: Disjunkte Rechercheantworten statt Leerfortschritt

Status: Accepted\
Datum: 2026-09-07\
Freigabe: Fortgesetzter Plan-10-Auftrag mit automatischer ADR-Annahme.\
Supersedes: ausschließlich die aktuelle Wire-Struktur und nullable Analyze-Fortschrittsdarstellung aus ADR-0057/0071/0072/0074; historische Decoder bleiben unverändert.

## Befund

Die V6-Nachtests enthalten gültigen leeren Analyze-Fortschritt bei Granite/Ornith
und zweimal leeren Planfortschritt bei Google Gemma. Getrennte `decision`- und
`work.results`-Eigenschaften erlauben im Providerschema Kombinationen, die keine
Arbeit leisten oder erst im unabhängigen Phasendecoder scheitern. Ein zusätzlicher
Prompt oder wiederholte Reads behebt diesen Strukturwiderspruch nicht zuverlässig.
GPT-OSS besteht dagegen den trivialen direkten V5-/V6-Wirevergleich; seine leeren
Streams in komplexeren Fällen sind damit nicht als allgemeine V6-Inkompatibilität
belegt und benötigen getrennte Nachtests.

## Entscheidung

Neue Ask-/Plan-/Agent-Vorbereitungsantworten verwenden V7 mit genau
`schema_version` und einer einzigen `response`. Diese ist eine phasengebundene
disjunkte Variante, kein getrenntes Fortschrittsversprechen:

- Initialize: `kind=questions` mit der nichtleeren begrenzten Fragenliste.
- Analyze: genau eine `interpretation` mit aktiver `question_id`, konkretem `text`
  und aktuellen E-Ankern; alternativ der bisherige originalgebundene `evidenceNeed`
  oder eine echte folgenreiche `question`.
- SummarizeOriginals: Interpretation oder konkreter Belegbedarf, keine Nutzerfrage.
- Design: genau eine `designDecision` mit leerer Evidence oder folgenreiche Frage.
- DesignTests: genau eine `designDecision`, keine Bestätigungsfrage.
- Legacy-Finalize: die bisherige geschlossene `plan`-Darstellung.

Ein neutraler `progress`-Arm, leere Ergebnisliste oder gleichzeitiges Ergebnis plus
Bedarf existiert im V7-Dokument nicht. Der feste Objektroot und die Union darunter
bleiben über die bestehenden Providerprojektionen darstellbar. Unabhängig vom
Providerschema werden exakte Felder, aktive Phase, Typen und Grenzen geprüft.
Erst danach normalisiert der Decoder in dieselben vorhandenen Work-/Bedarfstypen.
V3 bis V6 bleiben strikt lesbar, bestehende gespeicherte Zustände unverändert.
Der mutierende Agent-Replan bleibt beim gesondert geprüften V5-Vertrag.

Das ist keine semantische Wahrheitserkennung: Inhalt, reale Originalauslieferung,
Freshness, Abhängigkeiten und zulässige Suchkandidaten prüft weiterhin der Core.
Neue Variante bedeutet weder zusätzliche Reads noch Retry, Repair, Nutzerfreigabe
oder größere Kontext-/Outputbudgets. Kein Tool wird aus unzulässigen Restfeldern
rekonstruiert. Ungültige Dokumente erhalten höchstens den bestehenden Einzelrepair.

## Verifikation und Grenzen

Rot→Grün für die V7-Ergebnisalternative und Ablehnung von Leerfortschritt,
gemischten Varianten, falscher Frage/Phase, Modellstatus, Quellenkoordinaten und
überlangen Werten. V5/V6-Replay, alle Providerphasen, 8k/2k-Packing und echte
Mehrmodus-/Fortsetzungs-/Negativfixtures bleiben erforderlich. Größe und Live-
Abschluss werden auf unveränderten öffentlichen Aufgaben getrennt gemessen;
Writer-, Persistenz- und widersprüchliche Entwurfsbehauptungen bleiben eigene
Inhaltsbefunde. Keine DB-Migration, Abhängigkeit, neue Infrastruktur oder Rechte.
