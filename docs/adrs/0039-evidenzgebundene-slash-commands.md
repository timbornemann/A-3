# ADR-0039: Evidenzgebundene Slash Commands

Status: Accepted

Datum: 2026-09-04

Entscheider: Tim Bornemann

Ergänzt: ADR-0033 und ADR-0038. Die Berechtigungs-, Controller- und Evidence-Grenzen aus
ADR-0010, ADR-0012 und ADR-0038 bleiben unverändert maßgeblich.

## Kontext

Ask, Plan und Agent besitzen seit ADR-0038 denselben endlichen Recherchecontroller. Nutzer können
ihre Absicht bisher jedoch nur als freien Text formulieren. Wiederkehrende Arbeiten wie Review,
Impact-Analyse, Dokumentation oder Diagrammerstellung benötigen dadurch uneinheitliche Hinweise,
obwohl Fokus, Ergebnisform und Verification vorhersehbar sind. Frei editierbare Prompt-Makros
wären keine sichere Lösung: Sie könnten wie privilegierte Systemanweisungen wirken, den
Modusunterschied verwischen und bei kleinen lokalen Modellen unnötig Kontext verbrauchen.

Diagramme benötigen außerdem eine eigene Vertrauensgrenze. Rohe Modell-Mermaid könnte HTML,
Links, Direktiven oder Click-Aktionen enthalten. Ein gewöhnlicher WebView-Dateidownload würde der
unprivilegierten Oberfläche zudem einen Zielpfad oder eine allgemeine Schreibfähigkeit geben.

## Entscheidung

- A^3 besitzt einen Core-eigenen, fest versionierten Katalog aus `/diagram`, `/explain`, `/trace`,
  `/todos`, `/impact`, `/review`, `/debug`, `/doc`, `/refactor` und `/test` sowie den Linsen
  `/security`, `/performance` und `/architecture`. Namen sind englisch und kleingeschrieben.
- Eine Nachricht enthält höchstens einen Hauptauftrag und zwei unterschiedliche Linsen. Eine allein
  verwendete Linse impliziert `/review`; `//` am Nachrichtenanfang maskiert den Slash. Der
  Rust-Core validiert Syntax, Argumentanforderung und Modus vor Sessionappend und Jobstart.
- Jeder Katalogeintrag legt Modusmatrix, feste Rechercheintensität, Verhalten bei leerem Thema,
  Recherchefokus, Ergebnis- und Verifikationsprofil fest. Das resultierende
  `CommandExecutionProfile` wird typisiert weitergereicht. Weder Slash-Text noch Repositoryinhalt
  werden als privilegierte Systemanweisung übernommen.
- `/doc`, `/refactor`, `/debug` und `/test` ohne Ziel wechseln ohne Modell- oder Toolaufruf in
  einen Core-formulierten Rückfragezustand. Die unmittelbar folgende normale Nutzerantwort wird
  als Ziel desselben Commands mit denselben Linsen erneut Core-validiert und atomar persistiert;
  ein ausdrücklich neuer Slash-Aufruf ersetzt diese Fortsetzung.
- Commands verleihen keine Capability. Ask und Plan bleiben Read-only. Der Agent bleibt an Goal,
  Ledger, genau eine mutierende Aktion pro Controllerturn, zentrale Policy, Freigabe, Reindex und
  Verification gebunden. Nur bereits manifestbelegte und bestätigte Test-, Build-, Lint- und
  Format-Kommandos dürfen mit direktem argv unter den bestehenden Grenzen laufen.
- Im Agent-Modus muss jeder bestätigte, eigenständig änderbare Command-Fund als eigener
  Top-Level-Punkt unter `Implementation Changes` stehen. Der Core materialisiert daraus eine
  sequenzielle Ledger-Kette mit eigener Verification-Spezifikation je Schritt. Erst nach
  erfolgreicher Verification wird der nächste Schritt atomar im selben kontrollierten Run
  gestartet; Hypothesen und nicht änderbare Beobachtungen dürfen diese Grenze nicht passieren.
- Der geschlossene Recherchevertrag ergänzt `inspectWorkingChanges`, `queryIndexDiagnostics`,
  `inspectDependencyGraph`, `inspectTestTopology` und `scanSecurityCandidates`. Sie sind an
  Worktree und veröffentlichten Index gebunden, begrenzt und zählen gegen ADR-0038. Der feste
  Working-Changes-Adapter darf ausschließlich lokale Git-Metadaten mit einem Core-eigenen argv
  lesen; er ist keine freie Prozess- oder Shellfähigkeit. Security-Regeln erzeugen Kandidaten, die
  erst durch aktuelle Source-Inspektion Evidence werden.
- Das aufgelöste Profil legt eine kleine deduplizierte Mindestmenge dieser Reads fest, bevor das
  Modell weitere Aktionen wählen darf: insbesondere Working Changes für `/impact`, Diagnosen und
  Testtopologie für `/review` sowie Security- beziehungsweise Dependency-Reads für die passenden
  Linsen. Auch diese Core-erzwungenen Reads verbrauchen das unveränderte Aktionsbudget.
- `/diagram` reserviert innerhalb des bestehenden Budgets eine letzte Modellentscheidung für ein
  streng validiertes `EvidenceDiagramV1`. Elemente und Relationen benötigen turnlokale Quellen.
  Der Core prüft Größen, Topologie und Referenzen und kompiliert daraus deterministisch Mermaid.
  Modell-Mermaid, HTML, Frontmatter, Links, Direktiven und Click-Aktionen sind unzulässig.
- Die Oberfläche lädt Mermaid lokal und nur bei Bedarf, verwendet `securityLevel: strict` und
  sanitisiert das resultierende SVG zusätzlich. Exportformate sind SVG und PNG in Hell, Dunkel
  oder Transparent. Ein enger Rust-Adapter validiert Nutzdaten und Größe, öffnet den nativen
  Speicherdialog und schreibt atomar; Requests enthalten keinen Zielpfad.
- Knowledge-Schema V32 speichert append-only Command-Aufrufe, Linsen, Diagrammartefakte und ihre
  Source-Verknüpfungen. Antwort, Zitate und Diagramme schließen atomar ab. Presentation Delete
  entfernt diese Daten; Archivieren erhält sie. Gespeichert werden weder Quelltext noch Prompts,
  Modellrohantworten, Chain-of-Thought, Providerdaten oder Zugangsdaten.
- `submit_agent_message_v3` unterscheidet `standard | thorough | command`; `command` ist nur mit
  einem validierten Slash-Aufruf zulässig. Projektgebundene Katalog- und Diagramm-Reads geben
  ausschließlich geschlossene Daten und opake, sessiongebundene Referenzen aus. Ältere Submit-
  Verträge bleiben kompatibel.

## Konsequenzen

### Positiv

- Wiederkehrende Aufgaben erhalten vorhersehbaren Recherchefokus, Ausgabe und Verification, ohne
  neue Berechtigungen oder freie Prompt-Makros.
- Die Palette verhindert viele ungültige Kombinationen bereits bei der Eingabe; der Core bleibt
  dennoch alleinige Autorität.
- Diagramme sind evidence-gebundene, reproduzierbare Präsentationsartefakte und kein ausführbarer
  Modellinhalt.
- Schwierige Analysen können aktuelle Änderungen, Indexdiagnosen, Abhängigkeiten, Testtopologie
  und lokale Sicherheitskandidaten untersuchen, ohne Ask oder Plan Prozessautorität zu geben.

### Negativ

- Katalog, Parser, Rechercheprofile, Persistenz, IPC und Oberfläche müssen gemeinsam versioniert
  und getestet werden.
- Mermaid vergrößert das installierte Frontend, bleibt aber durch Lazy Loading aus dem initialen
  Arbeitsbereich entfernt. Rust benötigt eine kleine Base64-Abhängigkeit, weil die
  Standardbibliothek keinen Decoder für den begrenzten PNG-Datenvertrag bereitstellt.
- Diagramme können bei unvollständiger Evidence absichtlich weniger Details zeigen als eine freie
  Zeichnung.

### Risiken und Gegenmaßnahmen

- Prompt-Injection durch Slash- oder Repositorytext wird durch Core-Parsing, typisierte Profile
  und die Trennung von Systemconstraint und Nutzerinhalt begrenzt.
- Schädliches SVG wird durch Core-erzeugtes Mermaid, Mermaid Strict Mode, zusätzliche
  SVG-Sanitisierung und erneute native Exportvalidierung abgewehrt.
- Fremde Artefakte oder Zielpfade werden durch worktree-/session-/turngebundene opake Referenzen
  und den nativen Dialog ausgeschlossen.
- Falsche Vollständigkeit wird durch sichtbare Trunkierung, feste Budgets und
  `AwaitingContinuation` verhindert.

## Verworfene Alternativen

- Nutzerdefinierte Prompt-Makros: nicht zuverlässig typisierbar und zu nah an einer privilegierten
  Prompt-Erweiterung.
- Beliebig kombinierbare Commands: erzeugen widersprüchliche Ergebnis- und Berechtigungsprofile.
- Rohe Mermaid-Ausgabe des Modells: erweitert die WebView-Angriffsfläche und ist nicht
  deterministisch evidence-gebunden.
- Export über einen von der WebView gelieferten Pfad: verletzt die unprivilegierte
  WebView-Grenze.
- Externe CVE-Abfragen in `/security`: führen eine neue Netzwerk- und Freshness-Grenze ein und
  bleiben außerhalb von V1.

## Compliance

- Domain- und IPC-Tests prüfen Syntax, Modusmatrix, Escape, Linsen, feste Tiefe und ungültige
  Kombinationen vor jeder Persistenz.
- Controller- und Action-Tests prüfen neue Aktionen, Grenzen, Deduplizierung und das reservierte
  Diagrammbudget.
- Persistenztests prüfen Neuinstallation, V31→V32, atomaren Command-/Diagrammabschluss und
  Presentation Delete.
- Diagrammtests prüfen Source-Bindung, Topologie, Mermaid-Kompilierung, SVG-/PNG-Grenzen,
  Sanitizer, native Pfadwahl und path-freie Responses.
- Frontendtests prüfen Palette, Tastatursteuerung, Chips, feste Tiefe, Lazy Rendering,
  Fehlerwiederholung und Export.

## Referenzen

- [ADR-0010](0010-single-controller-state-machine.md)
- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0025](0025-bounded-desktop-rendering-and-lifecycle.md)
- [ADR-0033](0033-chatbasierter-agent-workspace.md)
- [ADR-0038](0038-agentische-mehr-runden-recherche.md)
- [Mermaid Security Level](https://mermaid.js.org/config/schema-docs/config-properties-securitylevel.html)
- [Mermaid Usage](https://mermaid.js.org/config/usage.html)
- [Tauri Dialog Plugin](https://v2.tauri.app/plugin/dialog/)
- [Daten und Persistenz](../DATA_AND_PERSISTENCE.md)
- [IPC-Protokoll](../IPC_PROTOCOL.md)
- [Security und Execution](../SECURITY_AND_EXECUTION.md)
- [Plan 06](../plans/06-DESKTOP_PRODUCT.md)
