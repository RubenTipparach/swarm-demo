//! Everything that lives on the GPU: the swarm, and the sparks that come off
//! things when they die.
//!
//! One storage buffer of motes, made once from a seed, advanced by a compute
//! pass before the cameras draw and bound as the instance buffer of one draw.
//! Nothing about a mote ever comes back to the CPU. That is the whole point of
//! the design: a million of anything cannot be entities, and the ECS holds the
//! things there are dozens of.
//!
//! Sparks are one more buffer on the same pattern, and it has TWO writers,
//! which is the only interesting thing about it. The app writes sparks for
//! what it can see happening (a hull cell chewed away, a gun going off, a ship
//! coming apart); the swarm shader writes them for what only it can see (a
//! mote it just killed). So the ring is cut in half: the lower half is the
//! CPU's, written with `write_buffer` at a cursor this module keeps, and the
//! upper half is the GPU's, claimed with one atomic per burst. Two regions,
//! no contention, and neither writer can tread on the other's half.

use bevy::{
    camera::visibility::NoFrustumCulling,
    core_pipeline::core_3d::Transparent3d,
    ecs::{
        query::QueryItem,
        system::{lifetimeless::*, SystemParamItem},
    },
    mesh::{MeshVertexBufferLayoutRef, VertexBufferLayout},
    pbr::{
        MeshPipeline, MeshPipelineKey, RenderMeshInstances, SetMeshBindGroup,
        SetMeshViewBindGroup, SetMeshViewBindingArrayBindGroup,
    },
    prelude::*,
    render::{
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        mesh::{allocator::MeshAllocator, RenderMesh, RenderMeshBufferInfo},
        render_asset::RenderAssets,
        render_graph::{self, RenderGraph, RenderLabel},
        render_phase::{
            AddRenderCommand, DrawFunctions, PhaseItem, PhaseItemExtraIndex, RenderCommand,
            RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewSortedRenderPhases,
        },
        render_resource::{
            binding_types::{sampler, storage_buffer, storage_buffer_sized, texture_2d, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
        sync_world::MainEntity,
        texture::GpuImage,
        view::ExtractedView,
        Render, RenderApp, RenderStartup, RenderSystems,
    },
    shader::PipelineCacheError,
};
use bytemuck::{Pod, Zeroable};
use std::borrow::Cow;
use swarm_core::{fx::Spark, rng::Rng};

const SWARM_SHADER: &str = "shaders/swarm.wgsl";
const PARTICLE_SHADER: &str = "shaders/particles.wgsl";
const MOTE_SHADER: &str = "shaders/mote.wgsl";
const SPARK_SHADER: &str = "shaders/spark.wgsl";
const WORKGROUP: u32 = 256;

/// How many capsules the swarm may be told about in one tick. A beam and a
/// blast are the same shape, so this is every shot in the air at once.
pub const MAX_SHOTS: usize = 32;
/// The app's half of the spark ring.
pub const CPU_SPARKS: u32 = 24_576;
/// The swarm's half.
pub const GPU_SPARKS: u32 = 40_960;
pub const SPARKS: u32 = CPU_SPARKS + GPU_SPARKS;

/// The chitin every mote wears and the ember every spark burns with, loaded by
/// the main world and bound by the render world once their pixels are there.
#[derive(Resource, Clone, ExtractResource)]
pub struct FxTextures {
    pub chitin: Handle<Image>,
    pub ember: Handle<Image>,
}

/// What the swarm is told about the world, once a frame.
#[derive(Resource, Clone, ExtractResource)]
pub struct SwarmConfig {
    pub count: u32,
    pub neighbours: u32,
    pub seed: u64,
    /// The hull the swarm wants, as a sphere for now.
    pub hull_centre: Vec3,
    pub hull_radius: f32,
    /// Where every LIVE mothership is (xyz) and how big it is (w). Compacted
    /// by the app each frame, so a hive that dies shortens the list and the
    /// motes that flew from it re-home to whatever is left.
    pub hives: Vec<Vec4>,
    /// The asteroid field, as spheres: centre xyz, radius w. Written once at
    /// startup, because a rock does not move.
    pub rocks: Vec<Vec4>,
    /// How long every mote waits inside its carrier before the first one
    /// comes out. The carriers arrive, they sit there, and THEN the swarm
    /// starts: an opening beat the player can read before anything is
    /// happening to them.
    pub launch_delay: f32,
    pub paused: bool,
    /// One tick a frame rather than the wall clock, so a headless render is a
    /// function of its frame count. See `--fixed-dt`.
    pub fixed_dt: bool,
}

impl Default for SwarmConfig {
    fn default() -> Self {
        SwarmConfig {
            count: 100_000,
            neighbours: 8,
            seed: 1,
            hull_centre: Vec3::ZERO,
            hull_radius: 3.5,
            hives: Vec::new(),
            rocks: Vec::new(),
            launch_delay: 10.0,
            paused: false,
            fixed_dt: false,
        }
    }
}

/// How many motherships the swarm may fly from at once.
pub const MAX_HIVES: usize = 16;

/// How many asteroids the swarm shader tests a mote against.
///
/// Every mote pays for every rock every tick, so this is a budget rather than
/// a limit on the field: thirty two is about a microsecond of the tick at a
/// million motes, and a field that needs more than thirty two rocks in one
/// place needs a grid rather than a longer list.
pub const MAX_ROCKS: usize = 32;

/// The capsules that kill this tick, rebuilt by the app every frame.
///
/// A beam is a capsule from muzzle to endpoint; a blast is a capsule of zero
/// length whose radius the app has already grown for this tick. One shape, so
/// the shader has one test and no branch on kind.
#[derive(Resource, Clone, Default, ExtractResource)]
pub struct Shots(pub Vec<Capsule>);

#[derive(Clone, Copy, Debug)]
pub struct Capsule {
    pub from: Vec3,
    pub to: Vec3,
    pub radius: f32,
}

/// Sparks the app wants in the air. Drained into the CPU half of the ring
/// every frame; anything past `CPU_SPARKS` in one frame is dropped rather than
/// wrapping over itself, because a burst that overwrote its own head would
/// come out as a few survivors in random places.
#[derive(Resource, Clone, Default, ExtractResource)]
pub struct SparkQueue(pub Vec<Spark>);

impl SparkQueue {
    pub fn push(&mut self, s: Spark) {
        if self.0.len() < CPU_SPARKS as usize {
            self.0.push(s);
        }
    }
    pub fn extend(&mut self, it: impl IntoIterator<Item = Spark>) {
        for s in it {
            self.push(s);
        }
    }
}

#[derive(Clone, Copy, Pod, Zeroable, ShaderType)]
#[repr(C)]
pub struct Mote {
    pub pos_scale: Vec4,
    pub vel_seed: Vec4,
    pub state: Vec4,
}

#[derive(Clone, Copy, Pod, Zeroable, ShaderType, Default)]
#[repr(C)]
pub struct GpuSpark {
    pub pos_life: Vec4,
    pub vel_size: Vec4,
    pub colour_seed: Vec4,
}

impl GpuSpark {
    fn of(s: &Spark) -> Self {
        GpuSpark {
            pos_life: Vec3::from(s.pos).extend(s.life),
            vel_size: Vec3::from(s.vel).extend(s.size),
            // The seed picks the ember tile. Hashed off the kind and the
            // colour rather than rolled, so a re-watch lights it the same.
            colour_seed: Vec3::from(s.colour).extend(spark_seed(s)),
        }
    }
}

fn spark_seed(s: &Spark) -> f32 {
    let bits = s.pos[0].to_bits() ^ s.pos[1].to_bits().rotate_left(11) ^ s.pos[2].to_bits().rotate_left(21) ^ s.kind.code();
    (swarm_core::rng::hash_cell(bits) >> 8) as f32 / 16_777_216.0
}

#[derive(Clone, Copy, Pod, Zeroable, ShaderType)]
#[repr(C)]
struct Params {
    dt: f32,
    time: f32,
    count: u32,
    nbrs: u32,
    hull: Vec4,
    tick: u32,
    shots: u32,
    spark_base: u32,
    spark_cap: u32,
    hives: u32,
    rocks: u32,
    pad1: u32,
    pad2: u32,
    shot: [Vec4; MAX_SHOTS * 2],
    hive: [Vec4; MAX_HIVES],
    rock: [Vec4; MAX_ROCKS],
}

/// Put this on an entity with a `Mesh3d` and that mesh is drawn once per mote.
#[derive(Component, Clone)]
pub struct MoteMesh;

/// And this draws it once per spark.
#[derive(Component, Clone)]
pub struct SparkMesh;

impl ExtractComponent for MoteMesh {
    type QueryData = &'static MoteMesh;
    type QueryFilter = ();
    type Out = Self;
    fn extract_component(_: QueryItem<'_, '_, Self::QueryData>) -> Option<Self> {
        Some(MoteMesh)
    }
}

impl ExtractComponent for SparkMesh {
    type QueryData = &'static SparkMesh;
    type QueryFilter = ();
    type Out = Self;
    fn extract_component(_: QueryItem<'_, '_, Self::QueryData>) -> Option<Self> {
        Some(SparkMesh)
    }
}

pub struct SwarmPlugin;

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct SwarmTickLabel;

impl Plugin for SwarmPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractResourcePlugin::<SwarmConfig>::default(),
            ExtractResourcePlugin::<FxTextures>::default(),
            ExtractResourcePlugin::<SwarmClock>::default(),
            ExtractResourcePlugin::<Shots>::default(),
            ExtractResourcePlugin::<SparkQueue>::default(),
            ExtractComponentPlugin::<MoteMesh>::default(),
            ExtractComponentPlugin::<SparkMesh>::default(),
        ))
        .init_resource::<SwarmClock>()
        .init_resource::<Shots>()
        .init_resource::<SparkQueue>()
        .add_systems(Update, advance_clock)
        // FIRST, not Last.
        //
        // Extraction runs after the main schedule has finished, so a queue
        // emptied in `Last` is emptied before the render world has seen it:
        // every spark the app asked for was counted, thrown away, and never
        // drawn. Cleared at the top of the next frame instead, which is the
        // only point where the render world is done with the last one.
        .add_systems(First, clear_spark_queue);

        let render_app = app.sub_app_mut(RenderApp);
        render_app
            .add_render_command::<Transparent3d, DrawMotes>()
            .add_render_command::<Transparent3d, DrawSparks>()
            .init_resource::<SpecializedMeshPipelines<MotePipeline>>()
            .init_resource::<SpecializedMeshPipelines<SparkPipeline>>()
            .add_systems(RenderStartup, (init_tick_pipeline, init_mote_pipeline, init_spark_pipeline))
            .add_systems(
                Render,
                (
                    prepare_swarm_buffers.in_set(RenderSystems::PrepareResources),
                    (prepare_tick_bind_group, prepare_fx_bind_group).in_set(RenderSystems::PrepareBindGroups),
                    (queue_motes, queue_sparks).in_set(RenderSystems::QueueMeshes),
                ),
            );

        let mut graph = render_app.world_mut().resource_mut::<RenderGraph>();
        graph.add_node(SwarmTickLabel, SwarmTickNode::default());
        graph.add_node_edge(SwarmTickLabel, bevy::render::graph::CameraDriverLabel);
    }
}

/// Seconds, from the main world, and a tick count the whole game shares.
#[derive(Resource, Clone, Default, ExtractResource)]
pub struct SwarmClock {
    pub dt: f32,
    pub time: f32,
    pub ticks: u64,
}

fn advance_clock(time: Res<Time>, cfg: Res<SwarmConfig>, mut clock: ResMut<SwarmClock>) {
    // Clamped so a slow frame does not fling the cloud apart.
    clock.dt = if cfg.paused {
        0.0
    } else if cfg.fixed_dt {
        1.0 / 60.0
    } else {
        time.delta_secs().min(1.0 / 20.0)
    };
    clock.time += clock.dt;
    if !cfg.paused {
        clock.ticks += 1;
    }
}

/// Empty the queue for the frame that is starting. See the note at the call.
fn clear_spark_queue(mut q: ResMut<SparkQueue>) {
    q.0.clear();
}

// ------------------------------------------------------------- buffers --

#[derive(Resource)]
pub struct SwarmBuffers {
    pub motes: Buffer,
    pub sparks: Buffer,
    pub counter: Buffer,
    pub count: u32,
    /// Where the app's next spark goes, in its own half of the ring.
    cpu_cursor: u32,
    params: UniformBuffer<Params>,
}

fn prepare_swarm_buffers(
    mut commands: Commands,
    existing: Option<ResMut<SwarmBuffers>>,
    cfg: Res<SwarmConfig>,
    clock: Res<SwarmClock>,
    shots: Res<Shots>,
    queue: Res<SparkQueue>,
    device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
) {
    let mut shot = [Vec4::ZERO; MAX_SHOTS * 2];
    let n = shots.0.len().min(MAX_SHOTS);
    for (i, c) in shots.0.iter().take(n).enumerate() {
        shot[i * 2] = c.from.extend(c.radius);
        shot[i * 2 + 1] = c.to.extend(0.0);
    }
    let mut hive = [Vec4::ZERO; MAX_HIVES];
    let hives = cfg.hives.len().min(MAX_HIVES);
    hive[..hives].copy_from_slice(&cfg.hives[..hives]);
    let mut rock = [Vec4::ZERO; MAX_ROCKS];
    let rocks = cfg.rocks.len().min(MAX_ROCKS);
    rock[..rocks].copy_from_slice(&cfg.rocks[..rocks]);
    let params = Params {
        dt: clock.dt,
        time: clock.time,
        count: cfg.count,
        nbrs: cfg.neighbours,
        hull: cfg.hull_centre.extend(cfg.hull_radius),
        tick: clock.ticks as u32,
        shots: n as u32,
        spark_base: CPU_SPARKS,
        spark_cap: GPU_SPARKS,
        hives: hives as u32,
        rocks: rocks as u32,
        pad1: 0,
        pad2: 0,
        shot,
        hive,
        rock,
    };

    let write_sparks = |buffer: &Buffer, cursor: &mut u32| {
        if queue.0.is_empty() {
            return;
        }
        let gpu: Vec<GpuSpark> = queue.0.iter().map(GpuSpark::of).collect();
        let stride = size_of::<GpuSpark>() as u64;
        // The ring may wrap, and a wrap is two writes rather than one that
        // runs off the end of the region and into the swarm's half.
        let head = (CPU_SPARKS - *cursor).min(gpu.len() as u32) as usize;
        render_queue.write_buffer(buffer, *cursor as u64 * stride, bytemuck::cast_slice(&gpu[..head]));
        if head < gpu.len() {
            render_queue.write_buffer(buffer, 0, bytemuck::cast_slice(&gpu[head..]));
        }
        *cursor = (*cursor + gpu.len() as u32) % CPU_SPARKS;
    };

    if let Some(mut b) = existing {
        if b.count == cfg.count {
            b.params.set(params);
            b.params.write_buffer(&device, &render_queue);
            let mut cursor = b.cpu_cursor;
            write_sparks(&b.sparks.clone(), &mut cursor);
            b.cpu_cursor = cursor;
            return;
        }
    }

    // Nothing to fly from yet: the app has not seated its carriers. Wait,
    // rather than build a swarm at the origin that would then have to be
    // thrown away.
    if cfg.hives.is_empty() {
        return;
    }
    // Seeded, so the same swarm starts the same way on every run.
    //
    // Every mote starts DEAD, with a staggered countdown: they are all inside
    // their carriers at t=0 and stream out over the first few seconds. The
    // alternative is a cloud that exists on the first frame, which is the
    // thing having carriers is meant to replace.
    let mut rng = Rng::new(cfg.seed);
    let per = (cfg.count as usize).div_ceil(hives.max(1));
    let motes: Vec<Mote> = (0..cfg.count)
        .map(|i| {
            let scale = rng.range(0.7, 1.3);
            let hive = (i as usize / per.max(1)).min(hives.saturating_sub(1)) as f32;
            Mote {
                pos_scale: Vec3::ZERO.extend(0.0),
                vel_seed: Vec3::ZERO.extend((i as f32 + 0.5) / cfg.count as f32),
                // The delay first, then spread over about eight seconds, and
                // never exactly nought, which the shader reads as "already
                // out".
                state: Vec4::new(cfg.launch_delay + 0.02 + rng.range(0.0, 8.0), scale, hive, 0.0),
            }
        })
        .collect();
    let motes = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("swarm motes"),
        contents: bytemuck::cast_slice(&motes),
        usage: BufferUsages::STORAGE | BufferUsages::VERTEX | BufferUsages::COPY_DST,
    });
    let sparks = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("sparks"),
        contents: bytemuck::cast_slice(&vec![GpuSpark::default(); SPARKS as usize]),
        usage: BufferUsages::STORAGE | BufferUsages::VERTEX | BufferUsages::COPY_DST,
    });
    let counter = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("spark counter"),
        contents: bytemuck::cast_slice(&[0u32; 4]),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
    });
    let mut params_buf = UniformBuffer::from(params);
    params_buf.write_buffer(&device, &render_queue);
    let mut cursor = 0;
    write_sparks(&sparks, &mut cursor);
    commands.insert_resource(SwarmBuffers { motes, sparks, counter, count: cfg.count, cpu_cursor: cursor, params: params_buf });
}

// ------------------------------------------------------------- compute --

#[derive(Resource)]
struct TickPipeline {
    swarm_layout: BindGroupLayoutDescriptor,
    particle_layout: BindGroupLayoutDescriptor,
    swarm: CachedComputePipelineId,
    particles: CachedComputePipelineId,
}

fn init_tick_pipeline(mut commands: Commands, assets: Res<AssetServer>, cache: Res<PipelineCache>) {
    let swarm_layout = BindGroupLayoutDescriptor::new(
        "swarm tick",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                storage_buffer::<Vec<Mote>>(false),
                uniform_buffer::<Params>(false),
                storage_buffer::<Vec<GpuSpark>>(false),
                storage_buffer_sized(false, std::num::NonZeroU64::new(16)),
            ),
        ),
    );
    let particle_layout = BindGroupLayoutDescriptor::new(
        "particle tick",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (storage_buffer::<Vec<GpuSpark>>(false), uniform_buffer::<Params>(false)),
        ),
    );
    let swarm = cache.queue_compute_pipeline(ComputePipelineDescriptor {
        layout: vec![swarm_layout.clone()],
        shader: assets.load(SWARM_SHADER),
        entry_point: Some(Cow::from("tick")),
        ..default()
    });
    let particles = cache.queue_compute_pipeline(ComputePipelineDescriptor {
        layout: vec![particle_layout.clone()],
        shader: assets.load(PARTICLE_SHADER),
        entry_point: Some(Cow::from("tick")),
        ..default()
    });
    commands.insert_resource(TickPipeline { swarm_layout, particle_layout, swarm, particles });
}

#[derive(Resource)]
struct TickBindGroups {
    swarm: BindGroup,
    particles: BindGroup,
}

fn prepare_tick_bind_group(
    mut commands: Commands,
    pipeline: Res<TickPipeline>,
    buffers: Option<Res<SwarmBuffers>>,
    device: Res<RenderDevice>,
    cache: Res<PipelineCache>,
) {
    let Some(b) = buffers else { return };
    let swarm = device.create_bind_group(
        None,
        &cache.get_bind_group_layout(&pipeline.swarm_layout),
        &BindGroupEntries::sequential((
            b.motes.as_entire_binding(),
            &b.params,
            b.sparks.as_entire_binding(),
            b.counter.as_entire_binding(),
        )),
    );
    let particles = device.create_bind_group(
        None,
        &cache.get_bind_group_layout(&pipeline.particle_layout),
        &BindGroupEntries::sequential((b.sparks.as_entire_binding(), &b.params)),
    );
    commands.insert_resource(TickBindGroups { swarm, particles });
}

#[derive(Default)]
struct SwarmTickNode {
    ready: bool,
}

impl render_graph::Node for SwarmTickNode {
    fn update(&mut self, world: &mut World) {
        if self.ready {
            return;
        }
        let pipeline = world.resource::<TickPipeline>();
        let cache = world.resource::<PipelineCache>();
        let mut ok = 0;
        for (id, name) in [(pipeline.swarm, SWARM_SHADER), (pipeline.particles, PARTICLE_SHADER)] {
            match cache.get_compute_pipeline_state(id) {
                CachedPipelineState::Ok(_) => ok += 1,
                CachedPipelineState::Err(PipelineCacheError::ShaderNotLoaded(_)) => {}
                CachedPipelineState::Err(err) => panic!("compiling {name}:\n{err}"),
                _ => {}
            }
        }
        self.ready = ok == 2;
    }

    fn run(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        ctx: &mut RenderContext,
        world: &World,
    ) -> Result<(), render_graph::NodeRunError> {
        if !self.ready {
            return Ok(());
        }
        let (Some(bgs), Some(buffers)) = (world.get_resource::<TickBindGroups>(), world.get_resource::<SwarmBuffers>()) else {
            return Ok(());
        };
        let cache = world.resource::<PipelineCache>();
        let pipeline = world.resource::<TickPipeline>();
        let (Some(swarm), Some(particles)) =
            (cache.get_compute_pipeline(pipeline.swarm), cache.get_compute_pipeline(pipeline.particles))
        else {
            return Ok(());
        };
        let mut pass = ctx.command_encoder().begin_compute_pass(&ComputePassDescriptor { label: Some("swarm tick"), ..default() });
        // The swarm first, so a mote killed this tick has its sparks in the
        // buffer before the pass that ages them runs: a spark that missed its
        // own first integration would appear one frame late, at the muzzle
        // rather than where it was thrown.
        pass.set_pipeline(swarm);
        pass.set_bind_group(0, &bgs.swarm, &[]);
        pass.dispatch_workgroups(buffers.count.div_ceil(WORKGROUP), 1, 1);
        pass.set_pipeline(particles);
        pass.set_bind_group(0, &bgs.particles, &[]);
        pass.dispatch_workgroups(SPARKS.div_ceil(WORKGROUP), 1, 1);
        Ok(())
    }
}

// -------------------------------------------------------------- textures --

/// One bind group for both instanced draws: the chitin a mote wears at 0 and
/// 1, the ember a spark burns with at 2 and 3. One layout rather than two,
/// because two pipelines wanting one texture each is still one place where a
/// texture is bound.
#[derive(Resource)]
struct FxBindGroup(BindGroup);

fn fx_layout() -> BindGroupLayoutDescriptor {
    BindGroupLayoutDescriptor::new(
        "fx textures",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
            ),
        ),
    )
}

fn prepare_fx_bind_group(
    mut commands: Commands,
    tex: Option<Res<FxTextures>>,
    images: Res<RenderAssets<GpuImage>>,
    device: Res<RenderDevice>,
    cache: Res<PipelineCache>,
    have: Option<Res<FxBindGroup>>,
) {
    if have.is_some() {
        return;
    }
    let Some(tex) = tex else { return };
    let (Some(chitin), Some(ember)) = (images.get(&tex.chitin), images.get(&tex.ember)) else {
        return;
    };
    let bg = device.create_bind_group(
        Some("fx textures"),
        &cache.get_bind_group_layout(&fx_layout()),
        &BindGroupEntries::sequential((&chitin.texture_view, &chitin.sampler, &ember.texture_view, &ember.sampler)),
    );
    commands.insert_resource(FxBindGroup(bg));
}

struct SetFxBindGroup<const I: usize>;

impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetFxBindGroup<I> {
    type Param = Option<SRes<FxBindGroup>>;
    type ViewQuery = ();
    type ItemQuery = ();

    #[inline]
    fn render<'w>(
        _item: &P,
        _view: (),
        _entity: Option<()>,
        fx: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(fx) = fx else { return RenderCommandResult::Skip };
        pass.set_bind_group(I, &fx.into_inner().0, &[]);
        RenderCommandResult::Success
    }
}

// ------------------------------------------------------------------ draw --

/// The two instanced draws are the same shape: a mesh, an instance buffer that
/// something else owns, and a count. This is what they share.
macro_rules! instanced_pipeline {
    ($pipeline:ident, $shader:expr, $stride:ty, $extra_attr:expr, $depth:expr, $additive:expr) => {
        #[derive(Resource)]
        struct $pipeline {
            shader: Handle<Shader>,
            mesh_pipeline: MeshPipeline,
            fx_layout: BindGroupLayoutDescriptor,
        }

        impl SpecializedMeshPipeline for $pipeline {
            type Key = MeshPipelineKey;

            fn specialize(
                &self,
                key: Self::Key,
                layout: &MeshVertexBufferLayoutRef,
            ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
                let mut d = self.mesh_pipeline.specialize(key, layout)?;
                d.vertex.shader = self.shader.clone();
                // Locations 8 and up: clear of the mesh's own, which run to 5
                // (colour) and include the tangents at 4. They were at 3 and 4
                // once, which is fine for a cube and collides the moment the
                // mesh carries any.
                let mut attributes = vec![
                    VertexAttribute { format: VertexFormat::Float32x4, offset: 0, shader_location: 8 },
                    VertexAttribute { format: VertexFormat::Float32x4, offset: 16, shader_location: 9 },
                ];
                attributes.extend($extra_attr);
                d.vertex.buffers.push(VertexBufferLayout {
                    array_stride: size_of::<$stride>() as u64,
                    step_mode: VertexStepMode::Instance,
                    attributes,
                });
                d.fragment.as_mut().unwrap().shader = self.shader.clone();
                if let Some(ds) = d.depth_stencil.as_mut() {
                    ds.depth_write_enabled = $depth;
                }
                if $additive {
                    // Light ADDS. The sorted phase hands out alpha blending by
                    // default, which is what you want for glass and is exactly
                    // wrong for a spark: an alpha blended spark DARKENS what is
                    // behind it wherever its own colour is dimmer, so a burst
                    // over a bright hull came out as a swarm of grey specks.
                    if let Some(Some(target)) = d.fragment.as_mut().and_then(|f| f.targets.first_mut()) {
                        target.blend = Some(BlendState {
                            color: BlendComponent {
                                src_factor: BlendFactor::One,
                                dst_factor: BlendFactor::One,
                                operation: BlendOperation::Add,
                            },
                            alpha: BlendComponent {
                                src_factor: BlendFactor::Zero,
                                dst_factor: BlendFactor::One,
                                operation: BlendOperation::Add,
                            },
                        });
                    }
                    // And a billboard has no back: which way its winding came
                    // out depends on where the eye is, so culling it would
                    // make half the burst vanish as the camera went round.
                    d.primitive.cull_mode = None;
                }
                d.layout.push(self.fx_layout.clone());
                Ok(d)
            }
        }
    };
}

// A mote is an opaque body drawn through the sorted phase: it must write depth
// or the near ones do not cover the far ones.
instanced_pipeline!(MotePipeline, MOTE_SHADER, Mote, Vec::<VertexAttribute>::new(), true, false);
// A spark is light. It must NOT write depth, or every spark in a burst
// occludes the ones behind it and the burst comes out as a shell.
instanced_pipeline!(
    SparkPipeline,
    SPARK_SHADER,
    GpuSpark,
    vec![VertexAttribute { format: VertexFormat::Float32x4, offset: 32, shader_location: 10 }],
    false,
    true
);

fn init_mote_pipeline(mut commands: Commands, assets: Res<AssetServer>, mesh_pipeline: Res<MeshPipeline>) {
    commands.insert_resource(MotePipeline {
        shader: assets.load(MOTE_SHADER),
        mesh_pipeline: mesh_pipeline.clone(),
        fx_layout: fx_layout(),
    });
}

fn init_spark_pipeline(mut commands: Commands, assets: Res<AssetServer>, mesh_pipeline: Res<MeshPipeline>) {
    commands.insert_resource(SparkPipeline {
        shader: assets.load(SPARK_SHADER),
        mesh_pipeline: mesh_pipeline.clone(),
        fx_layout: fx_layout(),
    });
}

/// Queue one instanced draw. Both of them are the same twenty lines with a
/// different pipeline and a different marker, so they are these twenty.
#[allow(clippy::too_many_arguments)]
fn queue_instanced<P: SpecializedMeshPipeline<Key = MeshPipelineKey> + Resource, M: Component>(
    name: &str,
    pipeline: &P,
    pipelines: &mut SpecializedMeshPipelines<P>,
    cache: &PipelineCache,
    meshes: &RenderAssets<RenderMesh>,
    instances: &RenderMeshInstances,
    query: &Query<(Entity, &MainEntity), With<M>>,
    phases: &mut ViewSortedRenderPhases<Transparent3d>,
    views: &Query<(&ExtractedView, &Msaa)>,
    draw: bevy::render::render_phase::DrawFunctionId,
) {
    for (view, msaa) in views {
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else { continue };
        let view_key = MeshPipelineKey::from_msaa_samples(msaa.samples()) | MeshPipelineKey::from_hdr(view.hdr);
        let rangefinder = view.rangefinder3d();
        for (entity, main_entity) in query {
            let Some(inst) = instances.render_mesh_queue_data(*main_entity) else { continue };
            let Some(mesh) = meshes.get(inst.mesh_asset_id) else { continue };
            let key = view_key | MeshPipelineKey::from_primitive_topology(mesh.primitive_topology());
            let pipeline_id = match pipelines.specialize(cache, pipeline, key, &mesh.layout) {
                Ok(p) => p,
                Err(e) => {
                    error!("{name} pipeline: {e}");
                    continue;
                }
            };
            phase.add(Transparent3d {
                entity: (entity, *main_entity),
                pipeline: pipeline_id,
                draw_function: draw,
                distance: rangefinder.distance(&inst.center),
                batch_range: 0..1,
                extra_index: PhaseItemExtraIndex::None,
                indexed: true,
            });
        }
    }
}

fn queue_motes(
    draw_functions: Res<DrawFunctions<Transparent3d>>,
    pipeline: Res<MotePipeline>,
    mut pipelines: ResMut<SpecializedMeshPipelines<MotePipeline>>,
    cache: Res<PipelineCache>,
    meshes: Res<RenderAssets<RenderMesh>>,
    instances: Res<RenderMeshInstances>,
    query: Query<(Entity, &MainEntity), With<MoteMesh>>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    views: Query<(&ExtractedView, &Msaa)>,
) {
    let draw = draw_functions.read().id::<DrawMotes>();
    queue_instanced(
        "mote", &*pipeline, &mut pipelines, &cache, &meshes, &instances, &query, &mut phases, &views, draw,
    );
}

fn queue_sparks(
    draw_functions: Res<DrawFunctions<Transparent3d>>,
    pipeline: Res<SparkPipeline>,
    mut pipelines: ResMut<SpecializedMeshPipelines<SparkPipeline>>,
    cache: Res<PipelineCache>,
    meshes: Res<RenderAssets<RenderMesh>>,
    instances: Res<RenderMeshInstances>,
    query: Query<(Entity, &MainEntity), With<SparkMesh>>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    views: Query<(&ExtractedView, &Msaa)>,
) {
    let draw = draw_functions.read().id::<DrawSparks>();
    queue_instanced(
        "spark", &*pipeline, &mut pipelines, &cache, &meshes, &instances, &query, &mut phases, &views, draw,
    );
}

type DrawMotes = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetMeshBindGroup<2>,
    SetFxBindGroup<3>,
    DrawInstanced<0>,
);

type DrawSparks = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetMeshBindGroup<2>,
    SetFxBindGroup<3>,
    DrawInstanced<1>,
);

/// `WHICH` is 0 for the motes and 1 for the sparks: the same command with a
/// different buffer, rather than two copies of the same twenty lines.
struct DrawInstanced<const WHICH: usize>;

impl<P: PhaseItem, const WHICH: usize> RenderCommand<P> for DrawInstanced<WHICH> {
    type Param = (
        SRes<RenderAssets<RenderMesh>>,
        SRes<RenderMeshInstances>,
        SRes<MeshAllocator>,
        Option<SRes<SwarmBuffers>>,
    );
    type ViewQuery = ();
    type ItemQuery = ();

    #[inline]
    fn render<'w>(
        item: &P,
        _view: (),
        _item: Option<()>,
        (meshes, instances, allocator, buffers): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let allocator = allocator.into_inner();
        let Some(buffers) = buffers else { return RenderCommandResult::Skip };
        let buffers = buffers.into_inner();
        let (buffer, count) = if WHICH == 0 { (&buffers.motes, buffers.count) } else { (&buffers.sparks, SPARKS) };
        let Some(inst) = instances.render_mesh_queue_data(item.main_entity()) else { return RenderCommandResult::Skip };
        let Some(gpu_mesh) = meshes.into_inner().get(inst.mesh_asset_id) else { return RenderCommandResult::Skip };
        let Some(vslice) = allocator.mesh_vertex_slice(&inst.mesh_asset_id) else { return RenderCommandResult::Skip };

        pass.set_vertex_buffer(0, vslice.buffer.slice(..));
        pass.set_vertex_buffer(1, buffer.slice(..));
        match &gpu_mesh.buffer_info {
            RenderMeshBufferInfo::Indexed { index_format, count: icount } => {
                let Some(islice) = allocator.mesh_index_slice(&inst.mesh_asset_id) else { return RenderCommandResult::Skip };
                pass.set_index_buffer(islice.buffer.slice(..), *index_format);
                pass.draw_indexed(islice.range.start..(islice.range.start + icount), vslice.range.start as i32, 0..count);
            }
            RenderMeshBufferInfo::NonIndexed => {
                pass.draw(vslice.range, 0..count);
            }
        }
        RenderCommandResult::Success
    }
}

/// Spawn the entity an instanced draw hangs off: at the origin, never culled,
/// because the instances are wherever the buffer says and the buffer is not a
/// transform the culler can see.
pub fn spawn_mote_mesh(commands: &mut Commands, mesh: Handle<Mesh>) {
    commands.spawn((Mesh3d(mesh), Transform::IDENTITY, MoteMesh, NoFrustumCulling));
}

pub fn spawn_spark_mesh(commands: &mut Commands, mesh: Handle<Mesh>) {
    commands.spawn((Mesh3d(mesh), Transform::IDENTITY, SparkMesh, NoFrustumCulling));
}
