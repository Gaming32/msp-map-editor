use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use std::ops::Range;

#[derive(Component, Clone, Debug)]
pub struct CullIfInside(pub Range<f32>);

pub struct CullingPlugin;

impl Plugin for CullingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, cull_if_inside);
    }
}

// Some help from https://github.com/bevyengine/bevy/blob/latest/examples/3d/visibility_range.rs
fn cull_if_inside(
    mut commands: Commands,
    new_meshes: Query<Entity, Added<Mesh3d>>,
    children: Query<(Option<&ChildOf>, Option<&CullIfInside>)>,
) {
    for new_mesh in new_meshes {
        let (mut current, mut cull) = (new_mesh, None);
        while let Ok((child_of, maybe_cull)) = children.get(current) {
            if let Some(found_cull) = maybe_cull {
                cull = Some(found_cull);
                break;
            }
            match child_of {
                Some(child_of) => current = child_of.parent(),
                None => break,
            }
        }
        if let Some(cull) = cull {
            commands.entity(new_mesh).insert(VisibilityRange {
                start_margin: cull.0.clone(),
                end_margin: 1000.0..1000.0,
                use_aabb: false,
            });
        }
    }
}
