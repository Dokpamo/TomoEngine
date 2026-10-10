// SPDX-License-Identifier: MIT
//! Headless, assertion-backed simulation. No renderer or native dependency.
use tomo_physics::{BodySettings, PhysicsSystem, Shape, Vec3};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut world = PhysicsSystem::new();
    world.add_body(BodySettings::fixed(
        Shape::cuboid(Vec3::new(10.0, 0.5, 10.0))?,
        Vec3::new(0.0, -0.5, 0.0),
    ))?;
    let id = world.add_body(BodySettings::dynamic(
        Shape::cuboid(Vec3::splat(0.5))?,
        Vec3::new(0.0, 5.0, 0.0),
        1.0,
    ))?;
    println!("time_s,height_m,vertical_velocity_m_s,sleeping");
    for frame in 0..600 {
        world.update(1.0 / 60.0)?;
        if frame % 60 == 59 {
            let body = world.body(id).ok_or("body disappeared")?;
            println!("{:.2},{:.6},{:.6},{}", world.simulation_time(), body.position().y, body.linear_velocity().y, body.is_sleeping());
        }
    }
    let body = world.body(id).ok_or("body disappeared")?;
    assert!((body.position().y - 0.5).abs() < 0.015, "box did not settle: {body:?}");
    assert!(body.is_sleeping(), "box did not sleep: {body:?}");
    eprintln!("PASS: box fell from y=5, contacted the floor and settled. This is not full Jolt parity.");
    Ok(())
}
