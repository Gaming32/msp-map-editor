use crate::assets::{icons_atlas, item_icons, unset_texture_icon};
use crate::docking::UiDocking;
use crate::load_file::{
    FileLoaded, LoadedFile, LoadedTexture, MapFileDialog, new_file, open_file, save_file,
    save_file_as,
};
use crate::schema::{
    Connection, ConnectionCondition, CubeMap, MapFile, MpsMaterial, MpsTransform, MpsVec2,
    PopupType, ShopItem, ShopNumber, TileHeight, TileRampDirection,
};
use crate::sync::{
    CameraId, Direction, ListEdit, MaterialLocation, PresetView, PreviewObject,
    PreviewResultsAnimation, TogglePreviewVisibility,
};
use crate::sync::{EditObject, MapEdit, MapEdited, SelectForEditing};
use crate::tile_range::TileRange;
use crate::utils::TriStateCheckbox;
use crate::viewport::ViewportTarget;
use crate::{Directories, shortcut_pressed};
use bevy::asset::LoadState;
use bevy::asset::io::embedded::GetAssetServer;
use bevy::image::{ImageFormatSetting, ImageLoaderSettings};
use bevy::prelude::Image as BevyImage;
use bevy::prelude::*;
use bevy::render::render_resource::Extent3d;
use bevy::window::{PrimaryWindow, WindowCloseRequested};
use bevy_file_dialog::{DialogFilePicked, FileDialogExt, FileDialogPlugin};
use bevy_mod_imgui::prelude::*;
use enum_map::{Enum, EnumMap, enum_map};
use imgui::Image as ImguiImage;
use itertools::Itertools;
use monostate::MustBeBool;
use std::borrow::Cow;
use std::{array, mem};
use strum::VariantArray;

pub struct MapEditorUi;

impl Plugin for MapEditorUi {
    fn build(&self, app: &mut App) {
        let mut imgui_plugin = ImguiPlugin::default();
        if let Some(dirs) = app.world().get_resource::<Directories>() {
            imgui_plugin.ini_filename = Some(dirs.data.join("imgui.ini"));
        }

        app.insert_resource(UiState {
            free_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
            unset_texture_icon: unset_texture_icon(app.get_asset_server()),
            icon_atlas_handle: icons_atlas(app.get_asset_server()),
            item_handles: item_icons(app.get_asset_server()),
            preview_star_warp_tile: true,
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
    item_handles: EnumMap<ShopItem, Handle<BevyImage>>,
    item_textures: Option<EnumMap<ShopItem, TextureId>>,
    material_target: Option<(TileRange, MaterialLocation)>,
    item_target: Option<(ShopNumber, usize)>,
    preview_star_warp_tile: bool,
    preview_podium: bool,
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
    mut state: ResMut<UiState>,
    mut context: NonSendMut<ImguiContext>,
) {
    let new_skybox_textures =
        array::from_fn(|_| context.register_bevy_texture(state.unset_texture_icon.clone()));
    if let Some(old_textures) = state.skybox_textures.replace(new_skybox_textures) {
        state.textures_to_free.extend(old_textures);
    }

    let new_atlas_texture = context.register_bevy_texture(state.unset_texture_icon.clone());
    if let Some(old_texture) = state.atlas_texture.replace(new_atlas_texture) {
        state.textures_to_free.push(old_texture);
    }

    state.preview_star_warp_tile = true;
}

fn on_map_edited(on: On<MapEdited>, mut state: ResMut<UiState>) {
    match &on.0 {
        MapEdit::StartingTile(_)
        | MapEdit::ShopWarpTile(_, _)
        | MapEdit::StarWarpTile(_)
        | MapEdit::PodiumPosition(_)
        | MapEdit::ResultsCamera(_, _) => {}
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
        | MapEdit::ChangeCameraPos(_, _)
        | MapEdit::ChangeCameraRot(_, _)
        | MapEdit::EditShop(_, _, _)
        | MapEdit::AdjustHeight(_, _)
        | MapEdit::ChangeHeight(_, _)
        | MapEdit::ChangeConnection(_, _, _)
        | MapEdit::ChangeMaterial(_, _, _)
        | MapEdit::ChangePopupType(_, _)
        | MapEdit::ChangeCoins(_, _)
        | MapEdit::ChangeWalkOver(_, _)
        | MapEdit::ChangeSilverStarSpawnable(_, _) => {}
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
    if state.item_textures.is_none()
        && state
            .item_handles
            .as_array()
            .iter()
            .all(|x| assets.is_loaded(x))
    {
        state.item_textures = Some(enum_map! {
            x => context.register_bevy_texture(state.item_handles[x].clone()),
        });
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

            if ui.menu_item_config("Selection").shortcut("Num .").build() {
                commands.trigger(PresetView::Selection);
            }

            if ui.menu_item_config("Top-down").shortcut("Num 7").build() {
                commands.trigger(PresetView::TopDown);
            }
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

    let mut open_item_picker = false;
    ui.window("Map settings").collapsible(true).build(|| {
        ui.text("Starting tile");
        ui.same_line();
        if ui.button("Select") {
            commands.trigger(SelectForEditing {
                object: EditObject::StartingTile,
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
            file.edit_map(&mut commands, MapEdit::StartingTile(starting_tile));
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

        const SUPPORTED_IMAGE_EXTENSIONS: &[&str] = &[
            "bmp", "gif", "hdr", "ico", "jpg", "jpeg", "ktx2", "png", "tif", "tiff", "webp",
        ];

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

        if let Some(_token) = ui
            .tree_node_config("Shops")
            .framed(true)
            .tree_push_on_open(false)
            .push()
        {
            for &shop in ShopNumber::VARIANTS {
                open_item_picker |=
                    shop_editor(&ui, &mut commands, &mut state, &mut file, shop, shop.into());
            }
        }

        if let Some(_token) = ui
            .tree_node_config("Cameras")
            .framed(true)
            .tree_push_on_open(false)
            .push()
        {
            let mut tutorial_camera = |label, id, default: fn(&MapFile) -> MpsTransform| {
                if let Some(_token) = ui.tree_node(label) {
                    let default = default(&file.file);
                    if ui.button("Select") {
                        commands.trigger(SelectForEditing {
                            object: EditObject::Camera(id),
                            exclusive: true,
                        });
                    }
                    ui.same_line();
                    if ui.button("Preview") {
                        commands.trigger(PresetView::Transform(default));
                    }
                    let mut pos = default.pos.as_array();
                    if ui
                        .input_scalar_n(format!("##{label} pos"), &mut pos)
                        .display_format("%.2f")
                        .build()
                    {
                        file.edit_map(&mut commands, MapEdit::ChangeCameraPos(id, pos.into()));
                    }
                    let mut rot = default.rot.as_array().map(f64::to_degrees);
                    if ui
                        .input_scalar_n(format!("##{label} rot"), &mut rot)
                        .display_format("%.1f")
                        .build()
                    {
                        file.edit_map(
                            &mut commands,
                            MapEdit::ChangeCameraRot(id, rot.map(f64::to_radians).into()),
                        );
                    }
                }
            };
            tutorial_camera("Star tutorial", CameraId::StarTutorial, |f| f.tutorial_star);
            tutorial_camera("Shop tutorial", CameraId::ShopTutorial, |f| f.tutorial_shop);
        }

        if let Some(_token) = ui
            .tree_node_config("Warp tiles")
            .framed(true)
            .tree_push_on_open(false)
            .push()
        {
            ui.text("Star warp tile");
            ui.same_line();
            if ui.button("Select##Star warp tile") {
                state.preview_star_warp_tile = true;
                commands.trigger(SelectForEditing {
                    object: EditObject::StarWarpTile,
                    exclusive: true,
                });
            }
            ui.same_line();
            if ui.checkbox("Preview##Star warp tile", &mut state.preview_star_warp_tile) {
                commands.trigger(TogglePreviewVisibility {
                    object: PreviewObject::GoldPipe,
                    visible: state.preview_star_warp_tile,
                });
            }
            let mut star_warp_tile = file.file.star_warp_tile.as_array();
            if ui
                .input_scalar_n("##Star warp tile", &mut star_warp_tile)
                .step(1)
                .build()
            {
                let star_warp_tile = file.in_bounds(star_warp_tile.into());
                file.edit_map(&mut commands, MapEdit::StarWarpTile(star_warp_tile));
            }

            ui.spacing();

            if let Some(icon_atlas) = state.icon_atlas_texture
                && let Some(_token) = ui
                    .tree_node_config("Shop hop tiles")
                    .tree_push_on_open(false)
                    .push()
            {
                let mut edit = None;
                let tiles = &file.file.shop_warp_tiles;
                for (index, &shop_hop) in tiles.iter().enumerate() {
                    ui.text(format!("Shop hop #{}", index + 1));

                    ui.same_line();
                    if ui.button(format!("Select##Shop hop {index}")) {
                        commands.trigger(SelectForEditing {
                            object: EditObject::ShopWarpTile(index),
                            exclusive: true,
                        });
                    }

                    ui.same_line();
                    ui.disabled(tiles.len() <= 1, || {
                        if ui
                            .image_button_config(
                                format!("Remove shop hop {index}"),
                                icon_atlas,
                                [16.0; 2],
                            )
                            .uv0([0.5, 0.0])
                            .uv1([1.0, 0.5])
                            .build()
                        {
                            edit = Some(MapEdit::ShopWarpTile(index, ListEdit::Remove));
                        }
                    });

                    let mut current_tile = shop_hop.as_array();
                    if ui
                        .input_scalar_n(format!("##Shop hop {index}"), &mut current_tile)
                        .step(1)
                        .build()
                    {
                        let current_tile = file.in_bounds(current_tile.into());
                        edit = Some(MapEdit::ShopWarpTile(index, ListEdit::Set(current_tile)));
                    }

                    ui.spacing();
                }
                if ui.button("Add shop hop") {
                    edit = Some(MapEdit::ShopWarpTile(
                        tiles.len(),
                        ListEdit::Insert(MpsVec2::ZERO),
                    ));
                }
                if let Some(edit) = edit {
                    let (index, hop) = match &edit {
                        MapEdit::ShopWarpTile(index, hop) => (*index, *hop),
                        _ => unreachable!(),
                    };
                    file.edit_map(&mut commands, edit);
                    if matches!(hop, ListEdit::Insert(_)) {
                        commands.trigger(SelectForEditing {
                            object: EditObject::ShopWarpTile(index),
                            exclusive: true,
                        });
                    }
                }
            }
        }

        if let Some(_token) = ui
            .tree_node_config("Ending sequence")
            .framed(true)
            .tree_push_on_open(false)
            .push()
        {
            ui.text("Podium");
            ui.same_line();
            if ui.button("Select##Podium") {
                state.preview_podium = true;
                commands.trigger(SelectForEditing {
                    object: EditObject::PodiumPosition,
                    exclusive: true,
                });
            }
            ui.same_line();
            if ui.checkbox("Preview##Podium", &mut state.preview_podium) {
                commands.trigger(TogglePreviewVisibility {
                    object: PreviewObject::Podium,
                    visible: state.preview_podium,
                });
            }
            let mut podium_pos = file.file.podium_position.as_array();
            if ui
                .input_scalar_n("##Podium", &mut podium_pos)
                .step(1)
                .build()
            {
                let podium_pos = file.in_bounds(podium_pos.into());
                file.edit_map(&mut commands, MapEdit::PodiumPosition(podium_pos));
            }

            ui.spacing();

            if let Some(icon_atlas) = state.icon_atlas_texture
                && let Some(_token) = ui
                    .tree_node_config("Results animation")
                    .tree_push_on_open(false)
                    .push()
            {
                let poses = &file.file.results_anim_cam_poses;

                let mut edit = None;
                if ui.button("Preview") {
                    commands.trigger(PreviewResultsAnimation);
                }
                ui.same_line();
                if ui.button("Add camera") {
                    edit = Some(MapEdit::ResultsCamera(0, ListEdit::Insert(poses[0])));
                }
                ui.same_line();
                if ui.button("Select all") {
                    commands.trigger(SelectForEditing {
                        object: EditObject::ResultsCamera(0),
                        exclusive: true,
                    });
                    for index in 1..poses.len() {
                        commands.trigger(SelectForEditing {
                            object: EditObject::ResultsCamera(index),
                            exclusive: false,
                        });
                    }
                }

                for (index, &pos) in poses.iter().enumerate() {
                    ui.spacing();

                    ui.text(format!("Camera #{}", index + 1));
                    ui.same_line();
                    if ui.button(format!("Select##Results camera {index}")) {
                        commands.trigger(SelectForEditing {
                            object: EditObject::ResultsCamera(index),
                            exclusive: true,
                        });
                    }

                    ui.same_line();
                    ui.disabled(index == 0, || {
                        if ui
                            .image_button_config(
                                format!("Move camera up {index}"),
                                icon_atlas,
                                [16.0; 2],
                            )
                            .uv0([0.0, 0.5])
                            .uv1([0.5, 1.0])
                            .build()
                        {
                            edit = Some(MapEdit::ResultsCamera(index, ListEdit::MoveUp));
                        }
                    });

                    ui.same_line();
                    ui.disabled(index >= poses.len() - 1, || {
                        if ui
                            .image_button_config(
                                format!("Move segment down {index}"),
                                icon_atlas,
                                [16.0; 2],
                            )
                            .uv0([0.5, 0.5])
                            .uv1([1.0, 1.0])
                            .build()
                        {
                            edit = Some(MapEdit::ResultsCamera(index, ListEdit::MoveDown));
                        }
                    });

                    ui.same_line();
                    ui.disabled(poses.len() <= 3, || {
                        if ui
                            .image_button_config(
                                format!("Remove results camera {index}"),
                                icon_atlas,
                                [16.0; 2],
                            )
                            .uv0([0.5, 0.0])
                            .uv1([1.0, 0.5])
                            .build()
                        {
                            edit = Some(MapEdit::ResultsCamera(index, ListEdit::Remove));
                        }
                    });

                    let mut current_pos = pos.as_array();
                    if ui
                        .input_scalar_n(format!("##Results camera {index}"), &mut current_pos)
                        .display_format("%.2f")
                        .build()
                    {
                        edit = Some(MapEdit::ResultsCamera(
                            index,
                            ListEdit::Set(current_pos.into()),
                        ));
                    }
                }

                ui.spacing();
                if ui.button("Add camera##Below") {
                    edit = Some(MapEdit::ResultsCamera(
                        poses.len(),
                        ListEdit::Insert(poses[poses.len() - 1]),
                    ));
                }

                if let Some(edit) = edit {
                    let (index, camera) = match &edit {
                        MapEdit::ResultsCamera(index, hop) => (*index, *hop),
                        _ => unreachable!(),
                    };
                    file.edit_map(&mut commands, edit);
                    if matches!(camera, ListEdit::Insert(_)) {
                        commands.trigger(SelectForEditing {
                            object: EditObject::ResultsCamera(index),
                            exclusive: true,
                        });
                    }
                }
            }
        }
    });

    let mut open_material_picker = false;
    ui.window("Tile settings").collapsible(true).build(|| {
        let Some(range) = file.selected_range else {
            ui.text("No tile selected");
            return;
        };

        const MULTIPLE_VALUES: &str = "<multiple values>";

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

        macro_rules! simple_combo_box {
            (
                label: $label:expr,
                getter: ($($getter:tt)+),
                options: [$($option:expr),* $(,)?],
                option_labels: {
                    $($option_pat:pat => $option_display:expr,)*
                },
                editor: |$editor_var:ident| $editor:expr,
            ) => {{
                let single_value = range
                    .into_iter()
                    .map(|x| file.file[x]$($getter)+)
                    .all_equal_value()
                    .ok();
                let options: &[_] = if single_value.is_some() {
                    &[
                        $(Some($option),)*
                    ]
                } else {
                    &[
                        None,
                        $(Some($option),)*
                    ]
                };
                let mut index = options.iter().position(|&x| x == single_value).unwrap();
                let changed = ui.combo($label, &mut index, options, |&value| {
                    match value {
                        None => MULTIPLE_VALUES.into(),
                        $(Some($option_pat) => $option_display.into(),)*
                    }
                });
                if changed && let Some($editor_var) = options[index] {
                    $editor;
                    Some($editor_var)
                } else {
                    single_value
                }
            }};
        }

        ui.spacing();

        let ramp_type = simple_combo_box!(
            label: "Height type",
            getter: (.height.ramp_dir()),
            options: [None, Some(TileRampDirection::Horizontal), Some(TileRampDirection::Vertical)],
            option_labels: {
                None => "Flat",
                Some(TileRampDirection::Horizontal) => "West/East Ramp",
                Some(TileRampDirection::Vertical) => "North/South Ramp",
            },
            editor: |new_type| file.change_heights(&mut commands, range, |h| h.with_ramp_dir(new_type)),
        );

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

            material_button(Cow::Borrowed("Top material"), None);
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
                    material_button(Cow::Owned(format!("{side} segment {index}")), location);

                    ui.same_line();
                    ui.disabled(index == 0, || {
                        if ui
                            .image_button_config(format!("Move segment up {index}"), icon_atlas, [16.0; 2])
                            .uv0([0.0, 0.5])
                            .uv1([0.5, 1.0])
                            .build()
                        {
                            edit = Some(MapEdit::ChangeMaterial(
                                range,
                                location,
                                vec![ListEdit::MoveUp; range.area()],
                            ));
                        }
                    });

                    ui.same_line();
                    ui.disabled(index >= min_segments - 1, || {
                        if ui
                            .image_button_config(
                                format!("Move segment down {index}"),
                                icon_atlas,
                                [16.0; 2],
                            )
                            .uv0([0.5, 0.5])
                            .uv1([1.0, 1.0])
                            .build()
                        {
                            edit = Some(MapEdit::ChangeMaterial(
                                range,
                                location,
                                vec![ListEdit::MoveDown; range.area()],
                            ));
                        }
                    });

                    ui.same_line();
                    ui.disabled(min_segments <= 1, || {
                        if ui
                            .image_button_config(format!("Remove segment {index}"), icon_atlas, [16.0; 2])
                            .uv0([0.5, 0.0])
                            .uv1([1.0, 0.5])
                            .build()
                        {
                            edit = Some(MapEdit::ChangeMaterial(
                                range,
                                location,
                                vec![ListEdit::Remove; range.area()],
                            ));
                        }
                    });
                }
                if segment_count.is_some() && ui.button("Add segment") {
                    edit = Some(MapEdit::ChangeMaterial(
                        range,
                        Some((*side, min_segments)),
                        vec![ListEdit::Insert(MpsMaterial::default()); range.area()],
                    ));
                }
            }
            if let Some(edit) = edit {
                let (location, material) = match &edit {
                    MapEdit::ChangeMaterial(_, location, material) => (*location, material[0]),
                    _ => unreachable!(),
                };
                file.edit_map(&mut commands, edit);
                if matches!(material, ListEdit::Insert(_)) {
                    state.material_target = Some((range, location));
                    open_material_picker = true;
                }
            }
        }

        if let Some(_token) = ui
            .tree_node_config("Connections")
            .framed(true)
            .tree_push_on_open(false)
            .push()
        {
            for direction in Direction::ALL_CLOCKWISE {
                simple_combo_box!(
                    label: direction,
                    getter: (.connections[*direction]),
                    options: [
                        Connection::Unconditional(false),
                        Connection::Unconditional(true),
                        Connection::Conditional(ConnectionCondition::Lock),
                    ],
                    option_labels: {
                        Connection::Unconditional(false) => "Block",
                        Connection::Unconditional(true) => "Passable",
                        Connection::Conditional(ConnectionCondition::Lock) => "Locked gate",
                    },
                    editor: |new_type| file.edit_map(
                        &mut commands,
                        MapEdit::ChangeConnection(range, *direction, vec![new_type; range.area()]),
                    ),
                );
            }
        }

        if let Some(_token) = ui
            .tree_node_config("Behavior")
            .framed(true)
            .tree_push_on_open(false)
            .push()
        {
            let mut coins = range
                .into_iter()
                .map(|x| file.file[x].coins.unwrap_or_default())
                .all_equal_value()
                .ok();
            let coins_changed = if let Some(coins) = coins.as_mut() {
                ui.input_int("Coin change", coins).build()
            } else {
                let mut buf = MULTIPLE_VALUES.to_string();
                if ui.input_text("Coin change", &mut buf).build() {
                    coins = buf.parse().ok();
                }
                coins.is_some()
            };
            if coins_changed {
                if coins == Some(0) {
                    coins = None;
                }
                file.edit_map(&mut commands, MapEdit::ChangeCoins(range, vec![coins; range.area()]));
            }

            let mut walk_over = range
                .into_iter()
                .map(|x| file.file[x].walk_over)
                .all_equal_value()
                .ok();
            if ui.checkbox_tri_state("Skip dice countdown", &mut walk_over) {
                file.edit_map(&mut commands, MapEdit::ChangeWalkOver(range, vec![walk_over.unwrap(); range.area()]));
            }

            let mut silver_star_spawnable = range
                .into_iter()
                .map(|x| file.file[x].silver_star_spawnable)
                .all_equal_value()
                .ok();
            if ui.checkbox_tri_state("Spawn silver stars", &mut silver_star_spawnable) {
                file.edit_map(&mut commands, MapEdit::ChangeSilverStarSpawnable(range, vec![silver_star_spawnable.unwrap(); range.area()]));
            }

            let popup = simple_combo_box!(
                label: "Popup type",
                getter: (.popup),
                options: [
                    None,
                    Some(PopupType::LuckySpace),
                    Some(PopupType::Star1),
                    Some(PopupType::Star2),
                    Some(PopupType::StarSteal),
                    Some(PopupType::Shop(ShopNumber::Shop1)),
                    Some(PopupType::Shop(ShopNumber::Shop2)),
                    Some(PopupType::Shop(ShopNumber::Shop3)),
                ],
                option_labels: {
                    None => "None",
                    Some(PopupType::LuckySpace) => "Lucky space",
                    Some(PopupType::Star1) => "Star",
                    Some(PopupType::Star2) => "Two stars",
                    Some(PopupType::StarSteal) => "Star steal",
                    Some(PopupType::Shop(shop)) => <ShopNumber as Into<&str>>::into(shop),
                },
                editor: |new_popup| file.edit_map(
                    &mut commands,
                    MapEdit::ChangePopupType(range, vec![new_popup; range.area()])
                ),
            );

            if let Some(Some(PopupType::Shop(shop)))= popup {
                open_item_picker |= shop_editor(&ui, &mut commands, &mut state, &mut file, shop, "Shop items");
            }
        }
    });

    if open_item_picker {
        ui.open_popup("Item picker");
    }
    if open_material_picker {
        ui.open_popup("Material picker");
    }

    viewport_target.disable_input = false;

    if let Some(textures) = state.item_textures
        && let Some((target_shop, target_index)) = state.item_target
    {
        ui.popup("Item picker", || {
            viewport_target.disable_input = true;
            const PER_ROW: usize = ShopItem::LENGTH.isqrt();
            for (index, &item) in ShopItem::VARIANTS.iter().enumerate() {
                if index % PER_ROW != 0 {
                    ui.same_line();
                }
                if ui.image_button(format!("Item {index}"), textures[item], [124.0; 2]) {
                    file.edit_map(
                        &mut commands,
                        MapEdit::EditShop(target_shop, target_index, ListEdit::Set(item)),
                    );
                    ui.close_current_popup();
                }
            }
        });
    }

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
                            vec![ListEdit::Set(material); target_range.area()],
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

fn shop_editor(
    ui: &&mut Ui,
    commands: &mut Commands,
    state: &mut ResMut<UiState>,
    file: &mut ResMut<LoadedFile>,
    shop: ShopNumber,
    label: &str,
) -> bool {
    let Some(icon_atlas) = state.icon_atlas_texture else {
        return false;
    };
    let Some(item_textures) = state.item_textures else {
        return false;
    };
    let Some(_token) = ui.tree_node(label) else {
        return false;
    };
    let shop_items = &file.file.shops[shop];

    let mut edit = None;
    let mut open_item_picker = false;
    for (index, &item) in shop_items.iter().enumerate() {
        if ui.image_button(format!("Item {index}"), item_textures[item], [62.0; 2]) {
            state.item_target = Some((shop, index));
            open_item_picker = true;
        }

        ui.same_line();
        ui.disabled(index == 0, || {
            if ui
                .image_button_config(format!("Move item up {index}"), icon_atlas, [16.0; 2])
                .uv0([0.0, 0.5])
                .uv1([0.5, 1.0])
                .build()
            {
                edit = Some(MapEdit::EditShop(shop, index, ListEdit::MoveUp));
            }
        });

        ui.same_line();
        ui.disabled(index >= shop_items.len() - 1, || {
            if ui
                .image_button_config(format!("Move item down {index}"), icon_atlas, [16.0; 2])
                .uv0([0.5, 0.5])
                .uv1([1.0, 1.0])
                .build()
            {
                edit = Some(MapEdit::EditShop(shop, index, ListEdit::MoveDown));
            }
        });

        ui.same_line();
        if ui
            .image_button_config(format!("Remove item {index}"), icon_atlas, [16.0; 2])
            .uv0([0.5, 0.0])
            .uv1([1.0, 0.5])
            .build()
        {
            edit = Some(MapEdit::EditShop(shop, index, ListEdit::Remove));
        }
    }
    if ui.button("Add item") {
        edit = Some(MapEdit::EditShop(
            shop,
            shop_items.len(),
            ListEdit::Insert(ShopItem::default()),
        ));
    }
    if let Some(edit) = edit {
        let (index, item) = match &edit {
            MapEdit::EditShop(_, index, item) => (*index, *item),
            _ => unreachable!(),
        };
        file.edit_map(commands, edit);
        if matches!(item, ListEdit::Insert(_)) {
            state.item_target = Some((shop, index));
            open_item_picker = true;
        }
    }
    open_item_picker
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

    if shortcut_pressed!(keys, NumpadDecimal) {
        commands.trigger(PresetView::Selection);
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
