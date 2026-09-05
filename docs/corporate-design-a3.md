# Corporate Design System: A^3

Dieses Corporate Design übersetzt die Ästhetik des Logos – Cyberpunk, Industrial Tech, Glitch-Art und funktionale Datenvisualisierung – direkt in ein konsistentes Design-System für Web und App.

---

## 1. Design-Philosophie & Grundstimmung

* **Stilrichtung:** Industrial Cyberpunk, High-Tech Brutalismus, Terminal/HUD-Ästhetik.
* **Tonalität:** Präzise, roh, funktionsgetrieben, datenzentriert.
* **Fokus:** Starke Typografie, scharfe Kanten, hoher Kontrast und subtile Micro-Interactions statt verspielter Verläufe.

---

## 2. Farbpalette

Das Farbsystem basiert auf einem tiefen, monochromen Fundament mit gezielt gesetzten Signalrot-Akzenten für interaktive Zustände, Statusanzeigen und visuelle Highlights:

| Rolle | Farbname | HEX | Verwendung |
| :--- | :--- | :--- | :--- |
| **Hintergrund** | Deep Void Black | `#0A0A0C` | Primärer App- & Webseiten-Hintergrund |
| **Surface / Cards** | Technical Charcoal | `#141418` | Panels, Container, Modals, Tabellenzeilen |
| **Border / Grid** | Grid Line Gray | `#24252C` | 1px-Raster, subtile Trennlinien, Wireframe-Elemente |
| **Text Primär** | Stark White | `#F3F4F6` | Headlines, dominante Datenwerte, Logo |
| **Text Sekundär** | Muted Data Gray | `#8B8F9A` | Metadaten, Labels, Fließtext, Code-Kommentare |
| **Akzent 1 (Signal)** | Crimson Signal | `#FF2A3B` | Primäre CTAs, aktive States, Cursor, Fokus-Ringe |
| **Akzent 2 (Sub-Signal)** | Dark Rust / Amber | `#7A141D` | Subtile Badges, Hover-Glow, inaktive Warnstufen |

---

## 3. Typografie-Konzept

Ein Duo aus einer industriellen Grotesk-Schrift und einer präzisen Monospace-Schrift schafft den Spagat zwischen Lesbarkeit und Coder-Ästhetik:

* **Display & Headlines:** *Space Grotesk*, *Syne* oder *Chakra Petch* (All-Caps, schwere Schnitte, reduziertes Letter-Spacing).
* **Body / Fließtext:** *Inter* oder *Geist Sans* (hervorragende Lesbarkeit auf Screens, neutrale Geometrie).
* **Code, Daten & Labels:** *JetBrains Mono*, *Geist Mono* oder *Fira Code* (für Metadaten, Tabellenwerte, technische Parameter, Versionshinweise).

---

## 4. UI-Komponenten & visuelle Sprache

* **Container & Panels:**
  * Reines 1px-Border-Design (`#24252C`) ohne weiche Drop-Shadows.
  * Leicht abgeflachte oder scharfe Ecken (entweder `rounded-none` für rohen Terminal-Look oder gezielt `rounded-2xl` angelehnt an die Icon-Silhouette).
* **Hintergrund-Texturen:**
  * Subtile Dot-Matrix-Raster (`background-size: 24px 24px`) oder feine Scanlines mit sehr geringer Opazität (3–5%).
* **Buttons & Interaktionselemente:**
  * Primäre Buttons in massivem `#FF2A3B` mit weißem oder tiefschwarzem Monospace-Text.
  * Sekundäre Buttons mit transparentem Hintergrund, scharfem 1px-Border in `#FF2A3B` und dezentem rotem Text.
  * Hover-Zustände mit leichtem Glitch-Offset (z. B. 1px Text-Verschiebung oder invertierte Rahmenlinie).
* **Gimmicks & Dekoration:**
  * Kleine Fadenkreuze (`+`), technische Bemaßungslinien, Koordinaten-Labels (z. B. `SYS_VER: 3.0 // LAT: OK`) an den Ecken von Bento-Boxen und Cards.

---

## 5. Motion & Interaktion

* **Keine trägen Easing-Kurven:** Schnelle, präzise Übergänge (100–150ms `linear` oder `cubic-bezier(0, 0, 0.2, 1)`).
* **Glitch & Noise:**
  * Glitch-Effekte nur als Micro-Interactions (z. B. beim Klick auf einen Trigger, Seitenwechsel oder Hover über das Logo).
  * Chromatische Aberration (Farbverschiebung) bei Glitch-Animationen trennt Weiß in Rot (`#FF2A3B`) und Cyan/Schwarz auf.
  * Ladezustände als ASCII-Balken, rot blinkender Terminal-Cursor (`_`) oder Data-Scramble-Text, der sich in die richtige Zahl einpendelt.

---

## 6. Tailwind-Konfiguration (Auszug)

```javascript
// tailwind.config.js
module.exports = {
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        void: '#0A0A0C',
        charcoal: '#141418',
        gridline: '#24252C',
        dataGray: '#8B8F9A',
        crimson: {
          DEFAULT: '#FF2A3B',
          hover: '#E01E2E',
          dim: '#7A141D',
          glow: 'rgba(255, 42, 59, 0.25)',
        },
      },
      boxShadow: {
        'crimson-glow': '0 0 16px -2px rgba(255, 42, 59, 0.4)',
      },
      fontFamily: {
        display: ['Space Grotesk', 'sans-serif'],
        mono: ['JetBrains Mono', 'monospace'],
        body: ['Inter', 'sans-serif'],
      },
      backgroundImage: {
        'dot-matrix': 'radial-gradient(#24252C 1px, transparent 1px)',
      },
    },
  },
};
```

---

## 7. Light Mode / Blueprint Modus (Optional)

Das System funktioniert am stärksten im Dark Mode. Für einen potenziellen Light Mode empfiehlt sich ein **"Blueprint"-Ansatz**: 
* Knochenweißes/hellgraues Papier-Finish mit tiefschwarzen Linien
* Rote Stempelfarbe als Akzent
* Subtiles technisches Rauschen
