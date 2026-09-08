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

const [dir, wasmPfad] = process.argv.slice(2);

/** Wie viele unbestaetigte Eingaben der Client hoechstens nachspielt. */
const FENSTER = 12;
/** Groesste hinnehmbare Abweichung am Ende eines Fensters, in Metern. */
const GRENZE = 0.001;

const { instance } = await WebAssembly.instantiate(readFileSync(wasmPfad), {});
const x = instance.exports;

function reichEin(text, fn) {
  const bytes = new TextEncoder().encode(text);
  if (bytes.length > x.scratch_len()) throw new Error("Puffer zu klein");
  // Erst den Zeiger holen, dann die Sicht anlegen: der erste Aufruf legt den
  // Puffer an, laesst den WASM-Speicher wachsen und entkoppelt damit jede
  // vorher angelegte Sicht.
  const ptr = x.scratch_ptr();
  new Uint8Array(x.memory.buffer).set(bytes, ptr);
  if (fn(bytes.length) !== 1) throw new Error("Modul hat die Eingabe abgelehnt");
}

reichEin(readFileSync(`${dir}/map.json`, "utf8"), x.load_level);
reichEin(readFileSync(`${dir}/config.json`, "utf8"), x.load_config);

const roh = new Uint32Array(1);
const alsFloat = new Float32Array(roh.buffer);
const f32 = (u) => { roh[0] = Number(u) >>> 0; return alsFloat[0]; };

// Erst nach dem Laden anlegen: das Parsen der Karte belegt Speicher und kann
// ihn wachsen lassen, was jede aeltere Sicht entkoppeln wuerde.
const state = new Float32Array(x.memory.buffer, x.state_ptr(), 13);

const bahn = readFileSync(`${dir}/bahn_rust.txt`, "utf8").trim().split("\n")
  .map((z) => { const t = z.split(" "); return { lauf: t[0], zustand: t.slice(1) }; });
const eingaben = readFileSync(`${dir}/eingaben.txt`, "utf8").trim().split("\n")
  .map((z) => z.split(" "));

let schlimmste = 0, schlimmsteStelle = "", fenster = 0;

for (let start = 0; start + FENSTER < bahn.length; start += FENSTER) {
  // Ein Fenster darf keine Laufgrenze ueberschreiten.
  if (bahn[start].lauf !== bahn[start + FENSTER].lauf) continue;

  // Auf den Serverzustand setzen, wie es der Client bei jedem Snapshot tut.
  for (let k = 0; k < 13; k++) state[k] = f32(bahn[start].zustand[k]);
  let prev = Number(eingaben[start][4]);

  for (let i = start + 1; i <= start + FENSTER; i++) {
    const [, mx, mz, yaw, btn] = eingaben[i];
    if (x.step(f32(mx), f32(mz), f32(yaw), 0, Number(btn), prev, 1) !== 1) {
      throw new Error("step abgelehnt");
    }
    prev = Number(btn);
  }

  const soll = bahn[start + FENSTER].zustand.slice(0, 3).map(f32);
  const ab = Math.hypot(state[0] - soll[0], state[1] - soll[1], state[2] - soll[2]);
  if (ab > schlimmste) {
    schlimmste = ab;
    schlimmsteStelle = `${bahn[start].lauf}, Tick ${start}`;
  }
  fenster++;
}

console.log(`${fenster} Fenster a ${FENSTER} Ticks geprueft.`);
console.log(`groesste Abweichung: ${(schlimmste * 1000).toFixed(4)} mm (${schlimmsteStelle})`);
if (schlimmste > GRENZE) {
  console.error(`ueber der Grenze von ${GRENZE * 1000} mm`);
  process.exit(1);
}
