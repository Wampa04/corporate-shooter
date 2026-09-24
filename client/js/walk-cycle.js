// Der Laufzyklus einer Spielerfigur: Zahlen rein, Winkel raus.
//
// Bewusst ohne einen einzigen Import - kein Three, kein DOM. Nur so laesst
// sich die Bewegung ohne Browser pruefen, und gerade die unauffaelligen Faelle
// brauchen das: dass eine stehende Figur die Beine zusammennimmt, dass Arme
// und Beine gegengleich schwingen, dass ein Wiedereinstieg die Figur nicht
// einmal quer durchs Buero rudern laesst. Im Bild ist all das kaum zu
// beurteilen, in einem Test dagegen genau.
//
// Dieselbe Trennung wie bei `graphics.js`: dort entscheidet ein Regler ueber
// Bildzeiten, hier ueber Strecken; wer das Ergebnis anwendet, steht woanders.

/**
 * Wie weit die Phase je Meter voranschreitet.
 *
 * Ein voller Zyklus sind zwei Schritte. Bei knapp achtzig Zentimetern
 * Schrittlaenge sind das rund anderthalb Meter je Zyklus - dieselbe
 * Groessenordnung, mit der `audio.js` seine Schritte setzt. Waeren beide
 * verschieden, liefen Ton und Bild sichtbar auseinander.
 */
export const PHASE_PER_METER = (2 * Math.PI) / 1.55;

/**
 * Ab diesem Tempo schwingen die Glieder voll aus.
 *
 * Darunter anteilig: wer sich langsam an einer Wand entlangschiebt, soll nicht
 * marschieren.
 */
export const FULL_SPEED = 3.2;

/** Wie schnell der Ausschlag dem Tempo folgt, in 1/s. */
export const CONVERGENCE = 9.0;

/** Groesster Ausschlag der Oberschenkel, in Radiant. */
export const SWING_LEG = 0.62;

/** Groesster Ausschlag der Oberarme. Kleiner als die Beine - so laeuft man. */
export const SWING_ARM = 0.44;

/** Groesste Kniebeugung. */
export const KNEE = 0.75;

/** Wie tief der Koerper je Halbschritt einsinkt, in Metern. */
export const BOB = 0.035;

/** Ruhezustand: Glieder unten, kein Ausschlag. */
export function rest() {
  return { phase: 0, amplitude: 0 };
}

/**
 * Schreibt den Laufzyklus um eine zurueckgelegte Strecke fort.
 *
 * Die Phase haengt an der **Strecke**, nicht an der Zeit. Wer langsam geht,
 * bewegt die Beine langsam; wer steht, bewegt sie gar nicht. Eine Phase, die
 * an der Uhr haengt, ergaebe Figuren, die auf der Stelle staksen.
 *
 * @param {{phase: number, amplitude: number}} state voriger Stand
 * @param {number} distance waagerecht zurueckgelegte Strecke seit dem letzten Bild, in Metern
 * @param {number} dt Zeitschritt in Sekunden
 */
export function advanceWalk(state, distance, dt) {
  const tempo = dt > 0 ? distance / dt : 0;
  const target = Math.min(1, tempo / FULL_SPEED);

  // Zeitschrittunabhaengige Annaeherung, wie bei der Kamera: bei jeder
  // Bildrate gleich schnell.
  const share = dt > 0 ? 1 - Math.exp(-CONVERGENCE * dt) : 0;
  const amplitude = state.amplitude + (target - state.amplitude) * share;

  // Modulo, damit die Phase in einer langen Sitzung nicht ins Grobe waechst -
  // bei einer Million Radiant liegen zwischen zwei f64-Nachbarn mehr als ein
  // Grad, und der Gang faengt an zu zittern.
  const phase = (state.phase + distance * PHASE_PER_METER) % (Math.PI * 2);

  return { phase, amplitude };
}

/**
 * Die Winkel eines Standes.
 *
 * Arme schwingen gegengleich zu den Beinen derselben Seite: rechtes Bein vor
 * heisst rechter Arm zurueck. Schwaengen beide Beine gleich, saehe es aus wie
 * Huepfen - deshalb prueft der Test die Vorzeichen und nicht nur, dass sich
 * ueberhaupt etwas bewegt.
 */
export function limbPose({ phase, amplitude }) {
  const s = Math.sin(phase) * amplitude;

  return {
    legLeft: s * SWING_LEG,
    legRight: -s * SWING_LEG,
    armLeft: -s * SWING_ARM,
    armRight: s * SWING_ARM,

    // Das Knie beugt sich nur beim Zurueckschwingen; nach vorn bleibt das Bein
    // gestreckt. Ohne das laeuft die Figur auf Stelzen.
    kneeLeft: Math.max(0, -s) * KNEE,
    kneeRight: Math.max(0, s) * KNEE,

    // Zweimal je Zyklus einsinken - einmal je Schritt. Immer nach unten, nie
    // nach oben: eine Figur, die beim Gehen ueber ihre Standhoehe steigt,
    // schwebt.
    bob: -(1 - Math.cos(2 * phase)) * 0.5 * BOB * amplitude,
  };
}
