# ADR-0017: Begrenzte deterministische Repository-Discovery

Status: Accepted
Datum: 2026-08-04
Entscheider: Tim Bornemann

## Kontext

Der Fast Index benötigt vor Hashing und Parsing eine reproduzierbare Liste relevanter Dateien.
Dabei treffen Git-Semantik, Repository-spezifische A^3-Konfiguration und nicht übersteuerbare
Sicherheitsgrenzen aufeinander. Insbesondere darf ein öffentliches Repository A^3 nicht zum Lesen
bekannter Secret-Pfade, Binärdateien, Vendor-Bäume oder beliebig großer Dateien veranlassen.

Git kann zusätzlich benutzerweite Excludes außerhalb des gewählten Worktrees konfigurieren. Diese
Dateien sind weder Teil des reproduzierbaren Repository-Zustands noch durch die beim Öffnen erteilte
Pfadautorität gedeckt. Ein automatisches Lesen würde daher der isolierten Repository-Inspektion und
der Regel widersprechen, Zugriffe außerhalb freigegebener Roots ausdrücklich genehmigen zu lassen.

## Entscheidung

- Discovery V1 vereinigt genau drei Quellen:
  - alle im Git-Index geführten, im Worktree vorhandenen regulären Dateien;
  - relevante untracked Dateien, wobei Git-kompatible Repository-Regeln aus `.gitignore` und
    `.git/info/exclude` gelten;
  - ausschließende A^3-Projektmuster aus `[discovery].ignore` in `.a3/project.toml`.
- „Globale Ignore-Regeln“ bezeichnet für A^3 eine versionierte, plattformunabhängige Menge sicherer
  Defaults. Benutzerweite Git-Konfiguration und `core.excludesFile` außerhalb des Worktrees werden
  nicht automatisch gelesen. Eine spätere opt-in Freigabe benötigt eine eigene Entscheidung.
- A^3-Projektmuster verwenden Gitignore-Mustersyntax, sind aber ausschließlich ausschließend.
  Negationsmuster sind ungültig. Sie können weder eingebaute Sicherheitsregeln noch Git-Ignores
  aufheben.
- Git-Ignores unterdrücken untracked Dateien. Tracked Dateien bleiben grundsätzlich Kandidaten,
  werden aber weiterhin durch A^3-Projektmuster und die Sicherheitsklassifikation ausgeschlossen.
- Bekannte Secret-Pfade und hochsichere Credential-Signaturen, Binärdateien, Symlinks und andere
  Spezialdateien, Vendor- und Generated-Bäume sowie Dateien oberhalb des festen Größenlimits gelangen
  nie in ein `DiscoveryResult`.
- Discovery liest pro Kandidat höchstens ein festes Präfix für Binär- und Secret-Erkennung. Dateien
  oberhalb des Limits werden allein anhand von Metadaten ausgeschlossen und nicht geöffnet.
- Ergebnis und Rollenklassifikation sind versioniert. Manifest-, Build-, Test- und CI-Rollen dürfen
  überlappen. Kandidaten werden anhand der verlustfreien Git-Pfadbytes eindeutig zusammengeführt und
  byteweise deterministisch sortiert.
- Kandidatenanzahl, Konfigurationsgröße und Präfixlesegröße sind fest begrenzt. Der Adapter prüft
  kooperative Cancellation und meldet Enumeration und Klassifikation als Fortschritt. Bei
  Grenzwertüberschreitung, ungültiger Konfiguration oder Pfad-/Dateisysteminkonsistenz wird der Lauf
  kontrolliert und ohne partielles Ergebnis beendet.
- Der Application-Port transportiert ausschließlich Domain-Typen und stabile Fehlerklassen. Git-,
  Dateisystem- und TOML-Typen bleiben im lokalen Adapter. Discovery persistiert und veröffentlicht
  noch keinen Snapshot; das bleibt den nachfolgenden Planabschnitten vorbehalten.
- Die vorhandene `gix`-Abhängigkeit erhält ihre `dirwalk`-Funktion für korrekte Index- und
  Gitignore-Semantik. `toml` wird als direkte, funktionsreduzierte Parser-Abhängigkeit geführt, weil
  ein eigener Teilparser die öffentliche Projektkonfiguration nicht vollständig und sicher
  validieren könnte. `rustix` wird für `O_NOFOLLOW` auf Unix wiederverwendet.

## Konsequenzen

### Positiv

- Derselbe Git- und Worktree-Zustand erzeugt unabhängig von Benutzerkonfiguration eine identische,
  sortierte Kandidatenmenge.
- Repository-Inhalte können Sicherheitsdefaults nicht durch Negation umgehen.
- Große oder offensichtlich ungeeignete Dateien verbrauchen weder vollständige Lesezeit noch
  Modellkontext.
- Tracked und relevante untracked Dateien werden mit nachvollziehbarer Git-Semantik behandelt.

### Negativ

- Eine persönliche globale Gitignore-Datei beeinflusst A^3 V1 nicht automatisch.
- Ausschlussmuster in `.a3/project.toml` sind absichtlich weniger ausdrucksstark als eine vollständige
  Gitignore-Datei, weil sie keine Re-Includes erlauben.
- Präfixbasierte Secret-Erkennung kann nur hochsichere Signaturen erkennen und ersetzt keinen späteren
  vollständigen Secret-Scanner.

### Risiken und Gegenmaßnahmen

- Umbenannte oder neuartige Secret-Datei — konservative Pfaddefaults, Signaturtests und spätere
  versionierte Erweiterung der Policy.
- TOCTOU zwischen Enumeration und Lesen — kanonischen Root beibehalten, Symlinks nicht verfolgen,
  Metadaten direkt vor dem begrenzten Lesen erneut prüfen und bei Inkonsistenz abbrechen.
- Sehr viele kleine Dateien — feste Kandidatengrenze, Cancellation und Fortschritt.
- Unterschiedliche Plattformpfade — Repository-Pfade verlustfrei in Git-Form speichern und nur in
  einem dedizierten Plattformadapter in Betriebssystempfade umwandeln.

## Verworfene Alternativen

- `git` per Shell starten — verletzt argv-/Adapterdisziplin und macht Fehler- und Pfadsemantik
  unnötig plattformabhängig.
- Benutzerweite Git-Konfiguration automatisch laden — nicht reproduzierbar und außerhalb der
  ausgewählten Pfadautorität.
- Eine neue `.a3ignore`-Datei — widerspricht ADR-0005, das im Repository nur `project.toml` und
  `rules.md` für A^3 zulässt.
- Erst beim Parser filtern — liest Geheimnisse und große Binärdaten bereits über die notwendige
  Trust Boundary.
- Vollständige Inhaltsprüfung aller Dateien — unbeschränkt und für Discovery nicht erforderlich.

## Compliance

- Golden- und Contract-Tests prüfen tracked/untracked, Git- und A^3-Ignores, nicht übersteuerbare
  Sicherheitsdefaults, Rollen, deterministische Reihenfolge und Cancellation.
- Ein instrumentierter Reader belegt, dass übergroße Dateien nicht geöffnet und normale Kandidaten
  nur bis zur festen Präfixgrenze gelesen werden.
- Domain und Application enthalten keine `gix`-, TOML- oder Betriebssystemtypen.
- CI führt Formatierung, Workspace-Tests, Clippy mit verweigerten Warnungen und Dokumentationstests aus.

## Referenzen

- [ADR-0005](0005-worktree-scoped-storage.md)
- [ADR-0006](0006-deterministic-index-before-llm.md)
- [Security and Execution](../SECURITY_AND_EXECUTION.md)
- [Indexing and Project Map](../INDEXING_AND_PROJECT_MAP.md)
- [Plan 02](../plans/02-STORAGE_AND_FAST_INDEX.md)
