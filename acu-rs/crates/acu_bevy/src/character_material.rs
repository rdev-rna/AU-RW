//! Character surface: packed RG normals for every material, plus the
//! `CN_Cloth_Tile_ColorMasked_AlphaTest` template's per-pixel detail normals,
//! gloss and specular reflectance re-derived from its G-buffer pixel shader
//! (ACU-RE-0084). The albedo recolouring is baked on the CPU (ACU-RE-0071).
//! `CN_Skin_Wrinkle` and `CN_Hair` materials get their templates' albedo
//! terms, F0 and gloss where the material data supplies them.
use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

pub type CharacterMaterial = ExtendedMaterial<StandardMaterial, CharacterSurface>;

/// Uniform block of the character shader. Register numbers refer to the
/// template's `cb3` constants (`acu_assets::color_mask`).
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct CharacterParams {
    /// x: packed normal bound, y: green sign, z: template surface (1 cloth,
    /// 2 skin, 3 hair), w: skin / hair mask bound.
    pub flags: Vec4,
    /// Detail weights: cb3[1] base, cb3[3] mid, cb3[4] cloth, cb3[11] metal.
    pub detail: Vec4,
    /// cb3[5] cloth specular, cb3[15] mid specular, cb3[12] cloth saturation,
    /// cb3[14] metal saturation.
    pub specular: Vec4,
    /// cb3[2] metal hue scale, cb3[17] hue scale, cb3[18] mid threshold,
    /// wetness (cb3[6] only; the global rain term is not modelled).
    pub bands: Vec4,
    /// xy: detail UV scale (cb3[23]); z: bound detail maps (1 t36, 2 t38, 4 t39).
    pub detail_uv: Vec4,
    /// Skin / hair saturation matrix rows (`character_template::saturation_matrix`).
    pub matrix_r: Vec4,
    pub matrix_g: Vec4,
    pub matrix_b: Vec4,
    /// Skin: cb3[11] (rgb tint, w weight bias). Hair: cb3[8] tint, w anisotropy.
    pub tint: Vec4,
}

/// SYNTHETIC: the skin template's base gloss comes from an engine-bound
/// texture (t35.y) that was not identified; this stands in for it before the
/// material's own gloss scale (cb3[2]).
pub const SKIN_BASE_GLOSS_SYNTHETIC: f32 = 0.35;
/// SYNTHETIC: Bevy anisotropy strength standing in for ACU's two-lobe
/// strand-tangent hair highlight.
pub const HAIR_ANISOTROPY_SYNTHETIC: f32 = 0.8;

fn rows(m: [[f32; 3]; 3]) -> [Vec4; 3] {
    m.map(|r| Vec4::new(r[0], r[1], r[2], 0.0))
}

impl CharacterParams {
    pub fn plain(has_normal: bool) -> Self {
        Self {
            flags: Vec4::new(if has_normal { 1.0 } else { 0.0 }, 1.0, 0.0, 0.0),
            ..default()
        }
    }

    pub fn cloth(s: &acu_assets::color_mask::Surface, detail_bits: u32) -> Self {
        Self {
            flags: Vec4::new(1.0, 1.0, 1.0, 0.0),
            detail: Vec4::new(s.detail_base, s.detail_mid, s.detail_cloth, s.detail_metal),
            specular: Vec4::new(
                s.cloth_specular,
                s.mid_specular,
                s.cloth_saturation,
                s.metal_saturation,
            ),
            bands: Vec4::new(
                s.metal_hue,
                s.hue_scale,
                s.mid_threshold,
                s.wet_bias.clamp(0.0, 1.0),
            ),
            detail_uv: Vec4::new(
                s.detail_uv_scale[0],
                s.detail_uv_scale[1],
                detail_bits as f32,
                0.0,
            ),
            ..default()
        }
    }

    pub fn skin(s: &acu_assets::character_template::Skin, has_mask: bool) -> Self {
        let [matrix_r, matrix_g, matrix_b] = rows(
            acu_assets::character_template::saturation_matrix(s.desaturate),
        );
        let wet = s.wet_bias.clamp(0.0, 1.0);
        Self {
            flags: Vec4::new(1.0, 1.0, 2.0, if has_mask { 1.0 } else { 0.0 }),
            specular: Vec4::new(
                SKIN_BASE_GLOSS_SYNTHETIC * (wet + s.gloss_scale).clamp(0.0, 1.0),
                0.0,
                0.0,
                0.0,
            ),
            matrix_r,
            matrix_g,
            matrix_b,
            tint: Vec4::from_array(s.tint),
            ..default()
        }
    }

    pub fn hair(h: &acu_assets::character_template::Hair, has_mask: bool) -> Self {
        let [matrix_r, matrix_g, matrix_b] = rows(
            acu_assets::character_template::saturation_matrix(h.desaturate),
        );
        Self {
            flags: Vec4::new(1.0, 1.0, 3.0, if has_mask { 1.0 } else { 0.0 }),
            bands: Vec4::new(0.0, 0.0, 0.0, h.wet_bias.clamp(0.0, 1.0)),
            matrix_r,
            matrix_g,
            matrix_b,
            tint: Vec4::new(h.tint[0], h.tint[1], h.tint[2], HAIR_ANISOTROPY_SYNTHETIC),
            ..default()
        }
    }
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct CharacterSurface {
    #[texture(100)]
    #[sampler(101)]
    pub packed_normal: Option<Handle<Image>>,
    #[uniform(102)]
    pub params: CharacterParams,
    #[texture(103)]
    #[sampler(104)]
    pub mask: Option<Handle<Image>>,
    #[texture(105)]
    #[sampler(106)]
    pub detail_metal: Option<Handle<Image>>,
    #[texture(107)]
    #[sampler(108)]
    pub detail_cloth: Option<Handle<Image>>,
    #[texture(109)]
    #[sampler(110)]
    pub detail_base: Option<Handle<Image>>,
}

impl CharacterSurface {
    pub fn plain(packed_normal: Option<Handle<Image>>) -> Self {
        Self {
            params: CharacterParams::plain(packed_normal.is_some()),
            packed_normal,
            mask: None,
            detail_metal: None,
            detail_cloth: None,
            detail_base: None,
        }
    }
}

impl MaterialExtension for CharacterSurface {
    fn fragment_shader() -> ShaderRef {
        "embedded://acu_bevy/shaders/character.wgsl".into()
    }
}
