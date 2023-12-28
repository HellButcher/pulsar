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

use std::{backtrace::Backtrace, ffi::CStr, rc::Rc, sync::Arc};

use ash::vk::{self, PipelineStageFlags};
use bitflags::bitflags;
use device::AshDevice;
use encoder::{AshCommandPool, SubmissionGroup};
use graph::AshRenderGraph;
use instance::AshInstance;
use pulz_ecs::prelude::*;
use pulz_render::{draw::DrawPhases, graph::RenderGraph, RenderModule, RenderSystemPhase};
use resources::AshResources;
use thiserror::Error;

mod alloc;
mod convert;
mod debug_utils;
mod device;
mod drop_guard;
mod encoder;
mod graph;
mod instance;
mod resources;
mod shader;
mod swapchain;

use pulz_window::{
    listener::WindowSystemListener, HasRawWindowAndDisplayHandle, Window, WindowId, Windows,
    WindowsMirror,
};

// wrapper object for printing backtrace, until provide() is stable
pub struct VkError {
    result: vk::Result,
    backtrace: Backtrace,
}

impl From<vk::Result> for VkError {
    fn from(result: vk::Result) -> Self {
        Self {
            result,
            backtrace: Backtrace::capture(),
        }
    }
}
impl std::error::Error for VkError {}
impl std::fmt::Debug for VkError {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as std::fmt::Display>::fmt(self, f)
    }
}
impl std::fmt::Display for VkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}\nVkResult backtrace:\n{}",
            self.result, self.backtrace
        )
    }
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("library loading error")]
    LoadingError(#[from] ash::LoadingError),

    #[error("Vulkan driver does not support {0:?}")]
    ExtensionNotSupported(&'static CStr),

    #[error("The used Window-System is not supported")]
    UnsupportedWindowSystem,

    #[error("The window is not available, or it has no raw-window-handle")]
    WindowNotAvailable,

    #[error("No suitable GPU adapters found on the system!")]
    NoAdapter,

    #[error("Device doesn't have swapchain support")]
    NoSwapchainSupport,

    #[error("Swapchain supports {supported:?}, but {requested:?} was requested")]
    SwapchainUsageNotSupported {
        requested: vk::ImageUsageFlags,
        supported: vk::ImageUsageFlags,
    },

    #[error("The surface was lost")]
    SurfaceLost,

    #[error("A next swapchain image was already acquired without beeing presented.")]
    SwapchainImageAlreadyAcquired,

    #[error("Vulkan Error")]
    VkError(#[from] VkError),

    #[error("Allocation Error")]
    AllocationError(#[from] gpu_alloc::AllocationError),

    #[error("Serialization Error")]
    SerializationError(Box<dyn std::error::Error>),

    #[error("Deserialization Error")]
    DeserializationError(Box<dyn std::error::Error>),

    #[error("unknown renderer error")]
    Unknown,
}

#[derive(Debug)]
pub struct ErrorNoExtension(pub &'static CStr);

impl From<ErrorNoExtension> for Error {
    #[inline]
    fn from(e: ErrorNoExtension) -> Self {
        Self::ExtensionNotSupported(e.0)
    }
}
impl From<vk::Result> for Error {
    #[inline]
    fn from(e: vk::Result) -> Self {
        Self::from(VkError::from(e))
    }
}
impl From<&vk::Result> for Error {
    #[inline]
    fn from(e: &vk::Result) -> Self {
        Self::from(*e)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

struct AshRendererFull {
    device: Arc<AshDevice>,
    res: AshResources,
    frames: Vec<Frame>,
    current_frame: usize,
    surfaces: WindowsMirror<swapchain::AshSurfaceSwapchain>,
    graph: AshRenderGraph,
}

impl Drop for AshRendererFull {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
        }
        for (_, swapchain) in self.surfaces.drain() {
            swapchain.destroy_with_surface(&mut self.res).unwrap();
        }
        self.frames.clear();
        self.res.clear_all().unwrap();
    }
}

bitflags! {
    /// Instance initialization flags.
    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
    pub struct AshRendererFlags: u32 {
        /// Generate debug information in shaders and objects.
        const DEBUG = 1 << 0;
    }
}

struct Frame {
    // TODO: multi-threaded command recording: CommandPool per thread
    command_pool: AshCommandPool,
    finished_fence: vk::Fence, // signaled ad end of command-cueue, waited at beginning of frame
    finished_semaphore: vk::Semaphore, // semaphore used for presenting to the swapchain
}

impl Frame {
    pub const NUM_FRAMES_IN_FLIGHT: usize = 2;
}

impl Frame {
    unsafe fn create(device: &Arc<AshDevice>) -> Result<Self> {
        let command_pool = device.new_command_pool(device.queues().graphics_family)?;
        let finished_fence = device.create(
            &vk::FenceCreateInfo::builder()
                .flags(vk::FenceCreateFlags::SIGNALED)
                .build(),
        )?;
        let finished_semaphore = device.create(&vk::SemaphoreCreateInfo::builder().build())?;
        Ok(Self {
            command_pool,
            finished_fence: finished_fence.take(),
            finished_semaphore: finished_semaphore.take(),
        })
    }

    unsafe fn reset(&mut self, _device: &AshDevice) -> Result<(), vk::Result> {
        self.command_pool.reset()?;
        Ok(())
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        unsafe {
            let device = self.command_pool.device();
            if self.finished_fence != vk::Fence::null() {
                device.destroy_fence(self.finished_fence, None);
            }
            if self.finished_semaphore != vk::Semaphore::null() {
                device.destroy_semaphore(self.finished_semaphore, None);
            }
        }
    }
}

impl AshRendererFull {
    fn from_device(device: Arc<AshDevice>) -> Result<Self> {
        let res = AshResources::new(&device, Frame::NUM_FRAMES_IN_FLIGHT)?;
        Ok(Self {
            device,
            res,
            frames: Vec::with_capacity(Frame::NUM_FRAMES_IN_FLIGHT),
            current_frame: 0,
            surfaces: WindowsMirror::new(),
            graph: AshRenderGraph::new(),
        })
    }

    fn begin_frame(&mut self) -> Result<SubmissionGroup> {
        let _span = tracing::trace_span!("BeginFrame").entered();

        if self.frames.is_empty() {
            self.frames.reserve_exact(Frame::NUM_FRAMES_IN_FLIGHT);
            for _ in 0..Frame::NUM_FRAMES_IN_FLIGHT {
                self.frames.push(unsafe { Frame::create(&self.device)? });
            }
        }

        let frame = &mut self.frames[self.current_frame];
        unsafe {
            self.device
                .wait_for_fences(&[frame.finished_fence], true, !0)?;
        }

        // cleanup old frame
        unsafe {
            frame.reset(&self.device)?;
            self.res.next_frame_and_clear_garbage();
        }

        Ok(SubmissionGroup::new())
    }

    fn render_frame(
        &mut self,
        submission_group: &mut SubmissionGroup,
        src_graph: &RenderGraph,
        phases: &DrawPhases,
    ) -> Result<()> {
        let _span = tracing::trace_span!("RunGraph").entered();
        let frame = &mut self.frames[self.current_frame];

        let span_update = tracing::trace_span!("Update").entered();
        self.graph.update(
            src_graph,
            submission_group,
            &mut self.res,
            &mut frame.command_pool,
            &mut self.surfaces,
        )?;
        drop(span_update);

        let span_exec = tracing::trace_span!("Execute").entered();
        self.graph.execute(
            src_graph,
            submission_group,
            &mut frame.command_pool,
            phases,
        )?;
        drop(span_exec);

        Ok(())
    }

    // TODO: remove this!
    fn clear_unacquired_swapchain_images(
        &mut self,
        submission_group: &mut SubmissionGroup,
    ) -> Result<()> {
        let unaquired_swapchains: Vec<_> = self
            .surfaces
            .iter()
            .filter_map(|(id, s)| if s.is_acquired() { None } else { Some(id) })
            .collect();
        if unaquired_swapchains.is_empty() {
            return Ok(());
        }

        // TODO: try to clear with empty render-pass
        let _span = tracing::trace_span!("ClearImages").entered();

        let mut images = Vec::with_capacity(unaquired_swapchains.len());
        for window_id in unaquired_swapchains.iter().copied() {
            let sem = self.frames[self.current_frame]
                .command_pool
                .request_semaphore()?;
            submission_group.wait(sem, PipelineStageFlags::TRANSFER);
            if let Some(texture) = self.acquire_swapchain_image(window_id, 0, sem)? {
                let image = self.res.textures[texture].0;
                images.push((image, self.surfaces[window_id].clear_value()));
            }
        }

        let subrange = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(vk::REMAINING_ARRAY_LAYERS)
            .level_count(vk::REMAINING_MIP_LEVELS)
            .build();

        let barriers = images
            .iter()
            .map(|(image, _)| {
                vk::ImageMemoryBarrier::builder()
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .subresource_range(subrange)
                    .image(*image)
                    .build()
            })
            .collect::<Vec<_>>();

        let frame = &mut self.frames[self.current_frame];
        let encoder = frame.command_pool.encoder()?;
        encoder.begin_debug_label("ClearImages");

        unsafe {
            encoder.pipeline_barrier(
                vk::PipelineStageFlags::TRANSFER | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::TRANSFER,
                &[],
                &[],
                &barriers,
            );
        }

        for (image, clear_color) in &images {
            unsafe {
                encoder.clear_color_image(
                    *image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    clear_color,
                    &[subrange],
                )
            }
        }

        let barriers = images
            .iter()
            .map(|(image, _)| {
                vk::ImageMemoryBarrier::builder()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::empty())
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                    .subresource_range(subrange)
                    .image(*image)
                    .build()
            })
            .collect::<Vec<_>>();
        unsafe {
            encoder.pipeline_barrier(
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                &[],
                &[],
                &barriers,
            );
        }

        encoder.submit(submission_group)?;
        Ok(())
    }

    fn end_frame(&mut self, mut submission_group: SubmissionGroup) -> Result<()> {
        let _span = tracing::trace_span!("EndFrame").entered();

        self.clear_unacquired_swapchain_images(&mut submission_group)?;

        let acquired_swapchains = self.get_num_acquired_swapchains();
        let frame = &self.frames[self.current_frame];

        unsafe {
            self.device
                .reset_fences(&[self.frames[self.current_frame].finished_fence])?;
        }

        submission_group.flush_queue();
        if acquired_swapchains == 0 {
            submission_group.submit(&self.device, frame.finished_fence)?;
        } else {
            submission_group
                .signal(frame.finished_semaphore)
                .submit(&self.device, frame.finished_fence)?;

            self.present_acquired_swapchain_images(&[frame.finished_semaphore])?;
        }

        let next_frame = self.current_frame;
        self.current_frame = next_frame + 1;
        if self.current_frame >= self.frames.len() {
            self.current_frame = 0;
        }
        Ok(())
    }

    fn run(&mut self, windows: &mut Windows, src_graph: &RenderGraph, draw_phases: &DrawPhases) {
        self.reconfigure_swapchains(windows);

        let mut submission_group = self.begin_frame().unwrap();

        self.render_frame(&mut submission_group, src_graph, draw_phases)
            .unwrap();
        self.end_frame(submission_group).unwrap();
    }
}

#[allow(clippy::large_enum_variant)]
enum AshRendererInner {
    Early {
        instance: Arc<AshInstance>,
        flags: AshRendererFlags,
    },
    Full(AshRendererFull),
}

pub struct AshRenderer(AshRendererInner);

impl AshRenderer {
    #[inline]
    pub fn new() -> Result<Self> {
        Self::with_flags(AshRendererFlags::DEBUG)
    }

    #[inline]
    pub fn with_flags(flags: AshRendererFlags) -> Result<Self> {
        let instance = AshInstance::new(flags)?;
        Ok(Self(AshRendererInner::Early { instance, flags }))
    }

    fn init(&mut self) -> Result<&mut AshRendererFull> {
        if let AshRendererInner::Early { instance, .. } = &self.0 {
            let device = instance.new_device(vk::SurfaceKHR::null())?;
            let renderer = AshRendererFull::from_device(device)?;
            self.0 = AshRendererInner::Full(renderer);
        }
        let AshRendererInner::Full(renderer) = &mut self.0 else {
            unreachable!()
        };
        Ok(renderer)
    }

    fn init_window(
        &mut self,
        window_id: WindowId,
        window_descriptor: &Window,
        window: Rc<dyn HasRawWindowAndDisplayHandle>,
    ) -> Result<&mut AshRendererFull> {
        if let AshRendererInner::Full(renderer) = &mut self.0 {
            let device = renderer.device.clone();
            // SAVETY: window is kept alive
            let surface = unsafe { device.instance().new_surface(&*window)? };
            renderer.init_swapchain(window_id, window_descriptor, window, surface)?;
        } else {
            let AshRendererInner::Early { instance, .. } = &self.0 else {
                unreachable!()
            };
            // SAVETY: window is kept alive
            let surface = unsafe { instance.new_surface(&*window)? };
            let device = instance.new_device(surface.raw())?;
            let mut renderer = AshRendererFull::from_device(device)?;
            renderer.init_swapchain(window_id, window_descriptor, window, surface)?;
            self.0 = AshRendererInner::Full(renderer);
        }
        let AshRendererInner::Full(renderer) = &mut self.0 else {
            unreachable!()
        };
        Ok(renderer)
    }

    fn run(&mut self, windows: &mut Windows, src_graph: &RenderGraph, draw_phases: &DrawPhases) {
        if let AshRendererInner::Full(renderer) = &mut self.0 {
            renderer.run(windows, src_graph, draw_phases);
        } else {
            panic!("renderer uninitialized");
        }
    }
}

impl WindowSystemListener for AshRenderer {
    fn on_created(
        &mut self,
        window_id: WindowId,
        window_desc: &Window,
        window: Rc<dyn HasRawWindowAndDisplayHandle>,
    ) {
        self.init_window(window_id, window_desc, window).unwrap();
    }
    fn on_resumed(&mut self) {
        self.init().unwrap();
    }
    fn on_closed(&mut self, window_id: WindowId) {
        let AshRendererInner::Full(renderer) = &mut self.0 else {
            return;
        };
        renderer.destroy_swapchain(window_id).unwrap();
    }
    fn on_suspended(&mut self) {
        let AshRendererInner::Full(renderer) = &mut self.0 else {
            return;
        };
        renderer.destroy_all_swapchains().unwrap();
    }
}

impl ModuleWithOutput for AshRenderer {
    type Output<'l> = &'l mut Self;

    fn install_modules(&self, res: &mut Resources) {
        res.install(RenderModule);
    }

    fn install_resources(self, res: &mut Resources) -> &mut Self {
        let resource_id = res.insert_unsend(self);
        res.init_meta_id::<dyn WindowSystemListener, _>(resource_id);
        res.get_mut_id(resource_id).unwrap()
    }

    fn install_systems(schedule: &mut Schedule) {
        schedule
            .add_system(Self::run)
            .into_phase(RenderSystemPhase::Render);
    }
}
