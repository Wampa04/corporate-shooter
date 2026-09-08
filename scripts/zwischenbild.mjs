// Prueft den Zwischenbild-Ausgleich der Vorhersage.
//
//     node scripts/zwischenbild.mjs
//
// Vorhergesagt wird mit der Tickrate (30 Hz), gezeichnet mit der Bildrate
// (hier 120 Hz). Ohne Ausgleich stuende die Kamera drei von vier Bildern still
// und spraenge dann achtzehn Zentimeter - das liest sich als Ruckeln, obwohl
// die Vorhersage stimmt. Genau so hat es sich angefuehlt, bevor es das gab.
//
// Laeuft ohne Browser: das Modul wird durch eine Attrappe ersetzt, die je
// Schritt eine feste Strecke zurueckgeht. Geprueft wird damit der Ausgleich
// selbst, nicht die Bewegung - die deckt `scripts/gleichlauf.sh` ab.
import { Prediction } from "../client/js/predict.js";

const SCHRITT = 0.18;   // Strecke je Simulationsschritt bei 5.4 m/s
const TICK = 1 / 30;
const BILD = 1 / 120;
const BILDER = 240;

/** Ersatzmodul: `step` schiebt die Position um `SCHRITT` nach vorn. */
function attrappe(speicher) {
  return {
    exports: {
      memory: { buffer: speicher.buffer },
      scratch_ptr: () => 0,
      scratch_len: () => 1 << 20,
      state_ptr: () => 0,
      load_level: () => 1,
      load_config: () => 1,
      step: () => { speicher[2] -= SCHRITT; return 1; },
    },
  };
}

/** Laesst eine Vorhersage laufen und gibt den Weg je Bild zurueck. */
function laufen({ ausgleich }) {
  const speicher = new Float32Array(16);
  const p = new Prediction(attrappe(speicher));
  p.init({}, { tick_rate: 30 });
  p.reconcile(
    { pos: [0, 0, 0], yaw: 0, pitch: 0, alive: true },
    { on_ground: true },
    0,
  );

  const wege = [];
  let vor = null;
  let konto = 0;
  for (let i = 0; i < BILDER; i++) {
    // Genau wie die Bildschleife: ein Zeitkonto treibt Schritte *und* den
    // Mischfaktor, damit es nur eine Uhr gibt.
    konto += BILD;
    while (konto >= TICK) {
      konto -= TICK;
      p.advance({ seq: i, move_x: 0, move_z: 1, yaw: 0, pitch: 0, buttons: 0 }, true);
    }
    // Ohne Ausgleich: der rohe Vorhersagestand, wie er vor dem Einbau war.
    const q = ausgleich
      ? p.position(BILD, konto / TICK)
      : { x: speicher[0], z: speicher[2] };
    if (vor) wege.push(Math.hypot(q.x - vor.x, q.z - vor.z));
    vor = { x: q.x, z: q.z };
  }
  return wege;
}

function kennzahlen(wege) {
  const mittel = wege.reduce((a, b) => a + b, 0) / wege.length;
  const streuung = Math.sqrt(
    wege.reduce((a, b) => a + (b - mittel) ** 2, 0) / wege.length);
  return {
    still: wege.filter((w) => w < 1e-9).length,
    von: wege.length,
    gleichmaessigkeit: streuung / mittel,
  };
}

const mit = kennzahlen(laufen({ ausgleich: true }));
const ohne = kennzahlen(laufen({ ausgleich: false }));

console.log(`mit Ausgleich:   ${mit.still}/${mit.von} Bilder ohne Bewegung, ` +
            `Ungleichmaessigkeit ${mit.gleichmaessigkeit.toFixed(3)}`);
console.log(`ohne Ausgleich:  ${ohne.still}/${ohne.von} Bilder ohne Bewegung, ` +
            `Ungleichmaessigkeit ${ohne.gleichmaessigkeit.toFixed(3)}`);

let fehler = 0;
if (mit.still > mit.von * 0.05) {
  console.error(`FEHLER: zu viele stehende Bilder (${mit.still})`);
  fehler = 1;
}
if (mit.gleichmaessigkeit > 0.3) {
  console.error(`FEHLER: Bewegung zu ungleichmaessig (${mit.gleichmaessigkeit.toFixed(3)})`);
  fehler = 1;
}
// Gegenprobe: ohne Ausgleich *muss* es ruckeln. Tut es das nicht, misst der
// Test nicht, was er zu messen vorgibt.
if (ohne.gleichmaessigkeit < 1.0) {
  console.error("FEHLER: die Gegenprobe ruckelt nicht - der Test misst nichts");
  fehler = 1;
}
process.exit(fehler);
