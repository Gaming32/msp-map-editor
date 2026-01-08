use crate::load_file::LoadedTexture;
use crate::schema::{Connection, MpsMaterial, MpsVec2, PopupType, TileData, TileHeight};
use crate::tile_range::TileRange;
use bevy::prelude::Event;
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
    AdjustHeight(TileRange, f64),
    ChangeHeight(TileRange, Vec<TileHeight>),
    ChangeConnection(TileRange, Direction, Vec<Connection>),
    ChangeMaterial(TileRange, MaterialLocation, Vec<MaterialEdit>),
    ChangePopupType(TileRange, Vec<Option<PopupType>>),
    ChangeCoins(TileRange, Vec<i32>),
    ChangeWalkOver(TileRange, Vec<bool>),
    ChangeSilverStarSpawnable(TileRange, Vec<bool>),
}

pub type MaterialLocation = Option<(Direction, usize)>;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum MaterialEdit {
    Set(MpsMaterial),
    MoveUp,
    MoveDown,
    Remove,
    Insert(MpsMaterial),
}

#[derive(Event, Copy, Clone, Debug)]
pub struct SelectForEditing {
    pub object: EditObject,
    pub exclusive: bool,
}

#[derive(Copy, Clone, Debug)]
pub enum EditObject {
    StartingPosition,
    MapSize(Direction),
    Tile(MpsVec2),
    None,
}

impl EditObject {
    pub fn exclusive_only(self) -> bool {
        match self {
            Self::StartingPosition => true,
            Self::MapSize(_) | Self::Tile { .. } | Self::None => false,
        }
    }

    pub fn directly_usable(self) -> bool {
        match self {
            Self::StartingPosition => true,
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

#[derive(Event, Copy, Clone, Debug, Eq, PartialEq)]
pub enum PresetView {
    Player,
    Center,
    TopDown,
}
