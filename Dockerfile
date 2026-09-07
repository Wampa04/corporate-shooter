# syntax=docker/dockerfile:1

# Corporate Shooter als Container.
#
# Das Ergebnis ist ein Bild mit dem Server *und* dem Browser-Client: der Server
# liefert den Client selbst aus, es wird also kein zweiter Webserver gebraucht.
#
# Bauen und starten:
#
#     docker build -t corporate-shooter .
#     docker run --rm --network host corporate-shooter
#
# Zum Netzwerkmodus siehe den Hinweis am Ende dieser Datei.

# Die Rust-Version ist bewusst festgenagelt, damit der Build reproduzierbar
# bleibt und nicht stillschweigend mit dem naechsten Compiler kippt. Bevy 0.19
# verlangt mindestens 1.95; beim Anheben von Bevy ist hier nachzuziehen.
ARG RUST_VERSION=1.98
ARG DEBIAN_RELEASE=bookworm

# ---------------------------------------------------------------------------
# Gemeinsame Grundlage der Build-Stufen
# ---------------------------------------------------------------------------
FROM rust:${RUST_VERSION}-slim-${DEBIAN_RELEASE} AS chef

# cargo-chef trennt das Uebersetzen der Abhaengigkeiten vom Uebersetzen des
# eigenen Codes. Ohne diese Trennung wuerde jede Quelltextaenderung Bevy
# komplett neu uebersetzen - mehrere Minuten pro Build.
ARG CARGO_CHEF_VERSION=0.1.72
RUN cargo install cargo-chef --locked --version ${CARGO_CHEF_VERSION}

WORKDIR /build

# ---------------------------------------------------------------------------
# Bauplan der Abhaengigkeiten erstellen
# ---------------------------------------------------------------------------
FROM chef AS planner
COPY . .

# `rust-toolchain.toml` fordert den Kanal "stable" samt rustfmt und clippy an.
# Das ist fuer die Entwicklung richtig, hier aber schaedlich: im Bild steckt
# bereits ein passender, festgenagelter Compiler, und bliebe die Datei liegen,
# laedt rustup bei jedem Build eine komplette zweite Toolchain herunter.
RUN rm -f rust-toolchain.toml \
 && cargo chef prepare --recipe-path recipe.json

# ---------------------------------------------------------------------------
# Uebersetzen
# ---------------------------------------------------------------------------
FROM chef AS builder

# Erst nur die Abhaengigkeiten. Diese Schicht bleibt gueltig, solange sich
# keine Abhaengigkeit aendert - unabhaengig davon, was am eigenen Code passiert.
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Dann der eigene Code.
COPY . .
RUN rm -f rust-toolchain.toml \
 && cargo build --release --bin server \
 && strip target/release/server

# ---------------------------------------------------------------------------
# Laufzeitbild
# ---------------------------------------------------------------------------
FROM debian:${DEBIAN_RELEASE}-slim AS runtime

LABEL org.opencontainers.image.title="Corporate Shooter" \
      org.opencontainers.image.description="LAN-Shooter im Grossraumbuero: autoritativer Rust-Server samt Browser-Client" \
      org.opencontainers.image.source="https://github.com/Wampa04/corporate-shooter" \
      org.opencontainers.image.licenses="MIT"

# Hier wird bewusst nichts nachinstalliert: die Binaerdatei braucht nur libc,
# libm und libgcc, und die bringt das Basisbild bereits mit. Kein Paketmanager-
# Aufruf heisst kleineres Bild und weniger Angriffsflaeche.

# Kein root. Das Spiel braucht keine Rechte ausser einem Port ueber 1024.
RUN useradd --system --create-home --home-dir /srv --shell /usr/sbin/nologin buero

WORKDIR /srv

COPY --from=builder /build/target/release/server /usr/local/bin/corporate-shooter

# Der Client liegt neben dem Arbeitsverzeichnis, wo der Server ihn von selbst
# findet. Er gehoert root und wird nur gelesen - der Serverprozess kann ihn
# also nicht veraendern.
COPY client /srv/client

USER buero

EXPOSE 4200/tcp

# Argumente werden durchgereicht: `docker run <bild> --port 8080 --name "Daily"`
ENTRYPOINT ["corporate-shooter"]

# Hinweis zum Netzwerkmodus
# -------------------------
# Im Standardmodus (bridge) funktioniert die mDNS-Bekanntmachung nicht: der
# Container sieht das LAN nur ueber NAT und wuerde seine interne Adresse
# verkuenden. Fuer LAN-Betrieb deshalb:
#
#     docker run --rm --network host corporate-shooter
#
# Wer bei bridge bleiben will, veroeffentlicht den Port und schaltet mDNS ab:
#
#     docker run --rm -p 4200:4200 corporate-shooter --no-mdns
#
# Die beim Start ausgegebene LAN-Adresse ist im bridge-Modus die des
# Containers, nicht die des Rechners - dann die Host-IP weitergeben.
