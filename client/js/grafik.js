// Die Grafikstufen und der Regler, der zwischen ihnen waehlt.
//
// Bewusst ohne Bezug auf Three, den Renderer oder das DOM: der Regler
// entscheidet nur anhand von Bildzeiten, welche Stufe gelten soll. Wer die
// Stufe dann anwendet, steht in `main.js`. So laesst sich die Entscheidung
// pruefen, ohne einen Browser zu starten - und gerade die Richtung nach oben
// ist von aussen sonst kaum zu erzwingen: dafuer braeuchte es einen Rechner,
// der erst langsam ist und dann schnell.

/**
 * Wie viele Bilder verworfen werden, bevor gemessen wird.
 *
 * Die ersten Bilder sind durch Shader-Uebersetzung und Texturupload verzerrt
 * und taugen nicht als Massstab.
 */
export const WARMLAUF_BILDER = 30;

/** Wie viele Bilder ein Messfenster hoechstens umfasst. */
export const FENSTER_BILDER = 60;

/**
 * Wie lange ein Messfenster hoechstens dauert.
 *
 * Ohne diesen Deckel dauert das Fenster auf einer langsamen Maschine genau so
 * lange, wie sie langsam ist: bei zwoelf Bildern je Sekunde waeren sechzig
 * Bilder fuenf Sekunden Ruckeln, bevor ueberhaupt reagiert wird. Gerade dort
 * soll es aber schnell gehen.
 */
export const FENSTER_MS = 1500;

/** Wie viele Bilder ein Fenster mindestens enthaelt. Ein Median aus fuenf
 *  Werten ist kein Median, sondern ein Zufall. */
export const MIN_BILDER = 20;

/**
 * Ab dieser Bildzeit wird eine Stufe heruntergeschaltet.
 *
 * 28 ms entsprechen gut fuenfunddreissig Bildern je Sekunde. Wer darunter
 * liegt, hat mehr davon, das Buero fluessig zu sehen als huebsch.
 */
export const BUDGET_MS = 28;

/**
 * Ab dieser Bildzeit darf wieder hochgeschaltet werden.
 *
 * Vierzehn Millisekunden sind gut siebzig Bilder je Sekunde. Der Abstand zum
 * Budget ist die Hysterese: laege die Schwelle knapp unter 28, schaltete eine
 * Maschine, die genau dazwischen liegt, im Sekundentakt hin und her - und ein
 * flackerndes Bild ist schlimmer als jede feste Stufe.
 */
export const KOMFORT_MS = 14;

/**
 * Wie viele gute Fenster hintereinander eine Stufe zurueckholen.
 *
 * Herunter geht es nach einem Fenster, denn Ruckeln merkt man sofort; hinauf
 * erst nach fuenf, denn eine ruhige Sekunde im Einzelbuero beweist noch nicht,
 * dass der Rechner auch die Halle traegt.
 */
export const ERHOLUNG_FENSTER = 5;

/**
 * Wie oft eine Stufe hoechstens wieder verlassen werden darf.
 *
 * Wer zweimal von derselben Stufe herunter musste, kommt nicht mehr hinauf:
 * die Maschine hat den Beweis zweimal angetreten und zweimal verloren.
 */
export const MAX_VERSUCHE = 2;

/**
 * Die Grafikstufen, von schoen nach schnell.
 *
 * Zuerst faellt der Schattenwurf, dann die Aufloesung. Diese Reihenfolge ist
 * kein Geschmack, sondern Rechnung: der Schattenwurf zeichnet das ganze
 * Stockwerk ein zweites Mal, kostet also unabhaengig von der Bildgroesse.
 * Erst wenn er weg ist und es immer noch klemmt, liegt es an der Zahl der
 * Bildpunkte - und die senkt man nur ungern, weil das Bild sichtbar weich
 * wird.
 *
 * Aufgenommen werden nur Stufen, die auf diesem Bildschirm auch etwas
 * aendern: auf einem Geraet mit Pixelverhaeltnis 1 waere `min(1, 1)` dieselbe
 * Stufe noch einmal, und der Regler verloere ein ganzes Fenster an nichts.
 */
export function baueStufen(geraeteVerhaeltnis) {
  // Ueber zwei hinaus lohnt sich nichts: der Zugewinn ist auf keinem
  // Bildschirm zu sehen, die Kosten wachsen quadratisch.
  const voll = Math.min(geraeteVerhaeltnis > 0 ? geraeteVerhaeltnis : 1, 2);
  const stufen = [
    { schatten: true, pixel: voll, name: "voll" },
    { schatten: false, pixel: voll, name: "ohne Schatten" },
  ];
  for (const ziel of [1, 0.75]) {
    const pixel = Math.min(voll, ziel);
    if (pixel < stufen[stufen.length - 1].pixel - 0.01) {
      stufen.push({
        schatten: false,
        pixel,
        name: `${Math.round((pixel / voll) * 100)} % Aufloesung`,
      });
    }
  }
  return stufen;
}

/**
 * Waehlt laufend die Grafikstufe anhand der gemessenen Bildzeiten.
 *
 * Gemessen wird der Median eines Fensters, nicht der Mittelwert: ein einzelnes
 * langes Bild - ein Nachladen der Textur, eine Speicherbereinigung, die
 * Rueckkehr aus einem anderen Tab - zieht den Mittelwert ueber die Schwelle,
 * sagt aber nichts darueber, ob die Maschine die Stufe traegt.
 *
 * Und gemessen wird dauernd, nicht einmal beim Start: die Bildrate haengt
 * davon ab, wo man steht (die offene Halle kostet mehr als ein Einzelbuero),
 * wie viele mitspielen und ob nebenher etwas anderes laeuft. Eine Entscheidung
 * nach den ersten anderthalb Sekunden trifft genau den Spawnpunkt und danach
 * nie wieder.
 */
export class Grafikregler {
  /**
   * @param {number} anzahlStufen  Zahl der verfuegbaren Stufen
   * @param {number} start         Stufe, mit der begonnen wird
   */
  constructor(anzahlStufen, start = 0) {
    this.anzahl = anzahlStufen;
    this.stufe = start;
    this._fenster = [];
    this._fensterMs = 0;
    this._verworfen = 0;
    this._guteFenster = 0;
    // Wie oft jede Stufe schon nach unten verlassen wurde. Verhindert, dass
    // eine Maschine an der Grenze dauerhaft zwischen zwei Stufen pendelt.
    this._abstiege = new Array(anzahlStufen).fill(0);
  }

  /**
   * Nimmt die Zeit eines Bildes entgegen.
   *
   * @param {number} ms Dauer des Bildes in Millisekunden
   * @returns {{stufe: number, median: number} | null} die neue Stufe, oder
   *   `null`, wenn alles bleibt, wie es ist.
   */
  bild(ms) {
    if (this._verworfen < WARMLAUF_BILDER) {
      this._verworfen++;
      return null;
    }

    this._fenster.push(ms);
    this._fensterMs += ms;
    const voll =
      this._fenster.length >= FENSTER_BILDER ||
      (this._fenster.length >= MIN_BILDER && this._fensterMs >= FENSTER_MS);
    if (!voll) return null;

    this._fenster.sort((a, b) => a - b);
    const median = this._fenster[this._fenster.length >> 1];
    this._fenster = [];
    this._fensterMs = 0;

    if (median > BUDGET_MS && this.stufe < this.anzahl - 1) {
      this._abstiege[this.stufe]++;
      return this._wechsle(this.stufe + 1, median);
    }

    if (
      median <= KOMFORT_MS &&
      this.stufe > 0 &&
      this._abstiege[this.stufe - 1] < MAX_VERSUCHE
    ) {
      if (++this._guteFenster >= ERHOLUNG_FENSTER) {
        return this._wechsle(this.stufe - 1, median);
      }
      return null;
    }

    this._guteFenster = 0;
    return null;
  }

  _wechsle(ziel, median) {
    this.stufe = ziel;
    this._guteFenster = 0;
    // Nach dem Umschalten uebersetzt Three die Shader neu; die naechsten
    // Bilder sind so wenig aussagekraeftig wie die beim Start.
    this._verworfen = 0;
    return { stufe: ziel, median };
  }
}
