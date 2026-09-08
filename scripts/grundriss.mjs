// Vergleicht zwei Grundriss-Dumps Box für Box.
//
//     server --dump-map vorher/ && server --dump-map nachher/
//     node scripts/grundriss.mjs vorher/map.json nachher/map.json [toleranz_mm]
//
// Wozu: ein Umbau der Kartenquelle, der nichts am Ergebnis ändern soll, ist
// sonst eine Behauptung. Ein reiner Textvergleich taugt dafür nicht - er
// bricht schon an der letzten Nachkommastelle, und die ist beim Verschieben
// in ein lokales Koordinatensystem unvermeidlich: (x - ursprung) + ursprung
// ist in f32 nicht wieder x. Bei x = -19,6 und Ursprung -16 fehlt knapp ein
// halbes Tausendstel Millimeter.
//
// Verglichen wird deshalb mit Toleranz - aber mit einer, die kleiner ist als
// alles, was im Spiel eine Rolle spielen kann. Ein Zehntelmillimeter
// verschiebt weder eine Kollision noch ein Pixel.
import { readFileSync } from "node:fs";

const [, , wegA, wegB, mmArg] = process.argv;
if (!wegA || !wegB) {
  console.error("Aufruf: node scripts/grundriss.mjs vorher/map.json nachher/map.json [toleranz_mm]");
  process.exit(2);
}
const TOLERANZ = Number(mmArg ?? 0.1) / 1000;

const a = JSON.parse(readFileSync(wegA, "utf8"));
const b = JSON.parse(readFileSync(wegB, "utf8"));

let fehler = 0;
const meldung = (text) => { console.error(`FEHLER: ${text}`); fehler = 1; };

if (a.name !== b.name) meldung(`Name: "${a.name}" -> "${b.name}"`);

// Vec3 kommt als Array [x, y, z] an, nicht als Objekt - glam serialisiert so.
const ecken = (aabb) => [...aabb.min, ...aabb.max];

// --- Spielfeldgrenze -------------------------------------------------------
{
  const abw = Math.max(...ecken(a.bounds).map((v, i) => Math.abs(v - ecken(b.bounds)[i])));
  if (abw > TOLERANZ) meldung(`Spielfeldgrenze weicht um ${(abw * 1000).toFixed(3)} mm ab`);
}

// --- Spawnpunkte -----------------------------------------------------------
if (a.spawns.length !== b.spawns.length) {
  meldung(`Spawnpunkte: ${a.spawns.length} -> ${b.spawns.length}`);
} else {
  let schlimmster = 0;
  for (let i = 0; i < a.spawns.length; i++) {
    const p = a.spawns[i], q = b.spawns[i];
    schlimmster = Math.max(schlimmster, Math.abs(p.yaw - q.yaw),
      ...p.pos.map((v, k) => Math.abs(v - q.pos[k])));
  }
  if (schlimmster > TOLERANZ) meldung(`Spawnpunkt weicht um ${(schlimmster * 1000).toFixed(3)} mm ab`);
}

// --- Boxen -----------------------------------------------------------------
if (a.brushes.length !== b.brushes.length) {
  meldung(`Boxen: ${a.brushes.length} -> ${b.brushes.length}`);
  zaehleArten();
  process.exit(1);
}

let schlimmster = 0;
let schlimmsterIndex = -1;
let artWechsel = 0;
for (let i = 0; i < a.brushes.length; i++) {
  const p = a.brushes[i], q = b.brushes[i];
  if (p.kind !== q.kind) {
    if (artWechsel < 5) meldung(`Box ${i}: Art ${p.kind} -> ${q.kind}`);
    artWechsel++;
    continue;
  }
  const ea = ecken(p.aabb), eb = ecken(q.aabb);
  if (ea.length !== 6 || eb.length !== 6 || ea.some((v) => !Number.isFinite(v))) {
    meldung(`Box ${i}: Ecken nicht lesbar (${JSON.stringify(p.aabb)})`);
    break;
  }
  for (let k = 0; k < 6; k++) {
    const d = Math.abs(ea[k] - eb[k]);
    if (d > schlimmster) { schlimmster = d; schlimmsterIndex = i; }
  }
}
if (artWechsel > 5) meldung(`... und ${artWechsel - 5} weitere Artwechsel`);

if (artWechsel) zaehleArten();

console.log(`${a.brushes.length} Boxen verglichen.`);
console.log(`groesste Abweichung: ${(schlimmster * 1000).toFixed(4)} mm` +
  (schlimmsterIndex >= 0 ? ` (Box ${schlimmsterIndex}, ${a.brushes[schlimmsterIndex].kind})` : ""));

if (schlimmster > TOLERANZ) {
  meldung(`ueber der Toleranz von ${(TOLERANZ * 1000).toFixed(3)} mm`);
}

function zaehleArten() {
  const zaehl = (liste) => liste.reduce((m, x) => m.set(x.kind, (m.get(x.kind) ?? 0) + 1), new Map());
  const ca = zaehl(a.brushes), cb = zaehl(b.brushes);
  for (const art of new Set([...ca.keys(), ...cb.keys()])) {
    const va = ca.get(art) ?? 0, vb = cb.get(art) ?? 0;
    if (va !== vb) console.error(`  ${art}: ${va} -> ${vb}`);
  }
}

process.exit(fehler);
