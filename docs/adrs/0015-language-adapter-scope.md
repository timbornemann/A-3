# ADR-0015: Initialer Sprachumfang über LanguageAdapter

Status: Accepted  
Datum: 2026-08-03

## Kontext

Hochwertige Symbol-, Import-, Call- und Testbeziehungen erfordern sprachspezifische Arbeit. Sofortige Gleichbehandlung aller Sprachen würde zu oberflächlicher Qualität führen. A^3 selbst nutzt Rust und TypeScript und kann sich damit früh selbst testen.

## Entscheidung

Struktureller V1-Support:

- Rust
- TypeScript und JavaScript
- Python

Alle anderen textuellen Sprachen erhalten generischen Datei-, Manifest- und Suchsupport.

Ein LanguageAdapter liefert versioniert:

- Dateierkennung
- Symbol- und Signaturausgabe
- Imports und Exports
- syntaktische Beziehungen
- Test- und Einstiegspunkterkennung
- Parse Diagnostics
- Coverage

Tree-sitter ist der Baselineparser. SCIP oder LSP dürfen später als zusätzliche Edge Provider ergänzt werden.

## Konsequenzen

### Positiv

- tiefe Qualität für einen überschaubaren Scope
- A^3 kann Rust- und TypeScript-Funktionen an der eigenen Codebasis erproben.
- neue Sprachen erhalten klaren Contract.

### Negativ

- Nutzer anderer Sprachen erhalten zunächst weniger Struktur.
- Python-Dynamik begrenzt deterministische Call-Auflösung.

### Risiken und Gegenmaßnahmen

- Parserdrift — Grammar-Version im Snapshot und Golden Fixtures.
- falsche Vollständigkeit — Coverage und Confidence sichtbar machen.

## Verworfene Alternativen

- alle Tree-sitter-Grammatiken sofort — Breite ohne Tests und Semantik.
- nur LSP — Serverinstallation und projektabhängige Laufzeit.
- nur Regex — unzureichende Strukturqualität.

## Compliance

Jeder strukturelle Adapter besteht dieselbe Contract-Suite und besitzt lizenzkompatible Golden Fixtures.

