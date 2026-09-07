//! LAN-Bekanntmachung per mDNS.
//!
//! Wichtige Einschränkung: **Browser können kein mDNS.** Sie haben keinen
//! Zugriff auf Multicast, und `http://name.local` aufzulösen setzt voraus, dass
//! das Betriebssystem selbst mDNS beherrscht (macOS immer, Windows 10+ meist,
//! Linux nur mit Avahi). Die Bekanntmachung hier richtet sich deshalb an
//! Werkzeuge, die im LAN nach Servern suchen - etwa einen Launcher oder
//! `avahi-browse`. Damit auch ohne all das jemand spielen kann, gibt der Server
//! beim Start die direkt erreichbaren URLs aus.

use std::net::{IpAddr, SocketAddr, UdpSocket};

use mdns_sd::{ServiceDaemon, ServiceInfo};
use tracing::warn;

/// Diensttyp, unter dem Server im LAN auffindbar sind.
pub const SERVICE_TYPE: &str = "_corpshoot._tcp.local.";

/// Meldet den Server im LAN an.
///
/// Der zurückgegebene Daemon muss am Leben bleiben, solange die Anmeldung
/// gelten soll; wird er verworfen, verschwindet der Eintrag wieder.
pub fn announce(instance: &str, port: u16, map_name: &str) -> anyhow::Result<ServiceDaemon> {
    let daemon = ServiceDaemon::new()?;
    // Eigener Hostname je Port, damit zwei Server auf einem Rechner sich nicht
    // gegenseitig überschreiben.
    let host = format!("corpshoot-{port}.local.");
    let properties = [
        ("game", "corporate-shooter"),
        ("map", map_name),
        ("path", "/"),
    ];

    let info = ServiceInfo::new(SERVICE_TYPE, instance, &host, "", port, &properties[..])?
        // Ermittelt die eigenen Adressen selbst und hält sie aktuell, auch wenn
        // sich das Netz unter dem laufenden Server ändert.
        .enable_addr_auto();

    daemon.register(info)?;
    Ok(daemon)
}

/// Ermittelt die IP-Adresse, über die dieser Rechner im LAN erreichbar ist.
///
/// Es wird nichts gesendet: ein verbundener UDP-Socket wählt nur die Route aus,
/// und daraus lässt sich die passende Quelladresse ablesen. Das Ziel ist mit
/// Absicht eine Dokumentationsadresse (TEST-NET-1), damit selbst bei einer
/// Fehlkonfiguration niemand angefunkt wird.
pub fn lan_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip())
}

/// Gibt aus, unter welchen Adressen der Server im Browser erreichbar ist.
pub fn print_urls(bound: SocketAddr) {
    println!("  lokal:   http://localhost:{}", bound.port());
    match lan_address() {
        Some(ip) => println!("  im LAN:  http://{}:{}", ip, bound.port()),
        None => warn!("LAN-Adresse nicht ermittelbar - bitte IP manuell weitergeben"),
    }
}
