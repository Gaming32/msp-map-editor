use crate::load_file::LoadedTexture;
use crate::schema::{MpsVec2, TileData, TileHeight};
use crate::tile_range::TileRange;
use bevy::prelude::Event;

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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Direction {
    West,
    East,
    North,
    South,
}

#[derive(Event, Copy, Clone, Debug, Eq, PartialEq)]
pub enum PresetView {
    Player,
    Center,
    TopDown,
}
