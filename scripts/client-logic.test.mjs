// Tests der Client-Logik, die ohne Browser auskommt.
//
// Geprueft wird, was reine Funktion ist: das Zusammenfuehren von `Local` und
// `Snapshot` und die Rangliste. Beides entscheidet darueber, was auf dem
// Bildschirm steht, und beides laesst sich ohne DOM und ohne Three aufrufen.
import assert from "node:assert/strict";
import { describe, test } from "node:test";

import { rankPlayers, tableKey } from "../client/js/hud.js";
import { mergeSnapshot } from "../client/js/net.js";

describe("mergeSnapshot", () => {
  test("fuehrt Local und Snapshot desselben Ticks zusammen", () => {
    const local = { tick: 7, ack_seq: 12, local: { ammo: 3 } };
    const snapshot = { tick: 7, players: [], events: [], match: {} };
    const merged = mergeSnapshot(local, snapshot);
    assert.equal(merged.ack_seq, 12);
    assert.deepEqual(merged.local, { ammo: 3 });
    assert.equal(merged.tick, 7);
  });

  test("verwirft den Snapshot ohne passendes Local", () => {
    // Lieber einen Snapshot auslassen, als die Vorhersage gegen den Stand
    // eines anderen Ticks abzugleichen.
    const snapshot = () => ({ tick: 7, players: [], events: [], match: {} });
    assert.equal(mergeSnapshot(null, snapshot()), null);
    assert.equal(mergeSnapshot({ tick: 6, ack_seq: 1, local: {} }, snapshot()), null);
  });
});

describe("Rangliste", () => {
  const player = (id, name, kills, deaths, team = "Marketing") => ({
    id,
    name,
    kills,
    deaths,
    team,
  });

  test("sortiert nach Abschuessen, dann wenigen Toden, dann Name", () => {
    const ranked = rankPlayers([
      player(1, "Bea", 2, 5),
      player(2, "Ada", 2, 5),
      player(3, "Cem", 4, 9),
      player(4, "Dora", 2, 1),
    ]);
    assert.deepEqual(
      ranked.map((p) => p.name),
      ["Cem", "Dora", "Ada", "Bea"],
    );
  });

  test("veraendert die uebergebene Liste nicht", () => {
    const list = [player(1, "B", 0, 0), player(2, "A", 1, 0)];
    rankPlayers(list);
    assert.deepEqual(
      list.map((p) => p.name),
      ["B", "A"],
    );
  });

  test("Fingerabdruck bleibt gleich, solange sich nichts Sichtbares aendert", () => {
    const a = rankPlayers([player(1, "Ada", 1, 0), player(2, "Bo", 0, 0)]);
    const b = rankPlayers([player(2, "Bo", 0, 0), player(1, "Ada", 1, 0)]);
    assert.equal(tableKey(a), tableKey(b));
  });

  test("Fingerabdruck aendert sich mit Abschuessen, Toden, Namen und Team", () => {
    const basis = tableKey([player(1, "Ada", 1, 0)]);
    for (const different of [
      player(1, "Ada", 2, 0),
      player(1, "Ada", 1, 1),
      player(1, "Ida", 1, 0),
      player(1, "Ada", 1, 0, "Engineering"),
    ]) {
      assert.notEqual(tableKey([different]), basis);
    }
  });
});
