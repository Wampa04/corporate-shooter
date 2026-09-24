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
export const WARMUP_FRAMES = 30;

/** Wie viele Bilder ein Messfenster hoechstens umfasst. */
export const WINDOW_FRAMES = 60;

/**
 * Wie lange ein Messfenster hoechstens dauert.
 *
 * Ohne diesen Deckel dauert das Fenster auf einer langsamen Maschine genau so
 * lange, wie sie langsam ist: bei zwoelf Bildern je Sekunde waeren sechzig
 * Bilder fuenf Sekunden Ruckeln, bevor ueberhaupt reagiert wird. Gerade dort
 * soll es aber schnell gehen.
 */
export const WINDOW_MS = 1500;

/** Wie viele Bilder ein Fenster mindestens enthaelt. Ein Median aus fuenf
 *  Werten ist kein Median, sondern ein Zufall. */
export const MIN_FRAMES = 20;

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
 * Der Abstand zum Budget ist die Hysterese: laege die Schwelle knapp unter 28,
 * schaltete eine Maschine, die genau dazwischen liegt, im Sekundentakt hin und
 * her - und ein flackerndes Bild ist schlimmer als jede feste Stufe.
 *
 * Nach oben ist der Wert aber ebenso gedeckelt, und zwar durch den Bildschirm:
 * bei 60 Hz kann kein Bild schneller als 16,7 ms fertig werden, weil der
 * Browser auf den Bildwechsel wartet. Eine Schwelle darunter waere unerreichbar
 * - der Regler koennte fallen, aber nie wieder steigen, und das auf der
 * verbreitetsten Hardware ueberhaupt. Genau so stand es hier zuerst.
 *
 * Achtzehn Millisekunden liegen knapp darueber: wer den Bildwechsel trifft,
 * gilt als schnell genug; wer bei 50 Bildern je Sekunde (20 ms) haengt, laesst
 * Bilder aus und gilt es nicht.
 */
export const COMFORT_MS = 18;

/**
 * Wie viele gute Fenster hintereinander eine Stufe zurueckholen.
 *
 * Herunter geht es nach einem Fenster, denn Ruckeln merkt man sofort; hinauf
 * erst nach fuenf, denn eine ruhige Sekunde im Einzelbuero beweist noch nicht,
 * dass der Rechner auch die Halle traegt.
 */
export const RECOVERY_WINDOWS = 5;

/**
 * Wie oft eine Stufe hoechstens wieder verlassen werden darf.
 *
 * Wer zweimal von derselben Stufe herunter musste, kommt nicht mehr hinauf:
 * die Maschine hat den Beweis zweimal angetreten und zweimal verloren.
 */
export const MAX_ATTEMPTS = 2;

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
export function buildTiers(deviceRatio) {
  // Ueber zwei hinaus lohnt sich nichts: der Zugewinn ist auf keinem
  // Bildschirm zu sehen, die Kosten wachsen quadratisch.
  const fullRatio = Math.min(deviceRatio > 0 ? deviceRatio : 1, 2);
  const tiers = [
    { shadows: true, pixel: fullRatio, name: "voll" },
    { shadows: false, pixel: fullRatio, name: "ohne Schatten" },
  ];
  for (const target of [1, 0.75]) {
    const pixel = Math.min(fullRatio, target);
    if (pixel < tiers[tiers.length - 1].pixel - 0.01) {
      tiers.push({
        shadows: false,
        pixel,
        name: `${Math.round((pixel / fullRatio) * 100)} % Aufloesung`,
      });
    }
  }
  return tiers;
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
export class GraphicsGovernor {
  /**
   * @param {number} tierCount  Zahl der verfuegbaren Stufen
   * @param {number} start         Stufe, mit der begonnen wird
   */
  constructor(tierCount, start = 0) {
    this.count = tierCount;
    this.tier = start;
    this._window = [];
    this._windowMs = 0;
    this._discarded = 0;
    this._goodWindows = 0;
    // Wie oft jede Stufe schon nach unten verlassen wurde. Verhindert, dass
    // eine Maschine an der Grenze dauerhaft zwischen zwei Stufen pendelt.
    this._downgrades = new Array(tierCount).fill(0);
  }

  /**
   * Nimmt die Zeit eines Bildes entgegen.
   *
   * @param {number} ms Dauer des Bildes in Millisekunden
   * @returns {{tier: number, median: number} | null} die neue Stufe, oder
   *   `null`, wenn alles bleibt, wie es ist.
   */
  recordFrame(ms) {
    if (this._discarded < WARMUP_FRAMES) {
      this._discarded++;
      return null;
    }

    this._window.push(ms);
    this._windowMs += ms;
    const fullRatio =
      this._window.length >= WINDOW_FRAMES ||
      (this._window.length >= MIN_FRAMES && this._windowMs >= WINDOW_MS);
    if (!fullRatio) return null;

    this._window.sort((a, b) => a - b);
    const median = this._window[this._window.length >> 1];
    this._window = [];
    this._windowMs = 0;

    if (median > BUDGET_MS && this.tier < this.count - 1) {
      this._downgrades[this.tier]++;
      return this._switchTier(this.tier + 1, median);
    }

    if (median <= COMFORT_MS && this.tier > 0 && this._downgrades[this.tier - 1] < MAX_ATTEMPTS) {
      if (++this._goodWindows >= RECOVERY_WINDOWS) {
        return this._switchTier(this.tier - 1, median);
      }
      return null;
    }

    this._goodWindows = 0;
    return null;
  }

  _switchTier(target, median) {
    this.tier = target;
    this._goodWindows = 0;
    // Nach dem Umschalten uebersetzt Three die Shader neu; die naechsten
    // Bilder sind so wenig aussagekraeftig wie die beim Start.
    this._discarded = 0;
    return { tier: target, median };
  }
}
