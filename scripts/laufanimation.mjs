// Prueft den Laufzyklus der Spielerfiguren.
//
//     node scripts/laufanimation.mjs
//
// Im Browser laesst sich davon nur der Augenschein pruefen, und der taugt
// gerade fuer die unauffaelligen Faelle nicht: ob eine stehende Figur die
// Beine wirklich zusammennimmt, ob Arme und Beine gegengleich schwingen und ob
// ein Wiedereinstieg die Figur nicht einmal quer durchrudern laesst, sieht man
// bei zehn Bildern je Sekunde nicht. Deshalb wird der Zyklus hier mit
// erfundenen Strecken gefuettert.
//
// `laufzyklus.js` importiert bewusst nichts - kein Three, kein DOM -, damit
// genau das hier ohne Browser geht.
import {
  PHASE_JE_METER,
  VOLLES_TEMPO,
  gliedmassen,
  ruhe,
  schritt,
} from "../client/js/laufzyklus.js";

let fehler = 0;

function pruefe(name, bedingung, hinweis) {
  if (bedingung) {
    console.log(`  ok    ${name}`);
  } else {
    console.error(`  FEHLER ${name}: ${hinweis}`);
    fehler = 1;
  }
}

/**
 * Laesst eine Figur `sekunden` lang mit `tempo` laufen.
 *
 * Ganze Schritte statt einer aufsummierten Zeit. `for (t = 0; t < s; t += dt)`
 * sammelt Rundungsfehler: bei einer Sekunde und 1/60 landet die Summe knapp
 * unter 1.0, und die Schleife laeuft ein 61. Mal. Der Vergleich "gleiche
 * Strecke, gleiche Phase" weiter unten ist dann um genau diesen einen Schritt
 * daneben - und das sah aus wie ein Fehler im geprueften Code.
 */
function laufen(zustand, tempo, sekunden, dt = 1 / 60) {
  const schritte = Math.round(sekunden / dt);
  for (let i = 0; i < schritte; i++) {
    zustand = schritt(zustand, tempo * dt, dt);
  }
  return zustand;
}

/** Groesster Ausschlag irgendeines Glieds. */
function ausschlag(zustand) {
  const w = gliedmassen(zustand);
  return Math.max(
    Math.abs(w.beinLinks), Math.abs(w.beinRechts),
    Math.abs(w.armLinks), Math.abs(w.armRechts),
    Math.abs(w.wippen),
  );
}

console.log("Stehen");
{
  const still = laufen(ruhe(), 0, 3);
  pruefe("bewegt nichts", ausschlag(still) < 1e-6,
    `Ausschlag ${ausschlag(still).toExponential(2)} im Stand`);
  pruefe("laesst die Phase stehen", still.phase === 0, `Phase ${still.phase}`);
}

console.log("Laufen");
{
  // Eine Vierteldrehung der Phase: mitten im Schritt, nicht zufaellig in der
  // Ruhelage. Feste Strecke statt "irgendwie lange laufen" - ein Test, der
  // sich seine Phase aus den geprueften Konstanten ausrechnet, kann nicht
  // scheitern.
  const strecke = (Math.PI / 2) / PHASE_JE_METER;
  let z = laufen(ruhe(), VOLLES_TEMPO, 2);      // Ausschlag hochfahren
  z = { phase: Math.PI / 2, ausschlag: z.ausschlag };
  const w = gliedmassen(z);

  pruefe("schlaegt aus", Math.abs(w.beinLinks) > 0.3,
    `linkes Bein nur ${w.beinLinks.toFixed(3)}`);
  pruefe("Beine gegengleich", w.beinLinks * w.beinRechts < 0,
    `beide Beine in dieselbe Richtung (${w.beinLinks.toFixed(2)} / ${w.beinRechts.toFixed(2)}) - das waere Huepfen`);
  pruefe("Arm gegengleich zum Bein derselben Seite", w.beinLinks * w.armLinks < 0,
    `linkes Bein ${w.beinLinks.toFixed(2)}, linker Arm ${w.armLinks.toFixed(2)}`);
  pruefe("Arme gegengleich zueinander", w.armLinks * w.armRechts < 0,
    `beide Arme in dieselbe Richtung`);
  pruefe("Arme schwingen weniger als Beine",
    Math.abs(w.armLinks) < Math.abs(w.beinLinks),
    "Arme schwingen mindestens so weit wie die Beine");
  pruefe("sinkt ein statt zu schweben", w.wippen <= 0,
    `Wippen ${w.wippen.toFixed(4)} zeigt nach oben`);
  pruefe("genau ein Knie gebeugt", (w.knieLinks > 0) !== (w.knieRechts > 0),
    `beide oder keins: ${w.knieLinks.toFixed(2)} / ${w.knieRechts.toFixed(2)}`);
  pruefe("Knie beugen nur in eine Richtung", w.knieLinks >= 0 && w.knieRechts >= 0,
    "ein Knie beugt nach vorn");
  const _ = strecke;
}

console.log("Phase haengt an der Strecke, nicht an der Zeit");
{
  // Dieselbe Strecke, einmal langsam und lange, einmal schnell und kurz. Die
  // Phase muss gleich sein - sonst staksen langsame Figuren auf der Stelle.
  const langsam = laufen(ruhe(), 1.0, 4.0);
  const schnell = laufen(ruhe(), 4.0, 1.0);
  const abweichung = Math.abs(langsam.phase - schnell.phase);
  pruefe("gleiche Strecke, gleiche Phase", abweichung < 1e-9,
    `${langsam.phase.toFixed(6)} gegen ${schnell.phase.toFixed(6)}`);
  pruefe("die Gegenprobe hat Zaehne", langsam.phase > 0.5,
    `Phase ${langsam.phase} - beide Laeufe waren zu kurz, um etwas zu zeigen`);
}

console.log("Anhalten");
{
  let z = laufen(ruhe(), VOLLES_TEMPO, 2);
  pruefe("lief vorher", ausschlag(z) > 0.3, `Ausschlag ${ausschlag(z).toFixed(3)}`);

  const nachEinerSekunde = laufen(z, 0, 1.0);
  pruefe("Glieder kommen zur Ruhe", ausschlag(nachEinerSekunde) < 0.01,
    `nach einer Sekunde Stillstand noch ${ausschlag(nachEinerSekunde).toFixed(4)}`);

  const nachDrei = laufen(nachEinerSekunde, 0, 2.0);
  pruefe("und bleiben dort", ausschlag(nachDrei) <= ausschlag(nachEinerSekunde) + 1e-9,
    "der Ausschlag waechst wieder");
}

console.log("Langsam gehen");
{
  // Halbes Tempo, halber Ausschlag: wer sich an einer Wand entlangschiebt,
  // soll nicht marschieren.
  const halb = laufen(ruhe(), VOLLES_TEMPO / 2, 2);
  const voll = laufen(ruhe(), VOLLES_TEMPO, 2);
  pruefe("schwingt weniger aus als volles Tempo", halb.ausschlag < voll.ausschlag * 0.7,
    `${halb.ausschlag.toFixed(3)} gegen ${voll.ausschlag.toFixed(3)}`);
  pruefe("schwingt aber ueberhaupt aus", halb.ausschlag > 0.2,
    `${halb.ausschlag.toFixed(3)} - halbes Tempo bewegt gar nichts`);
}

console.log("Zahlenbereich");
{
  // Nach einem Marathon darf die Phase nicht ins Grobe gewachsen sein: bei
  // einer Million Radiant liegen zwischen zwei f64-Nachbarn mehr als ein Grad,
  // und der Gang faengt an zu zittern.
  const weit = laufen(ruhe(), VOLLES_TEMPO, 600);
  pruefe("Phase bleibt klein", Math.abs(weit.phase) <= Math.PI * 2,
    `Phase ${weit.phase} nach zehn Minuten Laufen`);
  pruefe("und ist eine Zahl", Number.isFinite(weit.phase) && Number.isFinite(weit.ausschlag),
    `Phase ${weit.phase}, Ausschlag ${weit.ausschlag}`);
}

process.exit(fehler);
