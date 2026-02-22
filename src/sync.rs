use crate::load_file::LoadedTexture;
use crate::schema::{
    AnimationGroup, Connection, MpsMaterial, MpsTransform, MpsVec2, MpsVec3, PopupType, ShopItem,
    ShopNumber, TileAnimation, TileData, TileHeight,
};
use crate::tile_range::TileRange;
use bevy::prelude::{Component, Event};
use bit_set::BitSet;
use std::mem;
use std::sync::Arc;
use strum::{AsRefStr, Display};
use transform_gizmo_bevy::{GizmoHotkeys, GizmoMode, GizmoOptions};

#[derive(Event, Clone, Debug)]
pub struct MapEdited(pub MapEdit);

#[derive(Clone, Debug, PartialEq)]
pub enum MapEdit {
    StartingTile(MpsVec2),
    ShopWarpTile(usize, ListEdit<MpsVec2>),
    StarWarpTile(MpsVec2),
    PodiumPosition(MpsVec2),
    ResultsCamera(usize, ListEdit<MpsVec3>),
    Skybox(usize, LoadedTexture),
    Atlas(LoadedTexture),
    ExpandMap(Direction, Option<Vec<TileData>>),
    ShrinkMap(Direction),
    ChangeCameraPos(CameraId, MpsVec3),
    ChangeCameraRot(CameraId, MpsVec3),
    EditShop(ShopNumber, usize, ListEdit<ShopItem>),
    AdjustHeight(TileRange, f64),
    ChangeHeight(TileRange, Vec<TileHeight>),
    ChangeConnection(TileRange, Direction, Vec<Connection>),
    ChangeMaterial(TileRange, MaterialLocation, Vec<ListEdit<MpsMaterial>>),
    ChangePopupType(TileRange, Vec<Option<PopupType>>),
    ChangeCoins(TileRange, Vec<Option<i32>>),
    ChangeWalkOver(TileRange, Vec<bool>),
    ChangeSilverStarSpawnable(TileRange, Vec<bool>),
    AddAnimationGroup(String, AnimationGroup, Vec<Option<TileAnimation>>),
    DeleteAnimationGroup(String),
    RenameAnimationGroup(String, String, BitSet, Option<Box<(AnimationGroup, usize)>>),
    ChangeAnimationGroupAnchor(String, MpsVec2),
}

pub type MaterialLocation = Option<(Direction, usize)>;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ListEdit<V> {
    Set(V),
    MoveUp,
    MoveDown,
    Remove,
    Insert(V),
}

impl<V> ListEdit<V> {
    pub fn reverse(&self, old_value: impl FnOnce() -> V) -> Self {
        match self {
            Self::Set(_) => Self::Set(old_value()),
            Self::MoveUp => Self::MoveUp,
            Self::MoveDown => Self::MoveDown,
            Self::Remove => Self::Insert(old_value()),
            Self::Insert(_) => Self::Remove,
        }
    }

    pub fn is_self_opposite(&self) -> bool {
        matches!(self, Self::MoveUp | Self::MoveDown)
    }

    pub fn apply(self, index: usize, vec: &mut Vec<V>) {
        match self {
            Self::Set(value) => vec[index] = value,
            Self::MoveUp => vec.swap(index - 1, index),
            Self::MoveDown => vec.swap(index, index + 1),
            Self::Remove => {
                vec.remove(index);
            }
            Self::Insert(value) => vec.insert(index, value),
        }
    }
}

#[derive(Component, Copy, Clone, Debug, PartialEq, Eq)]
pub enum CameraId {
    StarTutorial,
    ShopTutorial,
}

#[derive(Event, Clone, Debug)]
pub struct SelectForEditing {
    pub object: EditObject,
    pub exclusive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditObject {
    StartingTile,
    ShopWarpTile(usize),
    StarWarpTile,
    PodiumPosition,
    ResultsCamera(usize),
    AnimationGroupAnchor(Arc<str>),
    Camera(CameraId),
    MapSize(Direction),
    Tile(MpsVec2),
    None,
}

impl EditObject {
    pub fn get_index_param(&self) -> usize {
        match self {
            EditObject::ShopWarpTile(index) | EditObject::ResultsCamera(index) => *index,
            _ => panic!("EditObject::get_index_param called on unsupported editor"),
        }
    }

    pub fn same_type(&self, other: &EditObject) -> bool {
        mem::discriminant(self) == mem::discriminant(other)
    }

    pub fn update_gizmos(&self, gizmos: GizmoOptions) -> GizmoOptions {
        match self {
            Self::StartingTile
            | Self::ShopWarpTile(_)
            | Self::StarWarpTile
            | Self::PodiumPosition
            | Self::AnimationGroupAnchor(_) => GizmoOptions {
                gizmo_modes: gizmos.gizmo_modes.intersection(
                    GizmoMode::TranslateX | GizmoMode::TranslateZ | GizmoMode::TranslateXZ,
                ),
                snapping: true,
                snap_distance: gizmos.snap_distance.max(1.0),
                group_targets: gizmos.group_targets,
                hotkeys: Some(GizmoHotkeys {
                    enable_snapping: None,
                    enable_accurate_mode: None,
                    ..gizmos.hotkeys.unwrap_or_default()
                }),
                ..gizmos
            },
            Self::ResultsCamera(_) => GizmoOptions {
                gizmo_modes: gizmos.gizmo_modes.intersection(GizmoMode::all_translate()),
                snapping: gizmos.snapping,
                snap_distance: gizmos.snap_angle.max(0.5),
                group_targets: gizmos.group_targets,
                hotkeys: gizmos.hotkeys,
                ..gizmos
            },
            Self::MapSize(_) => GizmoOptions {
                // One of these modes won't work, but since GizmoOptions are applied globally and not per-gizmo, this is all we can do.
                gizmo_modes: gizmos
                    .gizmo_modes
                    .intersection(GizmoMode::TranslateX | GizmoMode::TranslateZ),
                snapping: true,
                snap_distance: gizmos.snap_distance.max(1.0),
                group_targets: false,
                hotkeys: gizmos.hotkeys.map(|hotkeys| GizmoHotkeys {
                    enable_snapping: None,
                    enable_accurate_mode: None,
                    ..hotkeys
                }),
                ..gizmos
            },
            Self::Camera(_) => GizmoOptions {
                gizmo_modes: gizmos.gizmo_modes.intersection(
                    GizmoMode::all_translate() | GizmoMode::all_rotate() | GizmoMode::Arcball,
                ),
                snapping: gizmos.snapping,
                snap_distance: gizmos.snap_angle.max(0.5),
                group_targets: gizmos.group_targets,
                hotkeys: gizmos.hotkeys,
                ..gizmos
            },
            Self::Tile(_) => GizmoOptions {
                gizmo_modes: gizmos
                    .gizmo_modes
                    .intersection(GizmoMode::TranslateY.into()),
                snapping: true,
                snap_distance: gizmos.snap_scale.max(0.5),
                group_targets: gizmos.group_targets,
                hotkeys: gizmos.hotkeys.map(|hotkeys| GizmoHotkeys {
                    enable_snapping: None,
                    ..hotkeys
                }),
                ..gizmos
            },
            Self::None => GizmoOptions {
                gizmo_modes: GizmoMode::all(),
                snapping: false,
                snap_distance: 0.0,
                group_targets: true,
                hotkeys: Some(GizmoHotkeys::default()),
                ..gizmos
            },
        }
    }

    pub fn directly_usable(&self) -> bool {
        match self {
            Self::StartingTile
            | Self::ShopWarpTile(_)
            | Self::StarWarpTile
            | Self::PodiumPosition
            | Self::ResultsCamera(_)
            | Self::AnimationGroupAnchor(_)
            | Self::Camera(_) => true,
            Self::MapSize(_) | Self::Tile(_) | Self::None => false,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, AsRefStr, Display)]
pub enum Direction {
    West,
    East,
    North,
    South,
}

impl Direction {
    pub const ALL_CLOCKWISE: &[Direction] = &[
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ];
}

#[derive(Event, Copy, Clone, Debug, PartialEq)]
pub enum PresetView {
    Player,
    Center,
    Selection,
    TopDown,
    Transform(MpsTransform),
}

#[derive(Event, Copy, Clone, Debug, PartialEq, Eq)]
pub struct TogglePreviewVisibility {
    pub object: PreviewObject,
    pub visible: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PreviewObject {
    GoldPipe,
    Podium,
}

#[derive(Event, Copy, Clone, Debug)]
pub struct PreviewResultsAnimation;
