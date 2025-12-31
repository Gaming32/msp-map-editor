mod docking;
mod load_file;
mod schema;
mod ui;
mod utils;

use crate::load_file::LoadFilePlugin;
use crate::ui::MapEditorUi;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy_panic_handler::PanicHandler;
use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;

pub const TITLE: &str = "MSP Map Editor";

#[derive(Resource)]
pub struct Directories {
    pub data: PathBuf,
}

impl Directories {
    fn create_dirs(&self) -> bool {
        fs::create_dir_all(&self.data).is_ok()
    }
}

#[derive(Resource)]
pub struct ViewportTarget(Handle<Image>);

pub struct MapEditor;

impl Plugin for MapEditor {
    fn build(&self, app: &mut App) {
        if let Some(dirs) = ProjectDirs::from("io.github", "Gaming32", "mps-map-editor") {
            let dirs_resource = Directories {
                data: dirs.data_dir().to_owned(),
            };
            if dirs_resource.create_dirs() {
                app.insert_resource(dirs_resource);
            }
        }

        app.add_plugins((LoadFilePlugin, MapEditorUi));

        app.add_systems(
            Startup,
            |mut commands: Commands, mut images: ResMut<Assets<Image>>| {
                let viewport_texture = images.add(Image::new_target_texture(
                    1,
                    1,
                    TextureFormat::Rgba8UnormSrgb,
                ));
                commands.insert_resource(ViewportTarget(viewport_texture.clone()));

                commands.spawn(Camera2d);

                commands.spawn((
                    Camera {
                        target: viewport_texture.into(),
                        ..Default::default()
                    },
                    Camera3d::default(),
                    Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
                ));
            },
        );
    }
}

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: TITLE.to_string(),
                    ..Default::default()
                }),
                close_when_requested: false,
                ..Default::default()
            }),
            PanicHandler::new()
                .set_title_func(|_| TITLE.to_string())
                .build(),
            MapEditor,
        ))
        .run();
}
