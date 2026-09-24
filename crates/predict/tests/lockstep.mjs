// Prueft, dass das WASM-Modul dieselbe Bewegung rechnet wie der Server.
//
// Geprueft wird, was der Client tatsaechlich tut: von einem Serverzustand aus
// ein kurzes Stueck nachrechnen. Zwischen zwei Snapshots liegen wenige Ticks -
// ueber die darf nichts Sichtbares auseinanderlaufen.
//
// Eine ganze Bahn am Stueck zu vergleichen waere der falsche Massstab: an
// einer Tuerkante entscheidet ein Hauch darueber, ob man den Rahmen streift,
// und danach liegen die Bahnen weit auseinander, ohne dass etwas kaputt waere.
// Der Client setzt ja gerade deshalb bei jedem Snapshot zurueck.
import { readFileSync } from "node:fs";

const [dir, wasmPath] = process.argv.slice(2);

/** Wie viele unbestaetigte Eingaben der Client hoechstens nachspielt. */
const WINDOW = 12;
/** Groesste hinnehmbare Abweichung am Ende eines Fensters, in Metern. */
const LIMIT = 0.001;

const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), {});
// Die Exporte sind reine Funktionen und ein Speicher; ihre Form steht in
// crates/predict/src/lib.rs, nicht in einer Typbeschreibung.
const x = /** @type {any} */ (instance.exports);

function submit(text, fn) {
  const bytes = new TextEncoder().encode(text);
  if (bytes.length > x.scratch_len()) throw new Error("Puffer zu klein");
  // Erst den Zeiger holen, dann die Sicht anlegen: der erste Aufruf legt den
  // Puffer an, laesst den WASM-Speicher wachsen und entkoppelt damit jede
  // vorher angelegte Sicht.
  const ptr = x.scratch_ptr();
  new Uint8Array(x.memory.buffer).set(bytes, ptr);
  if (fn(bytes.length) !== 1) throw new Error("Modul hat die Eingabe abgelehnt");
}

submit(readFileSync(`${dir}/map.json`, "utf8"), x.load_level);
submit(readFileSync(`${dir}/config.json`, "utf8"), x.load_config);

const raw = new Uint32Array(1);
const asFloat = new Float32Array(raw.buffer);
const f32 = (u) => {
  raw[0] = Number(u) >>> 0;
  return asFloat[0];
};

// Erst nach dem Laden anlegen: das Parsen der Karte belegt Speicher und kann
// ihn wachsen lassen, was jede aeltere Sicht entkoppeln wuerde.
const state = new Float32Array(x.memory.buffer, x.state_ptr(), 13);

const path = readFileSync(`${dir}/bahn_rust.txt`, "utf8")
  .trim()
  .split("\n")
  .map((z) => {
    const t = z.split(" ");
    return { walk: t[0], stateRow: t.slice(1) };
  });
const inputs = readFileSync(`${dir}/eingaben.txt`, "utf8")
  .trim()
  .split("\n")
  .map((z) => z.split(" "));

let worst = 0,
  worstAt = "",
  window = 0;

for (let start = 0; start + WINDOW < path.length; start += WINDOW) {
  // Ein Fenster darf keine Laufgrenze ueberschreiten.
  if (path[start].walk !== path[start + WINDOW].walk) continue;

  // Auf den Serverzustand setzen, wie es der Client bei jedem Snapshot tut.
  for (let k = 0; k < 13; k++) state[k] = f32(path[start].stateRow[k]);
  let prev = Number(inputs[start][4]);

  for (let i = start + 1; i <= start + WINDOW; i++) {
    const [, mx, mz, yaw, btn] = inputs[i];
    if (x.step(f32(mx), f32(mz), f32(yaw), 0, Number(btn), prev, 1) !== 1) {
      throw new Error("step abgelehnt");
    }
    prev = Number(btn);
  }

  const expected = path[start + WINDOW].stateRow.slice(0, 3).map(f32);
  const offBy = Math.hypot(state[0] - expected[0], state[1] - expected[1], state[2] - expected[2]);
  if (offBy > worst) {
    worst = offBy;
    worstAt = `${path[start].walk}, Tick ${start}`;
  }
  window++;
}

console.log(`${window} Fenster a ${WINDOW} Ticks geprueft.`);
console.log(`groesste Abweichung: ${(worst * 1000).toFixed(4)} mm (${worstAt})`);
if (worst > LIMIT) {
  console.error(`ueber der Grenze von ${LIMIT * 1000} mm`);
  process.exit(1);
}
