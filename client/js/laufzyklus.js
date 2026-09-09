// Der Laufzyklus einer Spielerfigur: Zahlen rein, Winkel raus.
//
// Bewusst ohne einen einzigen Import - kein Three, kein DOM. Nur so laesst
// sich die Bewegung ohne Browser pruefen, und gerade die unauffaelligen Faelle
// brauchen das: dass eine stehende Figur die Beine zusammennimmt, dass Arme
// und Beine gegengleich schwingen, dass ein Wiedereinstieg die Figur nicht
// einmal quer durchs Buero rudern laesst. Im Bild ist all das kaum zu
// beurteilen, in einem Test dagegen genau.
//
// Dieselbe Trennung wie bei `grafik.js`: dort entscheidet ein Regler ueber
// Bildzeiten, hier ueber Strecken; wer das Ergebnis anwendet, steht woanders.

/**
 * Wie weit die Phase je Meter voranschreitet.
 *
 * Ein voller Zyklus sind zwei Schritte. Bei knapp achtzig Zentimetern
 * Schrittlaenge sind das rund anderthalb Meter je Zyklus - dieselbe
 * Groessenordnung, mit der `audio.js` seine Schritte setzt. Waeren beide
 * verschieden, liefen Ton und Bild sichtbar auseinander.
 */
export const PHASE_JE_METER = (2 * Math.PI) / 1.55;

/**
 * Ab diesem Tempo schwingen die Glieder voll aus.
 *
 * Darunter anteilig: wer sich langsam an einer Wand entlangschiebt, soll nicht
 * marschieren.
 */
export const VOLLES_TEMPO = 3.2;

/** Wie schnell der Ausschlag dem Tempo folgt, in 1/s. */
export const ANGLEICHUNG = 9.0;

/** Groesster Ausschlag der Oberschenkel, in Radiant. */
export const SCHWUNG_BEIN = 0.62;

/** Groesster Ausschlag der Oberarme. Kleiner als die Beine - so laeuft man. */
export const SCHWUNG_ARM = 0.44;

/** Groesste Kniebeugung. */
export const KNIE = 0.75;

/** Wie tief der Koerper je Halbschritt einsinkt, in Metern. */
export const WIPPEN = 0.035;

/** Ruhezustand: Glieder unten, kein Ausschlag. */
export function ruhe() {
  return { phase: 0, ausschlag: 0 };
}

/**
 * Schreibt den Laufzyklus um eine zurueckgelegte Strecke fort.
 *
 * Die Phase haengt an der **Strecke**, nicht an der Zeit. Wer langsam geht,
 * bewegt die Beine langsam; wer steht, bewegt sie gar nicht. Eine Phase, die
 * an der Uhr haengt, ergaebe Figuren, die auf der Stelle staksen.
 *
 * @param {{phase: number, ausschlag: number}} zustand voriger Stand
 * @param {number} strecke waagerecht zurueckgelegte Strecke seit dem letzten Bild, in Metern
 * @param {number} dt Zeitschritt in Sekunden
 */
export function schritt(zustand, strecke, dt) {
  const tempo = dt > 0 ? strecke / dt : 0;
  const ziel = Math.min(1, tempo / VOLLES_TEMPO);

  // Zeitschrittunabhaengige Annaeherung, wie bei der Kamera: bei jeder
  // Bildrate gleich schnell.
  const anteil = dt > 0 ? 1 - Math.exp(-ANGLEICHUNG * dt) : 0;
  const ausschlag = zustand.ausschlag + (ziel - zustand.ausschlag) * anteil;

  // Modulo, damit die Phase in einer langen Sitzung nicht ins Grobe waechst -
  // bei einer Million Radiant liegen zwischen zwei f64-Nachbarn mehr als ein
  // Grad, und der Gang faengt an zu zittern.
  const phase = (zustand.phase + strecke * PHASE_JE_METER) % (Math.PI * 2);

  return { phase, ausschlag };
}

/**
 * Die Winkel eines Standes.
 *
 * Arme schwingen gegengleich zu den Beinen derselben Seite: rechtes Bein vor
 * heisst rechter Arm zurueck. Schwaengen beide Beine gleich, saehe es aus wie
 * Huepfen - deshalb prueft der Test die Vorzeichen und nicht nur, dass sich
 * ueberhaupt etwas bewegt.
 */
export function gliedmassen({ phase, ausschlag }) {
  const s = Math.sin(phase) * ausschlag;

  return {
    beinLinks: s * SCHWUNG_BEIN,
    beinRechts: -s * SCHWUNG_BEIN,
    armLinks: -s * SCHWUNG_ARM,
    armRechts: s * SCHWUNG_ARM,

    // Das Knie beugt sich nur beim Zurueckschwingen; nach vorn bleibt das Bein
    // gestreckt. Ohne das laeuft die Figur auf Stelzen.
    knieLinks: Math.max(0, -s) * KNIE,
    knieRechts: Math.max(0, s) * KNIE,

    // Zweimal je Zyklus einsinken - einmal je Schritt. Immer nach unten, nie
    // nach oben: eine Figur, die beim Gehen ueber ihre Standhoehe steigt,
    // schwebt.
    wippen: -(1 - Math.cos(2 * phase)) * 0.5 * WIPPEN * ausschlag,
  };
}
