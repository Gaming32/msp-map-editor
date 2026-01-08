use crate::sync::{Direction, MaterialLocation};
use crate::tile_range::TileRange;
use crate::utils::grid_as_vec_vec;
use enum_map::{Enum, EnumMap};
use grid::{Grid, grid};
use monostate::{MustBe, MustBeBool};
use relative_path::RelativePathBuf;
use serde::{Deserialize, Serialize};
use serde_with::OneOrMany;
use serde_with::serde_as;
use std::ops::{AddAssign, Index, IndexMut, Sub};
use strum::IntoStaticStr;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapFile {
    pub starting_tile: MpsVec2,
    pub shop_warp_tiles: Vec<MpsVec2>,
    pub star_warp_tile: MpsVec2,
    pub podium_position: MpsVec2,
    pub results_anim_cam_poses: Vec<MpsVec3>,
    pub tutorial_star: MpsTransform,
    pub tutorial_shop: MpsTransform,
    #[serde(flatten)]
    pub textures: Textures<RelativePathBuf>,
    pub shops: EnumMap<ShopNumber, Vec<ShopItem>>,
    #[serde(with = "grid_as_vec_vec")]
    pub data: Grid<TileData>,
}

impl Default for MapFile {
    fn default() -> Self {
        Self {
            starting_tile: Default::default(),
            shop_warp_tiles: Default::default(),
            star_warp_tile: Default::default(),
            podium_position: Default::default(),
            results_anim_cam_poses: Default::default(),
            tutorial_star: Default::default(),
            tutorial_shop: Default::default(),
            textures: Default::default(),
            shops: Default::default(),
            data: grid![[TileData::default()]],
        }
    }
}

impl MapFile {
    pub fn map_size(&self) -> Option<MpsVec2> {
        Some(MpsVec2::new(
            self.data.cols().try_into().ok()?,
            self.data.rows().try_into().ok()?,
        ))
    }

    pub fn adjust_height(&mut self, range: TileRange, change: f64) {
        for y in range.start.y..=range.end.y {
            let y = y as usize;
            for x in range.start.x..=range.end.x {
                let x = x as usize;
                match &mut self.data[(y, x)].height {
                    TileHeight::Flat { height, .. } => *height += change,
                    TileHeight::Ramp {
                        height: TileRamp { pos, neg, .. },
                        ..
                    } => {
                        *pos += change;
                        *neg += change;
                    }
                }
            }
        }
    }
}

impl Index<MpsVec2> for MapFile {
    type Output = TileData;

    fn index(&self, index: MpsVec2) -> &Self::Output {
        &self.data[(index.y as usize, index.x as usize)]
    }
}

impl IndexMut<MpsVec2> for MapFile {
    fn index_mut(&mut self, index: MpsVec2) -> &mut Self::Output {
        &mut self.data[(index.y as usize, index.x as usize)]
    }
}

#[derive(Copy, Clone, Debug, Default, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub struct MpsVec2 {
    pub x: i32,
    pub y: i32,
}

impl MpsVec2 {
    pub const ZERO: Self = Self::new(0, 0);
    pub const ONE: Self = Self::new(1, 1);

    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self {
            x: self.x.clamp(min.x, max.x),
            y: self.y.clamp(min.y, max.y),
        }
    }

    pub fn max(self, other: Self) -> Self {
        Self {
            x: self.x.max(other.x),
            y: self.y.max(other.y),
        }
    }

    pub fn as_array(self) -> [i32; 2] {
        [self.x, self.y]
    }
}

impl AddAssign for MpsVec2 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl Sub for MpsVec2 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl From<MpsVec2> for [i32; 2] {
    fn from(val: MpsVec2) -> Self {
        val.as_array()
    }
}

impl From<[i32; 2]> for MpsVec2 {
    fn from(value: [i32; 2]) -> Self {
        Self::new(value[0], value[1])
    }
}

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct MpsTransform {
    pub pos: MpsVec3,
    pub rot: MpsVec3,
}

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct MpsVec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl AddAssign for MpsVec3 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
    }
}

impl From<MpsVec2> for MpsVec3 {
    fn from(val: MpsVec2) -> Self {
        MpsVec3 {
            x: val.x as f64,
            y: 0.0,
            z: val.y as f64,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Textures<T> {
    pub skybox: CubeMap<T>,
    pub atlas: T,
}

pub type CubeMap<T> = [T; 6];

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Enum)]
#[serde(rename_all = "kebab-case")]
pub enum ShopNumber {
    #[default]
    #[serde(rename = "shop-1")]
    Shop1,
    #[serde(rename = "shop-2")]
    Shop2,
    #[serde(rename = "shop-3")]
    Shop3,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, IntoStaticStr)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ShopItem {
    #[default]
    DoubleDice,
    TripleDice,
    Pipe,
    GoldPipe,
    CustomDice,
    Tacticooler,
    ShopHopBox,
    InkJet,
    Key,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TileData {
    #[serde(flatten)]
    pub height: TileHeight,
    pub connections: ConnectionMap,
    #[serde(flatten)]
    pub materials: MaterialMap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub popup: Option<PopupType>,
    #[serde(default)]
    pub walk_over: bool,
    pub silver_star_spawnable: bool,
}

impl TileData {
    pub fn ramp(&self) -> bool {
        matches!(self.height, TileHeight::Ramp { .. })
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TileHeight {
    Flat {
        ramp: MustBe!(false),
        height: f64,
    },
    Ramp {
        ramp: MustBe!(true),
        height: TileRamp,
    },
}

impl Default for TileHeight {
    fn default() -> Self {
        Self::Flat {
            ramp: MustBeBool,
            height: 0.0,
        }
    }
}

impl TileHeight {
    pub fn center_height(self) -> f64 {
        match self {
            Self::Flat { height, .. } => height,
            Self::Ramp { height, .. } => (height.pos - height.neg) / 2.0 + height.neg,
        }
    }

    pub fn min_height(self) -> f64 {
        match self {
            Self::Flat { height, .. } => height,
            Self::Ramp { height, .. } => height.pos.min(height.neg),
        }
    }

    pub fn max_height(self) -> f64 {
        match self {
            Self::Flat { height, .. } => height,
            Self::Ramp { height, .. } => height.pos.max(height.neg),
        }
    }

    pub fn pos_height(self) -> f64 {
        match self {
            Self::Flat { height, .. } => height,
            Self::Ramp { height, .. } => height.pos,
        }
    }

    pub fn neg_height(self) -> f64 {
        match self {
            Self::Flat { height, .. } => height,
            Self::Ramp { height, .. } => height.neg,
        }
    }

    pub fn with_pos_height(self, pos: f64) -> Self {
        match self {
            Self::Flat { .. } => panic!("with_pos_height called on TileHeight::flat"),
            Self::Ramp { height, .. } => Self::Ramp {
                ramp: MustBeBool,
                height: TileRamp { pos, ..height },
            },
        }
    }

    pub fn with_neg_height(self, neg: f64) -> Self {
        match self {
            Self::Flat { .. } => panic!("with_neg_height called on TileHeight::flat"),
            Self::Ramp { height, .. } => Self::Ramp {
                ramp: MustBeBool,
                height: TileRamp { neg, ..height },
            },
        }
    }

    pub fn with_flipped_heights(self) -> Self {
        match self {
            Self::Flat { .. } => self,
            Self::Ramp { height, .. } => Self::Ramp {
                ramp: MustBeBool,
                height: TileRamp {
                    pos: height.neg,
                    neg: height.pos,
                    ..height
                },
            },
        }
    }

    pub fn equals_flat(self, other: f64) -> bool {
        self == Self::Flat {
            ramp: MustBeBool,
            height: other,
        }
    }

    pub fn ramp_dir(self) -> Option<TileRampDirection> {
        match self {
            Self::Flat { .. } => None,
            Self::Ramp { height, .. } => Some(height.dir),
        }
    }

    pub fn with_ramp_dir(self, dir: Option<TileRampDirection>) -> Self {
        match (self, dir) {
            (Self::Flat { height, .. }, Some(dir)) => Self::Ramp {
                ramp: MustBeBool,
                height: TileRamp {
                    dir,
                    pos: height,
                    neg: height,
                },
            },
            (Self::Flat { .. }, None) => self,
            (Self::Ramp { height, .. }, Some(dir)) => Self::Ramp {
                ramp: MustBeBool,
                height: TileRamp { dir, ..height },
            },
            (Self::Ramp { .. }, None) => Self::Flat {
                ramp: MustBeBool,
                height: self.center_height(),
            },
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TileRamp {
    pub dir: TileRampDirection,
    pub pos: f64,
    pub neg: f64,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TileRampDirection {
    #[serde(rename = "h")]
    Horizontal,
    #[serde(rename = "v")]
    Vertical,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectionMap {
    #[serde(rename = "n")]
    pub north: Connection,
    #[serde(rename = "e")]
    pub east: Connection,
    #[serde(rename = "s")]
    pub south: Connection,
    #[serde(rename = "w")]
    pub west: Connection,
}

impl Index<Direction> for ConnectionMap {
    type Output = Connection;

    fn index(&self, index: Direction) -> &Self::Output {
        match index {
            Direction::West => &self.west,
            Direction::East => &self.east,
            Direction::North => &self.north,
            Direction::South => &self.south,
        }
    }
}

impl IndexMut<Direction> for ConnectionMap {
    fn index_mut(&mut self, index: Direction) -> &mut Self::Output {
        match index {
            Direction::West => &mut self.west,
            Direction::East => &mut self.east,
            Direction::North => &mut self.north,
            Direction::South => &mut self.south,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Connection {
    Unconditional(bool),
    Conditional(ConnectionCondition),
}

impl Default for Connection {
    fn default() -> Self {
        Self::Unconditional(false)
    }
}

impl Connection {
    pub fn impassible(self) -> bool {
        self == Self::Unconditional(false)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionCondition {
    Lock,
}

#[derive(Clone, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialMap {
    pub material: MpsMaterial,
    pub wall_material: WallMaterialMap,
}

impl Index<MaterialLocation> for MaterialMap {
    type Output = MpsMaterial;

    fn index(&self, index: MaterialLocation) -> &Self::Output {
        match index {
            None => &self.material,
            Some((side, index)) => &self.wall_material[side][index]
        }
    }
}

impl IndexMut<MaterialLocation> for MaterialMap {
    fn index_mut(&mut self, index: MaterialLocation) -> &mut Self::Output {
        match index {
            None => &mut self.material,
            Some((side, index)) => &mut self.wall_material[side][index]
        }
    }
}

#[serde_as]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WallMaterialMap {
    #[serde(rename = "n")]
    #[serde_as(as = "OneOrMany<_>")]
    pub north: Vec<MpsMaterial>,

    #[serde(rename = "e")]
    #[serde_as(as = "OneOrMany<_>")]
    pub east: Vec<MpsMaterial>,

    #[serde(rename = "s")]
    #[serde_as(as = "OneOrMany<_>")]
    pub south: Vec<MpsMaterial>,

    #[serde(rename = "w")]
    #[serde_as(as = "OneOrMany<_>")]
    pub west: Vec<MpsMaterial>,
}

impl Default for WallMaterialMap {
    fn default() -> Self {
        Self {
            north: vec![Default::default()],
            east: vec![Default::default()],
            south: vec![Default::default()],
            west: vec![Default::default()],
        }
    }
}

impl Index<Direction> for WallMaterialMap {
    type Output = Vec<MpsMaterial>;

    fn index(&self, index: Direction) -> &Self::Output {
        match index {
            Direction::West => &self.west,
            Direction::East => &self.east,
            Direction::North => &self.north,
            Direction::South => &self.south,
        }
    }
}

impl IndexMut<Direction> for WallMaterialMap {
    fn index_mut(&mut self, index: Direction) -> &mut Self::Output {
        match index {
            Direction::West => &mut self.west,
            Direction::East => &mut self.east,
            Direction::North => &mut self.north,
            Direction::South => &mut self.south,
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MpsMaterial(AtlasCoordValue);

type AtlasCoordValue = u8;
const ATLAS_SIZE: (AtlasCoordValue, AtlasCoordValue) = (16, 16);

impl MpsMaterial {
    pub const U_INCREMENT: f32 = 1.0 / ATLAS_SIZE.0 as f32;
    pub const V_INCREMENT: f32 = 1.0 / ATLAS_SIZE.1 as f32;
    pub const TEXTURES_PER_ROW: usize = ATLAS_SIZE.0 as usize;
    pub const TEXTURES_COUNT: usize = ATLAS_SIZE.0 as usize * ATLAS_SIZE.1 as usize;

    pub const fn from_index(index: usize) -> Option<Self> {
        if index < Self::TEXTURES_COUNT {
            Some(Self(index as AtlasCoordValue))
        } else {
            None
        }
    }

    /// Return value: `(u1, v1, u2, v2)`
    pub const fn to_uv_coords(self) -> (f32, f32, f32, f32) {
        let u = (self.0 % ATLAS_SIZE.0) as f32 / ATLAS_SIZE.0 as f32;
        let v = (ATLAS_SIZE.1 - 1 - self.0 / ATLAS_SIZE.0) as f32 / ATLAS_SIZE.1 as f32;
        (
            u + 0.001,
            v + 0.001,
            u + Self::U_INCREMENT - 0.001,
            v + Self::V_INCREMENT - 0.001,
        )
    }

    pub const fn from_uv_coords(u: f32, v: f32) -> Self {
        let x = (u * ATLAS_SIZE.0 as f32) as AtlasCoordValue;
        let y = ATLAS_SIZE.1 - 1 - (v * ATLAS_SIZE.1 as f32) as AtlasCoordValue;
        Self(y * ATLAS_SIZE.0 + x)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PopupType {
    #[default]
    LuckySpace,
    #[serde(rename = "star-1")]
    Star1,
    #[serde(rename = "star-2")]
    Star2,
    StarSteal,
    #[serde(untagged)]
    Shop(ShopNumber),
}
