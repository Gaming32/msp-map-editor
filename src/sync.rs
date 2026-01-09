use crate::load_file::LoadedTexture;
use crate::schema::{
    Connection, MpsMaterial, MpsTransform, MpsVec2, MpsVec3, PopupType, TileData, TileHeight,
};
use crate::tile_range::TileRange;
use bevy::prelude::{Component, Event};
use strum::{AsRefStr, Display};

#[derive(Event, Clone, Debug)]
pub struct MapEdited(pub MapEdit);

#[derive(Clone, Debug, PartialEq)]
pub enum MapEdit {
    StartingPosition(MpsVec2),
    Skybox(usize, LoadedTexture),
    Atlas(LoadedTexture),
    ExpandMap(Direction, Option<Vec<TileData>>),
    ShrinkMap(Direction),
    ChangeCameraPos(CameraId, MpsVec3),
    ChangeCameraRot(CameraId, MpsVec3),
    AdjustHeight(TileRange, f64),
    ChangeHeight(TileRange, Vec<TileHeight>),
    ChangeConnection(TileRange, Direction, Vec<Connection>),
    ChangeMaterial(TileRange, MaterialLocation, Vec<ListEdit<MpsMaterial>>),
    ChangePopupType(TileRange, Vec<Option<PopupType>>),
    ChangeCoins(TileRange, Vec<i32>),
    ChangeWalkOver(TileRange, Vec<bool>),
    ChangeSilverStarSpawnable(TileRange, Vec<bool>),
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

#[derive(Component, Copy, Clone, Debug, PartialEq, Eq)]
pub enum CameraId {
    StarTutorial,
    ShopTutorial,
}

#[derive(Event, Copy, Clone, Debug)]
pub struct SelectForEditing {
    pub object: EditObject,
    pub exclusive: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EditObject {
    StartingPosition,
    MapSize(Direction),
    Camera(CameraId),
    Tile(MpsVec2),
    None,
}

impl EditObject {
    // TODO: Rework exclusive_only to specify an exclusive set
    pub fn exclusive_only(self) -> bool {
        match self {
            Self::StartingPosition | Self::Camera(_) => true,
            Self::MapSize(_) | Self::Tile { .. } | Self::None => false,
        }
    }

    pub fn directly_usable(self) -> bool {
        match self {
            Self::StartingPosition | Self::Camera(_) => true,
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
