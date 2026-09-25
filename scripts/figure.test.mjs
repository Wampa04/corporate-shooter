// Die Spielerfigur ohne Browser: bauen, laufen lassen, hinlegen.
//
// Das Hinlegen laeuft nur, wenn ein anderer Spieler tot ist - im Pruefstand
// also praktisch nie. Als dort nach der Umbenennung auf Englisch noch
// `["links", "rechts"]` als Schluessel stand, warf es in jedem Bild eine
// Ausnahme, und bei allen, die den Toten sahen, blieb das Bild stehen.
import assert from "node:assert/strict";
import { test } from "node:test";

import { buildFigure, layFigureDown, poseFigure } from "../client/js/figure.js";
import { FULL_SPEED, advanceWalk, rest } from "../client/js/walk-cycle.js";

test("Figur laesst sich hinlegen", () => {
  const figure = buildFigure(0xe8559b);
  assert.doesNotThrow(() => layFigureDown(figure.limbs, figure.torso));
  assert.notEqual(figure.torso.rotation.x, 0, "Rumpf ist nicht gekippt");
  for (const side of ["left", "right"]) {
    assert.notEqual(figure.limbs.legs[side].top.rotation.x, 0, `Bein ${side} nicht angewinkelt`);
  }
});

test("Figur laesst sich im Lauf stellen", () => {
  const figure = buildFigure(0x29c1b8);
  let walk = rest();
  for (let i = 0; i < 30; i++) walk = advanceWalk(walk, FULL_SPEED / 60, 1 / 60);
  assert.doesNotThrow(() => poseFigure(figure.limbs, figure.torso, walk, 0.2));
});
