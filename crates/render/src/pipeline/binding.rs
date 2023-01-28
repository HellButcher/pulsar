use std::borrow::Cow;

pub use pulz_render_macros::AsBindingLayout;

crate::backend::define_gpu_resource!(BindGroupLayout, BindGroupLayoutDescriptor<'l>);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BindGroupLayoutDescriptor<'a> {
    pub label: Option<&'a str>,
    pub entries: Cow<'a, [BindGroupLayoutEntry]>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BindGroupLayoutEntry {
    pub binding: u32,
    // pub visibility: ShaderStages,
    // pub ty: BindingType,
    // TODO:
    pub count: u32,
}

pub trait AsBindingLayout {
    // TODO (also macro)
}
