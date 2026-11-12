//! Shared offscreen WGPU renderer for the desktop viewport and PNG exports.
//! Record arithmetic is the calibrated uber-shader (shaders/uber.*.glsl), translated by Naga from GLSL.

use super::packet::{
    self, BlendMode, DATA_WIDTH, DrawBatch, FRAME_TARGET, RenderPacket, TextureFormat, TextureId,
    TextureUpload,
};
use anyhow::{Context, Result, bail};
use std::{
    borrow::Cow,
    num::NonZeroU64,
    sync::{Arc, mpsc},
    time::Instant,
};
use wgpu::util::DeviceExt;

pub type GpuError = anyhow::Error;
/// Frames between GPU timestamp samples
const TIMESTAMP_INTERVAL: u32 = 16;
pub const OUTPUT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// `table[a][v]` = (v * 255 + a / 2) / a, the straight value of premultiplied `v` at alpha `a`
/// (at most 255); alpha 0 keeps its values and 255 maps every value to itself
fn unpremultiply_table() -> &'static [[u8; 256]; 256] {
    static TABLE: std::sync::OnceLock<Box<[[u8; 256]; 256]>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = Box::new([[0u8; 256]; 256]);
        for (alpha, row) in table.iter_mut().enumerate() {
            for (value, out) in row.iter_mut().enumerate() {
                *out = if alpha == 0 { value as u8 } else { ((value * 255 + alpha / 2) / alpha).min(255) as u8 };
            }
        }
        table
    })
}

/// A copy of the frame target on its way to mappable memory ([`GpuRenderer::begin_readback`])
pub struct PendingReadback {
    buffer: wgpu::Buffer,
    submission: wgpu::SubmissionIndex,
    mapped: mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    size: u64,
    padded: u32,
    width: u32,
    height: u32,
}

pub struct HeadlessGpu {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub adapter_info: wgpu::AdapterInfo,
}

/// The Vulkan loader's settings a caller chose (a capture tool's layers, a driver list): left as they are
const LOADER_SETTINGS: [&str; 5] = ["VK_LOADER_LAYERS_DISABLE", "VK_LOADER_LAYERS_ENABLE", "VK_INSTANCE_LAYERS", "VK_LOADER_DRIVERS_SELECT", "VK_LOADER_DRIVERS_DISABLE"];

/// What is set for the loader when the caller chose none of [`LOADER_SETTINGS`]: no implicit layers and no Dozen
const LOADER_DEFAULTS: [(&str, &str); 2] = [("VK_LOADER_LAYERS_DISABLE", "~implicit~"), ("VK_LOADER_DRIVERS_DISABLE", "*dzn*")];

/// True when `is_set` finds none of [`LOADER_SETTINGS`]. A setting with an empty value counts as chosen.
fn no_loader_setting_chosen(is_set: impl Fn(&str) -> bool) -> bool {
    LOADER_SETTINGS.iter().all(|name| !is_set(name))
}

fn no_loader_setting_in_environment() -> bool {
    no_loader_setting_chosen(|name| std::env::var_os(name).is_some())
}

/// A release build sets the loader's defaults once, at startup, and keeps them for the life of the process: for
/// every adapter it tries (the fallback one too) and for the helpers it starts. Checkout builds still set them
/// only around the probe in `native_vulkan`; setting and removing them there is the code they have always run.
///
/// # Safety
/// Must run before any thread starts. On Linux and macOS another thread reading the environment while it
/// changes is unsound, which is why a release build does not change it later (see the `paths` module comment).
pub unsafe fn initialize_loader_environment() {
    if !crate::paths::is_standalone() || !no_loader_setting_in_environment() {
        return;
    }
    // SAFETY: the caller guarantees that no other thread exists yet
    unsafe {
        for (name, value) in LOADER_DEFAULTS {
            std::env::set_var(name, value);
        }
    }
}

/// The discrete GPU all backends would pick (Vulkan's first), found without implicit layers or Dozen (Vulkan on D3D12)
async fn native_vulkan(options: &wgpu::RequestAdapterOptions<'_, '_>) -> Option<wgpu::Adapter> {
    // A release build has already set them for good at startup, and leaves a caller's alone
    let set_around_probe = !crate::paths::is_standalone() && no_loader_setting_in_environment();
    if set_around_probe {
        // SAFETY: Windows guards the environment block; the loader reads these while the instance starts
        unsafe {
            for (name, value) in LOADER_DEFAULTS {
                std::env::set_var(name, value);
            }
        }
    }
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..Default::default() });
    let adapter = instance.request_adapter(options).await.ok().filter(|adapter| adapter.get_info().device_type == wgpu::DeviceType::DiscreteGpu);
    if set_around_probe {
        // SAFETY: as above
        unsafe {
            for (name, _) in LOADER_DEFAULTS {
                std::env::remove_var(name);
            }
        }
    }
    adapter
}

impl HeadlessGpu {
    pub async fn new() -> Result<Self> {
        let options = wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        };
        let adapter = match native_vulkan(&options).await {
            Some(adapter) => adapter,
            None => {
                let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
                match instance.request_adapter(&options).await {
                    Ok(adapter) => adapter,
                    Err(_) => instance
                        .request_adapter(&wgpu::RequestAdapterOptions {
                            force_fallback_adapter: true,
                            ..options
                        })
                        .await
                        .context("No native GPU or software WGPU adapter is available")?,
                }
            }
        };
        let features = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("RUR headless device"),
                required_features: features,
                required_limits: wgpu::Limits::default(),
                // Smaller suballocation blocks, frame-sized buffers in allocations of their own: the
                // performance profile's 128-512 MB staging blocks stayed committed after the work that
                // grew them (no measured cost in renders, a render service or a batch)
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                ..Default::default()
            })
            .await
            .context("Creating native rendering device")?;
        Ok(Self {
            device: Arc::new(device),
            queue: Arc::new(queue),
            adapter_info: adapter.get_info(),
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct GpuStats {
    pub cpu_ms: f64,
    /// A GPU timestamp sample that completed since the previous render (an earlier frame's draw)
    pub new_gpu_ms: Option<f64>,
    /// Wall time from a completed earlier submission to the GPU finishing it, queueing included
    pub gpu_completion_ms: Option<f64>,
    /// Most recently completed GPU sample; never waits for the current frame.
    pub gpu_ms: Option<f64>,
    pub draw_calls: u32,
    pub instances: u32,
    pub passes: u32,
    pub upload_bytes: u64,
    pub textures: usize,
    pub bind_group_creations: u32,
    /// Frame-target pixels cleared and drawn again (the whole target, or the changed part)
    pub redrawn_pixels: u64,
    /// Batches drawn from the layer atlas (LayerAtlas::redirect)
    pub atlas_batches: u32,
    /// CPU phases: damage (record identities and matching), records and instance uploads, bind
    /// groups, pass encoding, queue submission
    pub damage_ms: f64,
    pub records_ms: f64,
    pub bind_ms: f64,
    pub encode_ms: f64,
    pub submit_ms: f64,
    pub finish_ms: f64,
}

pub struct RenderedFrame {
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    /// Changes only when the output texture is replaced, including resize.
    pub generation: u64,
    pub stats: GpuStats,
}

struct Texture {
    /// Unique per texture object (make_texture)
    version: u64,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    levels: u32,
    format: wgpu::TextureFormat,
    revision: Option<u64>,
    last_used: u64,
    transient_target: bool,
}

struct TimestampSlot {
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    pending: Option<mpsc::Receiver<std::result::Result<(), String>>>,
}

/// A frame-target draw as encoded last frame: its content identity (record texels, bindings, blend and
/// scissor) and the pixels its quad can touch ([x0, y0, x1, y1), clamped to the target).
#[derive(Clone, Copy, Debug, PartialEq)]
struct DrawnRecord {
    hash: u64,
    area: [u32; 4],
    /// The record's texel block in its packet
    offset: u32,
    len: u32,
    /// Index into the frame's batch keys
    batch: u32,
    /// Samples a texture its packet rewrote: never matches
    fresh: bool,
}

/// A batch's state apart from its records
#[derive(Clone, Copy, Debug, PartialEq, Hash)]
struct BatchKey {
    image_slots: [Option<TextureId>; packet::IMAGE_SLOTS],
    glyphs: Option<TextureId>,
    layer: Option<TextureId>,
    lut: Option<TextureId>,
    blend: BlendMode,
    scissor: Option<[u32; 4]>,
}

/// What the frame target holds: the draws of the last frame encoded into it. A pixel covered only by
/// draws that are unchanged (same content, same relative order) and whose textures were not rewritten
/// keeps its value, so a frame redraws only the area of the draws that differ (every draw is a
/// per-pixel function of its record and the pixel below; the scissor never changes a fragment).
struct FrameHistory {
    generation: u64,
    width: u32,
    height: u32,
    clear: Option<[u8; 4]>,
    records: Vec<DrawnRecord>,
    batches: Vec<BatchKey>,
}

/// The layer atlas: pages of ATLAS_SIZE squared, shelves of 8-row height classes. A layer record (kind
/// LAYER: texel = pixel - quad corner + rec(3).zw) whose texture is placed here samples its page at its
/// place instead; pixels are the same texels read from another texture.
const ATLAS_SIZE: u32 = 2048;
const ATLAS_MAX_LAYER: u32 = 512;
const ATLAS_PAGES: usize = 12;
fn atlas_page_id(page: usize) -> TextureId {
    TextureId(u64::MAX - page as u64)
}
fn is_atlas_page(id: TextureId) -> bool {
    id.0 > u64::MAX - ATLAS_PAGES as u64
}
#[derive(Clone, Copy, Debug)]
struct AtlasPlacement {
    page: usize,
    shelf: usize,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    revision: u64,
}
#[derive(Default)]
struct Shelf {
    y: u32,
    h: u32,
    next_x: u32,
    /// Freed spans (x, width)
    free: Vec<(u32, u32)>,
}
#[derive(Default)]
struct AtlasPage {
    shelves: Vec<Shelf>,
    next_y: u32,
    live: usize,
}
#[derive(Default)]
struct LayerAtlas {
    pages: Vec<AtlasPage>,
    placements: rustc_hash::FxHashMap<TextureId, AtlasPlacement>,
}
impl LayerAtlas {
    fn allocate(&mut self, w: u32, h: u32) -> Option<AtlasPlacement> {
        let class = h.div_ceil(8).max(1) * 8;
        for (page_index, page) in self.pages.iter_mut().enumerate() {
            let mut found = None;
            for (shelf_index, shelf) in page.shelves.iter_mut().enumerate() {
                if shelf.h != class {
                    continue;
                }
                if let Some(k) = shelf.free.iter().position(|&(_, fw)| fw >= w) {
                    let (fx, fw) = shelf.free[k];
                    if fw == w {
                        shelf.free.swap_remove(k);
                    } else {
                        shelf.free[k] = (fx + w, fw - w);
                    }
                    found = Some((shelf_index, fx, shelf.y));
                    break;
                }
                if shelf.next_x + w <= ATLAS_SIZE {
                    let x = shelf.next_x;
                    shelf.next_x += w;
                    found = Some((shelf_index, x, shelf.y));
                    break;
                }
            }
            if found.is_none() && page.next_y + class <= ATLAS_SIZE {
                page.shelves.push(Shelf {
                    y: page.next_y,
                    h: class,
                    next_x: w,
                    free: Vec::new(),
                });
                found = Some((page.shelves.len() - 1, 0, page.next_y));
                page.next_y += class;
            }
            if let Some((shelf, x, y)) = found {
                page.live += 1;
                return Some(AtlasPlacement {
                    page: page_index,
                    shelf,
                    x,
                    y,
                    w,
                    h,
                    revision: 0,
                });
            }
        }
        None
    }
    fn remove(&mut self, id: TextureId) {
        let Some(p) = self.placements.remove(&id) else {
            return;
        };
        let page = &mut self.pages[p.page];
        page.live -= 1;
        if page.live == 0 {
            page.shelves.clear();
            page.next_y = 0;
        } else {
            page.shelves[p.shelf].free.push((p.x, p.w));
        }
    }
    fn clear(&mut self) {
        self.placements.clear();
        for page in &mut self.pages {
            *page = AtlasPage::default();
        }
    }
    /// The atlas place for a batch that draws only whole-pixel layer records of its one texture, each
    /// quad the texture's size, with no other texture bound
    fn redirect(&self, batch: &DrawBatch, texels: &[[f32; 4]]) -> Option<(usize, [u32; 2])> {
        let id = batch.image_slots[0]?;
        if batch.image_slots[1..].iter().any(Option::is_some)
            || batch.glyphs.is_some()
            || batch.layer.is_some()
            || batch.lut.is_some()
        {
            return None;
        }
        let p = self.placements.get(&id)?;
        let fits = batch.record_offsets.iter().all(|&o| {
            let o = o as usize;
            let b = texels[o];
            texels[o + 2][0] == 18.0
                && texels[o + 3] == [0.0; 4]
                && b.iter().all(|v| v.fract() == 0.0 && v.abs() < 16_777_216.0)
                && b[2] - b[0] == p.w as f32
                && b[3] - b[1] == p.h as f32
        });
        fits.then_some((p.page, [p.x, p.y]))
    }
}
/// Records drawn together: the bindings after atlas redirection, the range of their instances
struct Draw {
    state: DrawBatch,
    range: std::ops::Range<u32>,
    binding: Option<wgpu::BindGroup>,
}
/// A draw being gathered: its state, whether every record in it samples no texture, its records and
/// the union of their pixel boxes (frame pass)
struct Group {
    state: DrawBatch,
    bare: bool,
    offsets: Vec<u32>,
    area: [u32; 4],
}
fn same_state(a: &DrawBatch, b: &DrawBatch) -> bool {
    a.image_slots == b.image_slots
        && a.glyphs == b.glyphs
        && a.layer == b.layer
        && a.lut == b.lut
        && a.blend == b.blend
        && a.scissor == b.scissor
}

/// The part of the frame target a frame redraws
#[derive(Clone, Debug, PartialEq)]
enum Damage {
    /// Cleared and drawn completely
    Full,
    /// Nothing changed
    None,
    /// Cleared to the pass colour inside this box [x0, y0, x1, y1) and redrawn there
    Rect([u32; 4]),
}

pub struct GpuRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    premultiplied: wgpu::RenderPipeline,
    image: wgpu::RenderPipeline,
    /// Writes the pass clear colour inside the scissor (a partial redraw's clear)
    clear: wgpu::RenderPipeline,
    clear_uniforms: wgpu::Buffer,
    clear_binding: wgpu::BindGroup,
    history: Option<FrameHistory>,
    /// Texels the record texture holds (rows written so far), to upload only changed rows
    data_valid: usize,
    texture_layout: wgpu::BindGroupLayout,
    target_layout: wgpu::BindGroupLayout,
    target_uniforms: wgpu::Buffer,
    target_binding: wgpu::BindGroup,
    uniform_capacity: u64,
    uniform_stride: u64,
    corners: wgpu::Buffer,
    instances: wgpu::Buffer,
    instance_capacity: u64,
    data: Texture,
    sampler: wgpu::Sampler,
    repeat_sampler: wgpu::Sampler,
    dummy: Texture,
    default_lut: Texture,
    textures: rustc_hash::FxHashMap<TextureId, Texture>,
    /// Bind groups by the texture objects they bind (Texture::version of the record data and the
    /// 11 sampled slots): a replaced texture never matches its predecessor's groups
    bindings: rustc_hash::FxHashMap<[u64; 12], (wgpu::BindGroup, u64)>,
    /// GPU timestamps are taken every TIMESTAMP_INTERVAL frames (lastCompletedGpuMs)
    timestamp_countdown: u32,
    /// Frames between timestamp samples (TIMESTAMP_INTERVAL; the desktop app samples every frame)
    timestamp_interval: u32,
    /// A sample completed since the last render (reported once, as GpuStats::new_gpu_ms)
    fresh_gpu_ms: Option<f64>,
    /// Wall time from the last completed submission to the GPU finishing it (microseconds, 0 = none
    /// yet): queueing behind other GPU work included, unlike a timestamp duration
    completion: Arc<std::sync::atomic::AtomicU64>,
    record_staging: Vec<[f32; 4]>,
    instance_staging: Vec<u32>,
    /// The packet's records with atlas placements written in (what the record texture holds)
    patched_staging: Vec<[f32; 4]>,
    /// The last frame's records as the preparer made them (frame_damage compares against these)
    prev_texels: Vec<[f32; 4]>,
    /// Small straight RGBA8 layers, packed into shared textures so their draws share bindings
    atlas: LayerAtlas,
    uniform_staging: Vec<u8>,
    frame_serial: u64,
    generation: u64,
    timestamps: Option<wgpu::QuerySet>,
    timestamp_slots: Vec<TimestampSlot>,
    last_gpu_ms: Option<f64>,
    adapter_info: Option<wgpu::AdapterInfo>,
    /// The buffer frames are read back through, kept between readbacks (a frame's worth of mappable
    /// memory allocated and freed for every PNG was most of the readback's time); released when idle
    readback: std::sync::Mutex<Option<wgpu::Buffer>>,
    /// Buffers of [`Self::begin_readback`] waiting for their next frame (a sequence keeps two in flight)
    readback_pool: std::sync::Mutex<Vec<wgpu::Buffer>>,
}

impl GpuRenderer {
    pub async fn headless() -> Result<Self> {
        let gpu = HeadlessGpu::new().await?;
        let mut renderer = Self::new(gpu.device, gpu.queue)?;
        renderer.adapter_info = Some(gpu.adapter_info);
        Ok(renderer)
    }

    /// Take a GPU timestamp sample (and a completion-latency sample) every `frames` drawn frames
    pub fn set_timestamp_interval(&mut self, frames: u32) {
        self.timestamp_interval = frames.max(1);
        self.timestamp_countdown = self.timestamp_countdown.min(self.timestamp_interval - 1);
    }
    pub fn adapter_info(&self) -> Option<&wgpu::AdapterInfo> {
        self.adapter_info.as_ref()
    }

    /// For hosts that create their own device (the web build's canvas adapter)
    pub fn set_adapter_info(&mut self, info: wgpu::AdapterInfo) {
        self.adapter_info = Some(info);
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn wait_idle(&self) -> Result<()> {
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .context("Waiting for GPU work")?;
        Ok(())
    }

    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Result<Self> {
        if device.limits().max_sampled_textures_per_shader_stage < 12 {
            bail!("The adapter needs at least 12 sampled textures per shader stage");
        }
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let vertex = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("RUR calibrated vertex"),
            source: wgpu::ShaderSource::Glsl {
                shader: Cow::Borrowed(include_str!("shaders/uber.vert.glsl")),
                stage: wgpu::naga::ShaderStage::Vertex,
                defines: Default::default(),
            },
        });
        let fragment = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("RUR calibrated fragment"),
            source: wgpu::ShaderSource::Glsl {
                shader: Cow::Borrowed(include_str!("shaders/uber.frag.glsl")),
                stage: wgpu::naga::ShaderStage::Fragment,
                defines: Default::default(),
            },
        });
        let target_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("RUR target uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(16),
                },
                count: None,
            }],
        });
        let mut texture_entries: Vec<_> = (0..12)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: if binding == 0 {
                    wgpu::ShaderStages::VERTEX_FRAGMENT
                } else {
                    wgpu::ShaderStages::FRAGMENT
                },
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            })
            .collect();
        texture_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 12,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
            count: None,
        });
        // The same point sampling with repeating coordinates: a tiled Pixelated image (uber.frag.glsl
        // hardwareTexel)
        texture_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 13,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
            count: None,
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("RUR record and pixel textures"),
            entries: &texture_entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("RUR draw layout"),
            bind_group_layouts: &[&target_layout, &texture_layout],
            push_constant_ranges: &[],
        });
        let pipeline = |image: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(if image {
                    "RUR straight image"
                } else {
                    "RUR premultiplied"
                }),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &vertex,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    buffers: &[
                        wgpu::VertexBufferLayout {
                            array_stride: 4,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &[wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Uint32,
                                offset: 0,
                                shader_location: 0,
                            }],
                        },
                        wgpu::VertexBufferLayout {
                            array_stride: 8,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &[wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 0,
                                shader_location: 1,
                            }],
                        },
                    ],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &fragment,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: OUTPUT_FORMAT,
                        blend: Some(wgpu::BlendState {
                            color: wgpu::BlendComponent {
                                src_factor: if image {
                                    wgpu::BlendFactor::SrcAlpha
                                } else {
                                    wgpu::BlendFactor::One
                                },
                                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                                operation: wgpu::BlendOperation::Add,
                            },
                            alpha: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::One,
                                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                                operation: wgpu::BlendOperation::Add,
                            },
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            })
        };
        let premultiplied = pipeline(false);
        let image = pipeline(true);
        let clear_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("RUR partial clear colour"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(16),
                },
                count: None,
            }],
        });
        let clear_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("RUR partial clear"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(CLEAR_WGSL)),
        });
        let clear_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("RUR partial clear layout"),
            bind_group_layouts: &[&clear_layout],
            push_constant_ranges: &[],
        });
        let clear = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("RUR partial clear"),
            layout: Some(&clear_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &clear_module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &clear_module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: OUTPUT_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        let clear_uniforms = stream_buffer(&device, "RUR partial clear colour", 16, wgpu::BufferUsages::UNIFORM);
        let clear_binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("RUR partial clear colour"),
            layout: &clear_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: clear_uniforms.as_entire_binding(),
            }],
        });
        if let Some(error) = pollster::block_on(device.pop_error_scope()) {
            bail!("Native GPU shader/pipeline validation failed: {error}");
        }
        let corners = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("RUR quad corners"),
            contents: bytemuck::cast_slice(&[[0.0f32, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]]),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let instance_capacity = 4096;
        let instances = stream_buffer(
            &device,
            "RUR record offsets",
            instance_capacity,
            wgpu::BufferUsages::VERTEX,
        );
        let uniform_stride = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(16);
        let uniform_capacity = uniform_stride * 16;
        let target_uniforms = stream_buffer(
            &device,
            "RUR target adjustments",
            uniform_capacity,
            wgpu::BufferUsages::UNIFORM,
        );
        let target_binding = uniform_binding(&device, &target_layout, &target_uniforms);
        let data = make_texture(
            &device,
            "RUR record data",
            DATA_WIDTH,
            16,
            1,
            wgpu::TextureFormat::Rgba32Float,
        );
        let dummy = make_texture(&device, "RUR transparent fallback", 1, 1, 1, OUTPUT_FORMAT);
        queue.write_texture(
            dummy.texture.as_image_copy(),
            &[0, 0, 0, 0],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            extent(1, 1),
        );
        let default_lut = make_texture(
            &device,
            "RUR calibrated integer lookup",
            256,
            1024,
            1,
            OUTPUT_FORMAT,
        );
        queue.write_texture(
            default_lut.texture.as_image_copy(),
            &default_lut_bytes(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1024),
                rows_per_image: Some(1024),
            },
            extent(256, 1024),
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("RUR integer texture fetch"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let repeat_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("RUR repeating point sampling"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let timestamps = device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY)
            .then(|| {
                device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("RUR asynchronous frame timing"),
                    ty: wgpu::QueryType::Timestamp,
                    count: 2,
                })
            });
        let timestamp_slots = if timestamps.is_some() {
            (0..6)
                .map(|_| TimestampSlot {
                    resolve: device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("RUR timestamp resolve"),
                        size: 256,
                        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                        mapped_at_creation: false,
                    }),
                    read: device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("RUR timestamp readback"),
                        size: 16,
                        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                    pending: None,
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(Self {
            device,
            queue,
            premultiplied,
            image,
            clear,
            clear_uniforms,
            clear_binding,
            history: None,
            data_valid: 0,
            texture_layout,
            target_layout,
            target_uniforms,
            target_binding,
            uniform_capacity,
            uniform_stride,
            corners,
            instances,
            instance_capacity,
            data,
            sampler,
            repeat_sampler,
            dummy,
            default_lut,
            textures: Default::default(),
            bindings: Default::default(),
            timestamp_countdown: 0,
            timestamp_interval: TIMESTAMP_INTERVAL,
            fresh_gpu_ms: None,
            completion: Default::default(),
            record_staging: Vec::new(),
            instance_staging: Vec::new(),
            patched_staging: Vec::new(),
            prev_texels: Vec::new(),
            atlas: LayerAtlas::default(),
            uniform_staging: Vec::new(),
            frame_serial: 0,
            generation: 0,
            timestamps,
            timestamp_slots,
            last_gpu_ms: None,
            adapter_info: None,
            readback: Default::default(),
            readback_pool: Default::default(),
        })
    }

    pub fn render(&mut self, packet: &RenderPacket) -> Result<RenderedFrame> {
        let started = Instant::now();
        let _render_scope = crate::microprofile::scope("GPU render");
        let _memory = crate::memtrace::scope(crate::memtrace::Tag::Gpu);
        validate_extent(&self.device, packet.width, packet.height)?;
        // A failed frame leaves the target in an unknown state: the next one redraws it completely
        let history = self.history.take();
        if packet.unchanged {
            let history = history.context("An unchanged packet needs the frame drawn before it")?;
            if history.generation != self.generation
                || history.width != packet.width
                || history.height != packet.height
            {
                bail!("An unchanged packet needs the frame drawn before it");
            }
            for id in &packet.retire_textures {
                if *id != FRAME_TARGET {
                    self.textures.remove(id);
                    self.atlas.remove(*id);
                }
            }
            let target = self
                .textures
                .get(&FRAME_TARGET)
                .context("An unchanged packet needs the frame drawn before it")?;
            let frame = RenderedFrame {
                view: target.view.clone(),
                width: packet.width,
                height: packet.height,
                generation: self.generation,
                stats: GpuStats {
                    gpu_ms: self.last_gpu_ms,
                    textures: self.textures.len() + 3,
                    cpu_ms: started.elapsed().as_secs_f64() * 1000.0,
                    ..Default::default()
                },
            };
            self.history = Some(history);
            return Ok(frame);
        }
        self.poll_timestamps();
        self.frame_serial += 1;
        self.bindings
            .retain(|_, (_, used)| *used + 1 >= self.frame_serial);
        let mut stats = GpuStats::default();
        for id in &packet.retire_textures {
            if *id != FRAME_TARGET {
                self.textures.remove(id);
                self.atlas.remove(*id);
            }
        }
        self.ensure_target(FRAME_TARGET, packet.width, packet.height)?;
        let upload_scope = crate::microprofile::scope_detail("Upload textures", packet.textures.len() as u64);
        for upload in &packet.textures {
            stats.upload_bytes += self.upload_texture(upload)?;
        }
        drop(upload_scope);
        for pass in &packet.passes {
            self.ensure_target(pass.target, pass.width, pass.height)?;
        }
        for pass in &packet.passes {
            for copy in &pass.copies_after {
                for id in [copy.source, copy.destination] {
                    self.textures
                        .get_mut(&id)
                        .with_context(|| format!("Missing copy texture {}", id.0))?
                        .last_used = self.frame_serial;
                }
            }
        }
        for pass in &packet.passes {
            for batch in &pass.batches {
                for &offset in &batch.record_offsets {
                    if offset as usize + 3 >= packet.texels.len() {
                        bail!("Record offset {offset} is outside the draw packet");
                    }
                }
            }
        }
        let phase = Instant::now();
        let damage_scope = crate::microprofile::scope("Frame damage");
        // Partial redraws need the frame drawn as one pass
        let frame_pass = {
            let mut frame_passes = packet
                .passes
                .iter()
                .enumerate()
                .filter(|(_, p)| p.target == FRAME_TARGET);
            match (frame_passes.next(), frame_passes.next()) {
                (Some((index, _)), None) => Some(index),
                _ => None,
            }
        };
        let (drawn, drawn_batches, damage) = match frame_pass {
            Some(index) => {
                let (drawn, keys) = drawn_records(packet, index);
                let pass = &packet.passes[index];
                let damage = match &history {
                    Some(h)
                        if h.generation == self.generation
                            && h.width == pass.width
                            && h.height == pass.height
                            && h.clear == pass.clear =>
                    {
                        frame_damage(h, &self.prev_texels, &drawn, &keys, &packet.texels, pass.width, pass.height)
                    }
                    _ => Damage::Full,
                };
                (drawn, keys, damage)
            }
            None => (Vec::new(), Vec::new(), Damage::Full),
        };
        drop(damage_scope);
        stats.damage_ms = phase.elapsed().as_secs_f64() * 1000.0;
        let phase = Instant::now();

        let rows = (packet.texels.len() as u32).div_ceil(DATA_WIDTH).max(1);
        if rows > self.device.limits().max_texture_dimension_2d {
            bail!("Draw records exceed GPU texture capacity");
        }
        if rows > self.data.height {
            self.data = make_texture(
                &self.device,
                "RUR record data",
                DATA_WIDTH,
                rows.next_power_of_two()
                    .min(self.device.limits().max_texture_dimension_2d),
                1,
                wgpu::TextureFormat::Rgba32Float,
            );
            self.data_valid = 0;
        }

        // Draws: each pass's batches with layers moved to the atlas (their records sampling their atlas
        // region), consecutive batches with the same state drawn together
        let mut texels = std::mem::take(&mut self.patched_staging);
        texels.clear();
        texels.extend_from_slice(&packet.texels);
        let mut offsets = std::mem::take(&mut self.instance_staging);
        offsets.clear();
        let mut draws: Vec<Vec<Draw>> = Vec::with_capacity(packet.passes.len());
        let mut materialize: Vec<TextureId> = Vec::new();
        let mut drawn_index = 0;
        for (pass_index, pass) in packet.passes.iter().enumerate() {
            let frame = Some(pass_index) == frame_pass;
            // The frame's draws: each record joins the latest earlier group of its state when it overlaps
            // no record of the groups after that one (records that share no pixel draw the same in either
            // order); other passes merge consecutive batches only
            let mut groups: Vec<Group> = Vec::new();
            for batch in &pass.batches {
                let mut state = DrawBatch {
                    record_offsets: Vec::new(),
                    image_slots: batch.image_slots,
                    glyphs: batch.glyphs,
                    layer: batch.layer,
                    lut: batch.lut,
                    blend: batch.blend,
                    scissor: batch.scissor,
                };
                match self.atlas.redirect(batch, &packet.texels) {
                    Some((page, [x, y])) => {
                        for &offset in &batch.record_offsets {
                            let at = &mut texels[offset as usize + 3];
                            at[2] = x as f32;
                            at[3] = y as f32;
                        }
                        state.image_slots[0] = Some(atlas_page_id(page));
                        stats.atlas_batches += 1;
                    }
                    None => {
                        for id in batch
                            .image_slots
                            .iter()
                            .copied()
                            .chain([batch.glyphs, batch.layer, batch.lut])
                            .flatten()
                        {
                            if !self.textures.contains_key(&id)
                                && self.atlas.placements.contains_key(&id)
                                && !materialize.contains(&id)
                            {
                                materialize.push(id);
                            }
                        }
                    }
                }
                // A batch sampling no texture (solid, gradient and stroke kinds read only the records and the
                // lookup table) draws the same under any other textures
                let bare = state.image_slots.iter().all(Option::is_none)
                    && state.glyphs.is_none()
                    && state.layer.is_none()
                    && batch
                        .record_offsets
                        .iter()
                        .all(|&o| matches!(packet.texels[o as usize + 2][0], 1.0 | 3.0 | 5.0));
                let join = |group: &Group| {
                    group.state.blend == state.blend
                        && group.state.scissor == state.scissor
                        && group.state.lut == state.lut
                        && (bare || group.bare || same_state(&group.state, &state))
                };
                for (k, &offset) in batch.record_offsets.iter().enumerate() {
                    let area = if frame {
                        let a = drawn[drawn_index + k].area;
                        // Only draws whose quads meet the redrawn box
                        let drawn_now = match damage {
                            Damage::Full => true,
                            Damage::None => false,
                            Damage::Rect(d) => a[0] < d[2] && a[2] > d[0] && a[1] < d[3] && a[3] > d[1],
                        };
                        if !drawn_now {
                            continue;
                        }
                        Some(a)
                    } else {
                        None
                    };
                    let mut target = None;
                    match area {
                        Some(a) => {
                            for (index, group) in groups.iter().enumerate().rev() {
                                if join(group) {
                                    target = Some(index);
                                    break;
                                }
                                let g = group.area;
                                if a[0] < g[2] && a[2] > g[0] && a[1] < g[3] && a[3] > g[1] {
                                    break;
                                }
                            }
                        }
                        None => {
                            if groups.last().is_some_and(|group| join(group)) {
                                target = Some(groups.len() - 1);
                            }
                        }
                    }
                    let a = area.unwrap_or([0; 4]);
                    match target {
                        Some(index) => {
                            let group = &mut groups[index];
                            if group.bare && !bare {
                                group.state = DrawBatch {
                                    record_offsets: Vec::new(),
                                    ..state.clone()
                                };
                                group.bare = false;
                            }
                            group.offsets.push(offset);
                            let g = &mut group.area;
                            *g = [g[0].min(a[0]), g[1].min(a[1]), g[2].max(a[2]), g[3].max(a[3])];
                        }
                        None => groups.push(Group {
                            state: DrawBatch {
                                record_offsets: Vec::new(),
                                ..state.clone()
                            },
                            bare,
                            offsets: vec![offset],
                            area: a,
                        }),
                    }
                }
                if frame {
                    drawn_index += batch.record_offsets.len();
                }
            }
            let mut list = Vec::with_capacity(groups.len());
            for group in groups {
                let start = offsets.len() as u32;
                offsets.extend_from_slice(&group.offsets);
                list.push(Draw {
                    state: group.state,
                    range: start..offsets.len() as u32,
                    binding: None,
                });
            }
            draws.push(list);
        }
        // Atlas layers sampled by a batch that cannot use the atlas get a texture of their own
        let mut materialized = Vec::new();
        for id in materialize {
            let placement = self.atlas.placements[&id];
            let texture = make_texture(
                &self.device,
                "RUR sampled pixels (from the layer atlas)",
                placement.w,
                placement.h,
                1,
                OUTPUT_FORMAT,
            );
            let mut texture = texture;
            texture.revision = Some(placement.revision);
            self.textures.insert(id, texture);
            materialized.push((id, placement));
        }
        stats.upload_bytes += self.upload_records(&texels);
        self.patched_staging = texels;
        let instance_bytes = (offsets.len() as u64 * 4).max(4);
        if instance_bytes > self.instance_capacity {
            self.instance_capacity = instance_bytes.next_power_of_two();
            self.instances = stream_buffer(
                &self.device,
                "RUR record offsets",
                self.instance_capacity,
                wgpu::BufferUsages::VERTEX,
            );
        }
        if !offsets.is_empty() {
            self.queue
                .write_buffer(&self.instances, 0, bytemuck::cast_slice(&offsets));
        }
        stats.upload_bytes += offsets.len() as u64 * 4;
        let uniform_bytes = (packet.passes.len().max(1) as u64) * self.uniform_stride;
        if uniform_bytes > self.uniform_capacity {
            self.uniform_capacity = uniform_bytes.next_power_of_two();
            self.target_uniforms = stream_buffer(
                &self.device,
                "RUR target adjustments",
                self.uniform_capacity,
                wgpu::BufferUsages::UNIFORM,
            );
            self.target_binding =
                uniform_binding(&self.device, &self.target_layout, &self.target_uniforms);
            self.uniform_staging.clear();
        }
        let mut adjustments = std::mem::take(&mut self.uniform_staging);
        let previous = adjustments.clone();
        adjustments.resize(uniform_bytes as usize, 0);
        for (i, pass) in packet.passes.iter().enumerate() {
            adjustments[i * self.uniform_stride as usize..i * self.uniform_stride as usize + 16]
                .copy_from_slice(bytemuck::cast_slice(&[
                    2.0 / pass.width as f32,
                    -1.0,
                    2.0 / pass.height as f32,
                    -1.0,
                ]));
        }
        if adjustments != previous {
            self.queue
                .write_buffer(&self.target_uniforms, 0, &adjustments);
            stats.upload_bytes += uniform_bytes;
        }
        if let (Damage::Rect(_), Some(index)) = (&damage, frame_pass) {
            let c = packet.passes[index].clear.unwrap_or([0; 4]);
            let colour: [f32; 4] = c.map(|v| v as f32 / 255.0);
            self.queue
                .write_buffer(&self.clear_uniforms, 0, bytemuck::cast_slice(&colour));
        }
        stats.records_ms = phase.elapsed().as_secs_f64() * 1000.0;
        let phase = Instant::now();
        let bind_scope = crate::microprofile::scope("Bind groups");
        for pass in &packet.passes {
            for batch in &pass.batches {
                self.touch_batch(batch, pass.target)?;
            }
        }
        for list in &mut draws {
            for draw in list.iter_mut() {
                let (binding, created) = self.batch_binding(&draw.state)?;
                draw.binding = Some(binding);
                stats.bind_group_creations += u32::from(created);
            }
        }
        drop(bind_scope);
        stats.bind_ms = phase.elapsed().as_secs_f64() * 1000.0;
        let phase = Instant::now();
        let encode_scope = crate::microprofile::scope_detail("Encode passes", packet.passes.len() as u64);
        // Every intermediate target is drawn; the frame unless nothing in it changed
        let encoded: Vec<bool> = (0..packet.passes.len())
            .map(|i| Some(i) != frame_pass || damage != Damage::None)
            .collect();
        let first_encoded = encoded.iter().position(|e| *e);
        let last_encoded = encoded.iter().rposition(|e| *e);
        // A timestamp query costs a resolve, a copy and a mapping: sampled, not taken every frame
        let sample = first_encoded.is_some() && self.timestamp_countdown == 0;
        if first_encoded.is_some() {
            self.timestamp_countdown = if sample { self.timestamp_interval - 1 } else { self.timestamp_countdown - 1 };
        }
        let timestamp_slot = if !sample {
            None
        } else {
            self.timestamp_slots
                .iter()
                .position(|slot| slot.pending.is_none())
        };
        let copies = packet.passes.iter().any(|pass| !pass.copies_after.is_empty());
        if first_encoded.is_some() || copies || !materialized.is_empty() {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("RUR offscreen frame"),
                });
            for (id, placement) in &materialized {
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.textures[&atlas_page_id(placement.page)].texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: placement.x,
                            y: placement.y,
                            z: 0,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    self.textures[id].texture.as_image_copy(),
                    extent(placement.w, placement.h),
                );
            }
            for (pass_index, pass) in packet.passes.iter().enumerate() {
                if !encoded[pass_index] {
                    for copy in &pass.copies_after {
                        self.encode_copy(&mut encoder, copy)?;
                    }
                    continue;
                }
                let target = &self.textures[&pass.target];
                let timestamp_writes =
                    timestamp_slot
                        .and(self.timestamps.as_ref())
                        .and_then(|query_set| {
                            let beginning = (Some(pass_index) == first_encoded).then_some(0);
                            let end = (Some(pass_index) == last_encoded).then_some(1);
                            (beginning.is_some() || end.is_some()).then_some(
                                wgpu::RenderPassTimestampWrites {
                                    query_set,
                                    beginning_of_pass_write_index: beginning,
                                    end_of_pass_write_index: end,
                                },
                            )
                        });
                let partial = match (&damage, Some(pass_index) == frame_pass) {
                    (Damage::Rect(d), true) => Some(*d),
                    _ => None,
                };
                {
                    let attachment = Some(wgpu::RenderPassColorAttachment {
                        view: &target.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: match (partial, pass.clear) {
                                (None, Some(c)) => wgpu::LoadOp::Clear(wgpu::Color {
                                    r: c[0] as f64 / 255.0,
                                    g: c[1] as f64 / 255.0,
                                    b: c[2] as f64 / 255.0,
                                    a: c[3] as f64 / 255.0,
                                }),
                                _ => wgpu::LoadOp::Load,
                            },
                            store: wgpu::StoreOp::Store,
                        },
                    });
                    let mut render = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("RUR CanvasGroup or frame"),
                        color_attachments: &[attachment],
                        depth_stencil_attachment: None,
                        timestamp_writes,
                        occlusion_query_set: None,
                    });
                    if let Some(d) = partial {
                        // The redrawn box starts from the pass colour, as a whole frame starts from its clear
                        if pass.clear.is_some() {
                            render.set_pipeline(&self.clear);
                            render.set_bind_group(0, &self.clear_binding, &[]);
                            render.set_scissor_rect(d[0], d[1], d[2] - d[0], d[3] - d[1]);
                            render.draw(0..3, 0..1);
                            stats.draw_calls += 1;
                        }
                        stats.redrawn_pixels += u64::from(d[2] - d[0]) * u64::from(d[3] - d[1]);
                    } else if Some(pass_index) == frame_pass {
                        stats.redrawn_pixels += u64::from(pass.width) * u64::from(pass.height);
                    }
                    render.set_bind_group(
                        0,
                        &self.target_binding,
                        &[(pass_index as u64 * self.uniform_stride) as u32],
                    );
                    render.set_vertex_buffer(0, self.instances.slice(..));
                    render.set_vertex_buffer(1, self.corners.slice(..));
                    let mut image_pipeline = None;
                    let mut texture_binding = None;
                    for draw in &draws[pass_index] {
                        let range = draw.range.clone();
                        let binding = draw.binding.as_ref().expect("bound above");
                        let image = draw.state.blend == BlendMode::Image;
                        if image_pipeline != Some(image) {
                            render.set_pipeline(if image {
                                &self.image
                            } else {
                                &self.premultiplied
                            });
                            image_pipeline = Some(image);
                        }
                        if texture_binding != Some(binding) {
                            render.set_bind_group(1, binding, &[]);
                            texture_binding = Some(binding);
                        }
                        let [x, y, w, h] = draw.state.scissor.unwrap_or([0, 0, pass.width, pass.height]);
                        if x > pass.width
                            || y > pass.height
                            || w > pass.width - x
                            || h > pass.height - y
                        {
                            bail!("Scissor exceeds render target");
                        }
                        let (x, y, w, h) = match partial {
                            Some(d) => {
                                let x0 = x.max(d[0]);
                                let y0 = y.max(d[1]);
                                let x1 = (x + w).min(d[2]);
                                let y1 = (y + h).min(d[3]);
                                (x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
                            }
                            None => (x, y, w, h),
                        };
                        if w > 0 && h > 0 {
                            render.set_scissor_rect(x, y, w, h);
                            render.draw(0..4, range.clone());
                            stats.draw_calls += 1;
                            stats.instances += range.end - range.start;
                        }
                    }
                }
                for copy in &pass.copies_after {
                    self.encode_copy(&mut encoder, copy)?;
                }
            }
            if let Some(index) = timestamp_slot {
                let slot = &self.timestamp_slots[index];
                encoder.resolve_query_set(self.timestamps.as_ref().unwrap(), 0..2, &slot.resolve, 0);
                encoder.copy_buffer_to_buffer(&slot.resolve, 0, &slot.read, 0, 16);
            }
            drop(encode_scope);
            stats.encode_ms = phase.elapsed().as_secs_f64() * 1000.0;
            let phase = Instant::now();
            let submit_scope = crate::microprofile::scope("Queue submit");
            let finished = encoder.finish();
            stats.finish_ms = phase.elapsed().as_secs_f64() * 1000.0;
            self.queue.submit([finished]);
            drop(submit_scope);
            stats.submit_ms = phase.elapsed().as_secs_f64() * 1000.0;
            if timestamp_slot.is_some() {
                // Submission to completion on the wall clock (the callback runs in a later poll): the
                // time other GPU work (another application) keeps this frame waiting shows here, not
                // in the timestamp duration
                let submitted = Instant::now();
                let completion = self.completion.clone();
                self.queue.on_submitted_work_done(move || {
                    let micros = (submitted.elapsed().as_micros() as u64).max(1);
                    completion.store(micros, std::sync::atomic::Ordering::Relaxed);
                });
            }
            if let Some(index) = timestamp_slot {
                let slot = &mut self.timestamp_slots[index];
                let (tx, rx) = mpsc::channel();
                slot.read
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        let _ = tx.send(result.map_err(|e| e.to_string()));
                    });
                slot.pending = Some(rx);
            }
        }
        // The records the frame target was drawn from, for the next frame's damage (as the preparer made
        // them: atlas placements are the renderer's)
        self.prev_texels.clear();
        self.prev_texels.extend_from_slice(&packet.texels);
        // Uploads are incremental and retired by the CPU cache. Intermediate render targets
        // are packet-local, so a target absent from the current dependency graph can be freed.
        self.textures.retain(|id, texture| {
            *id == FRAME_TARGET
                || !texture.transient_target
                || texture.last_used == self.frame_serial
        });
        self.bindings
            .retain(|_, (_, used)| *used == self.frame_serial);
        self.instance_staging = offsets;
        self.uniform_staging = adjustments;
        let target = &self.textures[&FRAME_TARGET];
        self.history = frame_pass.map(|index| FrameHistory {
            generation: self.generation,
            width: packet.passes[index].width,
            height: packet.passes[index].height,
            clear: packet.passes[index].clear,
            records: drawn,
            batches: drawn_batches,
        });
        stats.passes = packet.passes.len() as u32;
        stats.textures = self.textures.len() + 3;
        stats.gpu_ms = self.last_gpu_ms;
        stats.new_gpu_ms = self.fresh_gpu_ms.take();
        stats.gpu_completion_ms = match self.completion.swap(0, std::sync::atomic::Ordering::Relaxed) {
            0 => None,
            micros => Some(micros as f64 / 1000.0),
        };
        stats.cpu_ms = started.elapsed().as_secs_f64() * 1000.0;
        Ok(RenderedFrame {
            view: target.view.clone(),
            width: packet.width,
            height: packet.height,
            generation: self.generation,
            stats,
        })
    }

    /// Writes the packet's records into the record texture: only the rows from the first to the last
    /// texel that differ from what the texture holds (record_staging mirrors its rows).
    fn upload_records(&mut self, texels: &[[f32; 4]]) -> u64 {
        let _scope = crate::microprofile::scope_detail("Upload records", texels.len() as u64);
        let n = texels.len();
        let held = self.data_valid.min(self.record_staging.len());
        let common = held.min(n);
        let bits = |v: &[f32; 4]| bytemuck::cast::<[f32; 4], u128>(*v);
        let first = (0..common)
            .find(|&i| bits(&self.record_staging[i]) != bits(&texels[i]))
            .unwrap_or(common);
        let last = if n > common {
            n
        } else {
            (first..common)
                .rev()
                .find(|&i| bits(&self.record_staging[i]) != bits(&texels[i]))
                .map_or(first, |i| i + 1)
        };
        if last <= first {
            return 0;
        }
        let width = DATA_WIDTH as usize;
        let row_start = first / width;
        let row_end = last.div_ceil(width);
        let (start, end) = (row_start * width, row_end * width);
        if self.record_staging.len() < end {
            self.record_staging.resize(end, [0.0; 4]);
        }
        self.record_staging[first..last].copy_from_slice(&texels[first..last]);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.data.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: row_start as u32,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&self.record_staging[start..end]),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(DATA_WIDTH * 16),
                rows_per_image: Some((row_end - row_start) as u32),
            },
            extent(DATA_WIDTH, (row_end - row_start) as u32),
        );
        self.data_valid = self.data_valid.max(end);
        ((end - start) * 16) as u64
    }

    /// Explicit export/test path. Interactive rendering never calls this or waits for mapping.
    pub fn readback_rgba(&self) -> Result<Vec<u8>> {
        self.readback_with(None, |_| {})
    }

    /// The frame as straight-alpha RGBA (what render.png holds): each premultiplied pixel divided by its
    /// alpha, rounded, as it is copied out
    pub fn readback_png_rgba(&self) -> Result<Vec<u8>> {
        self.readback_png_rgba_region(None)
    }

    /// [`Self::readback_png_rgba`] of a region `[x, y, width, height]` of the frame (physical pixels, inside
    /// it): only those rows and columns are copied out of the GPU (a sequence cropped to one node)
    pub fn readback_png_rgba_region(&self, region: Option<[u32; 4]>) -> Result<Vec<u8>> {
        let table = unpremultiply_table();
        self.readback_with(region, |row| {
            for pixel in row.chunks_exact_mut(4) {
                let divide = &table[pixel[3] as usize];
                pixel[0] = divide[pixel[0] as usize];
                pixel[1] = divide[pixel[1] as usize];
                pixel[2] = divide[pixel[2] as usize];
            }
        })
    }

    /// The frame target's pixels (all of them, or `region`), each row passed through `convert` as it is
    /// copied out of the mapped readback buffer (reused while it is large enough)
    fn readback_with(&self, region: Option<[u32; 4]>, mut convert: impl FnMut(&mut [u8])) -> Result<Vec<u8>> {
        let target = self
            .textures
            .get(&FRAME_TARGET)
            .context("No frame has been rendered")?;
        let [x, y, width, height] = region.unwrap_or([0, 0, target.width, target.height]);
        if width == 0 || height == 0 || x.checked_add(width).is_none_or(|end| end > target.width) || y.checked_add(height).is_none_or(|end| end > target.height) {
            bail!("readback region {x},{y} {width}x{height} is not inside the {}x{} frame", target.width, target.height);
        }
        let row_bytes = width * 4;
        let padded = row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let size = u64::from(padded) * u64::from(height);
        let mut slot = self.readback.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if slot.as_ref().is_none_or(|buffer| buffer.size() < size) {
            *slot = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("RUR explicit PNG readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }));
        }
        let buffer = slot.as_ref().unwrap();
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("RUR explicit readback"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            extent(width, height),
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = mpsc::channel();
        let slice = buffer.slice(..size);
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .context("Waiting for GPU readback")?;
        rx.recv()
            .context("Readback callback was dropped")?
            .context("Mapping GPU frame")?;
        let mut rgba = vec![0u8; (row_bytes * height) as usize];
        {
            let mapped = slice.get_mapped_range();
            for (row, out) in mapped
                .chunks(padded as usize)
                .zip(rgba.chunks_exact_mut(row_bytes as usize))
            {
                out.copy_from_slice(&row[..row_bytes as usize]);
                convert(out);
            }
        }
        buffer.unmap();
        Ok(rgba)
    }

    /// Submits a copy of the frame target (all of it, or `region` as in [`Self::readback_png_rgba_region`])
    /// into a buffer of its own and asks for it mapped, without waiting: the caller prepares and draws
    /// the next frame meanwhile (the queue runs this copy before that frame's draw), then collects the
    /// pixels with [`Self::finish_readback`].
    pub fn begin_readback(&self, region: Option<[u32; 4]>) -> Result<PendingReadback> {
        let target = self
            .textures
            .get(&FRAME_TARGET)
            .context("No frame has been rendered")?;
        let [x, y, width, height] = region.unwrap_or([0, 0, target.width, target.height]);
        if width == 0 || height == 0 || x.checked_add(width).is_none_or(|end| end > target.width) || y.checked_add(height).is_none_or(|end| end > target.height) {
            bail!("readback region {x},{y} {width}x{height} is not inside the {}x{} frame", target.width, target.height);
        }
        let padded = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let size = u64::from(padded) * u64::from(height);
        let buffer = {
            let mut pool = self.readback_pool.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            match pool.iter().position(|buffer| buffer.size() >= size) {
                Some(index) => pool.swap_remove(index),
                None => self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("RUR sequence readback"),
                    size,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
            }
        };
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("RUR sequence readback") });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &target.texture, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(height) },
            },
            extent(width, height),
        );
        let submission = self.queue.submit([encoder.finish()]);
        let (tx, mapped) = mpsc::channel();
        buffer.slice(..size).map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        Ok(PendingReadback { buffer, submission, mapped, size, padded, width, height })
    }

    /// The straight-alpha RGBA rows of a readback [`Self::begin_readback`] submitted (as
    /// [`Self::readback_png_rgba_region`] returns them), waiting only for that copy, not for frames
    /// submitted after it; its buffer goes back to the pool.
    pub fn finish_readback(&self, pending: PendingReadback) -> Result<Vec<u8>> {
        let PendingReadback { buffer, submission, mapped, size, padded, width, height } = pending;
        let mut waited = false;
        let result = loop {
            match mapped.try_recv() {
                Ok(result) => break result,
                Err(mpsc::TryRecvError::Disconnected) => bail!("Readback callback was dropped"),
                Err(mpsc::TryRecvError::Empty) if waited => bail!("GPU readback did not map after its submission completed"),
                Err(mpsc::TryRecvError::Empty) => {
                    self.device
                        .poll(wgpu::PollType::Wait { submission_index: Some(submission.clone()), timeout: None })
                        .context("Waiting for GPU readback")?;
                    waited = true;
                }
            }
        };
        result.context("Mapping GPU frame")?;
        let table = unpremultiply_table();
        let row_bytes = (width * 4) as usize;
        let mut rgba = vec![0u8; row_bytes * height as usize];
        {
            let mapped = buffer.slice(..size).get_mapped_range();
            for (row, out) in mapped.chunks(padded as usize).zip(rgba.chunks_exact_mut(row_bytes)) {
                out.copy_from_slice(&row[..row_bytes]);
                // An opaque pixel is its own straight value (most of a frame over a background)
                for pixel in out.chunks_exact_mut(4).filter(|pixel| pixel[3] != 255) {
                    let divide = &table[pixel[3] as usize];
                    pixel[0] = divide[pixel[0] as usize];
                    pixel[1] = divide[pixel[1] as usize];
                    pixel[2] = divide[pixel[2] as usize];
                }
            }
        }
        buffer.unmap();
        self.readback_pool.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(buffer);
        Ok(rgba)
    }

    /// Frees the readback buffers (the render service does when it goes idle)
    pub fn release_readback(&self) {
        *self.readback.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        self.readback_pool.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clear();
    }

    /// Frees every texture scenes drew with (layers, images, the layer atlas's pages, the frame target)
    /// and the frame-sized staging: the render service after an idle stretch. The device, pipelines and
    /// samplers stay; the next frame is drawn whole, as a first one is. A preparer drawing to this
    /// renderer must forget its textures too (a new DrawPreparer).
    pub fn release_scene_textures(&mut self) {
        self.history = None;
        self.bindings.clear();
        self.textures.clear();
        self.atlas = LayerAtlas::default();
        self.data_valid = 0;
        self.record_staging = Vec::new();
        self.patched_staging = Vec::new();
        self.prev_texels = Vec::new();
        self.instance_staging = Vec::new();
        self.uniform_staging = Vec::new();
        self.release_readback();
    }

    /// Texture and staging bytes this renderer keeps (the render service's /memory)
    pub fn memory(&self) -> serde_json::Value {
        let texel = |format: wgpu::TextureFormat| format.block_copy_size(None).unwrap_or(4) as u64;
        let textures: u64 = self
            .textures
            .values()
            .map(|t| (0..t.levels).map(|level| u64::from((t.width >> level).max(1)) * u64::from((t.height >> level).max(1))).sum::<u64>() * texel(t.format))
            .sum();
        let readback = self.readback.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref().map_or(0, |buffer| buffer.size());
        serde_json::json!({
            "adapter": self.adapter_info.as_ref().map(|info| format!("{:?} {}", info.backend, info.name)),
            "textures": self.textures.len(),
            "textureBytes": textures,
            "atlasPages": self.atlas.pages.len(),
            "bindings": self.bindings.len(),
            "readbackBytes": readback,
            "stagingBytes": self.record_staging.capacity() * 16 + self.patched_staging.capacity() * 16 + self.prev_texels.capacity() * 16 + self.instance_staging.capacity() * 4 + self.uniform_staging.capacity(),
            "dataTexels": u64::from(self.data.width) * u64::from(self.data.height) * 16,
        })
    }

    pub fn reset_scene_resources(&mut self) {
        self.history = None;
        self.bindings.clear();
        self.textures.retain(|id, _| *id == FRAME_TARGET || is_atlas_page(*id));
        self.atlas.clear();
    }

    fn ensure_target(&mut self, id: TextureId, width: u32, height: u32) -> Result<()> {
        validate_extent(&self.device, width, height)?;
        let replace = self.textures.get(&id).is_none_or(|t| {
            t.width != width || t.height != height || t.format != OUTPUT_FORMAT || t.levels != 1
        });
        if replace {
            self.textures.insert(
                id,
                make_texture(
                    &self.device,
                    "RUR render target",
                    width,
                    height,
                    1,
                    OUTPUT_FORMAT,
                ),
            );
            if id == FRAME_TARGET {
                self.generation += 1;
            }
        }
        self.textures.get_mut(&id).unwrap().last_used = self.frame_serial;
        let texture = self.textures.get_mut(&id).unwrap();
        texture.transient_target = texture.revision.is_none();
        Ok(())
    }

    fn upload_texture(&mut self, upload: &TextureUpload) -> Result<u64> {
        if upload.id == FRAME_TARGET {
            bail!("Texture ID zero is reserved for the final frame");
        }
        let first = upload
            .levels
            .first()
            .context("Texture upload has no mip levels")?;
        validate_extent(&self.device, first.width, first.height)?;
        let format = match upload.format {
            TextureFormat::Rgba8 => OUTPUT_FORMAT,
            TextureFormat::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
            TextureFormat::R32Float => wgpu::TextureFormat::R32Float,
        };
        let bpp = match upload.format {
            TextureFormat::Rgba8 | TextureFormat::R32Float => 4,
            TextureFormat::Rgba32Float => 16,
        };
        let levels = upload.levels.len() as u32;
        if levels > 32 - first.width.max(first.height).leading_zeros() {
            bail!("Texture upload has too many mip levels");
        }
        if upload.format == TextureFormat::Rgba8
            && levels == 1
            && first.width <= ATLAS_MAX_LAYER
            && first.height <= ATLAS_MAX_LAYER
            && !self.textures.contains_key(&upload.id)
        {
            if first.bytes.len() != first.width as usize * first.height as usize * 4 {
                bail!("Texture upload has invalid mip dimensions or pixel byte length");
            }
            let placed = match self.atlas.placements.get(&upload.id).copied() {
                Some(p) if p.w == first.width && p.h == first.height => {
                    if p.revision == upload.revision {
                        return Ok(0);
                    }
                    Some(p)
                }
                Some(_) => {
                    self.atlas.remove(upload.id);
                    None
                }
                None => None,
            };
            let placed = placed.or_else(|| self.atlas_allocate(first.width, first.height));
            if let Some(mut placement) = placed {
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.textures[&atlas_page_id(placement.page)].texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: placement.x,
                            y: placement.y,
                            z: 0,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &first.bytes,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(first.width * 4),
                        rows_per_image: Some(first.height),
                    },
                    extent(first.width, first.height),
                );
                placement.revision = upload.revision;
                self.atlas.placements.insert(upload.id, placement);
                return Ok(first.bytes.len() as u64);
            }
        } else {
            // A texture of its own replaces an atlas placement
            self.atlas.remove(upload.id);
        }
        let replace = self.textures.get(&upload.id).is_none_or(|t| {
            t.width != first.width
                || t.height != first.height
                || t.levels != levels
                || t.format != format
        });
        if replace {
            self.textures.insert(
                upload.id,
                make_texture(
                    &self.device,
                    "RUR sampled pixels",
                    first.width,
                    first.height,
                    levels,
                    format,
                ),
            );
        }
        let texture = self.textures.get_mut(&upload.id).unwrap();
        texture.last_used = self.frame_serial;
        if texture.revision == Some(upload.revision) {
            return Ok(0);
        }
        let mut bytes = 0;
        for (mip, level) in upload.levels.iter().enumerate() {
            if level.width != (first.width >> mip).max(1)
                || level.height != (first.height >> mip).max(1)
                || level.bytes.len() != level.width as usize * level.height as usize * bpp as usize
            {
                bail!("Texture upload has invalid mip dimensions or pixel byte length");
            }
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture.texture,
                    mip_level: mip as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &level.bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(level.width * bpp),
                    rows_per_image: Some(level.height),
                },
                extent(level.width, level.height),
            );
            bytes += level.bytes.len() as u64;
        }
        texture.revision = Some(upload.revision);
        Ok(bytes)
    }

    /// A place for a w x h layer in the atlas, adding a page when the others are full (None at the page limit)
    fn atlas_allocate(&mut self, w: u32, h: u32) -> Option<AtlasPlacement> {
        if let Some(placement) = self.atlas.allocate(w, h) {
            return Some(placement);
        }
        if self.atlas.pages.len() >= ATLAS_PAGES {
            return None;
        }
        let page = self.atlas.pages.len();
        self.textures.insert(
            atlas_page_id(page),
            make_texture(&self.device, "RUR layer atlas", ATLAS_SIZE, ATLAS_SIZE, 1, OUTPUT_FORMAT),
        );
        self.atlas.pages.push(AtlasPage::default());
        self.atlas.allocate(w, h)
    }

    fn touch_batch(&mut self, batch: &DrawBatch, target: TextureId) -> Result<()> {
        for id in batch
            .image_slots
            .iter()
            .copied()
            .chain([batch.glyphs, batch.layer, batch.lut])
            .flatten()
        {
            if id == target {
                bail!("A draw cannot sample its own render target");
            }
            match self.textures.get_mut(&id) {
                Some(texture) => texture.last_used = self.frame_serial,
                None if self.atlas.placements.contains_key(&id) => {}
                None => bail!("Missing GPU texture {}", id.0),
            }
        }
        Ok(())
    }

    fn batch_binding(&mut self, batch: &DrawBatch) -> Result<(wgpu::BindGroup, bool)> {
        let slots: [(Option<TextureId>, &Texture); 11] = {
            let fallback = |index: usize| if index == 2 { &self.default_lut } else { &self.dummy };
            let ids = [
                batch.glyphs,
                batch.layer,
                batch.lut,
                batch.image_slots[0],
                batch.image_slots[1],
                batch.image_slots[2],
                batch.image_slots[3],
                batch.image_slots[4],
                batch.image_slots[5],
                batch.image_slots[6],
                batch.image_slots[7],
            ];
            let mut slots = [(None, &self.dummy); 11];
            for (index, id) in ids.into_iter().enumerate() {
                slots[index] = (id, fallback(index));
            }
            slots
        };
        let mut key = [0u64; 12];
        key[0] = self.data.version;
        for (index, (id, fallback)) in slots.iter().enumerate() {
            key[index + 1] = match id {
                Some(id) => self
                    .textures
                    .get(id)
                    .with_context(|| format!("Missing GPU texture {}", id.0))?
                    .version,
                None => fallback.version,
            };
        }
        // Only groups used this frame are kept (render), so a retired texture's groups do not outlive it
        if let Some((binding, used)) = self.bindings.get_mut(&key) {
            *used = self.frame_serial;
            return Ok((binding.clone(), false));
        }
        let mut views: [wgpu::TextureView; 12] = std::array::from_fn(|_| self.dummy.view.clone());
        views[0] = self.data.view.clone();
        for (index, (id, fallback)) in slots.iter().enumerate() {
            views[index + 1] = match id {
                Some(id) => self.textures[id].view.clone(),
                None => fallback.view.clone(),
            };
        }
        let entries: [wgpu::BindGroupEntry<'_>; 14] =
            std::array::from_fn(|binding| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: if binding == 12 {
                    wgpu::BindingResource::Sampler(&self.sampler)
                } else if binding == 13 {
                    wgpu::BindingResource::Sampler(&self.repeat_sampler)
                } else {
                    wgpu::BindingResource::TextureView(&views[binding])
                },
            });
        let binding = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("RUR draw textures"),
            layout: &self.texture_layout,
            entries: &entries,
        });
        self.bindings
            .insert(key, (binding.clone(), self.frame_serial));
        Ok((binding, true))
    }

    fn encode_copy(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        copy: &packet::TextureCopy,
    ) -> Result<()> {
        if copy.source == copy.destination {
            bail!("Texture copies require distinct targets");
        }
        let source = self
            .textures
            .get(&copy.source)
            .context("Missing copy source")?;
        let destination = self
            .textures
            .get(&copy.destination)
            .context("Missing copy destination")?;
        if source.format != destination.format {
            bail!("Texture copies require matching formats");
        }
        for (texture, origin) in [
            (source, copy.source_origin),
            (destination, copy.destination_origin),
        ] {
            if origin[0].saturating_add(copy.extent[0]) > texture.width
                || origin[1].saturating_add(copy.extent[1]) > texture.height
            {
                bail!("Texture copy exceeds bounds");
            }
        }
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &source.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: copy.source_origin[0],
                    y: copy.source_origin[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &destination.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: copy.destination_origin[0],
                    y: copy.destination_origin[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            extent(copy.extent[0], copy.extent[1]),
        );
        Ok(())
    }

    fn poll_timestamps(&mut self) {
        let _ = self.device.poll(wgpu::PollType::Poll);
        for slot in &mut self.timestamp_slots {
            let Some(receiver) = &slot.pending else {
                continue;
            };
            let Ok(result) = receiver.try_recv() else {
                continue;
            };
            if result.is_ok() {
                let mapped = slot.read.slice(..).get_mapped_range();
                let ticks: &[u64] = bytemuck::cast_slice(&mapped);
                if ticks[1] >= ticks[0] {
                    self.last_gpu_ms = Some(
                        (ticks[1] - ticks[0]) as f64 * self.queue.get_timestamp_period() as f64
                            / 1_000_000.0,
                    );
                    self.fresh_gpu_ms = self.last_gpu_ms;
                }
                drop(mapped);
            }
            slot.read.unmap();
            slot.pending = None;
        }
    }
}

/// A partial redraw's clear: the pass colour written (not blended) wherever the scissor allows. The
/// colour arrives as byte / 255 in f32 and is stored to the 8-bit target like a LoadOp::Clear value.
const CLEAR_WGSL: &str = "
@group(0) @binding(0) var<uniform> colour: vec4<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> {
    return colour;
}
";

/// The frame pass's draws in encoding order: each record's identity is its texel block (up to the next
/// record's start: records are appended as contiguous blocks that only address themselves) and its
/// batch state. A record sampling a texture this packet rewrites (an upload, an intermediate target
/// drawn or copied into) is marked fresh: it never matches, whatever its texels.
fn drawn_records(packet: &RenderPacket, pass_index: usize) -> (Vec<DrawnRecord>, Vec<BatchKey>) {
    use std::hash::{Hash, Hasher};
    let mut starts: Vec<u32> = packet
        .passes
        .iter()
        .flat_map(|p| p.batches.iter())
        .flat_map(|b| b.record_offsets.iter().copied())
        .collect();
    starts.sort_unstable();
    starts.dedup();
    let mut rewritten: rustc_hash::FxHashSet<TextureId> =
        packet.textures.iter().map(|t| t.id).collect();
    for pass in &packet.passes {
        if pass.target != FRAME_TARGET {
            rewritten.insert(pass.target);
        }
        for copy in &pass.copies_after {
            rewritten.insert(copy.destination);
        }
    }
    let pass = &packet.passes[pass_index];
    let (width, height) = (pass.width as f32, pass.height as f32);
    let mut records =
        Vec::with_capacity(pass.batches.iter().map(|b| b.record_offsets.len()).sum());
    let mut keys = Vec::with_capacity(pass.batches.len());
    for batch in &pass.batches {
        let key = BatchKey {
            image_slots: batch.image_slots,
            glyphs: batch.glyphs,
            layer: batch.layer,
            lut: batch.lut,
            blend: batch.blend,
            scissor: batch.scissor,
        };
        let mut hasher = rustc_hash::FxHasher::default();
        key.hash(&mut hasher);
        let batch_hash = hasher.finish();
        let fresh = batch
            .image_slots
            .iter()
            .copied()
            .chain([batch.glyphs, batch.layer, batch.lut])
            .flatten()
            .any(|id| rewritten.contains(&id));
        let batch_index = keys.len() as u32;
        keys.push(key);
        for &offset in &batch.record_offsets {
            let start = offset as usize;
            let next = starts.partition_point(|s| *s <= offset);
            let end = starts
                .get(next)
                .map_or(packet.texels.len(), |e| *e as usize);
            let block = &packet.texels[start..end];
            let mut hasher = rustc_hash::FxHasher::default();
            hasher.write_u64(batch_hash);
            for word in bytemuck::cast_slice::<[f32; 4], u64>(block) {
                hasher.write_u64(*word);
            }
            // The quad covers pixel centres inside [x0, x1) x [y0, y1) at most (the vertex shader
            // only ever shrinks it)
            let [x0, y0, x1, y1] = block[0];
            let area = if [x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
                [
                    x0.floor().clamp(0.0, width) as u32,
                    y0.floor().clamp(0.0, height) as u32,
                    x1.ceil().clamp(0.0, width) as u32,
                    y1.ceil().clamp(0.0, height) as u32,
                ]
            } else {
                [0, 0, pass.width, pass.height]
            };
            records.push(DrawnRecord {
                hash: hasher.finish(),
                area,
                offset,
                len: (end - start) as u32,
                batch: batch_index,
                fresh,
            });
        }
    }
    (records, keys)
}

/// The box to redraw so the frame target ends up as if every draw were encoded again. Draws are
/// matched old to new in order (a common subsequence: same texels, same batch state, same relative
/// order); what is left unmatched on either side is damaged. Outside the damaged box every pixel is
/// covered by matched draws only, in the same order, over the same clear colour.
fn frame_damage(
    old: &FrameHistory,
    old_texels: &[[f32; 4]],
    new: &[DrawnRecord],
    new_keys: &[BatchKey],
    new_texels: &[[f32; 4]],
    width: u32,
    height: u32,
) -> Damage {
    let mut sorted: Vec<(u64, u32)> = old
        .records
        .iter()
        .enumerate()
        .filter(|(_, r)| !r.fresh)
        .map(|(i, r)| (r.hash, i as u32))
        .collect();
    sorted.sort_unstable();
    let mut cursors: rustc_hash::FxHashMap<u64, usize> = Default::default();
    let mut matched = vec![false; old.records.len()];
    let mut last: i64 = -1;
    let mut damage: Option<[u32; 4]> = None;
    let mut add = |a: [u32; 4]| {
        if a[0] < a[2] && a[1] < a[3] {
            damage = Some(match damage {
                Some(d) => [d[0].min(a[0]), d[1].min(a[1]), d[2].max(a[2]), d[3].max(a[3])],
                None => a,
            });
        }
    };
    fn bits(v: &[[f32; 4]]) -> &[u32] {
        bytemuck::cast_slice(v)
    }
    for record in new {
        let mut hit = None;
        if !record.fresh {
            let pos = cursors
                .entry(record.hash)
                .or_insert_with(|| sorted.partition_point(|e| e.0 < record.hash));
            while *pos < sorted.len()
                && sorted[*pos].0 == record.hash
                && i64::from(sorted[*pos].1) <= last
            {
                *pos += 1;
            }
            if *pos < sorted.len() && sorted[*pos].0 == record.hash {
                hit = Some(sorted[*pos].1 as usize);
                *pos += 1;
            }
        }
        match hit {
            Some(i) => {
                let before = &old.records[i];
                // Equal hashes: confirm the content itself (a collision redraws everything)
                let old_block = old_texels
                    .get(before.offset as usize..(before.offset + before.len) as usize);
                let new_block =
                    &new_texels[record.offset as usize..(record.offset + record.len) as usize];
                if before.area != record.area
                    || old.batches.get(before.batch as usize) != new_keys.get(record.batch as usize)
                    || old_block.is_none_or(|block| bits(block) != bits(new_block))
                {
                    return Damage::Full;
                }
                matched[i] = true;
                last = i as i64;
            }
            None => add(record.area),
        }
    }
    for (record, matched) in old.records.iter().zip(&matched) {
        if !matched {
            add(record.area);
        }
    }
    match damage {
        None => Damage::None,
        // Most of the frame: a whole clear costs no more
        Some(d)
            if u64::from(d[2] - d[0]) * u64::from(d[3] - d[1]) * 10
                >= u64::from(width) * u64::from(height) * 7 =>
        {
            Damage::Full
        }
        Some(d) => Damage::Rect(d),
    }
}

fn extent(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}
fn validate_extent(device: &wgpu::Device, width: u32, height: u32) -> Result<()> {
    if width == 0
        || height == 0
        || width > device.limits().max_texture_dimension_2d
        || height > device.limits().max_texture_dimension_2d
    {
        bail!("Invalid texture extent {width}x{height}");
    }
    Ok(())
}
fn stream_buffer(
    device: &wgpu::Device,
    label: &str,
    size: u64,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
fn uniform_binding(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("RUR target"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset: 0,
                size: NonZeroU64::new(16),
            }),
        }],
    })
}
fn make_texture(
    device: &wgpu::Device,
    label: &str,
    width: u32,
    height: u32,
    levels: u32,
    format: wgpu::TextureFormat,
) -> Texture {
    let mut usage = wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::COPY_SRC
        | wgpu::TextureUsages::COPY_DST;
    if format == OUTPUT_FORMAT {
        usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: extent(width, height),
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    Texture {
        texture,
        view,
        width,
        height,
        levels,
        format,
        revision: None,
        last_used: 0,
        transient_target: false,
        version: {
            static VERSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            VERSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        },
    }
}
fn default_lut_bytes() -> Vec<u8> {
    let mut bytes = vec![0; 256 * 1024 * 4];
    for alpha in 1..256usize {
        for p in 0..=alpha {
            let value = ((p * 255 + alpha / 2) / alpha).min(255) as u8;
            let offset = (alpha * 256 + p) * 4;
            bytes[offset..offset + 4].copy_from_slice(&[value, value, value, 255]);
        }
    }
    for y in 0..8usize {
        for x in 0..8usize {
            let m = ((y & 1) << 5)
                | ((x & 1) << 4)
                | ((y & 2) << 2)
                | ((x & 2) << 1)
                | ((y & 4) >> 1)
                | ((x & 4) >> 2);
            let value = (((2 * m + 1) * 255 + 64) / 128) as u8;
            let offset = (256 * 256 + x + 8 * y) * 4;
            bytes[offset..offset + 4].copy_from_slice(&[value, value, value, 255]);
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::super::packet::{RenderPass, TextureLevel};
    use super::*;

    #[test]
    fn loader_defaults_yield_to_every_setting_a_caller_chose() {
        assert!(no_loader_setting_chosen(|_| false));
        for setting in LOADER_SETTINGS {
            assert!(!no_loader_setting_chosen(|name| name == setting));
        }
    }

    #[test]
    fn loader_defaults_are_loader_settings() {
        for (name, _) in LOADER_DEFAULTS {
            assert!(LOADER_SETTINGS.contains(&name));
        }
    }

    #[test]
    fn calibrated_glsl_passes_naga_validation() {
        for (stage, source) in [
            (
                wgpu::naga::ShaderStage::Vertex,
                include_str!("shaders/uber.vert.glsl"),
            ),
            (
                wgpu::naga::ShaderStage::Fragment,
                include_str!("shaders/uber.frag.glsl"),
            ),
        ] {
            let module = wgpu::naga::front::glsl::Frontend::default()
                .parse(
                    &wgpu::naga::front::glsl::Options {
                        stage,
                        defines: Default::default(),
                    },
                    source,
                )
                .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)));
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)));
        }
    }

    /// The web build draws through the GLES backend, whose translation is stricter than Vulkan's: a texture
    /// takes a single sampler there. RUR_WRITE_GLES=<file prefix> keeps the translated sources.
    #[test]
    fn calibrated_glsl_translates_for_webgl2() {
        use wgpu::naga;
        for (stage, name, source) in [
            (naga::ShaderStage::Vertex, "vert", include_str!("shaders/uber.vert.glsl")),
            (naga::ShaderStage::Fragment, "frag", include_str!("shaders/uber.frag.glsl")),
        ] {
            let module = naga::front::glsl::Frontend::default()
                .parse(&naga::front::glsl::Options { stage, defines: Default::default() }, source)
                .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)));
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            let options = naga::back::glsl::Options {
                version: naga::back::glsl::Version::Embedded { version: 300, is_webgl: true },
                ..Default::default()
            };
            let pipeline = naga::back::glsl::PipelineOptions {
                shader_stage: stage,
                entry_point: "main".into(),
                multiview: None,
            };
            let mut text = String::new();
            naga::back::glsl::Writer::new(
                &mut text,
                &module,
                &info,
                &options,
                &pipeline,
                naga::proc::BoundsCheckPolicies::default(),
            )
            .and_then(|mut writer| writer.write())
            .unwrap_or_else(|error| panic!("{name}: {error}"));
            // A bool vector cast to an integer vector becomes mix() of integer vectors, which GLSL ES 3.00
            // lacks: the browser rejected the whole shader for one
            assert!(!text.contains("mix(ivec") && !text.contains("mix(uvec"), "{name}: mix() of integer vectors");
            if let Some(prefix) = std::env::var_os("RUR_WRITE_GLES") {
                let mut path = prefix;
                path.push(format!(".{name}.glsl"));
                std::fs::write(path, &text).unwrap();
            }
        }
    }

    fn packet(records: &[[f32; 4]], width: u32, height: u32, batch: DrawBatch) -> RenderPacket {
        let mut texels = packet::byte_float_texels();
        texels.extend_from_slice(records);
        RenderPacket {
            width,
            height,
            texels,
            passes: vec![RenderPass {
                target: FRAME_TARGET,
                width,
                height,
                clear: Some([0; 4]),
                batches: vec![batch],
                copies_after: vec![],
            }],
            textures: vec![],
            retire_textures: vec![],
            unchanged: false,
        }
    }
    fn batch() -> DrawBatch {
        DrawBatch {
            record_offsets: vec![64],
            ..Default::default()
        }
    }

    /// A solid rect record (kind 1) over [x0, x1) x [y0, y1), premultiplied colour and alpha bytes
    fn rect(b: [f32; 4], rgb: [f32; 3], a: f32) -> Vec<[f32; 4]> {
        vec![b, [0., 0., 64., 64.], [1., 0., 0., 0.], [0.; 4], [0., rgb[0], rgb[1], rgb[2]], [a, 0., 0., 0.]]
    }
    #[test]
    fn partial_redraws_match_whole_frames() -> Result<()> {
        let frame = |second: bool| {
            let mut records = rect([0., 0., 16., 16.], [10., 20., 30.], 255.);
            records.extend(rect([2., 2., 6., 6.], if second { [0., 90., 0.] } else { [90., 0., 0.] }, 128.));
            records.extend(rect([4., 4., 12., 12.], [0., 0., 70.], 100.));
            records.extend(rect([13., 1., 15., 3.], [50., 50., 50.], 255.));
            let mut p = packet(&records, 16, 16, DrawBatch::default());
            p.passes[0].batches[0].record_offsets = vec![64, 70, 76, 82];
            p.passes[0].clear = Some([1, 2, 3, 255]);
            p
        };
        let mut gpu = pollster::block_on(GpuRenderer::headless())?;
        let first = gpu.render(&frame(false))?;
        assert_eq!(first.stats.redrawn_pixels, 256);
        let unchanged = gpu.render(&frame(false))?;
        assert_eq!(unchanged.stats.redrawn_pixels, 0, "identical draws redraw nothing");
        let partial = gpu.render(&frame(true))?;
        assert!(partial.stats.redrawn_pixels > 0 && partial.stats.redrawn_pixels < 256);
        let redrawn = gpu.readback_rgba()?;
        let mut fresh = pollster::block_on(GpuRenderer::headless())?;
        fresh.render(&frame(true))?;
        assert_eq!(redrawn, fresh.readback_rgba()?, "a partial redraw equals the whole frame");
        Ok(())
    }

    #[test]
    fn native_gpu_quad_alpha_texture_groups_and_clipped_glyph() -> Result<()> {
        let mut gpu = pollster::block_on(GpuRenderer::headless())?;
        // Top-down framebuffer coordinates and fractional alpha blending on the same output.
        let mut p = packet(
            &[
                [0., 0., 2., 1.],
                [1., 0., 2., 1.],
                [1., 0., 0., 0.],
                [0.; 4],
                [0., 128., 0., 0.],
                [128., 0., 0., 0.],
            ],
            2,
            2,
            batch(),
        );
        p.passes[0].clear = Some([0, 0, 255, 255]);
        p.passes[0].batches.push(DrawBatch::default());
        let first = gpu.render(&p)?;
        assert_eq!(
            first.stats.bind_group_creations, 1,
            "Identical texture sets share a binding within a frame"
        );
        assert_eq!(
            gpu.readback_rgba()?,
            vec![
                0, 0, 255, 255, 128, 0, 127, 255, 0, 0, 255, 255, 0, 0, 255, 255
            ]
        );
        let second = gpu.render(&p)?;
        assert_eq!(
            second.stats.bind_group_creations, 0,
            "Stable views reuse their binding across frames"
        );
        assert_eq!(
            first.generation, second.generation,
            "A stable viewport must reuse its texture"
        );

        // Uploaded bytes retain their row order and premultiplication.
        p = packet(
            &[
                [0., 0., 2., 2.],
                [0., 0., 2., 2.],
                [18., 0., 0., 0.],
                [0.; 4],
                [0.; 4],
            ],
            2,
            2,
            DrawBatch {
                image_slots: [Some(TextureId(8)), None, None, None, None, None, None, None],
                ..batch()
            },
        );
        let pixels = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 64, 32, 0, 128,
        ];
        p.textures.push(TextureUpload {
            id: TextureId(8),
            revision: 1,
            format: TextureFormat::Rgba8,
            levels: vec![TextureLevel {
                width: 2,
                height: 2,
                bytes: pixels.clone().into(),
            }],
        });
        gpu.render(&p)?;
        assert_eq!(gpu.readback_rgba()?, pixels);

        // Same public texture ID with different storage must not reuse the retired view.
        let mut wide_pixels = Vec::new();
        for row in pixels.chunks_exact(8) {
            wide_pixels.extend_from_slice(row);
            wide_pixels.extend_from_slice(&[0; 8]);
        }
        p.textures = vec![TextureUpload {
            id: TextureId(8),
            revision: 2,
            format: TextureFormat::Rgba8,
            levels: vec![TextureLevel {
                width: 4,
                height: 2,
                bytes: wide_pixels.into(),
            }],
        }];
        let resized = gpu.render(&p)?;
        assert_eq!(resized.stats.bind_group_creations, 1);
        assert_eq!(gpu.readback_rgba()?, pixels);
        p.textures.clear();
        p.texels.resize((DATA_WIDTH * 17) as usize, [0.; 4]);
        let records_grown = gpu.render(&p)?;
        assert_eq!(
            records_grown.stats.bind_group_creations, 1,
            "Growing the record texture also replaces a bound view"
        );
        assert_eq!(gpu.readback_rgba()?, pixels);

        // A complete straight-RGBA mip chain is uploaded and the IMAGE kind fetches lod 1.
        p = packet(
            &[
                [0., 0., 1., 1.],
                [0., 0., 1., 1.],
                [6., 0., 0., 0.],
                [0., 0., 1., 1.],
                [0., 0., 1., 1.],
                [0., 1., 0., 1.],
                [10., 10., 0., 0.],
                [1.; 4],
                [0.; 4],
                [0.; 4],
                [0.; 4],
            ],
            1,
            1,
            DrawBatch {
                image_slots: [Some(TextureId(8)), None, None, None, None, None, None, None],
                ..batch()
            },
        );
        p.textures = vec![TextureUpload {
            id: TextureId(8),
            revision: 3,
            format: TextureFormat::Rgba8,
            levels: vec![
                TextureLevel {
                    width: 2,
                    height: 2,
                    bytes: [255, 0, 0, 255].repeat(4).into(),
                },
                TextureLevel {
                    width: 1,
                    height: 1,
                    bytes: vec![12, 34, 56, 255].into(),
                },
            ],
        }];
        gpu.render(&p)?;
        assert_eq!(gpu.readback_rgba()?, vec![12, 34, 56, 255]);
        // An unchanged revision must avoid uploads while retaining the same sampled texture.
        p.textures.clear();
        let unchanged = gpu.render(&p)?;
        assert_eq!(unchanged.stats.bind_group_creations, 0);
        assert_eq!(gpu.readback_rgba()?, vec![12, 34, 56, 255]);

        // Render targets are sampled by later passes, using the exact CanvasGroup segment kind.
        p = packet(
            &[
                [0., 0., 2., 2.],
                [0., 0., 2., 2.],
                [17., 0., 0., 0.],
                [2., 2., 2., 2.],
            ],
            2,
            2,
            DrawBatch {
                layer: Some(TextureId(9)),
                ..batch()
            },
        );
        p.passes.insert(
            0,
            RenderPass {
                target: TextureId(9),
                width: 2,
                height: 2,
                clear: Some([10, 20, 30, 255]),
                batches: vec![],
                copies_after: vec![],
            },
        );
        gpu.render(&p)?;
        assert_eq!(gpu.readback_rgba()?, [10, 20, 30, 255].repeat(4));

        // Glyph alpha and record selection clipping use the real TEXT_QUAD implementation.
        p = packet(
            &[
                [0., 0., 3., 3.],
                [0., 0., 3., 3.],
                [15., 0., 7., 0.],
                [3., 255., 0., 0.],
                [0xff0000 as f32, 0., 0., 0.],
                [2., 1., 0., 0.],
                [1., 1., 2., 2.],
                [1., 0., 0., 1.],
                [0.; 4],
                [3., 3., 0., 0.],
            ],
            3,
            3,
            DrawBatch {
                glyphs: Some(TextureId(10)),
                ..batch()
            },
        );
        p.textures.push(TextureUpload {
            id: TextureId(10),
            revision: 1,
            format: TextureFormat::Rgba8,
            levels: vec![TextureLevel {
                width: 1,
                height: 1,
                bytes: vec![255, 255, 255, 128].into(),
            }],
        });
        p.retire_textures.push(TextureId(8));
        gpu.render(&p)?;
        assert!(
            !gpu.textures.contains_key(&TextureId(8)),
            "CPU-owned eviction must release the GPU upload"
        );
        assert!(
            !gpu.textures.contains_key(&TextureId(9)),
            "Unused intermediate targets must be released"
        );
        let mut expected = vec![0; 36];
        expected[16..20].copy_from_slice(&[128, 0, 0, 128]);
        assert_eq!(gpu.readback_rgba()?, expected);
        let straight = gpu.readback_png_rgba()?;
        assert_eq!(&straight[16..20], &[255, 0, 0, 128]);
        Ok(())
    }
}
