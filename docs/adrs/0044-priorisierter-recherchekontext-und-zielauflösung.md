# ADR-0044: Priorisierter Recherchekontext und deterministische Zielauflösung

Status: Accepted

Datum: 2026-09-05

Entscheider: Tim Bornemann

Ergänzt: ADR-0038 und ADR-0043. Deren feste Modell-, Aktions-, Zeit-, Retry-, Evidence- und
Sicherheitsgrenzen bleiben unverändert.

## Kontext

Die Core-gesteuerte Evidence-Vertiefung konnte ausdrücklich genannte Dateien zwar gegen den
aktuellen Index auflösen, baute den turnlokalen Modellkontext aber in einer ungünstigen Reihenfolge
auf: Zuerst wurden Quellen und öffentliche Befunde des vorherigen Conversation-Turns revalidiert.
Erst danach folgten aktuelle explizite Dateien und die Task Lens. Bei kleinen Kontextfenstern
belegten dadurch thematisch überholte, aber weiterhin frische Quellen den verfügbaren Evidence-
Bereich. Neu gefundene aktuelle Quellen wurden korrekt persistiert, erreichten das Modell jedoch
nicht mehr mit einem brauchbaren Ausschnitt.

Das Modell reagierte darauf mit wiederholten `searchIndex`- und `searchSourceText`-Aktionen für
bereits im Nutzerauftrag benannte Dateien. Da diese Runden keine neue in den Kontext aufnehmbare
Evidence erzeugten, beendete der Stagnationsschutz den Abschnitt nach zwei Nullrunden. Das feste
Gesamtprofil war nicht ausgeschöpft; die Recherche hatte ihren verfügbaren Kontext falsch
priorisiert.

Eine pauschale Erhöhung der Runden- oder Tokenbudgets löst dieses Problem nicht. Sie verlängert nur
die gleiche Suchschleife und benachteiligt kleine lokale Modelle.

## Entscheidung

- Der Core erstellt zu Beginn jedes Rechercheabschnitts eine deterministische Liste ausdrücklich
  genannter Repositoryziele. Jeder Kandidat wird gegen genau den gepinnten Published Index als
  eindeutiger Pfad beziehungsweise eindeutiges Pfadsuffix aufgelöst. Nicht oder mehrdeutig
  auflösbare Kandidaten bleiben als solche sichtbar, ohne interne IDs offenzulegen.
- Der turnlokale Evidence-Kontext wird in dieser Reihenfolge aufgebaut:
  1. aktuelle sicher lesbare explizite Ziele;
  2. aktuelle, auf höchstens zwölf Source-Ziele begrenzte Task-Lens-Auswahl;
  3. höchstens acht revalidierte Quellen des vorherigen Turns als nachrangiger Kontext.
- Enthält der aktuelle Auftrag explizite Repositoryziele, werden historische Quellen und Befunde
  nicht pauschal übernommen. Die expliziten Ziele werden aus dem aktuellen Index erneut gelesen.
  Damit kann ein vorheriger, thematisch anderer Turn das aktuelle Evidence-Fenster nicht belegen.
- Vor jeder Modellentscheidung enthält der Kontext eine Core-erzeugte Zielprojektion mit dem
  angefragten Namen, dem aufgelösten Repositorypfad und – falls erfolgreich gelesen – dem
  turnlokalen öffentlichen `S`-Label. Diese Projektion ist Navigationsmetadatum, keine zusätzliche
  fachliche Evidence.
- `searchIndex` löst in seiner Suchfrage enthaltene Dateiziele zuerst deterministisch auf und liest
  sie sicher, bevor die Task Lens neu kompiliert wird. Kleine Modelle müssen daher nicht die
  korrekte Wahl von `inspectPath` treffen, wenn sie bereits einen eindeutigen Dateinamen nennen.
- Nach einer Aktionsrunde ohne neue Evidence erhält die nächste Entscheidung einen Core-Hinweis,
  den Zugang zu wechseln und bekannte Pfade, Source-Referenzen oder Beziehungen direkt zu prüfen.
  Zwei aufeinanderfolgende echte Nullrunden bleiben gemäß ADR-0038 terminal. Runden mit neuer
  Evidence dürfen das vollständige feste Profil ausschöpfen.
- Öffentliche Arbeitsnotizen bleiben vollständig im Verlauf erhalten. Der kompakte turnlokale
  Modellcheckpoint dedupliziert identische Lücken und begrenzt ältere Arbeitshinweise, damit die
  aktuelle Evidence nicht durch wiederholte Formulierungen verdrängt wird.
- Die Priorisierung ist flüchtige Context-Compilation. Sie persistiert weder Quelltext noch neue
  Tabellen und erweitert keine WebView-, Datei-, Prozess-, Netzwerk- oder Mutationsberechtigung.

## Konsequenzen

### Positiv

- Konkrete Fragen zu benannten Dateien beginnen mit den tatsächlich angefragten Quellen statt mit
  zufällig frischen Quellen eines älteren Themas.
- Mehr-Runden-Recherche verwendet ihr Budget für eine fortschreitende Beweiskette statt für
  Varianten derselben erfolglosen Indexsuche.
- Ask, Plan und Agent-Vorbereitung erhalten dieselbe Verbesserung, weil sie denselben
  Recherche-Use-Case verwenden.
- Kleine Kontextfenster profitieren ohne höhere Providerkosten oder gelockerte Sicherheitsgrenzen.

### Negativ

- Bei einem Auftrag mit expliziten Dateizielen wird nicht automatisch der gesamte vorherige
  Quellenbestand reinjiziert. Weiterhin benötigte Dateien müssen durch aktuelle Ziele, Task Lens
  oder eine neue Rechercheaktion erneut ausgewählt werden.
- Die anfängliche Task-Lens-Quellenmenge sinkt von 32 auf zwölf tatsächlich gelesene Ziele; der
  vollständige begrenzte Auswahltrace bleibt als Metadatum sichtbar.

### Risiken und Gegenmaßnahmen

- Ein Dateiname kann mehrdeutig sein. Der Core rät nicht, sondern kennzeichnet das Ziel als nicht
  eindeutig und erlaubt anschließend `listDirectory`, `searchIndex` oder einen genaueren Pfad.
- Historische Evidence könnte für ein Follow-up fehlen. Aufträge ohne explizites Dateiziel dürfen
  weiterhin bis zu acht revalidierte Quellen übernehmen; aktuelle Task-Lens-Evidence bleibt
  vorrangig.
- Wiederholte Suchvarianten könnten weiterhin Ressourcen verbrauchen. Aktionsdeduplizierung,
  Nullrundenstopp sowie Modell-, Aktions- und Zeitgrenzen bleiben unverändert.

## Compliance

- Regressionstests prüfen Dateikandidaten mit Satzzeichen und eindeutige Pfadsuffixe.
- Controllernahe Tests prüfen, dass explizite Ziele historischen Kontext verdrängen, Task-Lens-
  und Reuse-Grenzen eingehalten werden und ein Search-Index-Dateiziel direkt lesbar wird.
- Prompttests prüfen die Core-Zielprojektion und den Wechsel des Suchzugangs nach einer Nullrunde.
- Workspace-Tests, Clippy, Rustdoc und die bestehenden Security-Regressions bleiben Abschlussgates.

## Referenzen

- [ADR-0030](0030-bounded-evidence-source-preview.md)
- [ADR-0037](0037-nachvollziehbare-adaptive-ask-recherche.md)
- [ADR-0038](0038-agentische-mehr-runden-recherche.md)
- [ADR-0043](0043-core-gesteuerte-evidence-vertiefung-und-retry.md)
- [Memory und Context](../MEMORY_AND_CONTEXT.md)
- [Job-Laufzeit](../JOB_RUNTIME.md)
- [Plan 06](../plans/06-DESKTOP_PRODUCT.md)
