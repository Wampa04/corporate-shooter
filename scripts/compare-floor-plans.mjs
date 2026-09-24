// Vergleicht zwei Grundriss-Dumps Box für Box.
//
//     server --dump-map vorher/ && server --dump-map nachher/
//     node scripts/compare-floor-plans.mjs vorher/map.json nachher/map.json [toleranz_mm]
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

const [, , distanceA, distanceB, mmArg] = process.argv;
if (!distanceA || !distanceB) {
  console.error(
    "Aufruf: node scripts/compare-floor-plans.mjs vorher/map.json nachher/map.json [toleranz_mm]",
  );
  process.exit(2);
}
const TOLERANCE = Number(mmArg ?? 0.1) / 1000;

const a = JSON.parse(readFileSync(distanceA, "utf8"));
const b = JSON.parse(readFileSync(distanceB, "utf8"));

let errors = 0;
const notice = (text) => {
  console.error(`FEHLER: ${text}`);
  errors = 1;
};

if (a.name !== b.name) notice(`Name: "${a.name}" -> "${b.name}"`);

// Vec3 kommt als Array [x, y, z] an, nicht als Objekt - glam serialisiert so.
const corners = (aabb) => [...aabb.min, ...aabb.max];

// --- Spielfeldgrenze -------------------------------------------------------
{
  const dev = Math.max(...corners(a.bounds).map((v, i) => Math.abs(v - corners(b.bounds)[i])));
  if (dev > TOLERANCE) notice(`Spielfeldgrenze weicht um ${(dev * 1000).toFixed(3)} mm ab`);
}

// --- Spawnpunkte -----------------------------------------------------------
if (a.spawns.length !== b.spawns.length) {
  notice(`Spawnpunkte: ${a.spawns.length} -> ${b.spawns.length}`);
} else {
  let worst = 0;
  for (let i = 0; i < a.spawns.length; i++) {
    const p = a.spawns[i],
      q = b.spawns[i];
    worst = Math.max(
      worst,
      Math.abs(p.yaw - q.yaw),
      ...p.pos.map((v, k) => Math.abs(v - q.pos[k])),
    );
  }
  if (worst > TOLERANCE) notice(`Spawnpunkt weicht um ${(worst * 1000).toFixed(3)} mm ab`);
}

// --- Boxen -----------------------------------------------------------------
if (a.brushes.length !== b.brushes.length) {
  notice(`Boxen: ${a.brushes.length} -> ${b.brushes.length}`);
  countKinds();
  process.exit(1);
}

let worst = 0;
let worstIndex = -1;
let kindChanges = 0;
for (let i = 0; i < a.brushes.length; i++) {
  const p = a.brushes[i],
    q = b.brushes[i];
  if (p.kind !== q.kind) {
    if (kindChanges < 5) notice(`Box ${i}: Art ${p.kind} -> ${q.kind}`);
    kindChanges++;
    continue;
  }
  const ea = corners(p.aabb),
    eb = corners(q.aabb);
  if (ea.length !== 6 || eb.length !== 6 || ea.some((v) => !Number.isFinite(v))) {
    notice(`Box ${i}: Ecken nicht lesbar (${JSON.stringify(p.aabb)})`);
    break;
  }
  for (let k = 0; k < 6; k++) {
    const d = Math.abs(ea[k] - eb[k]);
    if (d > worst) {
      worst = d;
      worstIndex = i;
    }
  }
}
if (kindChanges > 5) notice(`... und ${kindChanges - 5} weitere Artwechsel`);

if (kindChanges) countKinds();

console.log(`${a.brushes.length} Boxen verglichen.`);
console.log(
  `groesste Abweichung: ${(worst * 1000).toFixed(4)} mm` +
    (worstIndex >= 0 ? ` (Box ${worstIndex}, ${a.brushes[worstIndex].kind})` : ""),
);

if (worst > TOLERANCE) {
  notice(`ueber der Toleranz von ${(TOLERANCE * 1000).toFixed(3)} mm`);
}

function countKinds() {
  const count = (list) => list.reduce((m, x) => m.set(x.kind, (m.get(x.kind) ?? 0) + 1), new Map());
  const ca = count(a.brushes),
    cb = count(b.brushes);
  for (const brushKind of new Set([...ca.keys(), ...cb.keys()])) {
    const va = ca.get(brushKind) ?? 0,
      vb = cb.get(brushKind) ?? 0;
    if (va !== vb) console.error(`  ${brushKind}: ${va} -> ${vb}`);
  }
}

process.exit(errors);
