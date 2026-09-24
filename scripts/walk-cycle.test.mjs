// Prueft den Laufzyklus der Spielerfiguren.
//
//     node scripts/walk-cycle.test.mjs
//
// Im Browser laesst sich davon nur der Augenschein pruefen, und der taugt
// gerade fuer die unauffaelligen Faelle nicht: ob eine stehende Figur die
// Beine wirklich zusammennimmt, ob Arme und Beine gegengleich schwingen und ob
// ein Wiedereinstieg die Figur nicht einmal quer durchrudern laesst, sieht man
// bei zehn Bildern je Sekunde nicht. Deshalb wird der Zyklus hier mit
// erfundenen Strecken gefuettert.
//
// `walk-cycle.js` importiert bewusst nichts - kein Three, kein DOM -, damit
// genau das hier ohne Browser geht.
import { FULL_SPEED, limbPose, rest, advanceWalk } from "../client/js/walk-cycle.js";

let errors = 0;

function check(name, condition, hint) {
  if (condition) {
    console.log(`  ok    ${name}`);
  } else {
    console.error(`  FEHLER ${name}: ${hint}`);
    errors = 1;
  }
}

/**
 * Laesst eine Figur `seconds` lang mit `tempo` laufen.
 *
 * Ganze Schritte statt einer aufsummierten Zeit. `for (t = 0; t < s; t += dt)`
 * sammelt Rundungsfehler: bei einer Sekunde und 1/60 landet die Summe knapp
 * unter 1.0, und die Schleife laeuft ein 61. Mal. Der Vergleich "gleiche
 * Strecke, gleiche Phase" weiter unten ist dann um genau diesen einen Schritt
 * daneben - und das sah aus wie ein Fehler im geprueften Code.
 */
function walk(state, tempo, seconds, dt = 1 / 60) {
  const steps = Math.round(seconds / dt);
  for (let i = 0; i < steps; i++) {
    state = advanceWalk(state, tempo * dt, dt);
  }
  return state;
}

/** Groesster Ausschlag irgendeines Glieds. */
function amplitude(state) {
  const w = limbPose(state);
  return Math.max(
    Math.abs(w.legLeft),
    Math.abs(w.legRight),
    Math.abs(w.armLeft),
    Math.abs(w.armRight),
    Math.abs(w.bob),
  );
}

console.log("Stehen");
{
  const still = walk(rest(), 0, 3);
  check(
    "bewegt nichts",
    amplitude(still) < 1e-6,
    `Ausschlag ${amplitude(still).toExponential(2)} im Stand`,
  );
  check("laesst die Phase stehen", still.phase === 0, `Phase ${still.phase}`);
}

console.log("Laufen");
{
  // Eine Vierteldrehung der Phase: mitten im Schritt, nicht zufaellig in der
  // Ruhelage. Die Phase wird gesetzt statt "irgendwie lange" erlaufen - wie
  // Strecke zu Phase wird, prueft der Abschnitt weiter unten.
  let z = walk(rest(), FULL_SPEED, 2); // Ausschlag hochfahren
  z = { phase: Math.PI / 2, amplitude: z.amplitude };
  const w = limbPose(z);

  check("schlaegt aus", Math.abs(w.legLeft) > 0.3, `linkes Bein nur ${w.legLeft.toFixed(3)}`);
  check(
    "Beine gegengleich",
    w.legLeft * w.legRight < 0,
    `beide Beine in dieselbe Richtung (${w.legLeft.toFixed(2)} / ${w.legRight.toFixed(2)}) - das waere Huepfen`,
  );
  check(
    "Arm gegengleich zum Bein derselben Seite",
    w.legLeft * w.armLeft < 0,
    `linkes Bein ${w.legLeft.toFixed(2)}, linker Arm ${w.armLeft.toFixed(2)}`,
  );
  check(
    "Arme gegengleich zueinander",
    w.armLeft * w.armRight < 0,
    `beide Arme in dieselbe Richtung`,
  );
  check(
    "Arme schwingen weniger als Beine",
    Math.abs(w.armLeft) < Math.abs(w.legLeft),
    "Arme schwingen mindestens so weit wie die Beine",
  );
  check("sinkt ein statt zu schweben", w.bob <= 0, `Wippen ${w.bob.toFixed(4)} zeigt nach oben`);
  check(
    "genau ein Knie gebeugt",
    w.kneeLeft > 0 !== w.kneeRight > 0,
    `beide oder keins: ${w.kneeLeft.toFixed(2)} / ${w.kneeRight.toFixed(2)}`,
  );
  check(
    "Knie beugen nur in eine Richtung",
    w.kneeLeft >= 0 && w.kneeRight >= 0,
    "ein Knie beugt nach vorn",
  );
}

console.log("Phase haengt an der Strecke, nicht an der Zeit");
{
  // Dieselbe Strecke, einmal langsam und lange, einmal schnell und kurz. Die
  // Phase muss gleich sein - sonst staksen langsame Figuren auf der Stelle.
  const slow = walk(rest(), 1.0, 4.0);
  const fast = walk(rest(), 4.0, 1.0);
  const deviation = Math.abs(slow.phase - fast.phase);
  check(
    "gleiche Strecke, gleiche Phase",
    deviation < 1e-9,
    `${slow.phase.toFixed(6)} gegen ${fast.phase.toFixed(6)}`,
  );
  check(
    "die Gegenprobe hat Zaehne",
    slow.phase > 0.5,
    `Phase ${slow.phase} - beide Laeufe waren zu kurz, um etwas zu zeigen`,
  );
}

console.log("Anhalten");
{
  const z = walk(rest(), FULL_SPEED, 2);
  check("lief vorher", amplitude(z) > 0.3, `Ausschlag ${amplitude(z).toFixed(3)}`);

  const afterOneSecond = walk(z, 0, 1.0);
  check(
    "Glieder kommen zur Ruhe",
    amplitude(afterOneSecond) < 0.01,
    `nach einer Sekunde Stillstand noch ${amplitude(afterOneSecond).toFixed(4)}`,
  );

  const afterThree = walk(afterOneSecond, 0, 2.0);
  check(
    "und bleiben dort",
    amplitude(afterThree) <= amplitude(afterOneSecond) + 1e-9,
    "der Ausschlag waechst wieder",
  );
}

console.log("Langsam gehen");
{
  // Halbes Tempo, halber Ausschlag: wer sich an einer Wand entlangschiebt,
  // soll nicht marschieren.
  const half = walk(rest(), FULL_SPEED / 2, 2);
  const fullRatio = walk(rest(), FULL_SPEED, 2);
  check(
    "schwingt weniger aus als volles Tempo",
    half.amplitude < fullRatio.amplitude * 0.7,
    `${half.amplitude.toFixed(3)} gegen ${fullRatio.amplitude.toFixed(3)}`,
  );
  check(
    "schwingt aber ueberhaupt aus",
    half.amplitude > 0.2,
    `${half.amplitude.toFixed(3)} - halbes Tempo bewegt gar nichts`,
  );
}

console.log("Zahlenbereich");
{
  // Nach einem Marathon darf die Phase nicht ins Grobe gewachsen sein: bei
  // einer Million Radiant liegen zwischen zwei f64-Nachbarn mehr als ein Grad,
  // und der Gang faengt an zu zittern.
  const far = walk(rest(), FULL_SPEED, 600);
  check(
    "Phase bleibt klein",
    Math.abs(far.phase) <= Math.PI * 2,
    `Phase ${far.phase} nach zehn Minuten Laufen`,
  );
  check(
    "und ist eine Zahl",
    Number.isFinite(far.phase) && Number.isFinite(far.amplitude),
    `Phase ${far.phase}, Ausschlag ${far.amplitude}`,
  );
}

process.exit(errors);
