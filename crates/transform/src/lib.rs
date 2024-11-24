#![warn(
    // missing_docs,
    // rustdoc::missing_doc_code_examples,
    future_incompatible,
    rust_2018_idioms,
    unused,
    trivial_casts,
    trivial_numeric_casts,
    unused_lifetimes,
    unused_qualifications,
    unused_crate_dependencies,
    clippy::cargo,
    clippy::multiple_crate_versions,
    clippy::empty_line_after_outer_attr,
    clippy::fallible_impl_from,
    clippy::redundant_pub_crate,
    clippy::use_self,
    clippy::suspicious_operation_groupings,
    clippy::useless_let_if_seq,
    // clippy::missing_errors_doc,
    // clippy::missing_panics_doc,
    clippy::wildcard_imports
)]
#![doc(html_logo_url = "https://raw.githubusercontent.com/HellButcher/pulz/master/docs/logo.png")]
#![doc(html_no_source)]
#![doc = include_str!("../README.md")]

pub mod math {
    pub use glam::*;
    pub type Point2 = Vec2;
    pub type Size2 = Vec2;
    pub type Point3 = Vec3;
    pub type Size3 = Vec3;
    pub type USize2 = UVec2;
    pub type USize3 = UVec3;
    pub use glam::{uvec2 as usize2, uvec3 as usize3, vec2 as size2, vec3 as size3};
}
pub mod components;
