# Zusammenhängende Recherche: Regression vom 2026-09-06

## Befund

Der bereitgestellte Verlauf `4b84ba69-9089-4ed9-bfd7-a52eaaca7192` enthält eine
erfolgreiche Storage-Antwort und mehrere erfolglose `/diagram`-Versuche. Drei Dateien
sind bereits gelesen; trotzdem wechseln die sichtbaren Belege zwischen `add_task`,
Dispatcher, Callback, Pfadinitialisierung und Writer. Versuche enden mit Stagnation
oder ausgeschöpften Entscheidungen, bei **0/24 neuen Reads**.

Stand `001754f`: höchstens ein Fokus und Ausschnitt je Dateirevision; Symbolfokus
begrenzt nur den Anfang, nicht das Ende. Ganze Dateisuffixe konkurrieren um kleine
Textanteile. Eine neue Lücke verdrängt vorher benötigte Methoden. Eine vollständig
gelesene Datei oder über frühere Pakete akkumulierte Abdeckung beweist deshalb nicht,
dass der aktuelle Modellkontext zur Synthese ausreicht.

Frühere Tests verfehlten diesen Fall: Die progressive Fixture besitzt ein
Evidence-Gedächtnis im Testmodell; die stateless Plan-Fixture verlangt nur kurze
API-Anfänge in verschiedenen Dateien, keine vollständige Mehrmethodenkette.

Die historischen `research-v1/shape`-Fehler sind ohne abgelehnte Rohdokumente und
Modellpakete nicht genauer rekonstruierbar. Der Code fasste falsche JSON-Typen und
ungültige Response-Streams unter diesem gemeinsamen Fehler zusammen.

## Korrektur innerhalb ADR-0046

- Maximal 32 revisionsgebundene Funktionsintervalle aus dem vorhandenen Fast Index
  bleiben flüchtig ausgewählt. Validierte Hinweise ergänzen sie; bei expliziten
  Leseaktionen wird nur in deren Dateien verfeinert. Kein neuer Index oder Claim.
- Maximal acht disjunkte Ausschnitte dürfen mehrere Methoden derselben Datei
  gleichzeitig liefern. Überlappungen werden vereinigt; nur sicher gelesener
  Originaltext wird verwendet. Passen die Funktionen mit den aktiven Lesezielen,
  werden ihre vollständigen Kosten vor Hintergrundtreffern reserviert.
- Die umschließenden Klassendeklarationszeilen bleiben als Originalevidence sichtbar;
  tatsächliche kurze Leerzeilen dürfen angrenzende Bereiche verbinden. Bei Überlauf
  erhält die aktive Stelle Vorrang. Sonst konnten Kopfzeilen und Kürzungsmarker
  alle Textquoten aufbrauchen und trotz gefülltem Cache ein leeres Paket erzeugen.
  Symbolverfeinerung setzt einen teilweise gelesenen Rumpf nicht wieder zurück.
- Neue explizite Stellen und die einmalige Recovery bleiben erreichbar, auch in
  derselben Datei. Wiederholungen erhöhen weder Read- noch Delivery-Abdeckung.
- Objekt-, Array-, String-Typfehler und Streamfehler erhalten getrennte, content-freie
  Diagnosen und passende Repair-Hinweise. Strikte Validierung, Einzelrepair,
  Freshness, Cancellation, Freigaben und sämtliche Gesamtbudgets bleiben bestehen.

## Reproduktion und Messung

Der [Recherchetest](../../apps/desktop/src-tauri/src/agent_research_coherent_tests.rs)
erzeugt eine synthetische verschärfte Fixture, keine Kopie des privaten TaskFlow.
Jeweils 100 irrelevante Zeilen trennen Methoden; abstrakter und konkreter Callback
heißen gleich. Das Modell fordert zuerst Aufrufer, Dispatcher, Callback und Writer
an, anschließend nur den Konstruktor. Ohne fünf **gleichzeitig vollständig sichtbare**
Methodenkörper darf es nicht antworten. Es besitzt kein Evidence-Gedächtnis.

Parser, Index-Refresh, libSQL, Safe Reader, Scheduler und Controller sind real;
nur Modellausgaben sind deterministisch. Repositorydateien müssen bytegleich bleiben.
Ask, Plan, Agent-Vorbereitung und `/diagram` werden geprüft; Diagramme müssen
dieselben Originalbelege erhalten.

```powershell
$env:RUST_TEST_NOCAPTURE='1'
cargo test -p a3-desktop --lib research_keeps_complete_call_chain -- --nocapture
cargo test -p a3-desktop --lib research_ -- --nocapture
```

Vorher (`001754f`, neuer Test vor Produktionsänderung): bei 4096 Bytes Abbruch nach
fünf Entscheidungen, **0/5 vollständige Methoden im fünften Paket**. Nachher: bei
2048, 4096 und 8192 Bytes **5/5 vollständige Methoden**, Antwort nach drei
Rechercheaufrufen. `/diagram` nutzt anschließend einen separaten Formatierungsaufruf.
Das ist ein deterministischer Konvergenznachweis, keine Live-Modell-Erfolgsquote
oder gemessene Latenzverbesserung.

Zusätzliche Grenztests prüfen 512–8192 Bytes, UTF-8, Überlappungen, neue exakte Stellen,
unveränderte Read-Zähler und JSON-Typdiagnosen. Bestehende Dateiänderungs-, Cancellation-,
Repair-, Stagnations- und Providerpaket-Verträge gehören weiterhin zum Gate.

## Grenzen

Verifikation der finalen Fassung:

```powershell
cargo fmt --check
cargo test -p a3-desktop --lib research_ --offline -- --nocapture
cargo clippy --release --workspace --all-targets --all-features --offline -- -D warnings
cargo test --release --workspace --all-features --offline --no-fail-fast -- --test-threads=1
node scripts/check-markdown-links.mjs
git diff --check
```

Alle obigen Prüfungen bestanden: 45 gezielte Recherchetests, vollständiger serieller
Workspace einschließlich 164 Desktop-, 228 Application- und 111 Storage-Unit-Tests,
83 Markdown-Dateien/326 lokale Links. Frontend- und Persistenzschema wurden nicht geändert.

Zwei unabhängige Gate-Befunde bleiben ausdrücklich dokumentiert:

- Der erste parallele Lauf fand beim Katalogsuchwort `17` versehentlich den gemeinsamen
  temporären Pfad (`5917003e...`) und dadurch 25 statt einer Quelle. Der finale Lauf
  verwendete einen neutralen TEMP-/TMP-Pfad außerhalb des Workspace; Katalogcode unverändert.
- Der parallele `catalog_contract` beendete sich nativ mit `0xc0000005`
  (`STATUS_ACCESS_VIOLATION`). Das wurde separat mit
  `cargo test --release -p a3-storage-libsql --test catalog_contract --offline -- --nocapture`
  reproduziert. Seriell bestanden alle sieben Tests und der ganze Workspace. Das ist
  **kein** bestandener paralleler CI-Lauf und keine Behebung des unabhängigen libSQL-Testproblems.

Sehr große Methodenkörper, mehr als acht gleichzeitig benötigte Ausschnitte und
nicht auflösbare dynamische Aufrufe bleiben begrenzt. Der Core erzwingt dabei weder
Vollständigkeit noch automatische Neustarts. Ein Live-Vergleich mit dem tatsächlich
konfigurierten Modell braucht die vorgesehene ausdrückliche Provider-/Netzwerkfreigabe.
