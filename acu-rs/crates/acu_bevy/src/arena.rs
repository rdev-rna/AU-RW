//! Authored test-floor shader, deliberately separate from native material work.
use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

pub type ArenaMaterial = ExtendedMaterial<StandardMaterial, ArenaShader>;

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct ArenaShader {
    #[uniform(100)]
    pub grid: Vec4,
}

impl MaterialExtension for ArenaShader {
    fn fragment_shader() -> ShaderRef {
        "embedded://acu_bevy/shaders/arena.wgsl".into()
    }
}
