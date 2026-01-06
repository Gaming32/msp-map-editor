use crate::assets::{reload_icon, unset_texture_icon};
use crate::docking::UiDocking;
use crate::load_file::{
    FileLoaded, LoadedFile, LoadedTexture, MapFileDialog, new_file, open_file, save_file,
    save_file_as,
};
use crate::schema::CubeMap;
use crate::sync::{Direction, PresetView};
use crate::sync::{EditObject, MapEdit, MapEdited, SelectForEditing};
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
use imgui::Image as ImguiImage;
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
            reload_icon_handle: reload_icon(app.get_asset_server()),
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
    reload_icon_handle: Handle<BevyImage>,
    reload_icon: Option<TextureId>,
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
        MapEdit::ExpandMap(_, _) | MapEdit::ShrinkMap(_) => {}
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
    mut current_open_file: ResMut<LoadedFile>,
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

    if state.reload_icon.is_none() && assets.is_loaded(&state.reload_icon_handle) {
        state.reload_icon = Some(context.register_bevy_texture(state.reload_icon_handle.clone()));
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
                save_file(&mut commands, &mut current_open_file);
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
                .enabled(current_open_file.can_undo())
                .build()
            {
                current_open_file.undo(&mut commands);
            }

            if ui
                .menu_item_config("Redo")
                .shortcut("Ctrl+Shift+Z")
                .enabled(current_open_file.can_redo())
                .build()
            {
                current_open_file.redo(&mut commands);
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
        let mut starting_tile = current_open_file.file.starting_tile;
        if ui.input_int2("##Starting Tile", &mut starting_tile).build() {
            starting_tile = current_open_file.in_bounds(starting_tile);
            current_open_file.edit_map(&mut commands, MapEdit::StartingPosition(starting_tile));
        }

        ui.spacing();

        ui.text(format!(
            "Map size: {}x{}",
            current_open_file.file.data.cols(),
            current_open_file.file.data.rows()
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
                let texture = &current_open_file.loaded_textures.atlas;
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
            && let Some(reload_icon) = state.reload_icon
            && let Some(_token) = ui
                .tree_node_config("Skybox")
                .framed(true)
                .tree_push_on_open(false)
                .push()
        {
            const LABELS: CubeMap<&str> = ["Right", "Left", "Up", "Down", "Front", "Back"];
            for (index, (label, texture)) in LABELS.iter().zip(skybox.iter()).enumerate() {
                if ui.image_button(format!("Select {label}"), *texture, [128.0; 2]) {
                    commands
                        .dialog()
                        .set_title("Choose skybox file")
                        .add_filter("Images", SUPPORTED_IMAGE_EXTENSIONS)
                        .pick_file_path(SettingImagePick::Skybox(index));
                }
                ui.same_line();
                if ui.image_button(format!("Reload {label}"), reload_icon, [16.0; 2]) {
                    let texture = &current_open_file.loaded_textures.skybox[index];
                    assets.reload(texture.path.clone());
                    commands.trigger(MapEdited(MapEdit::Skybox(index, texture.clone())));
                }
                ui.same_line();
                ui.text(label);
            }
        }
    });

    ui.window("Tile settings").collapsible(true).build(|| {
        ui.text("No tile selected");
    });

    match mem::take(&mut state.pending_close_state) {
        PendingCloseState::PendingUi(action) if current_open_file.dirty => {
            ui.open_popup("Are you sure?");
            state.pending_close_state = PendingCloseState::PendingUserInput(action);
        }
        other => {
            state.pending_close_state = other;
        }
    }
    ui.modal_popup("Are you sure?", || {
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
            save_file(&mut commands, &mut current_open_file);
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
