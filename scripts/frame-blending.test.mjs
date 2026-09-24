// Prueft den Zwischenbild-Ausgleich der Vorhersage.
//
//     node scripts/frame-blending.test.mjs
//
// Vorhergesagt wird mit der Tickrate (30 Hz), gezeichnet mit der Bildrate
// (hier 120 Hz). Ohne Ausgleich stuende die Kamera drei von vier Bildern still
// und spraenge dann achtzehn Zentimeter - das liest sich als Ruckeln, obwohl
// die Vorhersage stimmt. Genau so hat es sich angefuehlt, bevor es das gab.
//
// Laeuft ohne Browser: das Modul wird durch eine Attrappe ersetzt, die je
// Schritt eine feste Strecke zurueckgeht. Geprueft wird damit der Ausgleich
// selbst, nicht die Bewegung - die deckt `scripts/lockstep.sh` ab.
import { Prediction } from "../client/js/predict.js";

const STEP = 0.18; // Strecke je Simulationsschritt bei 5.4 m/s
const TICK = 1 / 30;
const FRAME = 1 / 120;
const FRAMES = 240;

/** Ersatzmodul: `step` schiebt die Position um `STEP` nach vorn. */
function dummy(wasmMemory) {
  return {
    exports: {
      memory: { buffer: wasmMemory.buffer },
      scratch_ptr: () => 0,
      scratch_len: () => 1 << 20,
      state_ptr: () => 0,
      load_level: () => 1,
      load_config: () => 1,
      step: () => {
        wasmMemory[2] -= STEP;
        return 1;
      },
    },
  };
}

/** Laesst eine Vorhersage laufen und gibt den Weg je Bild zurueck. */
function walk({ blend }) {
  const wasmMemory = new Float32Array(16);
  const p = new Prediction(dummy(wasmMemory));
  p.init({}, { tick_rate: 30 });
  p.reconcile({ pos: [0, 0, 0], yaw: 0, pitch: 0, alive: true }, { on_ground: true }, 0);

  const distances = [];
  let previous = null;
  let budget = 0;
  for (let i = 0; i < FRAMES; i++) {
    // Genau wie die Bildschleife: ein Zeitkonto treibt Schritte *und* den
    // Mischfaktor, damit es nur eine Uhr gibt.
    budget += FRAME;
    while (budget >= TICK) {
      budget -= TICK;
      p.advance({ seq: i, move_x: 0, move_z: 1, yaw: 0, pitch: 0, buttons: 0 }, true);
    }
    // Ohne Ausgleich: der rohe Vorhersagestand, wie er vor dem Einbau war.
    const q = blend ? p.position(FRAME, budget / TICK) : { x: wasmMemory[0], z: wasmMemory[2] };
    if (previous) distances.push(Math.hypot(q.x - previous.x, q.z - previous.z));
    previous = { x: q.x, z: q.z };
  }
  return distances;
}

function metrics(distances) {
  const mean = distances.reduce((a, b) => a + b, 0) / distances.length;
  const spread = Math.sqrt(distances.reduce((a, b) => a + (b - mean) ** 2, 0) / distances.length);
  return {
    still: distances.filter((w) => w < 1e-9).length,
    total: distances.length,
    unevenness: spread / mean,
  };
}

const withBlend = metrics(walk({ blend: true }));
const withoutBlend = metrics(walk({ blend: false }));

console.log(
  `mit Ausgleich:   ${withBlend.still}/${withBlend.total} Bilder ohne Bewegung, ` +
    `Ungleichmaessigkeit ${withBlend.unevenness.toFixed(3)}`,
);
console.log(
  `ohne Ausgleich:  ${withoutBlend.still}/${withoutBlend.total} Bilder ohne Bewegung, ` +
    `Ungleichmaessigkeit ${withoutBlend.unevenness.toFixed(3)}`,
);

let errors = 0;
if (withBlend.still > withBlend.total * 0.05) {
  console.error(`FEHLER: zu viele stehende Bilder (${withBlend.still})`);
  errors = 1;
}
if (withBlend.unevenness > 0.3) {
  console.error(`FEHLER: Bewegung zu ungleichmaessig (${withBlend.unevenness.toFixed(3)})`);
  errors = 1;
}
// Gegenprobe: ohne Ausgleich *muss* es ruckeln. Tut es das nicht, misst der
// Test nicht, was er zu messen vorgibt.
if (withoutBlend.unevenness < 1.0) {
  console.error("FEHLER: die Gegenprobe ruckelt nicht - der Test misst nichts");
  errors = 1;
}
process.exit(errors);
