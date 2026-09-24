// Prueft den Regler, der die Grafikstufe waehlt.
//
//     node scripts/graphics-governor.test.mjs
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
  GraphicsGovernor,
  buildTiers,
  BUDGET_MS,
  RECOVERY_WINDOWS,
  WINDOW_FRAMES,
  COMFORT_MS,
  MAX_ATTEMPTS,
  MIN_FRAMES,
  WARMUP_FRAMES,
} from "../client/js/graphics.js";

let errors = 0;

function check(name, condition, hint) {
  if (condition) {
    console.log(`  ok    ${name}`);
  } else {
    console.error(`  FEHLER ${name}: ${hint}`);
    errors = 1;
  }
}

/** Fuettert den Regler mit Bildzeiten und sammelt alle Stufenwechsel. */
function feed(governor, times) {
  const switches = [];
  for (const ms of times) {
    const w = governor.recordFrame(ms);
    if (w) switches.push(w.tier);
  }
  return switches;
}

/** `n` gleiche Bildzeiten. */
const constantFrames = (n, ms) => new Array(n).fill(ms);

console.log("Stufenliste");
{
  // Auf einem gewoehnlichen Bildschirm faellt die 100-Prozent-Stufe weg, weil
  // sie dort dasselbe waere wie die volle.
  const simple = buildTiers(1);
  check(
    "Verhaeltnis 1 ergibt drei Stufen",
    simple.length === 3,
    `${simple.length} statt 3: ${simple.map((s) => s.name).join(", ")}`,
  );
  check(
    "keine doppelte Stufe",
    new Set(simple.map((s) => s.pixel)).size === 2,
    "zwei Stufen mit gleicher Aufloesung",
  );

  const retina = buildTiers(2);
  check("Verhaeltnis 2 ergibt vier Stufen", retina.length === 4, `${retina.length} statt 4`);

  const up = buildTiers(3);
  check("Verhaeltnis 3 wird auf 2 gedeckelt", up[0].pixel === 2, `${up[0].pixel} statt 2`);

  for (const list of [simple, retina, up]) {
    check(
      `Stufen werden monoton billiger (dpr ${list[0].pixel})`,
      list.every((s, i) => i === 0 || (!s.shadows && s.pixel <= list[i - 1].pixel)),
      "eine spaetere Stufe kostet mehr als eine fruehere",
    );
  }
}

console.log("Warmlauf");
{
  const r = new GraphicsGovernor(3);
  // Feste Zahl, absichtlich nicht `WARMUP_FRAMES`: ein Test, der seine
  // Eingabe aus der Konstanten ableitet, die er pruefen soll, kann nicht
  // scheitern. Mit `WARMUP_FRAMES = 0` haette er brav null Bilder gefuettert
  // und bestanden - genau so ist er beim ersten Versuch durchgerutscht.
  //
  // Fuenfundvierzig Bilder zu 500 ms: mit Warmlauf bleiben fuenfzehn uebrig,
  // zu wenig fuer ein Fenster. Ohne Warmlauf schliesst nach zwanzig Bildern
  // eines und die Stufe faellt.
  const switches = feed(r, constantFrames(45, 500));
  check(
    "verworfene Bilder loesen nichts aus",
    switches.length === 0,
    `${switches.length} Wechsel im Warmlauf`,
  );
}

console.log("Zeitdeckel des Messfensters");
{
  // Auf einer langsamen Maschine soll nicht erst nach sechzig Bildern
  // entschieden werden - das waeren bei zehn Bildern je Sekunde sechs
  // Sekunden Ruckeln. Fuenfundzwanzig Bilder zu 100 ms sind zweieinhalb
  // Sekunden und muessen genuegen.
  const AFTER_WARMUP = 25;
  const r = new GraphicsGovernor(3);
  const switches = feed(r, constantFrames(WARMUP_FRAMES + AFTER_WARMUP, 100));
  check(
    "langsame Bilder schliessen das Fenster frueher",
    switches.join(",") === "1",
    `Verlauf ${switches.join(",")} - ohne Zeitdeckel bliebe es leer`,
  );
  check(
    "die Gegenprobe hat Zaehne",
    AFTER_WARMUP < WINDOW_FRAMES && AFTER_WARMUP >= MIN_FRAMES,
    `${AFTER_WARMUP} Bilder fuellen schon ein volles Fenster - der Deckel ` +
      "wird gar nicht gebraucht",
  );
}

console.log("Weg nach unten");
{
  const r = new GraphicsGovernor(3);
  const switches = feed(r, constantFrames(WARMUP_FRAMES + 2000, 100));
  check("faellt bis zur letzten Stufe", r.tier === 2, `steht auf ${r.tier}`);
  check("faellt Stufe um Stufe", switches.join(",") === "1,2", `Verlauf ${switches.join(",")}`);
  check("bleibt dann stehen", switches.length === 2, `${switches.length} Wechsel statt 2`);
}

console.log("Weg nach oben");
{
  const r = new GraphicsGovernor(3);
  feed(r, constantFrames(WARMUP_FRAMES + 200, 100));
  const fallen = r.tier;
  const switches = feed(r, constantFrames(WARMUP_FRAMES + WINDOW_FRAMES * 40, 8));
  check("war vorher unten", fallen === 2, `stand auf ${fallen}`);
  check("steigt wieder auf die volle Stufe", r.tier === 0, `steht auf ${r.tier}`);
  check("steigt Stufe um Stufe", switches.join(",") === "1,0", `Verlauf ${switches.join(",")}`);
}

console.log("Zurueckhaltung beim Aufsteigen");
{
  const r = new GraphicsGovernor(2);
  feed(r, constantFrames(WARMUP_FRAMES + 200, 100));
  // Warmlauf plus *ein* gutes Fenster. Feste Zahlen, aus demselben Grund wie
  // oben: mit `RECOVERY_WINDOWS - 1` waere die Eingabe bei einem Wert von 1
  // leer und der Test bedeutungslos.
  const tight = feed(r, constantFrames(WARMUP_FRAMES + WINDOW_FRAMES, 8));
  check(
    "ein einzelnes gutes Fenster genuegt nicht",
    tight.length === 0,
    "stieg schon nach einem Fenster auf",
  );
  const enough = feed(r, constantFrames(WINDOW_FRAMES * RECOVERY_WINDOWS, 8));
  check("mehrere gute Fenster genuegen", enough.join(",") === "0", `Verlauf ${enough.join(",")}`);
  check(
    "die Zurueckhaltung ist mehr als ein Fenster",
    RECOVERY_WINDOWS > 1,
    "ein einziges gutes Fenster genuegt laut Konstante - der Test oben misst nichts",
  );
}

console.log("Bildschirm mit 60 Hz");
{
  // Der Browser wartet auf den Bildwechsel. Auf einem 60-Hz-Bildschirm ist
  // 16,7 ms deshalb die untere Grenze - schneller *kann* kein Bild fertig
  // werden, auch wenn die Grafikeinheit langweilt.
  //
  // Liegt die Komfortschwelle darunter, faellt der Regler zwar, kommt aber nie
  // wieder hoch. Genau so stand es hier zuerst, und aufgefallen ist es erst,
  // als jemand seine Bildrate genannt hat. Ein Regler, der nur in eine
  // Richtung regelt, ist keiner.
  const HZ60 = 1000 / 60;
  const r = new GraphicsGovernor(3);
  feed(r, constantFrames(WARMUP_FRAMES + 400, 100));
  const fallen = r.tier;
  const switches = feed(r, constantFrames(WARMUP_FRAMES + WINDOW_FRAMES * 40, HZ60));
  check("war vorher unten", fallen === 2, `stand auf ${fallen}`);
  check(
    "perfekte 60 Bilder je Sekunde holen die volle Stufe zurueck",
    r.tier === 0,
    `steht auf ${r.tier} nach ${switches.length} Aufstiegen`,
  );
  check(
    "die Schwelle ist auf 60 Hz ueberhaupt erreichbar",
    COMFORT_MS >= HZ60,
    `${COMFORT_MS} ms liegen unter den ${HZ60.toFixed(1)} ms, die ein ` +
      "60-Hz-Bildschirm bestenfalls zulaesst - der Weg nach oben ist dort tot",
  );

  // Gegenprobe: fuenfzig Bilder je Sekunde bedeuten, dass jedes sechste Bild
  // ausfaellt. Das ist nicht mehr "schnell genug".
  const s50 = new GraphicsGovernor(3);
  feed(s50, constantFrames(WARMUP_FRAMES + 400, 100));
  const at50 = feed(s50, constantFrames(WARMUP_FRAMES + WINDOW_FRAMES * 40, 20));
  check(
    "50 Bilder je Sekunde genuegen dafuer nicht",
    at50.length === 0,
    `${at50.length} Aufstiege - die Schwelle sitzt zu locker`,
  );
}

console.log("Hysterese");
{
  // Dreiundzwanzig Millisekunden sind gut vierzig Bilder je Sekunde: fluessig
  // genug, um nichts abzuschalten, aber nicht so gut, dass man riskieren
  // wollte, wieder aufzudrehen. Hier darf gar nichts passieren.
  //
  // Feste Zahl, nicht die Mitte zwischen den Schwellen: eine mitwandernde
  // Mitte liegt per Konstruktion immer im toten Bereich, egal wie schmal der
  // ist. Der Test bestuende noch bei einer Hysterese von einer Millisekunde.
  const DEAD_MS = 23;
  const r = new GraphicsGovernor(3, 1);
  const switches = feed(r, constantFrames(WARMUP_FRAMES + WINDOW_FRAMES * 40, DEAD_MS));
  check(`${DEAD_MS} ms loest nichts aus`, switches.length === 0, `${switches.length} Wechsel`);
  check(
    "der tote Bereich ist breit genug",
    COMFORT_MS <= DEAD_MS - 4 && BUDGET_MS >= DEAD_MS + 4,
    `${COMFORT_MS}..${BUDGET_MS} ms laesst um ${DEAD_MS} ms herum zu wenig Luft`,
  );
}

console.log("Kein Pendeln");
{
  // Eine Maschine, die abwechselnd gut und schlecht laeuft. Ohne Deckel
  // wechselte sie endlos die Stufe; das Flackern waere schlimmer als jede
  // feste Wahl.
  const r = new GraphicsGovernor(2);
  const times = [];
  for (let i = 0; i < 60; i++) {
    times.push(...constantFrames(WARMUP_FRAMES + WINDOW_FRAMES, 100));
    times.push(...constantFrames(WARMUP_FRAMES + WINDOW_FRAMES * RECOVERY_WINDOWS, 8));
  }
  const switches = feed(r, times);
  check(
    "kommt zur Ruhe",
    switches.length <= MAX_ATTEMPTS * 2,
    `${switches.length} Wechsel bei sechzig Wechseln der Last`,
  );
  check("bleibt am Ende unten", r.tier === 1, `steht auf ${r.tier}`);
}

console.log("Median statt Mittelwert");
{
  // Ein einzelnes langes Bild - eine Speicherbereinigung, die Rueckkehr aus
  // einem anderen Tab - darf die Stufe nicht kosten. Der Mittelwert dieses
  // Fensters liegt bei 24 ms und damit klar ueber dem Komfortwert; der Median
  // liegt bei 8.
  const r = new GraphicsGovernor(2, 1);
  const times = constantFrames(WARMUP_FRAMES, 8);
  for (let f = 0; f < RECOVERY_WINDOWS; f++) {
    times.push(1000, ...constantFrames(WINDOW_FRAMES - 1, 8));
  }
  const switches = feed(r, times);
  check(
    "ein Ausreisser verhindert den Aufstieg nicht",
    switches.join(",") === "0",
    `Verlauf ${switches.join(",")}`,
  );

  // Gegenprobe: waere es der Mittelwert, muesste derselbe Verlauf scheitern.
  const mean =
    times.slice(WARMUP_FRAMES, WARMUP_FRAMES + WINDOW_FRAMES).reduce((a, b) => a + b, 0) /
    WINDOW_FRAMES;
  check(
    "die Gegenprobe hat Zaehne",
    mean > COMFORT_MS,
    `Mittelwert ${mean.toFixed(1)} ms liegt unter ${COMFORT_MS} ms - ` +
      "der Ausreisser ist zu klein, der Test wuerde auch mit Mittelwert bestehen",
  );
}

process.exit(errors);
