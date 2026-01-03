use crate::TITLE;
use crate::schema::{MapFile, Textures};
use crate::sync::MapSettingChanged;
use crate::ui::UiState;
use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use bevy::window::PrimaryWindow;
use bevy_file_dialog::DialogFileLoaded;
use bevy_file_dialog::prelude::*;
use native_dialog::MessageLevel;
use serde::Serialize;
use serde_json::Serializer;
use serde_json::ser::PrettyFormatter;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::{env, fs, io};

#[derive(Resource, Default)]
pub struct LoadedFile {
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub file: MapFile,
    pub loaded_textures: Option<Textures<Option<LoadedTexture>>>, // TODO: Finish implementing
}

impl LoadedFile {
    pub fn mark_dirty(&mut self, commands: &mut Commands) {
        if !self.dirty {
            self.dirty = true;
            commands.write_message(UpdateHeader);
        }
    }
}

pub struct LoadedTexture {
    pub path: PathBuf,
    pub image: Handle<Image>,
}

pub struct LoadFilePlugin;

impl Plugin for LoadFilePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadedFile>()
            .add_message::<FileSaved>()
            .add_message::<UpdateHeader>()
            .add_observer(on_map_setting_changed)
            .add_systems(PostStartup, initial_open_file)
            .add_systems(Update, file_state_handler);
    }
}

#[derive(Event, Default)]
pub struct FileLoaded;

pub fn new_file(ui_state: &mut UiState) {
    ui_state.request_close_file(|commands, open_file| {
        *open_file = LoadedFile::default();
        commands.write_message(UpdateHeader);
        commands.trigger(FileLoaded);
    });
}

pub fn open_file(ui_state: &mut UiState) {
    ui_state.request_close_file(|commands, _| {
        commands
            .dialog()
            .set_title("Open MSP map file")
            .add_filter("MSP map files", &["json"])
            .load_file::<MapFile>();
    });
}

pub fn save_file(commands: &mut Commands, open_file: &LoadedFile) {
    if let Some(file_path) = &open_file.path {
        if let Some(data) = get_write_data(open_file) {
            commands.write_message(FileSaved {
                result: fs::write(file_path, data),
                path: file_path.clone(),
            });
        }
    } else {
        save_file_as(commands, open_file);
    }
}

pub fn save_file_as(commands: &mut Commands, open_file: &LoadedFile) {
    if let Some(data) = get_write_data(open_file) {
        commands
            .dialog()
            .set_title("Save MSP map file")
            .add_filter("MSP map files", &["json"])
            .save_file::<MapFile>(data);
    }
}

#[derive(Message)]
struct FileSaved {
    pub result: io::Result<()>,
    pub path: PathBuf,
}

#[derive(Message, Default)]
struct UpdateHeader;

fn on_map_setting_changed(
    on: On<MapSettingChanged>,
    mut open_file: ResMut<LoadedFile>,
    mut commands: Commands,
) {
    match on.event() {
        MapSettingChanged::StartingPosition(pos) => open_file.file.starting_tile = *pos,
    }
    open_file.mark_dirty(&mut commands);
}

fn initial_open_file(
    mut open_file: ResMut<LoadedFile>,
    mut commands: Commands,
    mut ui_state: ResMut<UiState>,
) {
    if let Some(path) = env::args_os().nth(1) {
        let path = PathBuf::from(path);
        let data = match fs::read(&path) {
            Ok(data) => data,
            Err(err) => {
                file_error("load", &err);
                return;
            }
        };
        if handle_load(&mut open_file, &data, path) {
            commands.write_message(UpdateHeader);
            commands.trigger(FileLoaded);
        }
    } else {
        new_file(&mut ui_state);
    }
}

fn file_state_handler(
    mut loaded_reader: MessageReader<DialogFileLoaded<MapFile>>,
    mut saved_reader: MessageReader<FileSaved>,
    mut saved_as_reader: MessageReader<DialogFileSaved<MapFile>>,
    mut update_header_reader: MessageReader<UpdateHeader>,
    mut commands: Commands,
    mut open_file: ResMut<LoadedFile>,
    mut window_query: Query<&mut Window, With<PrimaryWindow>>,
) {
    let mut update_header = {
        let mut update_header = update_header_reader.read();
        let update = update_header.next().is_some();
        for _ in update_header {}
        update
    };

    for loaded in loaded_reader.read() {
        if handle_load(&mut open_file, &loaded.contents, loaded.path.clone()) {
            update_header = true;
            commands.trigger(FileLoaded);
        }
    }

    macro_rules! handle_saved {
        ($reader:ident) => {
            for saved in $reader.read() {
                if let Err(err) = &saved.result {
                    file_error("save", err);
                    continue;
                }
                open_file.path = Some(saved.path.clone());
                open_file.dirty = false;
                update_header = true;
            }
        };
    }
    handle_saved!(saved_reader);
    handle_saved!(saved_as_reader);

    if update_header && let Ok(mut window) = window_query.single_mut() {
        window.title = format!(
            "{TITLE} - {}{}",
            open_file
                .path
                .as_ref()
                .and_then(|x| x.file_name())
                .map_or_else(|| OsStr::new("Untitled").display(), |x| x.display()),
            if open_file.dirty { "*" } else { "" }
        );
    }
}

fn handle_load(open_file: &mut LoadedFile, data: &[u8], path: PathBuf) -> bool {
    let file_data = match serde_json::from_slice(data) {
        Ok(data) => data,
        Err(err) => {
            file_error("open", &err);
            return false;
        }
    };
    open_file.file = file_data;
    open_file.path = Some(path);
    open_file.dirty = false;
    true
}

fn get_write_data(open_file: &LoadedFile) -> Option<Vec<u8>> {
    let mut serializer =
        Serializer::with_formatter(Vec::new(), PrettyFormatter::with_indent("\t".as_bytes()));
    match open_file.file.serialize(&mut serializer) {
        Ok(()) => Some(serializer.into_inner()),
        Err(err) => {
            file_error("save", &err);
            None
        }
    }
}

fn file_error(what: &str, error: &impl std::fmt::Display) {
    let text = format!("Failed to {what} file: {error}");
    AsyncComputeTaskPool::get()
        .spawn(async move {
            let _ = native_dialog::MessageDialogBuilder::default()
                .set_title(TITLE)
                .set_text(text)
                .set_level(MessageLevel::Error)
                .alert()
                .spawn()
                .await;
        })
        .detach();
}
