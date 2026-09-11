//! Textures, hulls and materials: how a design on disk becomes something
//! Bevy can draw.

use crate::*;

/// Every texture the picture uses, by name, loaded ONE way each: a finish or
/// a chitin is a normal map (linear, repeating), a window's colour and glow
/// are sRGB and clamped, its normal linear and clamped. `textures.ts` in
/// redux-tribes is the one loader for the same reason: a fourth caller cannot
/// spell a path or a colour space its own way if it has nowhere to spell it.
#[derive(Resource, Default)]
pub(crate) struct Textures {
    pub(crate) finishes: HashMap<String, Handle<Image>>,
    pub(crate) windows: HashMap<String, WindowMaps>,
    pub(crate) chitin: Option<Handle<Image>>,
    pub(crate) ember: Option<Handle<Image>>,
}

#[derive(Clone)]
pub(crate) struct WindowMaps {
    pub(crate) colour: Handle<Image>,
    pub(crate) emissive: Handle<Image>,
    pub(crate) normal: Handle<Image>,
}

pub(crate) const FINISHES: [&str; 9] = [
    "plate", "ribbed", "hex", "cracked", "tread", "greeble", "weave", "battered", "crate",
];

pub(crate) const WINDOW_KINDS: [&str; 9] = [
    "panes",
    "porthole",
    "strip",
    "bridge",
    "promenade",
    "beacons",
    "louvre",
    "cargo",
    "hangar",
];

pub(crate) fn sampler(repeat: bool) -> ImageSampler {
    let mode = if repeat {
        ImageAddressMode::Repeat
    } else {
        ImageAddressMode::ClampToEdge
    };
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode,
        address_mode_v: mode,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 4,
        ..default()
    })
}

pub(crate) fn load_textures(mut commands: Commands, assets: Res<AssetServer>) {
    let load = |path: String, srgb: bool, repeat: bool| -> Handle<Image> {
        assets.load_with_settings(path, move |s: &mut ImageLoaderSettings| {
            s.is_srgb = srgb;
            s.sampler = sampler(repeat);
        })
    };
    let mut t = Textures::default();
    for f in FINISHES {
        t.finishes.insert(
            f.into(),
            load(format!("textures/surf/armour_{f}_n.png"), false, true),
        );
    }
    for k in WINDOW_KINDS {
        t.windows.insert(
            k.into(),
            WindowMaps {
                colour: load(format!("textures/surf/window_{k}_c.png"), true, false),
                emissive: load(format!("textures/surf/window_{k}_e.png"), true, false),
                normal: load(format!("textures/surf/window_{k}_n.png"), false, false),
            },
        );
    }
    let chitin = load("textures/alien_chitin_n.png".into(), false, true);
    // The ember atlas is the one texture a burn comes off, wherever it is: the
    // inside of a hole, the soot round it, and every spark in the air. One
    // fire, one picture of it.
    let ember = load("textures/ember.png".into(), true, false);
    commands.insert_resource(FxTextures {
        chitin: chitin.clone(),
        ember: ember.clone(),
    });
    t.chitin = Some(chitin);
    t.ember = Some(ember);
    commands.insert_resource(t);
}

#[derive(Resource, Default)]
pub(crate) struct ChunkMaterials(pub(crate) HashMap<u32, Handle<StandardMaterial>>);

/// A mote, as one mesh with its LIT cells marked.
///
/// The motes are one instanced draw, so their lit cells cannot be a second
/// mesh the way a carrier's are: there is nowhere to put it. The marker rides
/// in the vertex colour's ALPHA instead, which every other picture ignores
/// because they are all opaque, and which `mote.wgsl` reads as "this cell is
/// its own light".
pub(crate) fn to_mote_mesh(s: &Surfaces) -> Mesh {
    let mut all = MeshData::default();
    for (i, md) in s.skin.iter().enumerate() {
        let mut copy = md.clone();
        let lit = i == SURF_DRIVE as usize;
        for c in copy.colours.iter_mut() {
            c[3] = if lit { 0.0 } else { 1.0 };
        }
        all.append(&copy);
    }
    to_mesh(&all)
}

pub(crate) fn load_hull(key: &str) -> VoxelModel {
    let path = format!("{HULLS}{key}.ftvx");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    VoxelModel::from_ftvx(&bytes).expect("a hull file this build understands")
}

/// One material per surface, as `hullMaterials` in hull.ts builds them: the
/// design's own finish, metalness and roughness for each, vertex colours
/// carrying the livery. `smooth` is no normal map at all.
pub(crate) fn surface_materials(
    model: &VoxelModel,
    tex: &Textures,
    materials: &mut Assets<StandardMaterial>,
) -> Vec<Handle<StandardMaterial>> {
    (0..SURF_COUNT)
        .map(|s| {
            let (finish, metal, rough) = model
                .surfaces
                .get(s)
                .map(|x| (x.finish.as_str(), x.metal, x.rough))
                .unwrap_or(("plate", 0.25, 0.55));
            materials.add(StandardMaterial {
                base_color: Color::WHITE,
                metallic: metal,
                perceptual_roughness: rough,
                normal_map_texture: tex.finishes.get(finish).cloned(),
                ..default()
            })
        })
        .collect()
}

/// One material per window kind, as `windowMaterial` in textures.ts: the
/// colour map multiplies the plating's paint down to glass, emission lights
/// the panes that are on and is the only part that survives with no light on
/// it, and the normal seats the frame into the plate.
pub(crate) fn window_materials(
    model: &VoxelModel,
    tex: &Textures,
    materials: &mut Assets<StandardMaterial>,
) -> Vec<Handle<StandardMaterial>> {
    model
        .window_kinds
        .iter()
        .map(|k| {
            let maps = tex.windows.get(k);
            materials.add(StandardMaterial {
                base_color: Color::WHITE,
                base_color_texture: maps.map(|m| m.colour.clone()),
                emissive: LinearRgba::WHITE * 1.6,
                emissive_texture: maps.map(|m| m.emissive.clone()),
                normal_map_texture: maps.map(|m| m.normal.clone()),
                metallic: 0.15,
                perceptual_roughness: 0.35,
                ..default()
            })
        })
        .collect()
}
