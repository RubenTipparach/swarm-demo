//! The swarm on the GPU: one storage buffer of motes, a compute pass that
//! ticks it, and one instanced draw that reads the same buffer as vertex data.
//!
//! Nothing about a mote ever comes back to the CPU. The buffer is made once
//! in the render world from a seed, the compute node advances it every frame
//! before the cameras draw, and the draw command binds it as the instance
//! buffer. That is the whole point of the design: a million of anything cannot
//! be entities, and the ECS holds the things there are dozens of.

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
            binding_types::{storage_buffer, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
        sync_world::MainEntity,
        view::ExtractedView,
        Render, RenderApp, RenderStartup, RenderSystems,
    },
    shader::PipelineCacheError,
};
use bytemuck::{Pod, Zeroable};
use std::borrow::Cow;
use swarm_core::rng::Rng;

const TICK_SHADER: &str = "shaders/swarm.wgsl";
const MOTE_SHADER: &str = "shaders/mote.wgsl";
const WORKGROUP: u32 = 256;

/// What the swarm is told about the world, once a frame.
#[derive(Resource, Clone, ExtractResource)]
pub struct SwarmConfig {
    pub count: u32,
    pub neighbours: u32,
    pub seed: u64,
    /// The hull the swarm wants, as a sphere for now.
    pub hull_centre: Vec3,
    pub hull_radius: f32,
    pub paused: bool,
}

impl Default for SwarmConfig {
    fn default() -> Self {
        SwarmConfig {
            count: 100_000,
            neighbours: 8,
            seed: 1,
            hull_centre: Vec3::ZERO,
            hull_radius: 3.5,
            paused: false,
        }
    }
}

#[derive(Clone, Copy, Pod, Zeroable, ShaderType)]
#[repr(C)]
pub struct Mote {
    pub pos_scale: Vec4,
    pub vel_seed: Vec4,
}

#[derive(Clone, Copy, Pod, Zeroable, ShaderType)]
#[repr(C)]
struct Params {
    dt: f32,
    time: f32,
    count: u32,
    nbrs: u32,
    hull: Vec4,
    _pad: Vec4,
}

/// Put this on an entity with a `Mesh3d` and that mesh is drawn once per mote.
#[derive(Component, Clone)]
pub struct MoteMesh;

impl ExtractComponent for MoteMesh {
    type QueryData = &'static MoteMesh;
    type QueryFilter = ();
    type Out = Self;
    fn extract_component(_: QueryItem<'_, '_, Self::QueryData>) -> Option<Self> {
        Some(MoteMesh)
    }
}

pub struct SwarmPlugin;

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct SwarmTickLabel;

impl Plugin for SwarmPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractResourcePlugin::<SwarmConfig>::default(),
            ExtractComponentPlugin::<MoteMesh>::default(),
        ))
        .add_plugins(ExtractResourcePlugin::<SwarmClock>::default())
        .init_resource::<SwarmClock>()
        .add_systems(Update, advance_clock);

        let render_app = app.sub_app_mut(RenderApp);
        render_app
            .add_render_command::<Transparent3d, DrawMotes>()
            .init_resource::<SpecializedMeshPipelines<MotePipeline>>()
            .add_systems(RenderStartup, (init_tick_pipeline, init_mote_pipeline))
            .add_systems(
                Render,
                (
                    prepare_swarm_buffers.in_set(RenderSystems::PrepareResources),
                    prepare_tick_bind_group.in_set(RenderSystems::PrepareBindGroups),
                    queue_motes.in_set(RenderSystems::QueueMeshes),
                ),
            );

        let mut graph = render_app.world_mut().resource_mut::<RenderGraph>();
        graph.add_node(SwarmTickLabel, SwarmTickNode::default());
        graph.add_node_edge(SwarmTickLabel, bevy::render::graph::CameraDriverLabel);
    }
}

/// Seconds, from the main world, because the render world has no Time of its
/// own that survives extraction the way this does.
#[derive(Resource, Clone, Default, ExtractResource)]
pub struct SwarmClock {
    pub dt: f32,
    pub time: f32,
    pub ticks: u64,
}

fn advance_clock(time: Res<Time>, cfg: Res<SwarmConfig>, mut clock: ResMut<SwarmClock>) {
    // Clamped so a slow frame does not fling the cloud apart.
    clock.dt = if cfg.paused { 0.0 } else { time.delta_secs().min(1.0 / 20.0) };
    clock.time += clock.dt;
    if !cfg.paused {
        clock.ticks += 1;
    }
}

// ------------------------------------------------------------- buffers --

#[derive(Resource)]
pub struct SwarmBuffers {
    pub motes: Buffer,
    pub count: u32,
    params: UniformBuffer<Params>,
}

fn prepare_swarm_buffers(
    mut commands: Commands,
    existing: Option<ResMut<SwarmBuffers>>,
    cfg: Res<SwarmConfig>,
    clock: Res<SwarmClock>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let params = Params {
        dt: clock.dt,
        time: clock.time,
        count: cfg.count,
        nbrs: cfg.neighbours,
        hull: cfg.hull_centre.extend(cfg.hull_radius),
        _pad: Vec4::ZERO,
    };
    if let Some(mut b) = existing {
        if b.count == cfg.count {
            b.params.set(params);
            b.params.write_buffer(&device, &queue);
            return;
        }
    }
    // Seeded, so the same swarm starts the same way on every run.
    let mut rng = Rng::new(cfg.seed);
    let c = cfg.hull_centre;
    let r = cfg.hull_radius;
    let motes: Vec<Mote> = (0..cfg.count)
        .map(|i| {
            let dir = Vec3::new(rng.range(-1.0, 1.0), rng.range(-0.5, 0.5), rng.range(-1.0, 1.0)).normalize_or_zero();
            let dist = r * rng.range(2.0, 6.0);
            let p = c + dir * dist;
            let v = Vec3::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
            Mote {
                pos_scale: Vec4::new(p.x, p.y, p.z, rng.range(0.7, 1.3)),
                vel_seed: Vec4::new(v.x, v.y, v.z, (i as f32 + 0.5) / cfg.count as f32),
            }
        })
        .collect();
    let buffer = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("swarm motes"),
        contents: bytemuck::cast_slice(&motes),
        usage: BufferUsages::STORAGE | BufferUsages::VERTEX | BufferUsages::COPY_DST,
    });
    let mut params_buf = UniformBuffer::from(params);
    params_buf.write_buffer(&device, &queue);
    commands.insert_resource(SwarmBuffers { motes: buffer, count: cfg.count, params: params_buf });
}

// ------------------------------------------------------------- compute --

#[derive(Resource)]
struct TickPipeline {
    layout: BindGroupLayoutDescriptor,
    pipeline: CachedComputePipelineId,
}

fn init_tick_pipeline(mut commands: Commands, assets: Res<AssetServer>, cache: Res<PipelineCache>) {
    let layout = BindGroupLayoutDescriptor::new(
        "swarm tick",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (storage_buffer::<Vec<Mote>>(false), uniform_buffer::<Params>(false)),
        ),
    );
    let pipeline = cache.queue_compute_pipeline(ComputePipelineDescriptor {
        layout: vec![layout.clone()],
        shader: assets.load(TICK_SHADER),
        entry_point: Some(Cow::from("tick")),
        ..default()
    });
    commands.insert_resource(TickPipeline { layout, pipeline });
}

#[derive(Resource)]
struct TickBindGroup(BindGroup);

fn prepare_tick_bind_group(
    mut commands: Commands,
    pipeline: Res<TickPipeline>,
    buffers: Option<Res<SwarmBuffers>>,
    device: Res<RenderDevice>,
    cache: Res<PipelineCache>,
) {
    let Some(b) = buffers else { return };
    let bg = device.create_bind_group(
        None,
        &cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((b.motes.as_entire_binding(), &b.params)),
    );
    commands.insert_resource(TickBindGroup(bg));
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
        match cache.get_compute_pipeline_state(pipeline.pipeline) {
            CachedPipelineState::Ok(_) => self.ready = true,
            CachedPipelineState::Err(PipelineCacheError::ShaderNotLoaded(_)) => {}
            CachedPipelineState::Err(err) => panic!("compiling {TICK_SHADER}:\n{err}"),
            _ => {}
        }
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
        let (Some(bg), Some(buffers)) = (world.get_resource::<TickBindGroup>(), world.get_resource::<SwarmBuffers>()) else {
            return Ok(());
        };
        let cache = world.resource::<PipelineCache>();
        let pipeline = world.resource::<TickPipeline>();
        let Some(p) = cache.get_compute_pipeline(pipeline.pipeline) else { return Ok(()) };
        let mut pass = ctx.command_encoder().begin_compute_pass(&ComputePassDescriptor { label: Some("swarm tick"), ..default() });
        pass.set_pipeline(p);
        pass.set_bind_group(0, &bg.0, &[]);
        pass.dispatch_workgroups(buffers.count.div_ceil(WORKGROUP), 1, 1);
        Ok(())
    }
}

// ---------------------------------------------------------------- draw --

#[derive(Resource)]
struct MotePipeline {
    shader: Handle<Shader>,
    mesh_pipeline: MeshPipeline,
}

fn init_mote_pipeline(mut commands: Commands, assets: Res<AssetServer>, mesh_pipeline: Res<MeshPipeline>) {
    commands.insert_resource(MotePipeline { shader: assets.load(MOTE_SHADER), mesh_pipeline: mesh_pipeline.clone() });
}

impl SpecializedMeshPipeline for MotePipeline {
    type Key = MeshPipelineKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayoutRef,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mut d = self.mesh_pipeline.specialize(key, layout)?;
        d.vertex.shader = self.shader.clone();
        d.vertex.buffers.push(VertexBufferLayout {
            array_stride: size_of::<Mote>() as u64,
            step_mode: VertexStepMode::Instance,
            attributes: vec![
                VertexAttribute { format: VertexFormat::Float32x4, offset: 0, shader_location: 3 },
                VertexAttribute { format: VertexFormat::Float32x4, offset: 16, shader_location: 4 },
            ],
        });
        d.fragment.as_mut().unwrap().shader = self.shader.clone();
        // Motes are opaque bodies drawn through the sorted phase: they must
        // write depth or the near ones do not cover the far ones.
        if let Some(ds) = d.depth_stencil.as_mut() {
            ds.depth_write_enabled = true;
        }
        Ok(d)
    }
}

fn queue_motes(
    draw_functions: Res<DrawFunctions<Transparent3d>>,
    mote_pipeline: Res<MotePipeline>,
    mut pipelines: ResMut<SpecializedMeshPipelines<MotePipeline>>,
    cache: Res<PipelineCache>,
    meshes: Res<RenderAssets<RenderMesh>>,
    instances: Res<RenderMeshInstances>,
    motes: Query<(Entity, &MainEntity), With<MoteMesh>>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    views: Query<(&ExtractedView, &Msaa)>,
) {
    let draw = draw_functions.read().id::<DrawMotes>();
    for (view, msaa) in &views {
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else { continue };
        let view_key = MeshPipelineKey::from_msaa_samples(msaa.samples()) | MeshPipelineKey::from_hdr(view.hdr);
        let rangefinder = view.rangefinder3d();
        for (entity, main_entity) in &motes {
            let Some(inst) = instances.render_mesh_queue_data(*main_entity) else { continue };
            let Some(mesh) = meshes.get(inst.mesh_asset_id) else { continue };
            let key = view_key | MeshPipelineKey::from_primitive_topology(mesh.primitive_topology());
            let pipeline = match pipelines.specialize(&cache, &mote_pipeline, key, &mesh.layout) {
                Ok(p) => p,
                Err(e) => {
                    error!("mote pipeline: {e}");
                    continue;
                }
            };
            phase.add(Transparent3d {
                entity: (entity, *main_entity),
                pipeline,
                draw_function: draw,
                distance: rangefinder.distance(&inst.center),
                batch_range: 0..1,
                extra_index: PhaseItemExtraIndex::None,
                indexed: true,
            });
        }
    }
}

type DrawMotes = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetMeshBindGroup<2>,
    DrawMotesInstanced,
);

struct DrawMotesInstanced;

impl<P: PhaseItem> RenderCommand<P> for DrawMotesInstanced {
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
        let Some(inst) = instances.render_mesh_queue_data(item.main_entity()) else { return RenderCommandResult::Skip };
        let Some(gpu_mesh) = meshes.into_inner().get(inst.mesh_asset_id) else { return RenderCommandResult::Skip };
        let Some(vslice) = allocator.mesh_vertex_slice(&inst.mesh_asset_id) else { return RenderCommandResult::Skip };

        pass.set_vertex_buffer(0, vslice.buffer.slice(..));
        pass.set_vertex_buffer(1, buffers.motes.slice(..));
        match &gpu_mesh.buffer_info {
            RenderMeshBufferInfo::Indexed { index_format, count } => {
                let Some(islice) = allocator.mesh_index_slice(&inst.mesh_asset_id) else { return RenderCommandResult::Skip };
                pass.set_index_buffer(islice.buffer.slice(..), *index_format);
                pass.draw_indexed(islice.range.start..(islice.range.start + count), vslice.range.start as i32, 0..buffers.count);
            }
            RenderMeshBufferInfo::NonIndexed => {
                pass.draw(vslice.range, 0..buffers.count);
            }
        }
        RenderCommandResult::Success
    }
}

/// Spawn the entity the instanced draw hangs off: at the origin, never culled,
/// because the motes are wherever the buffer says and the buffer is not a
/// transform the culler can see.
pub fn spawn_mote_mesh(commands: &mut Commands, mesh: Handle<Mesh>) {
    commands.spawn((Mesh3d(mesh), Transform::IDENTITY, MoteMesh, NoFrustumCulling));
}
