use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;

#[derive(Resource)]
pub struct ViewportTarget(pub Handle<Image>);

pub struct ViewportPlugin;

impl Plugin for ViewportPlugin {
    fn build(&self, app: &mut App) {
        let viewport_texture =
            app.world_mut()
                .resource_mut::<Assets<Image>>()
                .add(Image::new_target_texture(
                    1,
                    1,
                    TextureFormat::Rgba8UnormSrgb,
                ));
        app.insert_resource(ViewportTarget(viewport_texture));
        app.add_systems(Startup, setup_viewport);
    }
}

fn setup_viewport(mut commands: Commands, viewport_target: Res<ViewportTarget>) {
    commands.spawn((
        Camera {
            target: viewport_target.0.clone().into(),
            ..Default::default()
        },
        Camera3d::default(),
        Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
