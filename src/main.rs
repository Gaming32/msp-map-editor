#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assets;
mod docking;
mod load_file;
mod schema;
mod shortcuts;
mod sync;
mod ui;
mod utils;
mod viewport;

use crate::assets::EmbeddedAssetsPlugin;
use crate::load_file::LoadFilePlugin;
use crate::ui::MapEditorUi;
use crate::viewport::ViewportPlugin;
use bevy::asset::UnapprovedPathMode;
use bevy::log::LogPlugin;
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
        if let Some(dirs) = ProjectDirs::from("io.github", "Gaming32", "msp-map-editor") {
            let dirs_resource = Directories {
                data: dirs.data_dir().to_owned(),
            };
            if dirs_resource.create_dirs() {
                app.insert_resource(dirs_resource);
            }
        }

        app.add_plugins((
            EmbeddedAssetsPlugin,
            LoadFilePlugin,
            ViewportPlugin,
            MapEditorUi,
        ));
    }
}

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: TITLE.to_string(),
                        ..Default::default()
                    }),
                    close_when_requested: false,
                    ..Default::default()
                })
                .set(LogPlugin {
                    filter: "info,wgpu=error,naga=warn,bevy_map_camera::controller::mouse=error"
                        .to_string(),
                    ..Default::default()
                })
                .set(AssetPlugin {
                    file_path: "".to_string(),
                    unapproved_path_mode: UnapprovedPathMode::Deny,
                    ..Default::default()
                }),
            PanicHandler::new()
                .set_title_func(|_| TITLE.to_string())
                .build(),
            MapEditor,
        ))
        .run();
}
