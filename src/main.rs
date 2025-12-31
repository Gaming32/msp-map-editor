mod docking;
mod load_file;
mod schema;
mod ui;
mod utils;
mod viewport;

use crate::load_file::LoadFilePlugin;
use crate::ui::MapEditorUi;
use crate::viewport::ViewportPlugin;
use bevy::prelude::*;
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

        app.add_plugins((LoadFilePlugin, ViewportPlugin, MapEditorUi));
        app.add_systems(
            Startup,
            |mut commands: Commands,
             mut meshes: ResMut<Assets<Mesh>>,
             mut materials: ResMut<Assets<StandardMaterial>>| {
                commands.spawn((
                    Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
                    MeshMaterial3d(materials.add(Color::srgb_u8(124, 144, 255))),
                    Transform::from_xyz(0.0, 0.5, 0.0),
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
