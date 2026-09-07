# ADR-0073: Originalgebundene Navigationsstellen im Kontext

Status: Accepted\
Datum: 2026-09-07\
Freigabe: Fortgesetzter Plan-10-Auftrag einschließlich erforderlicher ADRs.

## Befund

Eine reale V6-Mehrstufenfixture mit langen Helfern findet eine spätere Aufrufstelle,
verliert sie aber beim nächsten Helfer durch neue Bytequoten. Ganze große Funktionen
als geschützte Einheit passen nicht gemeinsam. Das Modell fordert die gerade verlorene
Stelle erneut an; identische Pakete werden korrekt gestoppt, die Aufgabe bleibt offen.

## Entscheidung

Der vorhandene begrenzte Originaleinheiten-Cache behält bei zugelassenem V6-Belegbedarf
auch die tatsächlichen kurzen Originalzeilen, in denen die angeforderten Literale
vorkamen. Nur gegenwärtig gelieferte, zeilenbündige Originalbereiche bis 512 Bytes
werden übernommen; keine Modellparaphrase oder Suchmetadaten. Die bestehenden
32-Einheiten-, acht Fenster-, Gesamtbyte-, Revisions- und Quellankergrenzen bleiben.

Eine ganze indizierte Funktion wird nur dann als dauerhafte Kontexteinheit ausgewählt,
wenn ihr deklarierter Bytebereich höchstens die Hälfte des Evidence-Paketbudgets
umfasst. Größere Funktionen bleiben regulär paginierbare Originale. Ein neuer
Core-Navigationsfokus außerhalb einer behaltenen Stelle derselben Datei muss weiterhin
aus dem realen Cache geliefert werden können; eine behaltene Stelle sperrt keine Seite.

Dies präzisiert die bisherige progressive Auswahl bei nicht passenden Originalen.
Der Vorrang tatsächlich vollständig passender benannter Dateien aus ADR-0056,
explizite Stellen, Safety, Freshness und Fortschrittsmessung bleiben unverändert.
Der Cache ist flüchtige Auswahl, kein neuer Speicher, Fakt oder Rechercheergebnis.
Fortsetzungen revalidieren Originale; Audit-Suchhistorie ersetzt diese Prüfung nicht.

## Nachweis

Realer Index, Safe Reader und libSQL müssen die Helferkette innerhalb derselben
4096 Evidence-Bytes ohne größere Leserunden-/Modellbudgets abschließen. Negative
unbelegte Literale dürfen keine Reads auslösen. Bestehende kleine/größere Fenster,
UTF-8-/Zeilengrenzen, disjunkte Stellen, explizite neue Seiten, deterministische
Pakete und gültige Originalanker werden erneut geprüft. Modellnachtests bleiben
von diesem deterministischen Zustandsnachweis getrennt.
