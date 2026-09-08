// Prueft den Regler, der die Grafikstufe waehlt.
//
//     node scripts/grafikregler.mjs
//
// Im Browser laesst sich davon nur die Haelfte pruefen: der Prueflauf zeichnet
// hier per SwiftShader mit zwoelf Bildern je Sekunde, also faellt die Stufe
// zuverlaessig - aber sie steigt nie wieder. Fuer den Weg nach oben braeuchte
// es einen Rechner, der erst langsam ist und dann schnell. Den gibt es nicht,
// also wird der Regler mit erfundenen Bildzeiten gefuettert.
//
// Gemessen wurde im Browser dagegen, was der Regler *bewirkt*: im selben
// Fenster 83 ms bei voller Stufe, 67 ms ohne Schatten, 50 ms bei 75 Prozent
// Aufloesung - zwoelf, fuenfzehn, zwanzig Bilder je Sekunde.
import {
  Grafikregler,
  baueStufen,
  BUDGET_MS,
  ERHOLUNG_FENSTER,
  FENSTER_BILDER,
  KOMFORT_MS,
  MAX_VERSUCHE,
  MIN_BILDER,
  WARMLAUF_BILDER,
} from "../client/js/grafik.js";

let fehler = 0;

function pruefe(name, bedingung, hinweis) {
  if (bedingung) {
    console.log(`  ok    ${name}`);
  } else {
    console.error(`  FEHLER ${name}: ${hinweis}`);
    fehler = 1;
  }
}

/** Fuettert den Regler mit Bildzeiten und sammelt alle Stufenwechsel. */
function fuettern(regler, zeiten) {
  const wechsel = [];
  for (const ms of zeiten) {
    const w = regler.bild(ms);
    if (w) wechsel.push(w.stufe);
  }
  return wechsel;
}

/** `n` gleiche Bildzeiten. */
const gleich = (n, ms) => new Array(n).fill(ms);

console.log("Stufenliste");
{
  // Auf einem gewoehnlichen Bildschirm faellt die 100-Prozent-Stufe weg, weil
  // sie dort dasselbe waere wie die volle.
  const einfach = baueStufen(1);
  pruefe("Verhaeltnis 1 ergibt drei Stufen", einfach.length === 3,
    `${einfach.length} statt 3: ${einfach.map((s) => s.name).join(", ")}`);
  pruefe("keine doppelte Stufe", new Set(einfach.map((s) => s.pixel)).size === 2,
    "zwei Stufen mit gleicher Aufloesung");

  const retina = baueStufen(2);
  pruefe("Verhaeltnis 2 ergibt vier Stufen", retina.length === 4,
    `${retina.length} statt 4`);

  const hoch = baueStufen(3);
  pruefe("Verhaeltnis 3 wird auf 2 gedeckelt", hoch[0].pixel === 2,
    `${hoch[0].pixel} statt 2`);

  for (const liste of [einfach, retina, hoch]) {
    pruefe(`Stufen werden monoton billiger (dpr ${liste[0].pixel})`,
      liste.every((s, i) => i === 0 ||
        (!s.schatten && s.pixel <= liste[i - 1].pixel)),
      "eine spaetere Stufe kostet mehr als eine fruehere");
  }
}

console.log("Warmlauf");
{
  const r = new Grafikregler(3);
  // Feste Zahl, absichtlich nicht `WARMLAUF_BILDER`: ein Test, der seine
  // Eingabe aus der Konstanten ableitet, die er pruefen soll, kann nicht
  // scheitern. Mit `WARMLAUF_BILDER = 0` haette er brav null Bilder gefuettert
  // und bestanden - genau so ist er beim ersten Versuch durchgerutscht.
  //
  // Fuenfundvierzig Bilder zu 500 ms: mit Warmlauf bleiben fuenfzehn uebrig,
  // zu wenig fuer ein Fenster. Ohne Warmlauf schliesst nach zwanzig Bildern
  // eines und die Stufe faellt.
  const wechsel = fuettern(r, gleich(45, 500));
  pruefe("verworfene Bilder loesen nichts aus", wechsel.length === 0,
    `${wechsel.length} Wechsel im Warmlauf`);
}

console.log("Zeitdeckel des Messfensters");
{
  // Auf einer langsamen Maschine soll nicht erst nach sechzig Bildern
  // entschieden werden - das waeren bei zehn Bildern je Sekunde sechs
  // Sekunden Ruckeln. Fuenfundzwanzig Bilder zu 100 ms sind zweieinhalb
  // Sekunden und muessen genuegen.
  const NACH_WARMLAUF = 25;
  const r = new Grafikregler(3);
  const wechsel = fuettern(r, gleich(WARMLAUF_BILDER + NACH_WARMLAUF, 100));
  pruefe("langsame Bilder schliessen das Fenster frueher", wechsel.join(",") === "1",
    `Verlauf ${wechsel.join(",")} - ohne Zeitdeckel bliebe es leer`);
  pruefe("die Gegenprobe hat Zaehne",
    NACH_WARMLAUF < FENSTER_BILDER && NACH_WARMLAUF >= MIN_BILDER,
    `${NACH_WARMLAUF} Bilder fuellen schon ein volles Fenster - der Deckel ` +
    "wird gar nicht gebraucht");
}

console.log("Weg nach unten");
{
  const r = new Grafikregler(3);
  const wechsel = fuettern(r, gleich(WARMLAUF_BILDER + 2000, 100));
  pruefe("faellt bis zur letzten Stufe", r.stufe === 2, `steht auf ${r.stufe}`);
  pruefe("faellt Stufe um Stufe", wechsel.join(",") === "1,2",
    `Verlauf ${wechsel.join(",")}`);
  pruefe("bleibt dann stehen", wechsel.length === 2,
    `${wechsel.length} Wechsel statt 2`);
}

console.log("Weg nach oben");
{
  const r = new Grafikregler(3);
  fuettern(r, gleich(WARMLAUF_BILDER + 200, 100));
  const gefallen = r.stufe;
  const wechsel = fuettern(r, gleich(WARMLAUF_BILDER + FENSTER_BILDER * 40, 8));
  pruefe("war vorher unten", gefallen === 2, `stand auf ${gefallen}`);
  pruefe("steigt wieder auf die volle Stufe", r.stufe === 0, `steht auf ${r.stufe}`);
  pruefe("steigt Stufe um Stufe", wechsel.join(",") === "1,0",
    `Verlauf ${wechsel.join(",")}`);
}

console.log("Zurueckhaltung beim Aufsteigen");
{
  const r = new Grafikregler(2);
  fuettern(r, gleich(WARMLAUF_BILDER + 200, 100));
  // Warmlauf plus *ein* gutes Fenster. Feste Zahlen, aus demselben Grund wie
  // oben: mit `ERHOLUNG_FENSTER - 1` waere die Eingabe bei einem Wert von 1
  // leer und der Test bedeutungslos.
  const knapp = fuettern(r, gleich(WARMLAUF_BILDER + FENSTER_BILDER, 8));
  pruefe("ein einzelnes gutes Fenster genuegt nicht", knapp.length === 0,
    "stieg schon nach einem Fenster auf");
  const genug = fuettern(r, gleich(FENSTER_BILDER * ERHOLUNG_FENSTER, 8));
  pruefe("mehrere gute Fenster genuegen", genug.join(",") === "0",
    `Verlauf ${genug.join(",")}`);
  pruefe("die Zurueckhaltung ist mehr als ein Fenster", ERHOLUNG_FENSTER > 1,
    "ein einziges gutes Fenster genuegt laut Konstante - der Test oben misst nichts");
}

console.log("Hysterese");
{
  // Zwanzig Millisekunden sind fuenfzig Bilder je Sekunde: fluessig genug, um
  // nichts abzuschalten, aber nicht so gut, dass man riskieren wollte, wieder
  // aufzudrehen. Hier darf gar nichts passieren.
  //
  // Feste Zahl, nicht die Mitte zwischen den Schwellen: eine mitwandernde
  // Mitte liegt per Konstruktion immer im toten Bereich, egal wie schmal der
  // ist. Der Test bestuende noch bei einer Hysterese von einer Millisekunde.
  const TOT_MS = 20;
  const r = new Grafikregler(3, 1);
  const wechsel = fuettern(r, gleich(WARMLAUF_BILDER + FENSTER_BILDER * 40, TOT_MS));
  pruefe(`${TOT_MS} ms loest nichts aus`, wechsel.length === 0,
    `${wechsel.length} Wechsel`);
  pruefe("der tote Bereich ist breit genug",
    KOMFORT_MS <= TOT_MS - 4 && BUDGET_MS >= TOT_MS + 4,
    `${KOMFORT_MS}..${BUDGET_MS} ms laesst um ${TOT_MS} ms herum zu wenig Luft`);
}

console.log("Kein Pendeln");
{
  // Eine Maschine, die abwechselnd gut und schlecht laeuft. Ohne Deckel
  // wechselte sie endlos die Stufe; das Flackern waere schlimmer als jede
  // feste Wahl.
  const r = new Grafikregler(2);
  const zeiten = [];
  for (let i = 0; i < 60; i++) {
    zeiten.push(...gleich(WARMLAUF_BILDER + FENSTER_BILDER, 100));
    zeiten.push(...gleich(WARMLAUF_BILDER + FENSTER_BILDER * ERHOLUNG_FENSTER, 8));
  }
  const wechsel = fuettern(r, zeiten);
  pruefe("kommt zur Ruhe", wechsel.length <= MAX_VERSUCHE * 2,
    `${wechsel.length} Wechsel bei sechzig Wechseln der Last`);
  pruefe("bleibt am Ende unten", r.stufe === 1, `steht auf ${r.stufe}`);
}

console.log("Median statt Mittelwert");
{
  // Ein einzelnes langes Bild - eine Speicherbereinigung, die Rueckkehr aus
  // einem anderen Tab - darf die Stufe nicht kosten. Der Mittelwert dieses
  // Fensters liegt bei 24 ms und damit klar ueber dem Komfortwert; der Median
  // liegt bei 8.
  const r = new Grafikregler(2, 1);
  const zeiten = gleich(WARMLAUF_BILDER, 8);
  for (let f = 0; f < ERHOLUNG_FENSTER; f++) {
    zeiten.push(1000, ...gleich(FENSTER_BILDER - 1, 8));
  }
  const wechsel = fuettern(r, zeiten);
  pruefe("ein Ausreisser verhindert den Aufstieg nicht", wechsel.join(",") === "0",
    `Verlauf ${wechsel.join(",")}`);

  // Gegenprobe: waere es der Mittelwert, muesste derselbe Verlauf scheitern.
  const mittel = zeiten.slice(WARMLAUF_BILDER, WARMLAUF_BILDER + FENSTER_BILDER)
    .reduce((a, b) => a + b, 0) / FENSTER_BILDER;
  pruefe("die Gegenprobe hat Zaehne", mittel > KOMFORT_MS,
    `Mittelwert ${mittel.toFixed(1)} ms liegt unter ${KOMFORT_MS} ms - ` +
    "der Ausreisser ist zu klein, der Test wuerde auch mit Mittelwert bestehen");
}

process.exit(fehler);
