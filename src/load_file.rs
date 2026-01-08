use crate::schema::{MapFile, MpsVec2, Textures, TileData, TileHeight};
use crate::sync::{Direction, MapEdit, MapEdited, MaterialEdit};
use crate::tile_range::TileRange;
use crate::ui::UiState;
use crate::TITLE;
use bevy::image::{ImageFormatSetting, ImageLoaderSettings, ImageSampler};
use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use bevy::window::PrimaryWindow;
use bevy_file_dialog::prelude::*;
use bevy_file_dialog::DialogFileLoaded;
use itertools::Itertools;
use native_dialog::MessageLevel;
use relative_path::{PathExt, RelativePathBuf};
use serde::Serialize;
use serde_json::ser::PrettyFormatter;
use serde_json::Serializer;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::{env, fs, io, path};

#[derive(Resource, Default)]
pub struct LoadedFile {
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub file: MapFile,
    pub loaded_textures: Textures<LoadedTexture>,
    pub history: Vec<HistoryItem>,
    pub history_index: usize,
    pub selected_range: Option<TileRange>,
}

impl LoadedFile {
    pub fn in_bounds(&self, pos: MpsVec2) -> MpsVec2 {
        if let Some(map_size) = self.file.map_size() {
            pos.clamp(MpsVec2::ZERO, (map_size - MpsVec2::ONE).max(MpsVec2::ZERO))
        } else {
            pos
        }
    }

    pub fn change_heights(
        &mut self,
        commands: &mut Commands,
        range: TileRange,
        editor: impl Fn(TileHeight) -> TileHeight,
    ) -> bool {
        let new_heights = range
            .into_iter()
            .map(|x| editor(self.file[x].height))
            .collect();
        self.edit_map(commands, MapEdit::ChangeHeight(range, new_heights))
    }

    pub fn edit_map(&mut self, commands: &mut Commands, edit: MapEdit) -> bool {
        let invalid = match edit {
            MapEdit::ShrinkMap(Direction::West | Direction::East) if self.file.data.cols() < 2 => {
                true
            }
            MapEdit::ShrinkMap(Direction::North | Direction::South)
                if self.file.data.rows() < 2 =>
            {
                true
            }
            _ => false,
        };
        if invalid {
            return false;
        }

        let reversed = match &edit {
            MapEdit::StartingPosition(_) => MapEdit::StartingPosition(self.file.starting_tile),
            MapEdit::Skybox(index, _) => {
                MapEdit::Skybox(*index, self.loaded_textures.skybox[*index].clone())
            }
            MapEdit::Atlas(_) => MapEdit::Atlas(self.loaded_textures.atlas.clone()),
            MapEdit::ExpandMap(side, _) => MapEdit::ShrinkMap(*side),
            MapEdit::ShrinkMap(side) => MapEdit::ExpandMap(
                *side,
                Some(match side {
                    Direction::West => self.file.data.iter_col(0).cloned().collect(),
                    Direction::East => self
                        .file
                        .data
                        .iter_col(self.file.data.cols() - 1)
                        .cloned()
                        .collect(),
                    Direction::North => self.file.data.iter_row(0).cloned().collect(),
                    Direction::South => self
                        .file
                        .data
                        .iter_row(self.file.data.rows() - 1)
                        .cloned()
                        .collect(),
                }),
            ),
            MapEdit::AdjustHeight(range, change) => MapEdit::AdjustHeight(*range, -change),
            MapEdit::ChangeHeight(range, _) => MapEdit::ChangeHeight(
                *range,
                range.into_iter().map(|pos| self.file[pos].height).collect(),
            ),
            MapEdit::ChangeConnection(range, direction, _) => MapEdit::ChangeConnection(
                *range,
                *direction,
                range
                    .into_iter()
                    .map(|pos| self.file[pos].connections[*direction])
                    .collect(),
            ),
            MapEdit::ChangeMaterial(range, location, edits) => MapEdit::ChangeMaterial(
                *range,
                *location,
                range
                    .into_iter()
                    .zip(edits.iter())
                    .map(|(pos, &edit)| match edit {
                        MaterialEdit::Set(_) => {
                            MaterialEdit::Set(self.file[pos].materials[*location])
                        }
                        MaterialEdit::MoveUp => MaterialEdit::MoveUp,
                        MaterialEdit::MoveDown => MaterialEdit::MoveDown,
                        MaterialEdit::Remove => {
                            MaterialEdit::Insert(self.file[pos].materials[*location])
                        }
                        MaterialEdit::Insert(_) => MaterialEdit::Remove,
                    })
                    .collect(),
            ),
        };
        if edit == reversed {
            let is_equal_reverse = match &reversed {
                MapEdit::ChangeMaterial(_, _, edits) => edits
                    .iter()
                    .all(|e| matches!(e, MaterialEdit::MoveUp | MaterialEdit::MoveDown)),
                _ => false,
            };
            if !is_equal_reverse {
                return false;
            }
        }

        self.history.truncate(self.history_index);
        self.history.push(HistoryItem {
            forward: edit.clone(),
            back: reversed,
        });
        self.history_index += 1;

        self.apply_edit(commands, edit);
        true
    }

    pub fn undo(&mut self, commands: &mut Commands) {
        if self.history_index > 0 {
            self.history_index -= 1;
            self.apply_edit(commands, self.history[self.history_index].back.clone());
        }
    }

    pub fn redo(&mut self, commands: &mut Commands) {
        if self.history_index < self.history.len() {
            self.apply_edit(commands, self.history[self.history_index].forward.clone());
            self.history_index += 1;
        }
    }

    fn apply_edit(&mut self, commands: &mut Commands, edit: MapEdit) {
        match &edit {
            MapEdit::StartingPosition(pos) => self.file.starting_tile = *pos,
            MapEdit::Skybox(index, image) => {
                self.loaded_textures.skybox[*index] = image.clone();
            }
            MapEdit::Atlas(image) => {
                self.loaded_textures.atlas = image.clone();
            }
            MapEdit::ExpandMap(side, data) => {
                let data = data.clone().unwrap_or_else(|| {
                    let size = match side {
                        Direction::West | Direction::East => self.file.data.rows(),
                        Direction::North | Direction::South => self.file.data.cols(),
                    };
                    vec![TileData::default(); size]
                });
                match side {
                    Direction::West => {
                        self.file.data.insert_col(0, data);
                        self.adjust_coords(MpsVec2::new(1, 0));
                    }
                    Direction::East => self.file.data.insert_col(self.file.data.cols(), data),
                    Direction::North => {
                        self.file.data.insert_row(0, data);
                        self.adjust_coords(MpsVec2::new(0, 1));
                    }
                    Direction::South => self.file.data.insert_row(self.file.data.rows(), data),
                }
            }
            MapEdit::ShrinkMap(side) => match side {
                Direction::West => {
                    self.file.data.remove_col(0);
                    self.adjust_coords(MpsVec2::new(-1, 0));
                }
                Direction::East => {
                    self.file.data.remove_col(self.file.data.cols() - 1);
                }
                Direction::North => {
                    self.file.data.remove_row(0);
                    self.adjust_coords(MpsVec2::new(0, -1));
                }
                Direction::South => {
                    self.file.data.remove_row(self.file.data.rows() - 1);
                }
            },
            MapEdit::AdjustHeight(range, change) => self.file.adjust_height(*range, *change),
            MapEdit::ChangeHeight(range, new) => {
                assert_eq!(
                    range.area(),
                    new.len(),
                    "MapEdit::ChangeHeight params have differing sizes"
                );
                for (pos, height) in range.into_iter().zip(new) {
                    self.file[pos].height = *height;
                }
            }
            MapEdit::ChangeConnection(range, direction, new) => {
                assert_eq!(
                    range.area(),
                    new.len(),
                    "MapEdit::ChangeConnection params have differing sizes"
                );
                for (pos, connection) in range.into_iter().zip(new) {
                    self.file[pos].connections[*direction] = *connection;
                }
            }
            MapEdit::ChangeMaterial(range, location, new) => {
                assert_eq!(
                    range.area(),
                    new.len(),
                    "MapEdit::ChangeMaterial params have differing sizes"
                );
                for (pos, &edit) in range.into_iter().zip(new) {
                    match edit {
                        MaterialEdit::Set(material) => {
                            self.file[pos].materials[*location] = material;
                        }
                        _ => {
                            let (side, index) = location.unwrap();
                            let vec = &mut self.file[pos].materials.wall_material[side];
                            match edit {
                                MaterialEdit::Set(_) => unreachable!(),
                                MaterialEdit::MoveUp => vec.swap(index - 1, index),
                                MaterialEdit::MoveDown => vec.swap(index, index + 1),
                                MaterialEdit::Remove => {
                                    vec.remove(index);
                                }
                                MaterialEdit::Insert(material) => vec.insert(index, material),
                            }
                        }
                    }
                }
            }
        }

        if !self.dirty {
            self.dirty = true;
            commands.write_message(UpdateHeader);
        }
        commands.trigger(MapEdited(edit));
    }

    fn adjust_coords(&mut self, adjust: MpsVec2) {
        self.file.starting_tile += adjust;
        for tile in &mut self.file.shop_warp_tiles {
            *tile += adjust;
        }
        self.file.star_warp_tile += adjust;
        self.file.podium_position += adjust;
        for pos in &mut self.file.results_anim_cam_poses {
            *pos += adjust.into();
        }
        self.file.tutorial_star.pos += adjust.into();
        self.file.tutorial_shop.pos += adjust.into();
    }

    pub fn can_undo(&self) -> bool {
        self.history_index > 0
    }

    pub fn can_redo(&self) -> bool {
        self.history_index < self.history.len()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LoadedTexture {
    pub path: PathBuf,
    pub image: Handle<Image>,
}

#[derive(Clone, Debug)]
pub struct HistoryItem {
    pub forward: MapEdit,
    pub back: MapEdit,
}

pub struct LoadFilePlugin;

impl Plugin for LoadFilePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadedFile>()
            .add_message::<FileSaved>()
            .add_message::<UpdateHeader>()
            .add_systems(PostStartup, initial_open_file)
            .add_systems(Update, file_state_handler);
    }
}

#[derive(Event, Default)]
pub struct FileLoaded;

pub(super) struct MapFileDialog;

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
            .load_file(MapFileDialog);
    });
}

pub fn save_file(commands: &mut Commands, open_file: &mut LoadedFile) {
    if let Some(file_path) = open_file.path.clone() {
        match get_write_data(open_file) {
            Ok(data) => {
                commands.write_message(FileSaved {
                    result: fs::write(&file_path, data),
                    path: file_path,
                });
            }
            Err(err) => {
                file_error("save", &err);
            }
        }
    } else {
        save_file_as(commands);
    }
}

pub fn save_file_as(commands: &mut Commands) {
    commands
        .dialog()
        .set_title("Save MSP map file")
        .add_filter("MSP map files", &["json"])
        .save_file(vec![], MapFileDialog);
}

#[derive(Message)]
struct FileSaved {
    pub result: io::Result<()>,
    pub path: PathBuf,
}

#[derive(Message, Default)]
struct UpdateHeader;

fn initial_open_file(
    mut open_file: ResMut<LoadedFile>,
    mut commands: Commands,
    mut ui_state: ResMut<UiState>,
    assets: Res<AssetServer>,
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
        if handle_load(&mut open_file, &data, path, &assets) {
            commands.write_message(UpdateHeader);
            commands.trigger(FileLoaded);
        }
    } else {
        new_file(&mut ui_state);
    }
}

#[allow(clippy::too_many_arguments)]
fn file_state_handler(
    mut loaded_reader: MessageReader<DialogFileLoaded<MapFileDialog>>,
    mut saved_reader: MessageReader<FileSaved>,
    mut saved_as_reader: MessageReader<DialogFileSaved<MapFileDialog>>,
    mut update_header_reader: MessageReader<UpdateHeader>,
    mut commands: Commands,
    mut open_file: ResMut<LoadedFile>,
    mut window_query: Query<&mut Window, With<PrimaryWindow>>,
    assets: Res<AssetServer>,
) {
    let mut update_header = update_header_reader.is_empty();
    update_header_reader.clear();

    for loaded in loaded_reader.read() {
        if handle_load(
            &mut open_file,
            &loaded.contents,
            loaded.path.clone(),
            &assets,
        ) {
            update_header = true;
            commands.trigger(FileLoaded);
        }
    }

    for saved in saved_reader.read() {
        if let Err(err) = &saved.result {
            file_error("save", err);
            continue;
        }
        open_file.path = Some(saved.path.clone());
        open_file.dirty = false;
        update_header = true;
    }

    for saved in saved_as_reader.read() {
        if let Err(err) = &saved.result {
            file_error("save", err);
            continue;
        }
        open_file.path = Some(saved.path.clone());
        save_file(&mut commands, &mut open_file);
    }

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

fn handle_load(
    open_file: &mut LoadedFile,
    data: &[u8],
    path: PathBuf,
    assets: &AssetServer,
) -> bool {
    let root_dir = path
        .parent()
        .expect("File shouldn't have been loadable without a parent");
    open_file.file = match serde_json::from_slice(data) {
        Ok(data) => data,
        Err(err) => {
            file_error("open", &err);
            return false;
        }
    };
    open_file.dirty = false;

    let load_texture = |path: &RelativePathBuf, sampler: ImageSampler| {
        let path = path.to_path(root_dir);
        LoadedTexture {
            path: path.clone(),
            image: assets.load_with_settings_override(path, move |settings| {
                *settings = ImageLoaderSettings {
                    format: ImageFormatSetting::Guess,
                    sampler: sampler.clone(),
                    ..Default::default()
                };
            }),
        }
    };

    open_file.loaded_textures = Textures {
        skybox: open_file
            .file
            .textures
            .skybox
            .each_ref()
            .map(|path| load_texture(path, ImageSampler::Default)),
        atlas: load_texture(&open_file.file.textures.atlas, ImageSampler::nearest()),
    };

    open_file.path = Some(path);
    open_file.history.clear();
    open_file.history_index = 0;
    open_file.selected_range = None;
    true
}

fn get_write_data(open_file: &mut LoadedFile) -> Result<Vec<u8>> {
    let root_path = normalize_path(
        open_file
            .path
            .as_deref()
            .expect("get_write_data called without a path"),
    )?;
    let root_path = root_path
        .parent()
        .expect("get_write_data called with an invalid path");
    let convert_path = |from: &LoadedTexture, to: &mut RelativePathBuf| -> Result<()> {
        *to = normalize_path(&from.path)?.relative_to(root_path)?;
        Ok(())
    };
    for (from, to) in open_file
        .loaded_textures
        .skybox
        .iter()
        .zip(open_file.file.textures.skybox.iter_mut())
    {
        convert_path(from, to)?;
    }
    convert_path(
        &open_file.loaded_textures.atlas,
        &mut open_file.file.textures.atlas,
    )?;

    let mut serializer =
        Serializer::with_formatter(Vec::new(), PrettyFormatter::with_indent("\t".as_bytes()));
    open_file.file.serialize(&mut serializer)?;
    Ok(serializer.into_inner())
}

fn normalize_path(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    use path::Component;
    let mut result = PathBuf::new();
    for component in path::absolute(path)?.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    return Err(io::Error::other("Path attempted to resolve .. from root"));
                }
            }
            Component::Normal(part) => result.push(part),
        }
    }
    Ok(result)
}

fn file_error(what: &str, error: &impl std::fmt::Display) {
    let text = format!("Failed to {what} file: {error}");
    error!("{text}");
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
