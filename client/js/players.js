// Darstellung der Mitspieler.
//
// Snapshots treffen mit der Tickrate ein (30 Hz), gerendert wird mit
// Bildwiederholrate. Dazwischen wird interpoliert, und zwar bewusst mit
// Verzoegerung: gerendert wird der Zustand, der zwei Ticks alt ist. Dadurch
// liegen immer zwei Snapshots vor, zwischen denen interpoliert werden kann,
// statt in die Zukunft raten zu muessen.

import * as THREE from "../vendor/three.module.min.js";
import { baueFigur, legeFigurHin, stelleFigur } from "./figur.js";
import { ruhe, schritt } from "./laufzyklus.js";

export const TEAM_COLOR = {
  Marketing:   0xe8559b,
  Engineering: 0x29c1b8,
};

/**
 * Verzoegerung, mit der fremde Spieler gezeigt werden.
 *
 * In Millisekunden gedacht und nicht in Ticks: der Puffer soll einen
 * ausgefallenen Snapshot ueberbruecken, und wie lange das dauert, haengt an
 * der Leitung und nicht an der Tickrate. Zwei Ticks bei 30 Hz waren 67 ms -
 * bei 60 Hz waeren daraus 33 ms geworden, also ein halb so grosser Puffer.
 *
 * Mindestens zwei Ticks, damit immer zwischen zwei Staenden interpoliert
 * werden kann statt fortzuschreiben.
 */
const INTERPOLATION_MS = 70;

/** Ab dieser Distanz wird gesprungen statt interpoliert (Respawn, Wiederverbinden). */
const TELEPORT_DISTANCE = 4.0;

/** Kuerzester Weg zwischen zwei Winkeln. */
function lerpAngle(a, b, t) {
  let delta = (b - a) % (Math.PI * 2);
  if (delta > Math.PI) delta -= Math.PI * 2;
  if (delta < -Math.PI) delta += Math.PI * 2;
  return a + delta * t;
}

const TAG_FONT = "600 44px 'Segoe UI', system-ui, sans-serif";
const TAG_CANVAS_HEIGHT = 64;
const TAG_PADDING = 32;

/** Hoehe des Namensschilds in Metern. Die Breite folgt der Textlaenge. */
const TAG_HEIGHT = 0.32;

/** Obergrenze des Seitenverhaeltnisses, damit lange Namen nicht ausufern. */
const TAG_MAX_ASPECT = 7.5;

/**
 * Namensschild als Textur auf einem Sprite.
 *
 * Die Textur wird auf den Text zugeschnitten statt der Text auf eine feste
 * Textur - sonst werden laengere Namen einfach abgeschnitten.
 */
function makeNameTag(name, color) {
  const canvas = document.createElement("canvas");
  let ctx = canvas.getContext("2d");
  ctx.font = TAG_FONT;
  const textWidth = Math.ceil(ctx.measureText(name).width);

  // Das Setzen der Groesse verwirft den Kontextzustand, die Schrift muss
  // danach erneut gesetzt werden.
  canvas.width = Math.max(64, textWidth + TAG_PADDING);
  canvas.height = TAG_CANVAS_HEIGHT;
  ctx = canvas.getContext("2d");
  ctx.font = TAG_FONT;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.lineWidth = 8;
  ctx.strokeStyle = "rgba(10, 12, 16, 0.92)";
  ctx.strokeText(name, canvas.width / 2, TAG_CANVAS_HEIGHT / 2 + 2);
  ctx.fillStyle = `#${color.toString(16).padStart(6, "0")}`;
  ctx.fillText(name, canvas.width / 2, TAG_CANVAS_HEIGHT / 2 + 2);

  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  // Die Textur ist keine Zweierpotenz; ohne Mipmaps bleibt sie trotzdem sauber.
  texture.generateMipmaps = false;
  texture.minFilter = THREE.LinearFilter;

  const sprite = new THREE.Sprite(
    new THREE.SpriteMaterial({ map: texture, depthTest: false, transparent: true }),
  );
  const aspect = Math.min(canvas.width / canvas.height, TAG_MAX_ASPECT);
  sprite.scale.set(aspect * TAG_HEIGHT, TAG_HEIGHT, 1);
  // Ueber dem Kopf (1.96 m), aber unter der Decke.
  sprite.position.y = 2.28;
  sprite.renderOrder = 10;
  return sprite;
}

/** Eine Figur samt Namensschild. */
function makeAvatar(state) {
  const color = TEAM_COLOR[state.team] ?? 0xffffff;
  const figur = baueFigur(color);
  figur.schild = makeNameTag(state.name, color);
  figur.wurzel.add(figur.schild);
  return figur;
}

export class PlayerViews {
  /**
   * @param {THREE.Scene} scene
   * @param {number} selfId eigene Spieler-ID; der eigene Koerper wird nicht gezeichnet
   * @param {number} tickRate Simulationsschritte pro Sekunde
   */
  constructor(scene, selfId, tickRate) {
    this.scene = scene;
    this.selfId = selfId;
    this.delayMs = Math.max(INTERPOLATION_MS, (2 * 1000) / tickRate);
    /**
     * @type {Map<number, {group: THREE.Group, figur: object, team: string,
     *                     lauf: {phase: number, ausschlag: number}}>}
     *
     * `audio.js` liest diese Karte fuer die Schritte der Mitspieler und
     * verlaesst sich auf `group.position` und `group.visible`.
     */
    this.avatars = new Map();

  }

  /**
   * Setzt alle Mitspieler auf den Stand von `now - delay`.
   *
   * @param {Array} snapshots Puffer aus `Connection`, aelteste zuerst
   * @param {number} now Zeitstempel von `performance.now()`
   */
  update(snapshots, now) {
    if (snapshots.length === 0) return;

    // Der Zeitschritt wird hier gebildet und nicht durchgereicht: `update`
    // haengt ohnehin schon an `now`, und ein zweiter Parameter waere eine
    // zweite Gelegenheit, ihn falsch zu uebergeben. Gedeckelt gegen den
    // Sprung nach einem Tabwechsel.
    const dt = this._zuletzt === undefined ? 0 : Math.min((now - this._zuletzt) / 1000, 0.1);
    this._zuletzt = now;

    const target = now - this.delayMs;
    const [older, newer, t] = bracket(snapshots, target);
    const states = interpolateStates(older, newer, t);

    for (const [id, state] of states) {
      if (id === this.selfId) continue;

      let entry = this.avatars.get(id);
      // Beim Teamwechsel oder nach einer Namensaenderung neu aufbauen, damit
      // Farbe und Schild stimmen.
      if (entry && entry.team !== state.team) {
        this._remove(id);
        entry = undefined;
      }
      if (!entry) {
        const figur = makeAvatar(state);
        entry = {
          group: figur.wurzel,
          figur,
          team: state.team,
          lauf: ruhe(),
        };
        this.scene.add(entry.group);
        this.avatars.set(id, entry);
      }

      // Die waagerechte Strecke seit dem letzten Bild treibt den Laufzyklus.
      // Sie kommt aus der Position, die die Interpolation ohnehin liefert -
      // eine zweite Messung waere eine zweite Quelle fuer dasselbe.
      const vorher = entry.group.position;
      let strecke = entry.gesehen
        ? Math.hypot(state.pos[0] - vorher.x, state.pos[2] - vorher.z)
        : 0;
      // Ein Sprung ist kein Weg. Beim Wiedereinstieg liegen dreissig Meter
      // zwischen zwei Bildern; ungedeckelt drehte die Figur dabei zwanzig
      // Schrittzyklen in einem einzigen Bild. Dieselbe Grenze, mit der die
      // Interpolation weiter unten einen Sprung von einer Bewegung
      // unterscheidet.
      if (strecke > TELEPORT_DISTANCE) strecke = 0;
      entry.gesehen = true;

      entry.group.position.set(state.pos[0], state.pos[1], state.pos[2]);
      entry.group.rotation.y = state.yaw;

      if (state.alive) {
        entry.lauf = schritt(entry.lauf, strecke, dt);
        stelleFigur(entry.figur.glieder, entry.figur.koerper, entry.lauf, state.pitch ?? 0);
      } else {
        // Tote bleiben liegen, wo sie gefallen sind, statt zu verschwinden:
        // wer um die Ecke kommt, soll sehen, dass hier gerade jemand
        // freigestellt wurde. Das Namensschild geht weg - es haengt an der
        // Wurzel und stuende sonst zwei Meter ueber einer Leiche in der Luft.
        entry.lauf = ruhe();
        legeFigurHin(entry.figur.glieder, entry.figur.koerper);
      }
      entry.figur.schild.visible = state.alive;
    }

    // Wer nicht mehr im Snapshot steht, hat das Unternehmen verlassen.
    for (const id of [...this.avatars.keys()]) {
      if (!states.has(id)) this._remove(id);
    }
  }

  /**
   * Welchen Serverstand ein Betrachter zum Zeitpunkt `now` sieht, gebrochen.
   *
   * Der Server braucht das fuer die Lag-Kompensation: fremde Spieler werden
   * hier bewusst verzoegert gezeigt, damit ihre Bewegung nicht ruckelt, und
   * dazu kommt die Laufzeit. Wer auf einen Kopf zielt, zielt also auf eine
   * Vergangenheit.
   *
   * Bewusst frisch gerechnet und nicht aus `update` gemerkt: Eingaben gehen
   * mit der Tickrate raus, gezeichnet wird mit der Bildrate. Bei zwanzig
   * Bildern je Sekunde waere ein gemerkter Wert bis zu fuenfzig Millisekunden
   * alt, und der Server spulte entsprechend zu weit zurueck.
   */
  viewTick(snapshots, now) {
    if (!snapshots || snapshots.length === 0) return null;
    const [older, newer, t] = bracket(snapshots, now - this.delayMs);
    return older.tick + (newer.tick - older.tick) * t;
  }

  _remove(id) {
    const entry = this.avatars.get(id);
    if (!entry) return;
    this.scene.remove(entry.group);
    entry.group.traverse((object) => {
      object.geometry?.dispose();
      object.material?.map?.dispose();
      object.material?.dispose();
    });
    this.avatars.delete(id);
  }

  dispose() {
    for (const id of [...this.avatars.keys()]) this._remove(id);
  }
}

/**
 * Sucht die beiden Snapshots, zwischen denen `target` liegt.
 *
 * Liegt `target` vor dem aeltesten oder nach dem neuesten Snapshot, wird der
 * jeweilige Randzustand ohne Interpolation geliefert.
 *
 * @returns {[object, object, number]} aelterer, neuerer, Mischfaktor 0..1
 */
function bracket(snapshots, target) {
  const newest = snapshots[snapshots.length - 1];
  if (target >= newest.recvTime) return [newest, newest, 0];

  const oldest = snapshots[0];
  if (target <= oldest.recvTime) return [oldest, oldest, 0];

  for (let i = snapshots.length - 1; i > 0; i--) {
    const b = snapshots[i];
    const a = snapshots[i - 1];
    if (a.recvTime <= target && target <= b.recvTime) {
      const span = b.recvTime - a.recvTime;
      return [a, b, span > 0 ? (target - a.recvTime) / span : 0];
    }
  }
  return [newest, newest, 0];
}

/** Mischt die Spielerzustaende zweier Snapshots. */
function interpolateStates(older, newer, t) {
  const previous = new Map(older.players.map((p) => [p.id, p]));
  const result = new Map();

  for (const state of newer.players) {
    const before = previous.get(state.id);
    if (!before || t <= 0) {
      result.set(state.id, state);
      continue;
    }

    const dx = state.pos[0] - before.pos[0];
    const dy = state.pos[1] - before.pos[1];
    const dz = state.pos[2] - before.pos[2];
    // Nach einem Respawn liegen Welten zwischen den Snapshots - dann waere
    // eine Interpolation ein Flug quer durchs Buero.
    if (dx * dx + dy * dy + dz * dz > TELEPORT_DISTANCE * TELEPORT_DISTANCE) {
      result.set(state.id, state);
      continue;
    }

    result.set(state.id, {
      ...state,
      pos: [
        before.pos[0] + dx * t,
        before.pos[1] + dy * t,
        before.pos[2] + dz * t,
      ],
      yaw: lerpAngle(before.yaw, state.yaw, t),
      pitch: before.pitch + (state.pitch - before.pitch) * t,
    });
  }

  return result;
}
