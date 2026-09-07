//! Der Strahl-Box-Test trägt die gesamte Trefferauswertung des Servers.

use protocol::{Aabb, Vec3};

fn wuerfel() -> Aabb {
    Aabb::from_center_size(Vec3::new(0.0, 0.0, 10.0), Vec3::splat(2.0))
}

#[test]
fn frontaler_treffer_liefert_eintrittsdistanz() {
    let hit = wuerfel().ray_intersection(Vec3::ZERO, Vec3::Z, 100.0);
    assert_eq!(hit, Some(9.0));
}

#[test]
fn vorbeischuss_trifft_nicht() {
    assert_eq!(
        wuerfel().ray_intersection(Vec3::new(5.0, 0.0, 0.0), Vec3::Z, 100.0),
        None
    );
}

#[test]
fn zu_kurze_reichweite_trifft_nicht() {
    assert_eq!(wuerfel().ray_intersection(Vec3::ZERO, Vec3::Z, 5.0), None);
}

#[test]
fn schuss_nach_hinten_trifft_nicht() {
    assert_eq!(
        wuerfel().ray_intersection(Vec3::ZERO, -Vec3::Z, 100.0),
        None
    );
}

#[test]
fn achsenparalleler_strahl_neben_der_box_trifft_nicht() {
    // Deckt den Sonderfall dir[achse] ~ 0 ab: ohne die Parallelbehandlung
    // liefert die Slab-Rechnung hier NaN und meldet faelschlich einen Treffer.
    let box_oben = Aabb::from_center_size(Vec3::new(0.0, 5.0, 10.0), Vec3::splat(2.0));
    assert_eq!(box_oben.ray_intersection(Vec3::ZERO, Vec3::Z, 100.0), None);
}

#[test]
fn strahl_aus_dem_inneren_trifft_sofort() {
    assert_eq!(
        wuerfel().ray_intersection(Vec3::new(0.0, 0.0, 10.0), Vec3::Z, 100.0),
        Some(0.0)
    );
}

#[test]
fn beruehrende_boxen_ueberlappen_nicht_im_strengen_sinn() {
    // Der Unterschied traegt die gesamte Kollisionsaufloesung: wer an einer
    // Wand steht, beruehrt sie, steckt aber nicht in ihr.
    let a = Aabb::new(Vec3::ZERO, Vec3::ONE);
    let beruehrend = Aabb::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(2.0, 1.0, 1.0));
    assert!(a.intersects(&beruehrend));
    assert!(!a.overlaps_strictly(&beruehrend));

    let ueberlappend = Aabb::new(Vec3::new(0.9, 0.0, 0.0), Vec3::new(2.0, 1.0, 1.0));
    assert!(a.overlaps_strictly(&ueberlappend));

    // Auch eine Beruehrung auf nur einer Achse genuegt, um die strenge
    // Ueberlappung auszuschliessen.
    let nur_y_beruehrend = Aabb::new(Vec3::new(0.5, 1.0, 0.5), Vec3::new(1.5, 2.0, 1.5));
    assert!(nur_y_beruehrend.intersects(&a));
    assert!(!nur_y_beruehrend.overlaps_strictly(&a));
}

#[test]
fn beruehrende_boxen_ueberlappen() {
    let a = Aabb::new(Vec3::ZERO, Vec3::ONE);
    let b = Aabb::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(2.0, 1.0, 1.0));
    assert!(a.intersects(&b));
    let c = Aabb::new(Vec3::new(1.001, 0.0, 0.0), Vec3::new(2.0, 1.0, 1.0));
    assert!(!a.intersects(&c));
}

#[test]
fn expanded_vergroessert_in_beide_richtungen() {
    let a = Aabb::new(Vec3::ZERO, Vec3::ONE).expanded(Vec3::splat(0.5));
    assert_eq!(a.min, Vec3::splat(-0.5));
    assert_eq!(a.max, Vec3::splat(1.5));
}
