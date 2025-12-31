mod open_file;
mod schema;
mod ui;

use crate::open_file::OpenFilePlugin;
use crate::ui::MapEditorUi;
use bevy::prelude::*;
use bevy_panic_handler::PanicHandler;

pub const TITLE: &str = "MSP Map Editor";

pub struct MapEditor;

impl Plugin for MapEditor {
    fn build(&self, app: &mut App) {
        app.add_plugins((OpenFilePlugin, MapEditorUi));

        app.add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera3d::default());
        });
    }
}

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: format!("{TITLE} - Untitled"),
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
