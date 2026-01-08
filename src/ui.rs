use crate::assets::{icons_atlas, unset_texture_icon};
use crate::docking::UiDocking;
use crate::load_file::{
    new_file, open_file, save_file, save_file_as, FileLoaded, LoadedFile, LoadedTexture,
    MapFileDialog,
};
use crate::schema::{
    Connection, ConnectionCondition, CubeMap, MpsMaterial, TileHeight, TileRampDirection,
};
use crate::sync::{Direction, MaterialEdit, MaterialLocation, PresetView};
use crate::sync::{EditObject, MapEdit, MapEdited, SelectForEditing};
use crate::tile_range::TileRange;
use crate::viewport::ViewportTarget;
use crate::{shortcut_pressed, Directories};
use bevy::asset::io::embedded::GetAssetServer;
use bevy::asset::LoadState;
use bevy::image::{ImageFormatSetting, ImageLoaderSettings};
use bevy::prelude::Image as BevyImage;
use bevy::prelude::*;
use bevy::render::render_resource::Extent3d;
use bevy::window::{PrimaryWindow, WindowCloseRequested};
use bevy_file_dialog::{DialogFilePicked, FileDialogExt, FileDialogPlugin};
use bevy_mod_imgui::prelude::*;
use imgui::Image as ImguiImage;
use itertools::Itertools;
use monostate::MustBeBool;
use std::borrow::Cow;
use std::mem;
use std::time::Duration;

pub struct MapEditorUi;

impl Plugin for MapEditorUi {
    fn build(&self, app: &mut App) {
        let mut imgui_plugin = ImguiPlugin::default();
        if let Some(dirs) = app.world().get_resource::<Directories>() {
            imgui_plugin.ini_filename = Some(dirs.data.join("imgui.ini"));
        }

        app.insert_resource(UiState {
            free_timer: Timer::new(Duration::from_millis(500), TimerMode::Repeating),
            unset_texture_icon: unset_texture_icon(app.get_asset_server()),
            icon_atlas_handle: icons_atlas(app.get_asset_server()),
            ..Default::default()
        })
        .add_plugins((
            imgui_plugin,
            FileDialogPlugin::new()
                .with_pick_file::<SettingImagePick>()
                .with_load_file::<MapFileDialog>()
                .with_save_file::<MapFileDialog>(),
        ))
        .add_systems(Startup, |mut imgui: NonSendMut<ImguiContext>| {
            imgui.with_io_mut(|io| {
                io.config_docking_always_tab_bar = true;
            });
        })
        .add_observer(on_file_loaded)
        .add_observer(on_map_edited)
        .add_systems(
            Update,
            (
                setting_image_picked,
                draw_imgui,
                keyboard_handler,
                close_handler,
            ),
        );
    }
}

#[derive(Resource, Default)]
pub struct UiState {
    setup_complete: bool,
    viewport_texture: Option<TextureId>,
    textures_to_free: Vec<TextureId>,
    free_timer: Timer,
    pending_close_state: PendingCloseState,
    skybox_textures: Option<CubeMap<TextureId>>,
    atlas_texture: Option<TextureId>,
    waiting_textures: Vec<SettingImageLoadWait>,
    unset_texture_icon: Handle<BevyImage>,
    icon_atlas_handle: Handle<BevyImage>,
    icon_atlas_texture: Option<TextureId>,
    material_target: Option<(TileRange, MaterialLocation)>,
}

impl UiState {
    pub fn request_close_file(
        &mut self,
        action: impl FnOnce(&mut Commands, &mut LoadedFile) + Send + Sync + 'static,
    ) {
        self.pending_close_state = PendingCloseState::PendingUi(Box::new(action));
    }
}

type BoxedCloseHandler = Box<dyn FnOnce(&mut Commands, &mut LoadedFile) + Send + Sync>;

#[derive(Default)]
enum PendingCloseState {
    #[default]
    None,
    PendingUi(BoxedCloseHandler),
    PendingUserInput(BoxedCloseHandler),
    Confirmed(BoxedCloseHandler),
}

#[derive(Copy, Clone)]
enum SettingImagePick {
    Skybox(usize),
    Atlas,
}

struct SettingImageLoadWait {
    image: Handle<BevyImage>,
    pick: SettingImagePick,
}

fn on_file_loaded(
    _: On<FileLoaded>,
    file: Res<LoadedFile>,
    mut state: ResMut<UiState>,
    mut context: NonSendMut<ImguiContext>,
) {
    let new_skybox_textures = file.loaded_textures.skybox.each_ref().map(|tex| {
        context.register_bevy_texture(if tex.image != Handle::default() {
            tex.image.clone()
        } else {
            state.unset_texture_icon.clone()
        })
    });
    if let Some(old_textures) = state.skybox_textures.replace(new_skybox_textures) {
        state.textures_to_free.extend(old_textures);
    }

    let new_atlas_texture =
        context.register_bevy_texture(if file.loaded_textures.atlas.image != Handle::default() {
            file.loaded_textures.atlas.image.clone()
        } else {
            state.unset_texture_icon.clone()
        });
    if let Some(old_texture) = state.atlas_texture.replace(new_atlas_texture) {
        state.textures_to_free.push(old_texture);
    }
}

fn on_map_edited(on: On<MapEdited>, mut state: ResMut<UiState>) {
    match &on.0 {
        MapEdit::StartingPosition(_) => {}
        MapEdit::Skybox(index, image) => {
            state.waiting_textures.push(SettingImageLoadWait {
                image: image.image.clone(),
                pick: SettingImagePick::Skybox(*index),
            });
        }
        MapEdit::Atlas(image) => {
            state.waiting_textures.push(SettingImageLoadWait {
                image: image.image.clone(),
                pick: SettingImagePick::Atlas,
            });
        }
        MapEdit::ExpandMap(_, _)
        | MapEdit::ShrinkMap(_)
        | MapEdit::AdjustHeight(_, _)
        | MapEdit::ChangeHeight(_, _)
        | MapEdit::ChangeConnection(_, _, _)
        | MapEdit::ChangeMaterial(_, _, _) => {}
    }
}

fn setting_image_picked(
    mut files: MessageReader<DialogFilePicked<SettingImagePick>>,
    assets: Res<AssetServer>,
    mut commands: Commands,
    mut file: ResMut<LoadedFile>,
) {
    for picked in files.read() {
        let image = assets.load_with_settings_override(picked.path.clone(), |settings| {
            *settings = ImageLoaderSettings {
                format: ImageFormatSetting::Guess,
                ..Default::default()
            };
        });
        let texture = LoadedTexture {
            path: picked.path.clone(),
            image,
        };
        match picked.data {
            SettingImagePick::Skybox(index) => {
                file.edit_map(&mut commands, MapEdit::Skybox(index, texture));
            }
            SettingImagePick::Atlas => {
                file.edit_map(&mut commands, MapEdit::Atlas(texture));
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "This system requires a lot of arguments"
)]
fn draw_imgui(
    mut context: NonSendMut<ImguiContext>,
    mut state: ResMut<UiState>,
    time: Res<Time>,
    mut file: ResMut<LoadedFile>,
    mut commands: Commands,
    window_query: Query<Entity, With<PrimaryWindow>>,
    mut viewport_target: ResMut<ViewportTarget>,
    mut images: ResMut<Assets<BevyImage>>,
    assets: Res<AssetServer>,
) {
    if state.viewport_texture.is_none() {
        state.viewport_texture =
            Some(context.register_bevy_texture(viewport_target.texture.clone()));
    }

    let mut skybox_textures = state.skybox_textures;
    let mut atlas_texture = state.atlas_texture;
    let mut removed_textures = vec![];
    state
        .waiting_textures
        .retain(|texture| match assets.load_state(&texture.image) {
            LoadState::Loaded => {
                let image_id = context.register_bevy_texture(texture.image.clone());
                match texture.pick {
                    SettingImagePick::Skybox(index) => {
                        let skybox_textures = skybox_textures
                            .as_mut()
                            .expect("skybox_textures should be assigned by now");
                        removed_textures.push(mem::replace(&mut skybox_textures[index], image_id));
                    }
                    SettingImagePick::Atlas => {
                        if let Some(old_texture) = atlas_texture.replace(image_id) {
                            removed_textures.push(old_texture);
                        }
                    }
                }
                false
            }
            LoadState::Failed(_) => false,
            _ => true,
        });
    state.skybox_textures = skybox_textures;
    state.atlas_texture = atlas_texture;
    state.textures_to_free.extend(removed_textures);

    if state.icon_atlas_texture.is_none() && assets.is_loaded(&state.icon_atlas_handle) {
        state.icon_atlas_texture =
            Some(context.register_bevy_texture(state.icon_atlas_handle.clone()));
    }

    state.free_timer.tick(time.delta());
    if state.free_timer.just_finished() && !state.textures_to_free.is_empty() {
        let len = state.textures_to_free.len() - 1;
        for texture in state.textures_to_free.drain(..len) {
            context.unregister_bevy_texture(&texture);
        }
    }

    let ui = context.ui();

    if !state.setup_complete {
        ui.dockspace_over_viewport().split(
            imgui::Direction::Left,
            0.8,
            |left| {
                left.dock_window("Viewport");
            },
            |right| {
                right.dock_window("Map settings");
                right.dock_window("Tile settings");
            },
        );
        state.setup_complete = true;
    } else {
        ui.dockspace_over_main_viewport();
    }

    ui.main_menu_bar(|| {
        ui.menu("File", || {
            if ui.menu_item_config("New").shortcut("Ctrl+N").build() {
                new_file(&mut state);
            }

            if ui.menu_item_config("Open").shortcut("Ctrl+O").build() {
                open_file(&mut state);
            }

            if ui.menu_item_config("Save").shortcut("Ctrl+S").build() {
                save_file(&mut commands, &mut file);
            }

            if ui
                .menu_item_config("Save as")
                .shortcut("Ctrl+Shift+S")
                .build()
            {
                save_file_as(&mut commands);
            }

            ui.separator();

            if ui.menu_item_config("Quit").shortcut("Alt+F4").build()
                && let Ok(window) = window_query.single_inner()
            {
                commands.write_message(WindowCloseRequested { window });
            }
        });

        ui.menu("Edit", || {
            if ui
                .menu_item_config("Undo")
                .shortcut("Ctrl+Z")
                .enabled(file.can_undo())
                .build()
            {
                file.undo(&mut commands);
            }

            if ui
                .menu_item_config("Redo")
                .shortcut("Ctrl+Shift+Z")
                .enabled(file.can_redo())
                .build()
            {
                file.redo(&mut commands);
            }
        });

        ui.menu("View", || {
            if ui.menu_item("Player") {
                commands.trigger(PresetView::Player);
            }

            if ui.menu_item("Center") {
                commands.trigger(PresetView::Center);
            }

            if ui.menu_item_config("Top-down").shortcut("Num 7").build() {
                commands.trigger(PresetView::TopDown);
            }

            // TODO: Selected
        });
    });

    ui.window("Viewport").collapsible(true).build(|| {
        if let Some(texture) = state.viewport_texture {
            let dest_size = ui.content_region_avail();
            if dest_size[0] < 1.0 || dest_size[1] < 1.0 {
                return;
            }
            let target_size = UVec2::new(dest_size[0] as u32, dest_size[1] as u32);
            if images
                .get(&viewport_target.texture)
                .is_some_and(|i| i.size() != target_size)
            {
                let real_image = images.get_mut(&viewport_target.texture).unwrap();
                real_image.resize_in_place(Extent3d {
                    width: target_size.x,
                    height: target_size.y,
                    depth_or_array_layers: 1,
                });
                state.textures_to_free.push(texture);
                state.viewport_texture = None;
            }
            viewport_target.upper_left = ui.cursor_screen_pos().into();
            viewport_target.size = dest_size.into();
            ImguiImage::new(texture, dest_size).build(ui);
        }
    });

    ui.window("Map settings").collapsible(true).build(|| {
        ui.text("Starting tile");
        ui.same_line();
        if ui.button("Select") {
            commands.trigger(SelectForEditing {
                object: EditObject::StartingPosition,
                exclusive: true,
            });
        }
        let mut starting_tile = file.file.starting_tile.as_array();
        if ui
            .input_scalar_n("##Starting Tile", &mut starting_tile)
            .step(1)
            .build()
        {
            let starting_tile = file.in_bounds(starting_tile.into());
            file.edit_map(&mut commands, MapEdit::StartingPosition(starting_tile));
        }

        ui.spacing();

        ui.text(format!(
            "Map size: {}x{}",
            file.file.data.cols(),
            file.file.data.rows()
        ));
        if ui.button("Edit map bounds") {
            commands.trigger(SelectForEditing {
                object: EditObject::MapSize(Direction::West),
                exclusive: true,
            });
            commands.trigger(SelectForEditing {
                object: EditObject::MapSize(Direction::East),
                exclusive: false,
            });
            commands.trigger(SelectForEditing {
                object: EditObject::MapSize(Direction::North),
                exclusive: false,
            });
            commands.trigger(SelectForEditing {
                object: EditObject::MapSize(Direction::South),
                exclusive: false,
            });
        }

        ui.spacing();

        if let Some(atlas) = state.atlas_texture
            && let Some(_token) = ui
                .tree_node_config("Atlas")
                .framed(true)
                .tree_push_on_open(false)
                .push()
        {
            if ui.button("Reload##Reload Atlas") {
                let texture = &file.loaded_textures.atlas;
                assets.reload(texture.path.clone());
                commands.trigger(MapEdited(MapEdit::Atlas(texture.clone())));
            }
            if ui.image_button("Select Atlas", atlas, [256.0; 2]) {
                commands
                    .dialog()
                    .set_title("Choose atlas file")
                    .add_filter("Images", SUPPORTED_IMAGE_EXTENSIONS)
                    .pick_file_path(SettingImagePick::Atlas);
            }
        }

        ui.spacing();

        const SUPPORTED_IMAGE_EXTENSIONS: &[&str] = &[
            "bmp", "gif", "hdr", "ico", "jpg", "jpeg", "ktx2", "png", "tif", "tiff", "webp",
        ];

        if let Some(skybox) = state.skybox_textures
            && let Some(icon_atlas) = state.icon_atlas_texture
            && let Some(_token) = ui
                .tree_node_config("Skybox")
                .framed(true)
                .tree_push_on_open(false)
                .push()
        {
            const LABELS: CubeMap<&str> = ["East", "West", "Up", "Down", "North", "South"];
            for (index, (label, texture)) in LABELS.iter().zip(skybox.iter()).enumerate() {
                if ui.image_button(format!("Select {label}"), *texture, [128.0; 2]) {
                    commands
                        .dialog()
                        .set_title("Choose skybox file")
                        .add_filter("Images", SUPPORTED_IMAGE_EXTENSIONS)
                        .pick_file_path(SettingImagePick::Skybox(index));
                }
                ui.same_line();
                if ui
                    .image_button_config(format!("Reload {label}"), icon_atlas, [16.0; 2])
                    .uv0([0.0, 0.0])
                    .uv1([0.5, 0.5])
                    .build()
                {
                    let texture = &file.loaded_textures.skybox[index];
                    assets.reload(texture.path.clone());
                    commands.trigger(MapEdited(MapEdit::Skybox(index, texture.clone())));
                }
                ui.same_line();
                ui.text(label);
            }
        }
    });

    let mut open_material_picker = false;
    ui.window("Tile settings").collapsible(true).build(|| {
        const MULTIPLE_VALUES: &str = "<multiple values>";

        if let Some(range) = file.selected_range {
            let single_tile = (range.start == range.end).then_some(range.start);
            if single_tile.is_some() {
                ui.text(format!(
                    "Selected tile ({}, {})",
                    range.start.x, range.start.y
                ));
            } else {
                ui.text(format!(
                    "Selected {} tiles",
                    (range.end.x - range.start.x + 1) * (range.end.y - range.start.y + 1)
                ));
            }

            ui.spacing();

            let ramp_type = {
                let ramp_type = range
                    .into_iter()
                    .map(|x| file.file[x].height.ramp_dir())
                    .all_equal_value()
                    .ok();
                let options: &[_] = if ramp_type.is_some() {
                    &[
                        Some(None),
                        Some(Some(TileRampDirection::Horizontal)),
                        Some(Some(TileRampDirection::Vertical)),
                    ]
                } else {
                    &[
                        None,
                        Some(None),
                        Some(Some(TileRampDirection::Horizontal)),
                        Some(Some(TileRampDirection::Vertical)),
                    ]
                };
                let mut index = options.iter().position(|&x| x == ramp_type).unwrap();
                let changed = ui.combo("Height type", &mut index, options, |&value| {
                    match value {
                        None => MULTIPLE_VALUES,
                        Some(None) => "Flat",
                        Some(Some(TileRampDirection::Horizontal)) => "West/East Ramp",
                        Some(Some(TileRampDirection::Vertical)) => "North/South Ramp",
                    }
                    .into()
                });
                if changed && let Some(new_type) = options[index] {
                    file.change_heights(&mut commands, range, |h| h.with_ramp_dir(new_type));
                    Some(new_type)
                } else {
                    ramp_type
                }
            };

            let height_input = |label, mut value: Option<_>| {
                if let Some(value) = value.as_mut() {
                    if ui
                        .input_scalar(label, value)
                        .step(0.25)
                        .step_fast(1.0)
                        .display_format("%.2f")
                        .build()
                    {
                        Some(*value)
                    } else {
                        None
                    }
                } else {
                    let mut buf = MULTIPLE_VALUES.to_string();
                    if ui.input_text(label, &mut buf).build() {
                        buf.parse().ok()
                    } else {
                        None
                    }
                }
            };
            match ramp_type {
                None => {}
                Some(None) => {
                    let height = range
                        .into_iter()
                        .map(|x| file.file[x].height.center_height())
                        .all_equal_value()
                        .ok();
                    if let Some(height) = height_input("Height", height) {
                        file.edit_map(
                            &mut commands,
                            MapEdit::ChangeHeight(
                                range,
                                vec![
                                    TileHeight::Flat {
                                        ramp: MustBeBool,
                                        height,
                                    };
                                    range.area()
                                ],
                            ),
                        );
                    }
                }
                Some(Some(dir)) => {
                    let (neg_label, pos_label) = match dir {
                        TileRampDirection::Horizontal => ("West height", "East height"),
                        TileRampDirection::Vertical => ("North height", "South height"),
                    };
                    let neg_height = range
                        .into_iter()
                        .map(|x| file.file[x].height.neg_height())
                        .all_equal_value()
                        .ok();
                    let pos_height = range
                        .into_iter()
                        .map(|x| file.file[x].height.pos_height())
                        .all_equal_value()
                        .ok();
                    if let Some(height) = height_input(neg_label, neg_height) {
                        file.change_heights(&mut commands, range, |h| h.with_neg_height(height));
                    }
                    if let Some(height) = height_input(pos_label, pos_height) {
                        file.change_heights(&mut commands, range, |h| h.with_pos_height(height));
                    }
                    if ui.button("Flip") {
                        file.change_heights(&mut commands, range, TileHeight::with_flipped_heights);
                    }
                }
            }

            ui.spacing();

            if let Some(_token) = ui
                .tree_node_config("Connections")
                .framed(true)
                .tree_push_on_open(false)
                .push()
            {
                for direction in Direction::ALL_CLOCKWISE {
                    let connection_type = range
                        .into_iter()
                        .map(|x| file.file[x].connections[*direction])
                        .all_equal_value()
                        .ok();
                    let options: &[_] = if connection_type.is_some() {
                        &[
                            Some(Connection::Unconditional(false)),
                            Some(Connection::Unconditional(true)),
                            Some(Connection::Conditional(ConnectionCondition::Lock)),
                        ]
                    } else {
                        &[
                            None,
                            Some(Connection::Unconditional(false)),
                            Some(Connection::Unconditional(true)),
                            Some(Connection::Conditional(ConnectionCondition::Lock)),
                        ]
                    };
                    let mut index = options.iter().position(|&x| x == connection_type).unwrap();
                    let changed = ui.combo(direction, &mut index, options, |&value| {
                        match value {
                            None => MULTIPLE_VALUES,
                            Some(Connection::Unconditional(false)) => "Block",
                            Some(Connection::Unconditional(true)) => "Passable",
                            Some(Connection::Conditional(ConnectionCondition::Lock)) => {
                                "Locked gate"
                            }
                        }
                        .into()
                    });
                    if changed && let Some(new_type) = options[index] {
                        file.edit_map(
                            &mut commands,
                            MapEdit::ChangeConnection(
                                range,
                                *direction,
                                vec![new_type; range.area()],
                            ),
                        );
                    }
                }
            }

            if let Some(atlas) = state.atlas_texture
                && let Some(icon_atlas) = state.icon_atlas_texture
                && let Some(_token) = ui
                    .tree_node_config("Materials")
                    .framed(true)
                    .tree_push_on_open(false)
                    .push()
            {
                const MATERIAL_PREVIEW_SIZE: [f32; 2] = [64.0; 2];
                let mut material_button = |id, location| {
                    let common_material = range
                        .into_iter()
                        .map(|x| file.file[x].materials[location])
                        .all_equal_value()
                        .ok();
                    let clicked = if let Some(material) = common_material {
                        let (u1, v1, u2, v2) = material.to_uv_coords();
                        ui.image_button_config(id, atlas, MATERIAL_PREVIEW_SIZE)
                            .uv0([u1, v1])
                            .uv1([u2, v2])
                            .build()
                    } else {
                        ui.button_with_size(id, MATERIAL_PREVIEW_SIZE)
                    };
                    if clicked {
                        state.material_target = Some((range, location));
                        open_material_picker = true;
                    }
                };

                material_button(Cow::Borrowed("##Top material"), None);
                ui.same_line();
                ui.text("Top material");

                let mut edit = None;
                for side in Direction::ALL_CLOCKWISE {
                    let Some(_token) = ui.tree_node(side) else {
                        continue;
                    };
                    let len_iter = range
                        .into_iter()
                        .map(|x| file.file[x].materials.wall_material[*side].len());
                    let segment_count = len_iter.clone().all_equal_value().ok();
                    let min_segments = segment_count.unwrap_or_else(|| len_iter.min().unwrap());
                    for index in 0..min_segments {
                        let location = Some((*side, index));
                        material_button(Cow::Owned(format!("##{side} material {index}")), location);

                        ui.same_line();
                        ui.disabled(index == 0, || {
                            if ui
                                .image_button_config(
                                    format!("Move up {index}"),
                                    icon_atlas,
                                    [16.0; 2],
                                )
                                .uv0([0.0, 0.5])
                                .uv1([0.5, 1.0])
                                .build()
                            {
                                edit = Some(MapEdit::ChangeMaterial(
                                    range,
                                    location,
                                    vec![MaterialEdit::MoveUp; range.area()],
                                ));
                            }
                        });

                        ui.same_line();
                        ui.disabled(index >= min_segments - 1, || {
                            if ui.image_button_config(
                                format!("Move down {index}"),
                                icon_atlas,
                                [16.0; 2],
                            )
                            .uv0([0.5, 0.5])
                            .uv1([1.0, 1.0])
                            .build() {
                                edit = Some(MapEdit::ChangeMaterial(
                                    range,
                                    location,
                                    vec![MaterialEdit::MoveDown; range.area()],
                                ));
                            }
                        });

                        ui.same_line();
                        ui.disabled(min_segments < 2, || {
                            if ui.image_button_config(
                                format!("Remove {index}"),
                                icon_atlas,
                                [16.0; 2],
                            )
                            .uv0([0.5, 0.0])
                            .uv1([1.0, 0.5])
                            .build() {
                                edit = Some(MapEdit::ChangeMaterial(
                                    range,
                                    location,
                                    vec![MaterialEdit::Remove; range.area()],
                                ));
                            }
                        });
                    }
                    if segment_count.is_some() && ui.button("Add segment") {
                        edit = Some(MapEdit::ChangeMaterial(
                            range,
                            Some((*side, min_segments)),
                            vec![MaterialEdit::Insert(MpsMaterial::default()); range.area()],
                        ));
                    }
                }
                if let Some(edit) = edit {
                    let (location, material) = match &edit {
                        MapEdit::ChangeMaterial(_, location, material) => (*location, material[0]),
                        _ => unreachable!(),
                    };
                    file.edit_map(&mut commands, edit);
                    if matches!(material, MaterialEdit::Insert(_)) {
                        state.material_target = Some((range, location));
                        open_material_picker = true;
                    }
                }
            }
        } else {
            ui.text("No tile selected");
        }
    });
    if open_material_picker {
        ui.open_popup("Material picker");
    }

    viewport_target.disable_input = false;
    if let Some(atlas) = state.atlas_texture
        && let Some((target_range, target_location)) = state.material_target
    {
        ui.popup("Material picker", || {
            viewport_target.disable_input = true;
            let _style = ui.push_style_var(StyleVar::ItemSpacing([0.0, 0.0]));
            let _style = ui.push_style_var(StyleVar::FramePadding([0.0, 0.0]));
            for index in 0..MpsMaterial::TEXTURES_COUNT {
                if index % MpsMaterial::TEXTURES_PER_ROW != 0 {
                    ui.same_line();
                }
                let material = MpsMaterial::from_index(index)
                    .expect("MpsMaterial::from_index out of sync with TEXTURES_COUNT");
                let (u1, v1, u2, v2) = material.to_uv_coords();
                if ui
                    .image_button_config(format!("Material {index}"), atlas, [32.0; 2])
                    .uv0([u1, v1])
                    .uv1([u2, v2])
                    .build()
                {
                    file.edit_map(
                        &mut commands,
                        MapEdit::ChangeMaterial(
                            target_range,
                            target_location,
                            vec![MaterialEdit::Set(material); target_range.area()],
                        ),
                    );
                    ui.close_current_popup();
                }
            }
        });
    }

    match mem::take(&mut state.pending_close_state) {
        PendingCloseState::PendingUi(action) if file.dirty => {
            ui.open_popup("Are you sure?");
            state.pending_close_state = PendingCloseState::PendingUserInput(action);
        }
        other => {
            state.pending_close_state = other;
        }
    }
    ui.modal_popup("Are you sure?", || {
        viewport_target.disable_input = true;
        ui.text("The current file has not been saved. Are you sure?");

        let mut confirm = false;
        let mut close = false;
        if ui.button("Cancel") {
            state.pending_close_state = PendingCloseState::None;
            close = true;
        }
        ui.same_line();
        if ui.button("Don't Save") {
            confirm = true;
            close = true;
        }
        ui.same_line();
        if ui.button("Save") {
            save_file(&mut commands, &mut file);
            close = true;
        }

        if confirm
            && let PendingCloseState::PendingUserInput(action) =
                mem::take(&mut state.pending_close_state)
        {
            state.pending_close_state = PendingCloseState::Confirmed(action);
        }
        if close {
            ui.close_current_popup();
        }
    });
}

fn keyboard_handler(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut ui_state: ResMut<UiState>,
    mut current_open_file: ResMut<LoadedFile>,
) {
    if shortcut_pressed!(keys, Ctrl + KeyN) {
        new_file(&mut ui_state);
    }
    if shortcut_pressed!(keys, Ctrl + KeyO) {
        open_file(&mut ui_state);
    }
    if shortcut_pressed!(keys, Ctrl + Shift + KeyS) {
        save_file_as(&mut commands);
    } else if shortcut_pressed!(keys, Ctrl + KeyS) {
        save_file(&mut commands, &mut current_open_file);
    }

    if shortcut_pressed!(keys, Ctrl + Shift + KeyZ) {
        current_open_file.redo(&mut commands);
    } else if shortcut_pressed!(keys, Ctrl + KeyZ) {
        current_open_file.undo(&mut commands);
    }

    if shortcut_pressed!(keys, Numpad7) {
        commands.trigger(PresetView::TopDown);
    }
}

fn close_handler(
    mut commands: Commands,
    mut close_requested: MessageReader<WindowCloseRequested>,
    mut ui_state: ResMut<UiState>,
    mut open_file: ResMut<LoadedFile>,
) {
    for event in close_requested.read() {
        let window = event.window;
        ui_state.request_close_file(move |commands, _| {
            commands.entity(window).despawn();
        });
    }

    match mem::take(&mut ui_state.pending_close_state) {
        PendingCloseState::Confirmed(action) => {
            action(&mut commands, &mut open_file);
        }
        PendingCloseState::PendingUi(action) | PendingCloseState::PendingUserInput(action)
            if !open_file.dirty =>
        {
            action(&mut commands, &mut open_file);
        }
        other => {
            ui_state.pending_close_state = other;
        }
    }
}
