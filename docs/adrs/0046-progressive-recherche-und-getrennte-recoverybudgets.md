# ADR-0046: Progressive Recherche und getrennte Recoverybudgets

Status: Accepted  
Datum: 2026-09-05  
Entscheider: Tim Bornemann

Supersedes: die Recherchebudgets, abschnittsweite Einzelreparatur,
Duplikatabrechnung und unmittelbare Zwei-Nullrunden-Fortsetzung aus ADR-0038;
die gemeinsame Recherche-/Diagrammreserve aus ADR-0039 sowie die entsprechenden
Budget-/Retry-Festschreibungen in ADR-0043 und ADR-0044. Ihre übrigen Entscheidungen
bleiben gültig. Der mutierende Controller aus ADR-0010 und sämtliche Berechtigungs-,
Evidence-, Source-Reader- und WebView-Grenzen werden nicht ersetzt.

**Freigabestatus:** Am 2026-09-05 durch Tim Bornemann ausdrücklich angenommen
(„ich stimme zu setze das um“). Implementierung und Abnahme werden in Plan 06 verfolgt.

## Kontext

Der gemeldete TaskFlow-Verlauf vom 2026-09-05 zeigt nach der vorigen Korrektur:

- einen ungültigen ersten Storage-Modellschritt einschließlich ausgeschöpfter Reparatur;
- nach manueller Fortsetzung eine Antwort nach drei sichtbaren Entscheidungen;
- wiederholte `/diagram`-Versuche mit nur wenigen sichtbaren Entscheidungen;
- Quellen für `manager.py:1–143`, anschließend `6–143` und `7–143` beziehungsweise
  `14–143` und `15–143`, obwohl das Modell den benötigten Abschnitt weiter vermisst;
- einen weiteren ungültigen Diagramm-Rechercheentscheid bereits in der ersten Runde.

Der Export enthält keine Rohantworten, Decoder-Unterursachen, tatsächlichen
Modellpakete oder verbrauchten Entscheidungszähler. Der konkrete historische
Syntax-/Validierungsfehler und eine einzelne exakte Budgetabrechnung sind deshalb
nicht rückwirkend beweisbar. Die folgenden Mechanismen sind dagegen im Code belegt:

1. `agent_research_context.rs::compile_evidence_window` verteilt gekürzte Ausschnitte
   auf mehrere Kandidaten. Kleine explizite Dateien erhalten Vorrang vor großen
   fokussierten Bereichen. Die Deduplizierung entfernt enthaltene Bereiche, aber
   nicht alle überlappenden großen Dateiende-Ausschnitte. Der ausgegebene
   Kontextcursor wird wieder zur Aufforderung für einen Dateiread.
2. `AskResearchWorkingSet` kennt gelesene Quellen und Dateicursor, aber keine
   eigenständige Abdeckung der tatsächlich an das Modell ausgelieferten Bereiche.
   `complete_files` sperrt in `research_followup::candidates` weitere Dateikandidaten,
   auch wenn der relevante Inhalt im Modellfenster abgeschnitten wurde. Ein anderer
   Startpunkt erhöht unter Umständen `evidence_revision`, ohne die fachlich benötigte
   Stelle sichtbar zu machen.
3. `/diagram` verwendet fest Standard: sechs Modellentscheidungen. Davon reserviert
   die Implementierung zwei für Formatierung/Repair. Es bleiben höchstens drei
   Entscheidungen mit neuen Reads und eine abschließende Rechercheentscheidung;
   interne Retries/Repairs verbrauchen weitere dieser Plätze, ohne jeweils eine
   eigene sichtbare Rechercherunde zu bilden. Das ist nicht gleichbedeutend mit
   zwölf tatsächlich ausgeschöpften Leseaktionen.
4. Strukturreparatur, Zitatkorrektur und Diagrammreparatur verbrauchen dieselbe
   einzelne `use_repair`-Befugnis. Ein früher korrigierter Fehler kann somit einen
   späteren, unabhängigen Fehler unreparierbar machen. Der Runtime-Collector fasst
   verschiedene Finish-Fehler als `InvalidOutput` zusammen; der Recherchevalidator
   fasst sämtliche Codecfehler als `Shape` zusammen. Wiederholungen erhalten dadurch
   keinen ausreichend spezifischen, überprüfbaren Reparaturauftrag.
5. Die Provider-Nachrichtenbudgetierung arbeitet rückwärts durch die Nachrichten.
   Ein hinten angehängter Repair-Hinweis kann bei engem Profil das vorhandene
   Evidence-Paket erneut kürzen. Quelle im Working Set bedeutet damit nicht
   Quelle im unveränderten Reparaturkontext.

Der bisherige Storage-Test verwendet einen sehr kleinen Manager und 8 KiB
Evidencebudget mit skriptierten Modellantworten. Er beweist den neuen Core-Follow-up,
aber nicht die Kontextversorgung eines 143-Zeilen-Managers mit Plugins und
Diagrammerzeugung. Eine größere Wiederholungszahl allein würde die Kürzungsschleife
nicht beheben.

## Entscheidung

### 1. Gelesene Evidence und ausgelieferte Bereiche getrennt führen

- Der flüchtige, revisionsgebundene Recherchekontext unterscheidet sicher gelesene
  Bereiche, aktuell ausgelieferte Bereiche und noch nicht ausgelieferte relevante
  Bereiche. Ein Kürzungsmarker besitzt eine konkrete Ursache: Dateiseite,
  Kontextfenster oder überlange Einzelzeile.
- Schon sicher gelesene Bytes werden innerhalb ihrer aktuellen Revision aus dem
  begrenzten Cache neu gepackt. Eine reine Kontextkürzung fordert keinen weiteren
  Dateisystemread an. Vor Verwendung gelten dieselben Freshness-/Secret-Prüfungen;
  ein Cachetreffer darf veränderte Dateien nicht zu aktueller Evidence erklären.
- Der aktuelle adaptive Fokus erhält einen zusammenhängenden brauchbaren Bereich.
  Andere explizite Dateien behalten kompakte Navigationsanker und werden bei Bedarf
  erneut fokussiert; allgemeine Treffer und alte Notizen verdrängen diesen Bereich
  nicht. Überlappungen derselben Revision werden nicht mehrfach als Source-Text
  übertragen. Der Gesamtpreis einschließlich Schema, Frage, Hinweisen, Reparatur
  und Outputreserve bleibt innerhalb des verifizierten Modellprofils.
- Funktionsnamen aus dem Auftrag beziehungsweise einer validierten Lücke werden
  nach Möglichkeit an aktuelle Fast-Index-Symbolbereiche gebunden. Das vorhandene
  Such-/Flow-Werkzeugset wird verwendet; kein zweiter Index entsteht.
- Eine vollständige Datei im Cache gilt nicht als vollständig an das Modell
  ausgeliefert. Umgekehrt ist ein bloßer neuer Source-Ordinal oder ein um eine Zeile
  verschobener überlappender Read kein neuer Belegfortschritt.

### 2. Ein endlicher Auftrag mit getrennten Budgets

| Harte Obergrenze pro Auftrag | Standard | Gründlich |
| --- | ---: | ---: |
| Recherche-Modellaufrufe einschließlich Retry und Repair | 12 | 24 |
| Ausgeführte neue adaptive Read-Aktionen | 24 | 48 |
| Struktur-/Zitat-Reparaturen innerhalb dieser Modellaufrufe | 3 | 6 |
| Transiente Modell-Retries innerhalb dieser Modellaufrufe | 2 | 4 |
| Diagramm-Modellaufrufe, falls angefordert, inklusive einer Fehlerwiederholung | 2 | 2 |
| Gesamtdauer einschließlich Formatierung | 5 Minuten | 15 Minuten |
| Source-Referenzen | 200 | 200 |
| Rein deterministische Recovery-Leserunden pro Auftrag | 1 | 1 |

- Die Obergrenzen sind Maxima, keine Zielwerte. Einfache Fragen bleiben bei einem
  erfolgreichen Modellaufruf. Kein Profil- oder Budgetwechsel erfolgt im laufenden
  Auftrag; kein automatischer Neustart erzeugt frische Budgets.
- `/diagram` besitzt dieselbe vollständige Recherchekapazität wie Standard-Ask.
  Formatierung zieht keine Plätze mehr vorab aus der Recherche ab. Insgesamt sind
  dadurch höchstens 14 beziehungsweise 26 Modellaufrufe möglich, stets innerhalb
  derselben Deadline. Die zwei Formatierungsplätze sind keine zusätzlichen Reads.
- Pro ungültigem Dokument bleibt genau **ein** Reparaturversuch erlaubt. Ein späterer
  unabhängiger Modellentscheid kann seine eigene Einzelreparatur nutzen, solange
  die abschnittsweiten Obergrenzen noch nicht erreicht sind. Ein ungültiger Repair
  wird nicht erneut repariert. Dies präzisiert die Einzelreparaturregel aus
  AGENTS.md und Architekturregeln Abschnitt 8; diese Regel wird nicht aufgehoben.
- Identische oder vollständig abgedeckte Leseanforderungen werden vor I/O erkannt
  und als Cache-/Fokusoperation behandelt. Sie verbrauchen keinen neuen Read, aber
  die anfordernde Modellentscheidung bleibt verbraucht. Neue Reads bleiben auf
  vier sequenzielle Aktionen je Runde begrenzt; die bisherigen Reader-, Such-,
  Byte- und Timeoutgrenzen gelten unverändert.
- Die bereits separat begrenzte Initialauswahl und deren maximale Quellenzahl
  bleiben erhalten. Sie darf weder bei Recovery noch vor Formatierung neu starten.
  Vorhandene Read-Retry-Grenzen bleiben zusätzlich bindend.

### 3. Fehler gezielt beheben, statt denselben Abschnitt neu anzufangen

- Ein versionierter, content-freier Fehlergrund unterscheidet mindestens ungültiges
  JSON, falsche Felder/Version, ungültige Werte, fehlende/unbekannte Quellen,
  Markerabweichung, geschlossene Leserunden und abgeschnittene Ausgabe. Nur diese
  geschlossenen Diagnosen, Phase und verbrauchte Zähler werden angezeigt/aufgezeichnet;
  Rohantworten, Providertexte, Prompts und Quelltext werden nicht persistiert.
- Der Repair erhält ein frisch budgetiertes Paket mit identischem Goal und den
  erforderlichen aktuellen Quellen. Sein Hinweis benennt den konkreten Fehler.
  Abgeschnittene Ausgabe verlangt einen kürzeren schema-konformen Inhalt. Der Core
  ergänzt keine erfundenen Werte, Quellen oder Aktionen aus ungültigen Rohbytes.
- Das Modellschema passt zur aktiven Phase: Rechercheentscheidung, Abschluss ohne
  weitere Reads oder Diagramm. Eine Diagrammanforderung wird nicht zugleich als
  Aufforderung verstanden, schon im Rechercheentscheid Diagramm-JSON zu liefern.
- Nach zwei echten Nullrunden beziehungsweise einem erfolglosen Einzelrepair darf
  der Core genau einmal seine bereits bekannte aktuelle Evidence-Frontier neu
  packen und, falls erforderlich, bis zu vier neue sichere Reads auswählen.
  Auswahlgrund sind ausschließlich bereits validierte Ziele und aktuelle
  Indexanker, niemals Aktionen aus der ungültigen Ausgabe. Reads und ein folgender
  Modellentscheid werden aus den unveränderten Restbudgets abgerechnet.
- Ohne neuen Zugang oder nach erfolgloser Recovery endet der Auftrag mit belegtem
  Zwischenstand und konkretem Blocker. Die Rückfrage verlangt nur eine echte
  fehlende Nutzerentscheidung; ein Ressourcenende bietet eine optionale bewusste
  Fortsetzung. Kein automatisches Weiterklicken, kein unendlicher Retry.
- Eine misslungene optionale Diagrammformatierung erhält die gültige Textantwort
  und Quellen. Sie startet die fachliche Recherche nicht erneut.

### 4. Unveränderte Sicherheitsgrenzen

Ask und Plan bleiben ausschließlich lesend. Agent-Mutationen benötigen weiterhin
den bestehenden Harness, exakte Freigaben, Worktree-Serialisierung und Verification.
Pfad-, Symlink-, Secret-, Binary-, Größen-, Stale-, Capability- und Providerablehnungen
sowie Cancellation sind keine reparierbaren Modellfehler. Recovery darf diese
Ablehnungen nicht umgehen. Es gibt keine neuen Netzwerk-, Shell-, Credential- oder
WebView-Rechte und keine Nutzung unvalidierter Modellaktionen.

## Konsequenzen

### Positiv

- Verfügbarer Code erreicht das Modell gezielt, ohne Dateianfang-/Einzeilenschleifen.
- Ein früher Reparaturbedarf verhindert nicht jede spätere unabhängige Korrektur.
- Diagramme kürzen die fachliche Recherche nicht künstlich auf wenige Runden.
- Recovery läuft ohne Routine-Nutzerklick innerhalb eines einzigen endlichen Auftrags.

### Negativ

- Schwierige Aufgaben können mehr Modellaufrufe und Kosten verursachen als bisher.
- Kontextabdeckung und Retry-Verträge benötigen zusätzliche Zustands- und Grenztests.
- Auch diese Grenzen garantieren keinen Erfolg beliebiger Modelle oder Aufgaben;
  echte fehlende Informationen und ausgeschöpfte Obergrenzen bleiben Haltepunkte.

### Risiken und Gegenmaßnahmen

- Höhere Kosten: sichtbares Profil, unveränderte Gesamtdauer und feste Aufrufmaxima.
- Scheinerfolg durch Cache: Revisionsbindung und getrennte Read-/Delivery-Abdeckung.
- Scheinfakten aus Repair: kein Rohoutput-Salvage und erneute vollständige Validierung.
- Endlosschleifen: Recovery genau einmal, sämtliche Aufrufe abgerechnet, Cancellation.

## Verworfene Alternativen

- Nur Limits erhöhen: beseitigt weder Kontextverdrängung noch ungeeignete Reparaturen.
- Nach jedem Fehler automatisch einen frischen Abschnitt starten: umgeht Gesamtbudgets.
- Beliebig viele Repairs oder tolerantes Ausführen ungültiger JSON-Fragmente: unsicher.
- Alle Quellen in jeden Prompt kopieren: unbrauchbar für kleine lokale Modelle.
- Allein die vorige kleine Storage-Fixture als Abnahme: deckt den gemeldeten Fall nicht ab.

## Compliance

- Vorher-/Nachher-Regression mit 143-Zeilen-Manager, Funktionen hinter langen
  Initialisierungsbereichen, Plugin-Dispatcher, Audit-Plugin und Storage-Konfiguration.
  Der reale Parser-/Index-/libSQL-/Safe-Reader-/Scheduler-/Kontextpfad wird verwendet.
- Kontextfenster von 1, 2, 4 und 8 KiB sowie UTF-8, überlange Einzelzeilen und mehrere
  überlappende Ranges: harte Bytegrenze, ehrliche sichtbare Bereiche, Fokusfortschritt,
  kein `6 → 7 → 8`-Dateiread nur aufgrund bereits gecachter Kontextkürzung.
- Ask, Plan und Agent-Vorbereitung bearbeiten die gleiche Mehrdatei-Beweiskette;
  `/diagram` hat dieselbe Recherchekapazität und liefert source-gebundene Artefakte.
- Skriptierte Fehler folgen auf gültige Schritte und auf andere bereits reparierte
  Schritte. Zwei Fehler desselben Dokuments, globale Limits, transiente Fehler,
  Stagnation und die einzige Recovery werden separat geprüft.
- Produktionsnahe Provider-Stubs prüfen tatsächlich versendete Pakete und
  Phasenschemas. Es reicht nicht, direkt eine fertige valide Antwort zu injizieren.
- Null mutierende Tools, kein unvalidierter Aktions-Salvage, Abbruch während Repair
  und Recovery, Live-Dateiänderung, Symlink-Escape und unveränderte Freigaben.
- Ein Live-Modellvergleich braucht eigene ausdrückliche Provider-/Netzwerkfreigabe;
  Offline-Erfolg wird nicht als gemessene Live-Modell-Erfolgsquote ausgegeben.
- Rust-/Frontend-/IPC-Gates nach Änderungsumfang. Der bekannte unabhängige
  Start/Pause-Testfehler bleibt als separater Abnahmeblocker sichtbar, solange er
  nicht mit einem eigenen Synchronisationsnachweis behoben ist.

## Referenzen

- [ADR-0038](0038-agentische-mehr-runden-recherche.md)
- [ADR-0039](0039-evidenzgebundene-slash-commands.md)
- [ADR-0043](0043-core-gesteuerte-evidence-vertiefung-und-retry.md)
- [ADR-0044](0044-priorisierter-recherchekontext-und-zielauflösung.md)
- [Architekturregeln](../ARCHITECTURE_RULES.md)
- [Memory und Context](../MEMORY_AND_CONTEXT.md)
- [Qualitätsgates](../QUALITY_GATES.md)
- [Plan 06](../plans/06-DESKTOP_PRODUCT.md)
