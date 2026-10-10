// SPDX-License-Identifier: MIT
use tomo_physics::{BodySettings, PhysicsSystem, Shape, Vec3};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut world = PhysicsSystem::new();
    world.add_body(BodySettings::fixed(
        Shape::cuboid(Vec3::new(10.0, 0.5, 10.0))?,
        Vec3::new(0.0, -0.5, 0.0),
    ))?;
    let shape = Shape::cuboid(Vec3::splat(0.5))?;
    let mut ids = Vec::new();
    for index in 0..5 {
        ids.push(world.add_body(BodySettings::dynamic(
            shape,
            Vec3::Y * (0.51 + index as f32 * 1.01),
            1.0,
        ))?);
    }
    for _ in 0..1200 {
        world.update(1.0 / 60.0)?;
    }
    for (index, id) in ids.into_iter().enumerate() {
        let body = world.body(id).ok_or("body disappeared")?;
        println!(
            "box {index}: y={:.6}, speed={:.6}, sleeping={}",
            body.position().y,
            body.linear_velocity().length(),
            body.is_sleeping()
        );
        assert!((body.position().y - (0.5 + index as f32)).abs() < 0.08);
        assert!(body.is_sleeping());
    }
    Ok(())
}
