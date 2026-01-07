use crate::assets;
use crate::assets::key_gate;
use crate::schema::{
    Connection, ConnectionCondition, MpsMaterial, TileData, TileHeight, TileRampDirection,
};
use crate::sync::{Direction, TileRange};
use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use grid::Grid;
use std::cmp::Ordering;
use std::f32::consts::{FRAC_PI_2, PI};

#[derive(Component)]
pub struct MapMeshMarker;

pub fn mesh_map(
    map: &Grid<TileData>,
    atlas: Handle<StandardMaterial>,
    assets: &AssetServer,
    materials: &mut Assets<StandardMaterial>,
    meshes: &mut Assets<Mesh>,
) -> impl Bundle {
    #[derive(Bundle)]
    struct MeshObject {
        mesh: Mesh3d,
        material: MeshMaterial3d<StandardMaterial>,
        transform: Transform,
    }

    let mut state = State::new(map);
    let mut block_children = vec![];
    let mut key_gates = vec![];

    let block_material = materials.add(StandardMaterial {
        base_color: Srgba::rgb_u8(0x11, 0x11, 0x11).into(),
        perceptual_roughness: 1.0,
        ..Default::default()
    });
    let trim_material = materials.add(StandardMaterial {
        base_color: Srgba::rgb_u8(0xAA, 0xAA, 0xAA).into(),
        perceptual_roughness: 1.0,
        ..Default::default()
    });

    for ((y, x), tile) in map.indexed_iter() {
        if tile.height == TileHeight::default() {
            continue;
        }

        internal_mesh_top(&mut state, x, y, tile, 0.0);
        match tile.height {
            TileHeight::Flat { height, .. } => {
                if x == 0 || height > map[(y, x - 1)].height.min_height() {
                    mesh_wall(&mut state, x, y, tile, Direction::West);
                }
                if x == map.cols() - 1 || height > map[(y, x + 1)].height.min_height() {
                    mesh_wall(&mut state, x, y, tile, Direction::East);
                }
                if y == 0 || height > map[(y - 1, x)].height.min_height() {
                    mesh_wall(&mut state, x, y, tile, Direction::North);
                }
                if y == map.rows() - 1 || height > map[(y + 1, x)].height.min_height() {
                    mesh_wall(&mut state, x, y, tile, Direction::South);
                }
            }
            TileHeight::Ramp { height, .. } => {
                let dir_v = height.dir == TileRampDirection::Vertical;
                let height = tile.height.center_height();
                if dir_v && (x == 0 || height > map[(y, x - 1)].height.center_height()) {
                    mesh_wall(&mut state, x, y, tile, Direction::West);
                }
                if dir_v && (x == map.cols() - 1 || height > map[(y, x + 1)].height.center_height())
                {
                    mesh_wall(&mut state, x, y, tile, Direction::East);
                }
                if !dir_v && (y == 0 || height > map[(y - 1, x)].height.center_height()) {
                    mesh_wall(&mut state, x, y, tile, Direction::North);
                }
                if !dir_v
                    && (y == map.rows() - 1 || height > map[(y + 1, x)].height.center_height())
                {
                    mesh_wall(&mut state, x, y, tile, Direction::South);
                }
            }
        }

        let xf = x as f32;
        let yf = y as f32;
        let center_height = tile.height.center_height();

        // Blocks
        const BLOCK_SIZE: f32 = 1.0 / 8.0;
        const BLOCK_SIZE_2: f32 = BLOCK_SIZE / 2.0;
        let mut add_block = |width, depth, x, z| {
            block_children.push(MeshObject {
                mesh: Mesh3d(meshes.add(Cuboid::new(width, BLOCK_SIZE, depth).mesh())),
                material: MeshMaterial3d(block_material.clone()),
                transform: Transform::from_translation(Vec3::new(
                    x,
                    center_height as f32 + BLOCK_SIZE_2,
                    z,
                )),
            })
        };
        if x > 0 && tile.connections.west.impassible() && !tile.ramp() {
            let neighbor = &map[(y, x - 1)];
            if !neighbor.ramp() {
                let neighbor_height = neighbor.height.center_height();
                if center_height > neighbor_height {
                    add_block(BLOCK_SIZE, 1.0, xf - 0.5 + BLOCK_SIZE_2, yf);
                } else if center_height == neighbor_height {
                    add_block(BLOCK_SIZE, 1.0, xf - 0.5, yf);
                }
            }
        }
        if x < map.cols() - 1
            && tile.connections.east.impassible()
            && !tile.ramp()
            && center_height > map[(y, x + 1)].height.center_height()
        {
            add_block(BLOCK_SIZE, 1.0, xf + 0.5 - BLOCK_SIZE_2, yf);
        }
        if y > 0 && tile.connections.north.impassible() && !tile.ramp() {
            let neighbor = &map[(y - 1, x)];
            if !neighbor.ramp() {
                let neighbor_height = neighbor.height.center_height();
                if center_height > neighbor.height.center_height() {
                    add_block(1.0, BLOCK_SIZE, xf, yf - 0.5 + BLOCK_SIZE_2);
                } else if center_height == neighbor_height {
                    add_block(1.0, BLOCK_SIZE, xf, yf - 0.5);
                }
            }
        }
        if y < map.rows() - 1
            && tile.connections.south.impassible()
            && !tile.ramp()
            && center_height > map[(y + 1, x)].height.center_height()
        {
            add_block(1.0, BLOCK_SIZE, xf, yf + 0.5 - BLOCK_SIZE_2);
        }

        // Trims
        const TRIM_SIZE: f32 = BLOCK_SIZE / 2.0;
        const TRIM_SIZE_2: f32 = TRIM_SIZE / 2.0;
        let mut add_trim = |width, depth, x, y_offset, z, x_angle, z_angle| {
            block_children.push(MeshObject {
                mesh: Mesh3d(meshes.add(Cuboid::new(width, TRIM_SIZE, depth).mesh())),
                material: MeshMaterial3d(trim_material.clone()),
                transform: Transform::from_translation(Vec3::new(
                    x,
                    center_height as f32 + y_offset,
                    z,
                ))
                .with_rotation(Quat::from_euler(
                    EulerRot::XYZ,
                    x_angle,
                    0.0,
                    z_angle,
                )),
            })
        };
        macro_rules! x_axis_trim {
            ($non_ramp_x_cond:expr, $x_check_col:expr, $x_coord:expr) => {
                match tile.height {
                    TileHeight::Flat { height, .. } => {
                        let mut extension = 0.0;
                        let mut offset = 0.0;
                        if y > 0
                            && let TileHeight::Ramp {
                                height: neighbor, ..
                            } = map[(y - 1, x)].height
                            && neighbor.neg < height
                        {
                            let v =
                                (f64::atan2(neighbor.neg - neighbor.pos, 1.0) / -2.0).sin() as f32;
                            offset += v * TRIM_SIZE_2;
                            extension += v.abs() * TRIM_SIZE;
                        }
                        if y < map.rows() - 1
                            && let TileHeight::Ramp {
                                height: neighbor, ..
                            } = map[(y + 1, x)].height
                            && neighbor.neg < height
                        {
                            let v =
                                (f64::atan2(neighbor.neg - neighbor.pos, 1.0) / -2.0).sin() as f32;
                            offset += v * TRIM_SIZE_2;
                            extension += v.abs() * TRIM_SIZE;
                        }
                        if $non_ramp_x_cond
                            && y > 0
                            && map[(y - 1, $x_check_col)].height == tile.height
                        {
                            offset += TRIM_SIZE_2;
                            extension += TRIM_SIZE;
                        }
                        if $non_ramp_x_cond
                            && y < map.rows() - 1
                            && map[(y + 1, $x_check_col)].height == tile.height
                        {
                            offset += TRIM_SIZE_2;
                            extension += TRIM_SIZE;
                        }
                        add_trim(
                            TRIM_SIZE,
                            1.0 + extension,
                            $x_coord,
                            TRIM_SIZE_2,
                            yf - offset,
                            0.0,
                            0.0,
                        );
                    }
                    TileHeight::Ramp { height, .. } => {
                        let mut extension = 0.0;
                        let angle = f64::atan2(height.neg - height.pos, 1.0) as f32;
                        let max_height = height.pos.max(height.neg);
                        let v = (-angle / 2.0).sin().abs() / 16.0;
                        if y > 0 && map[(y - 1, x)].height.equals_flat(max_height) {
                            extension += v;
                        }
                        if y < map.rows() - 1 && map[(y + 1, x)].height.equals_flat(max_height) {
                            extension += v;
                        }
                        add_trim(
                            TRIM_SIZE,
                            ((height.pos - height.neg).powi(2) + 1.0).sqrt() as f32 + extension,
                            $x_coord,
                            (TRIM_SIZE_2 + extension / 2.0) * angle.cos(),
                            yf + (TRIM_SIZE_2 - extension / 2.0) * angle.sin(),
                            angle,
                            0.0,
                        );
                    }
                }
            };
        }
        macro_rules! z_axis_trim {
            ($z_coord:expr) => {
                match tile.height {
                    TileHeight::Flat { height, .. } => {
                        let mut extension = 0.0;
                        let mut offset = 0.0;
                        if x > 0
                            && let TileHeight::Ramp {
                                height: neighbor, ..
                            } = map[(y, x - 1)].height
                            && neighbor.neg < height
                        {
                            let v =
                                (f64::atan2(neighbor.neg - neighbor.pos, 1.0) / -2.0).sin() as f32;
                            offset += v * TRIM_SIZE_2;
                            extension += v.abs() * TRIM_SIZE;
                        }
                        if x < map.cols() - 1
                            && let TileHeight::Ramp {
                                height: neighbor, ..
                            } = map[(y, x + 1)].height
                            && neighbor.neg < height
                        {
                            let v =
                                (f64::atan2(neighbor.neg - neighbor.pos, 1.0) / -2.0).sin() as f32;
                            offset += v * TRIM_SIZE_2;
                            extension += v.abs() * TRIM_SIZE;
                        }
                        add_trim(
                            1.0 + extension,
                            TRIM_SIZE,
                            xf - offset,
                            TRIM_SIZE_2,
                            $z_coord,
                            0.0,
                            0.0,
                        );
                    }
                    TileHeight::Ramp { height, .. } => {
                        let mut extension = 0.0;
                        let angle = f64::atan2(height.neg - height.pos, 1.0) as f32;
                        let max_height = height.pos.max(height.neg);
                        let v = (-angle / 2.0).sin().abs() / 16.0;
                        if x > 0 && map[(y, x - 1)].height.equals_flat(max_height) {
                            extension += v;
                        }
                        if x < map.cols() - 1 && map[(y, x + 1)].height.equals_flat(max_height) {
                            extension += v;
                        }
                        add_trim(
                            ((height.pos - height.neg).powi(2) + 1.0).sqrt() as f32 + extension,
                            TRIM_SIZE,
                            xf + (TRIM_SIZE_2 + extension / 2.0) * angle.cos(),
                            (TRIM_SIZE_2 - extension / 2.0) * angle.sin(),
                            $z_coord,
                            0.0,
                            angle,
                        );
                    }
                }
            };
        }
        if x == 0 || map[(y, x - 1)].height == TileHeight::default() {
            x_axis_trim!(x > 0, x - 1, xf - 0.5 + TRIM_SIZE_2);
        }
        if x == map.cols() - 1 || map[(y, x + 1)].height == TileHeight::default() {
            x_axis_trim!(x < map.cols() - 1, x + 1, xf + 0.5 - TRIM_SIZE_2);
        }
        if y == 0 || map[(y - 1, x)].height == TileHeight::default() {
            z_axis_trim!(yf - 0.5 + TRIM_SIZE_2);
        }
        if y == map.rows() - 1 || map[(y + 1, x)].height == TileHeight::default() {
            z_axis_trim!(yf + 0.5 - TRIM_SIZE_2);
        }

        const LOCKED_CONNECTION: Connection = Connection::Conditional(ConnectionCondition::Lock);
        if x > 0
            && tile.connections.west == LOCKED_CONNECTION
            && map[(y, x - 1)].connections.east == LOCKED_CONNECTION
        {
            let neighbor = &map[(y, x - 1)];
            let height = tile.height.center_height() as f32;
            let neighbor_height = neighbor.height.center_height() as f32;
            key_gates.push(key_gate(
                assets,
                match height.total_cmp(&neighbor_height) {
                    Ordering::Greater => Vec3::new(xf - 0.4375, height, yf),
                    Ordering::Less => Vec3::new(xf - 1.0 + 0.4375, neighbor_height, yf),
                    Ordering::Equal => Vec3::new(xf - 0.5, height, yf),
                },
                if height < neighbor_height {
                    -FRAC_PI_2
                } else {
                    FRAC_PI_2
                },
            ));
        }
        if y > 0
            && tile.connections.north == LOCKED_CONNECTION
            && map[(y - 1, x)].connections.south == LOCKED_CONNECTION
        {
            let neighbor = &map[(y - 1, x)];
            let height = tile.height.center_height() as f32;
            let neighbor_height = neighbor.height.center_height() as f32;
            key_gates.push(key_gate(
                assets,
                match height.total_cmp(&neighbor_height) {
                    Ordering::Greater => Vec3::new(xf, height, yf - 0.4375),
                    Ordering::Less => Vec3::new(xf, neighbor_height, yf - 1.0 + 0.4375),
                    Ordering::Equal => Vec3::new(xf, height, yf - 0.5),
                },
                if height < neighbor_height { PI } else { 0.0 },
            ));
        }
    }

    (
        MeshObject {
            mesh: Mesh3d(meshes.add(state.into_mesh())),
            material: MeshMaterial3d(atlas),
            transform: Transform::default(),
        },
        MapMeshMarker,
        Children::spawn((
            block_children,
            key_gates,
            Spawn((
                MeshObject {
                    mesh: Mesh3d(meshes.add({
                        let mut floor = State::new(map);
                        let x2 = map.cols() as f32 - 0.5;
                        let y2 = map.rows() as f32 - 0.5;
                        floor.positions.push([-0.5, 0.0, -0.5]);
                        floor.positions.push([x2, 0.0, -0.5]);
                        floor.positions.push([-0.5, 0.0, y2]);
                        floor.positions.push([x2, 0.0, y2]);
                        floor.push_quad_uv_indices(
                            (0.0, 0.0, map.cols() as f32, map.rows() as f32),
                            0,
                        );
                        floor.into_mesh()
                    })),
                    material: MeshMaterial3d(materials.add(StandardMaterial {
                        base_color_texture: Some(assets::floor(assets)),
                        perceptual_roughness: 1.0,
                        double_sided: true,
                        cull_mode: None,
                        alpha_mode: AlphaMode::Add,
                        ..Default::default()
                    })),
                    transform: Transform::default(),
                },
                NotShadowCaster,
                NotShadowReceiver,
            )),
        )),
    )
}

pub fn mesh_top_highlights(
    map: &Grid<TileData>,
    tile_range: TileRange,
    materials: &mut Assets<StandardMaterial>,
    meshes: &mut Assets<Mesh>,
) -> impl Bundle {
    let mut state = State::new(map);

    for y in tile_range.start.y..=tile_range.end.y {
        let y = y as usize;
        for x in tile_range.start.x..=tile_range.end.x {
            let x = x as usize;
            let tile = &map[(y, x)];
            internal_mesh_top(&mut state, x, y, tile, 0.01);
        }
    }

    (
        Mesh3d(meshes.add(state.into_mesh())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Srgba::rgba_u8(0x54, 0xAF, 0xE7, 0x80).into(),
            perceptual_roughness: 1.0,
            double_sided: true,
            cull_mode: None,
            alpha_mode: AlphaMode::Add,
            ..Default::default()
        })),
        NotShadowCaster,
        NotShadowReceiver,
    )
}

struct State<'a> {
    map: &'a Grid<TileData>,
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl<'a> State<'a> {
    fn new(map: &'a Grid<TileData>) -> Self {
        Self {
            map,
            positions: vec![],
            uvs: vec![],
            indices: vec![],
        }
    }

    fn push_quad_uv_indices(&mut self, (u1, v1, u2, v2): (f32, f32, f32, f32), index_start: u32) {
        self.uvs.push([u1, v1]);
        self.uvs.push([u2, v1]);
        self.uvs.push([u1, v2]);
        self.uvs.push([u2, v2]);
        self.push_quad_indices(index_start);
    }

    fn push_quad_indices(&mut self, index_start: u32) {
        self.indices
            .extend([index_start, index_start + 3, index_start + 1]);
        self.indices
            .extend([index_start, index_start + 2, index_start + 3]);
    }

    fn push_flipped_quad_indices(&mut self, index_start: u32) {
        self.indices
            .extend([index_start, index_start + 1, index_start + 3]);
        self.indices
            .extend([index_start, index_start + 3, index_start + 2]);
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_indices(Indices::U32(self.indices));
        mesh.compute_normals();
        mesh
    }
}

fn internal_mesh_top(state: &mut State, x: usize, y: usize, tile: &TileData, y_offset: f32) {
    let xf = x as f32;
    let yf = y as f32;
    let uv = tile.material.to_uv_coords();
    match tile.height {
        TileHeight::Flat { height, .. } => {
            let height32 = height as f32 + y_offset;
            let index_start = state.positions.len() as u32;
            state.positions.push([xf - 0.5, height32, yf - 0.5]);
            state.positions.push([xf + 0.5, height32, yf - 0.5]);
            state.positions.push([xf - 0.5, height32, yf + 0.5]);
            state.positions.push([xf + 0.5, height32, yf + 0.5]);
            state.push_quad_uv_indices(uv, index_start);
        }
        TileHeight::Ramp { height, .. } => {
            let index_start = state.positions.len() as u32;
            let dir_v = height.dir == TileRampDirection::Vertical;
            let pos = height.pos as f32 + y_offset;
            let neg = height.neg as f32 + y_offset;

            state.positions.push([xf - 0.5, neg, yf - 0.5]);
            state
                .positions
                .push([xf + 0.5, if dir_v { neg } else { pos }, yf - 0.5]);
            state
                .positions
                .push([xf - 0.5, if dir_v { pos } else { neg }, yf + 0.5]);
            state.positions.push([xf + 0.5, pos, yf + 0.5]);
            state.push_quad_uv_indices(uv, index_start);
        }
    }
}

fn mesh_wall(
    state: &mut State,
    x: usize,
    y: usize,
    tile: &TileData,
    direction: Direction,
) -> Option<()> {
    let mut index_start = state.positions.len() as u32;
    let xf = x as f32;
    let yf = y as f32;

    let materials = match direction {
        Direction::West => &tile.wall_material.west,
        Direction::East => &tile.wall_material.east,
        Direction::North => &tile.wall_material.north,
        Direction::South => &tile.wall_material.south,
    };

    let min_height = tile.height.min_height() as f32;

    if let TileHeight::Ramp { height, .. } = tile.height {
        let (u1, _, u2, v2) = materials.first()?.to_uv_coords();
        let max_height = height.pos.max(height.neg) as f32;
        let pos_is_max = max_height == height.pos as f32;
        let y_offset = if pos_is_max { 0.5 } else { -0.5 };
        let high_v = v2 + (min_height - max_height) * MpsMaterial::V_INCREMENT;
        match direction {
            Direction::West => {
                state.positions.push([xf - 0.5, min_height, yf - 0.5]);
                state.positions.push([xf - 0.5, min_height, yf + 0.5]);
                state.positions.push([xf - 0.5, max_height, yf + y_offset]);
                state.uvs.push([u1, v2]);
                state.uvs.push([u2, v2]);
                state.uvs.push([if pos_is_max { u2 } else { u1 }, high_v]);
                state
                    .indices
                    .extend([index_start, index_start + 1, index_start + 2]);
            }
            Direction::East => {
                state.positions.push([xf + 0.5, min_height, yf - 0.5]);
                state.positions.push([xf + 0.5, min_height, yf + 0.5]);
                state.positions.push([xf + 0.5, max_height, yf + y_offset]);
                state.uvs.push([u2, v2]);
                state.uvs.push([u1, v2]);
                state.uvs.push([if pos_is_max { u1 } else { u2 }, high_v]);
                state
                    .indices
                    .extend([index_start, index_start + 2, index_start + 1]);
            }
            Direction::North => {
                state.positions.push([xf - 0.5, min_height, yf - 0.5]);
                state.positions.push([xf + 0.5, min_height, yf - 0.5]);
                state.positions.push([xf + y_offset, max_height, yf - 0.5]);
                state.uvs.push([u2, v2]);
                state.uvs.push([u1, v2]);
                state.uvs.push([if pos_is_max { u1 } else { u2 }, high_v]);
                state
                    .indices
                    .extend([index_start, index_start + 2, index_start + 1]);
            }
            Direction::South => {
                state.positions.push([xf - 0.5, min_height, yf + 0.5]);
                state.positions.push([xf + 0.5, min_height, yf + 0.5]);
                state.positions.push([xf + y_offset, max_height, yf + 0.5]);
                state.uvs.push([u1, v2]);
                state.uvs.push([u2, v2]);
                state.uvs.push([if pos_is_max { u2 } else { u1 }, high_v]);
                state
                    .indices
                    .extend([index_start, index_start + 1, index_start + 2]);
            }
        }
        index_start += 3;
    }

    let segments = min_height.ceil() as usize;
    let mut last_segment = min_height % 1.0;
    if last_segment == 0.0 {
        last_segment = 1.0;
    }

    for seg in (0..segments).rev() {
        let seg_f = seg as f32;
        let (u1, v1, u2, mut v2) = materials
            .get(segments - 1 - seg)
            .or_else(|| materials.last())?
            .to_uv_coords();
        if seg == segments - 1 {
            v2 = v2 - MpsMaterial::V_INCREMENT + last_segment * MpsMaterial::V_INCREMENT;
        }
        let seg_height = if seg == segments - 1 {
            last_segment
        } else {
            1.0
        };

        match direction {
            Direction::West => {
                state
                    .positions
                    .push([xf - 0.5, seg_f + seg_height, yf - 0.5]);
                state.positions.push([xf - 0.5, seg_f, yf - 0.5]);
                state
                    .positions
                    .push([xf - 0.5, seg_f + seg_height, yf + 0.5]);
                state.positions.push([xf - 0.5, seg_f, yf + 0.5]);
                state.uvs.push([u1, v1]);
                state.uvs.push([u1, v2]);
                state.uvs.push([u2, v1]);
                state.uvs.push([u2, v2]);
                state.push_flipped_quad_indices(index_start);
                if x != 0 && state.map[(y, x - 1)].height.min_height() as f32 >= seg_f {
                    break;
                }
            }
            Direction::East => {
                state
                    .positions
                    .push([xf + 0.5, seg_f + seg_height, yf - 0.5]);
                state.positions.push([xf + 0.5, seg_f, yf - 0.5]);
                state
                    .positions
                    .push([xf + 0.5, seg_f + seg_height, yf + 0.5]);
                state.positions.push([xf + 0.5, seg_f, yf + 0.5]);
                state.uvs.push([u2, v1]);
                state.uvs.push([u2, v2]);
                state.uvs.push([u1, v1]);
                state.uvs.push([u1, v2]);
                state.push_quad_indices(index_start);
                if x != state.map.cols() - 1
                    && state.map[(y, x + 1)].height.min_height() as f32 >= seg_f
                {
                    break;
                }
            }
            Direction::North => {
                state.positions.push([xf - 0.5, seg_f, yf - 0.5]);
                state.positions.push([xf + 0.5, seg_f, yf - 0.5]);
                state
                    .positions
                    .push([xf - 0.5, seg_f + seg_height, yf - 0.5]);
                state
                    .positions
                    .push([xf + 0.5, seg_f + seg_height, yf - 0.5]);
                state.uvs.push([u2, v2]);
                state.uvs.push([u1, v2]);
                state.uvs.push([u2, v1]);
                state.uvs.push([u1, v1]);
                state.push_quad_indices(index_start);
                if y != 0 && state.map[(y - 1, x)].height.min_height() as f32 >= seg_f {
                    break;
                }
            }
            Direction::South => {
                state.positions.push([xf - 0.5, seg_f, yf + 0.5]);
                state.positions.push([xf + 0.5, seg_f, yf + 0.5]);
                state
                    .positions
                    .push([xf - 0.5, seg_f + seg_height, yf + 0.5]);
                state
                    .positions
                    .push([xf + 0.5, seg_f + seg_height, yf + 0.5]);
                state.uvs.push([u1, v2]);
                state.uvs.push([u2, v2]);
                state.uvs.push([u1, v1]);
                state.uvs.push([u2, v1]);
                state.push_flipped_quad_indices(index_start);
                if y != state.map.rows() - 1
                    && state.map[(y + 1, x)].height.min_height() as f32 >= seg_f
                {
                    break;
                }
            }
        }
        index_start += 4;
    }

    Some(())
}
