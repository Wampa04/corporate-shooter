//! Der Strahl-Box-Test trägt die gesamte Trefferauswertung des Servers.

use protocol::{Aabb, Vec3};

fn cube() -> Aabb {
    Aabb::from_center_size(Vec3::new(0.0, 0.0, 10.0), Vec3::splat(2.0))
}

#[test]
fn frontal_hit_returns_entry_distance() {
    let hit = cube().ray_intersection(Vec3::ZERO, Vec3::Z, 100.0);
    assert_eq!(hit, Some(9.0));
}

#[test]
fn miss_does_not_hit() {
    assert_eq!(
        cube().ray_intersection(Vec3::new(5.0, 0.0, 0.0), Vec3::Z, 100.0),
        None
    );
}

#[test]
fn too_short_range_misses() {
    assert_eq!(cube().ray_intersection(Vec3::ZERO, Vec3::Z, 5.0), None);
}

#[test]
fn shot_backwards_misses() {
    assert_eq!(cube().ray_intersection(Vec3::ZERO, -Vec3::Z, 100.0), None);
}

#[test]
fn axis_parallel_ray_beside_box_misses() {
    // Deckt den Sonderfall dir[achse] ~ 0 ab: ohne die Parallelbehandlung
    // liefert die Slab-Rechnung hier NaN und meldet faelschlich einen Treffer.
    let box_top = Aabb::from_center_size(Vec3::new(0.0, 5.0, 10.0), Vec3::splat(2.0));
    assert_eq!(box_top.ray_intersection(Vec3::ZERO, Vec3::Z, 100.0), None);
}

#[test]
fn ray_from_inside_hits_immediately() {
    assert_eq!(
        cube().ray_intersection(Vec3::new(0.0, 0.0, 10.0), Vec3::Z, 100.0),
        Some(0.0)
    );
}

#[test]
fn touching_boxes_do_not_overlap_strictly() {
    // Der Unterschied traegt die gesamte Kollisionsaufloesung: wer an einer
    // Wand steht, beruehrt sie, steckt aber nicht in ihr.
    let a = Aabb::new(Vec3::ZERO, Vec3::ONE);
    let touching = Aabb::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(2.0, 1.0, 1.0));
    assert!(a.intersects(&touching));
    assert!(!a.overlaps_strictly(&touching));

    let overlapping = Aabb::new(Vec3::new(0.9, 0.0, 0.0), Vec3::new(2.0, 1.0, 1.0));
    assert!(a.overlaps_strictly(&overlapping));

    // Auch eine Beruehrung auf nur einer Achse genuegt, um die strenge
    // Ueberlappung auszuschliessen.
    let touching_only_y = Aabb::new(Vec3::new(0.5, 1.0, 0.5), Vec3::new(1.5, 2.0, 1.5));
    assert!(touching_only_y.intersects(&a));
    assert!(!touching_only_y.overlaps_strictly(&a));
}

#[test]
fn touching_boxes_overlap() {
    let a = Aabb::new(Vec3::ZERO, Vec3::ONE);
    let b = Aabb::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(2.0, 1.0, 1.0));
    assert!(a.intersects(&b));
    let c = Aabb::new(Vec3::new(1.001, 0.0, 0.0), Vec3::new(2.0, 1.0, 1.0));
    assert!(!a.intersects(&c));
}

#[test]
fn expanded_grows_in_both_directions() {
    let a = Aabb::new(Vec3::ZERO, Vec3::ONE).expanded(Vec3::splat(0.5));
    assert_eq!(a.min, Vec3::splat(-0.5));
    assert_eq!(a.max, Vec3::splat(1.5));
}
