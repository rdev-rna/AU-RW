//! Presentation interpretation of packed RG normals; native template math open.
use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

pub type CharacterMaterial = ExtendedMaterial<StandardMaterial, CharacterSurface>;

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct CharacterSurface {
    #[texture(100)]
    #[sampler(101)]
    pub packed_normal: Option<Handle<Image>>,
    #[uniform(102)]
    pub params: Vec4,
}

impl MaterialExtension for CharacterSurface {
    fn fragment_shader() -> ShaderRef {
        "embedded://acu_bevy/shaders/character.wgsl".into()
    }
}
