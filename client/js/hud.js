// Die Oberflaeche waehrend des Spiels.
//
// Alle angezeigten Zahlen stammen aus den Snapshots und aus der
// Serverkonfiguration. Der Client rechnet nichts nach - was hier steht, ist
// der Zustand, den der Server fuer verbindlich haelt.

/** Wie lange ein Eintrag in der Killfeed stehen bleibt (ms). */
const KILLFEED_LIFETIME = 7000;

/** Hoechstzahl gleichzeitiger Eintraege. */
const KILLFEED_MAX = 6;

const TEAM_CLASS = { Marketing: "marketing", Engineering: "engineering" };

const el = (id) => document.getElementById(id);

export class Hud {
  constructor(config, selfId) {
    this.config = config;
    this.selfId = selfId;
    this.weaponNames = new Map(config.weapons.map((w) => [w.id, w.name]));

    this.root = el("hud");
    this.healthFill = el("health-fill");
    this.healthValue = el("health-value");
    this.ammoBox = el("ammo");
    this.ammoCurrent = el("ammo-current");
    this.ammoMax = el("ammo-max");
    this.weaponName = el("weapon-name");
    this.reloading = el("reloading");
    this.dash = el("skill-dash");
    this.heal = el("skill-heal");
    this.killfeed = el("killfeed");
    this.respawn = el("respawn");
    this.respawnTimer = el("respawn-timer");
    this.scoreboard = el("scoreboard");
    this.scoreboardBody = el("scoreboard-body");
    this.hitmarker = el("hitmarker");
    this.damageFlash = el("damage-flash");
    this.ping = el("ping");

    this._entries = [];
    this._damageTimer = null;
  }

  show() {
    this.root.classList.remove("hidden");
  }

  /** Uebernimmt eigenen Zustand und Munition aus dem Snapshot. */
  update(self, local) {
    if (self) {
      const ratio = Math.max(0, Math.min(1, self.health / this.config.max_health));
      this.healthFill.style.width = `${ratio * 100}%`;
      this.healthFill.style.backgroundColor =
        ratio > 0.5 ? "var(--ok)" : ratio > 0.25 ? "#e0a33c" : "var(--danger)";
      this.healthValue.textContent = String(self.health);
      this.weaponName.textContent = this.weaponNames.get(self.weapon) ?? "–";
    }

    this.ammoCurrent.textContent = String(local.ammo);
    this.ammoMax.textContent = String(local.mag_size);
    this.ammoBox.classList.toggle("empty", local.ammo === 0);
    this.reloading.classList.toggle("hidden", !local.reloading);

    this._cooldown(this.dash, local.dash_cooldown_remaining, this.config.dash_cooldown);
    this._cooldown(this.heal, local.heal_cooldown_remaining, this.config.heal_cooldown);

    const dead = self ? !self.alive : false;
    this.respawn.classList.toggle("hidden", !dead);
    if (dead) {
      this.respawnTimer.textContent = local.respawn_remaining.toFixed(1);
    }
  }

  _cooldown(node, remaining, total) {
    const ready = remaining <= 0;
    node.classList.toggle("ready", ready);
    // Der Balken laeuft von voll nach leer; `total` kann 0 sein, wenn eine
    // Faehigkeit ohne Cooldown konfiguriert ist.
    const fraction = total > 0 ? Math.max(0, Math.min(1, remaining / total)) : 0;
    node.querySelector("i").style.transform = `scaleX(${fraction})`;
  }

  setPing(ms) {
    this.ping.textContent = ms === null ? "–" : Math.round(ms);
  }

  setScoreboard(players, visible) {
    this.scoreboard.classList.toggle("hidden", !visible);
    if (!visible) return;

    const sorted = [...players].sort(
      (a, b) => b.kills - a.kills || a.deaths - b.deaths || a.name.localeCompare(b.name),
    );
    this.scoreboardBody.replaceChildren(
      ...sorted.map((player) => {
        const row = document.createElement("tr");
        if (player.id === this.selfId) row.className = "self";
        row.innerHTML =
          `<td class="${TEAM_CLASS[player.team] ?? ""}">${escapeHtml(player.team)}</td>` +
          `<td>${escapeHtml(player.name)}</td>` +
          `<td class="num">${player.kills}</td>` +
          `<td class="num">${player.deaths}</td>`;
        return row;
      }),
    );
  }

  /**
   * Wertet die Ereignisse eines Ticks aus.
   *
   * @param {Array} events Ereignisliste des Snapshots
   * @param {Map<number, object>} byId Spielerzustaende dieses Snapshots
   */
  handleEvents(events, byId) {
    for (const event of events) {
      switch (event.t) {
        case "Death": {
          const victim = byId.get(event.d.victim);
          const killer = event.d.killer !== null ? byId.get(event.d.killer) : null;
          const weapon = event.d.weapon ? this.weaponNames.get(event.d.weapon) : null;
          this._addEntry(
            killer
              ? `${tag(killer)} <span class="verb">hat</span> ${tag(victim)} ` +
                `<span class="verb">freigestellt${weapon ? ` (${escapeHtml(weapon)})` : ""}</span>`
              : `${tag(victim)} <span class="verb">hat sich selbst wegrationalisiert</span>`,
            killer?.team,
          );
          break;
        }
        case "Joined":
          this._addEntry(
            `<span class="${TEAM_CLASS[event.d.team] ?? ""}">${escapeHtml(event.d.name)}</span> ` +
              `<span class="verb">wurde onboarded</span>`,
            event.d.team,
          );
          break;
        case "Left":
          this._addEntry(
            `<span class="verb">${escapeHtml(event.d.name)} hat innerlich gekündigt</span>`,
          );
          break;
        case "Hit":
          // Rueckmeldung nur fuer die eigenen Treffer und die eigenen Wunden.
          if (event.d.attacker === this.selfId) this._flashHitmarker();
          if (event.d.target === this.selfId) this._flashDamage();
          break;
      }
    }
  }

  _addEntry(html, team) {
    const node = document.createElement("div");
    node.className = "kill-entry";
    if (team) node.style.borderLeftColor = `var(--${TEAM_CLASS[team]})`;
    node.innerHTML = html;
    this.killfeed.append(node);
    this._entries.push({ node, until: performance.now() + KILLFEED_LIFETIME });

    while (this._entries.length > KILLFEED_MAX) {
      this._entries.shift().node.remove();
    }
  }

  /** Raeumt abgelaufene Killfeed-Eintraege ab. Wird pro Bild aufgerufen. */
  tick(now) {
    while (this._entries.length && this._entries[0].until <= now) {
      this._entries.shift().node.remove();
    }
  }

  _flashHitmarker() {
    this.hitmarker.classList.remove("on");
    // Erzwingt einen Neustart der Animation, auch bei Treffern kurz nacheinander.
    void this.hitmarker.offsetWidth;
    this.hitmarker.classList.add("on");
  }

  _flashDamage() {
    this.damageFlash.classList.add("on");
    clearTimeout(this._damageTimer);
    this._damageTimer = setTimeout(
      () => this.damageFlash.classList.remove("on"),
      60,
    );
  }
}

/** Namen sind frei waehlbar und landen im DOM - also escapen. */
function escapeHtml(text) {
  return String(text).replace(
    /[&<>"']/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c],
  );
}

function tag(player) {
  if (!player) return `<span class="verb">jemand</span>`;
  return `<span class="${TEAM_CLASS[player.team] ?? ""}">${escapeHtml(player.name)}</span>`;
}
