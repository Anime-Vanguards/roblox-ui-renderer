//! Device-independent draw packets: RGBA32F records laid out as shaders/uber.frag.glsl reads them (layout
//! above each record kind there).

pub const DATA_WIDTH: u32 = 2048;
pub const IMAGE_SLOTS: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextureId(pub u64);

/// Reserved final offscreen color target. Asset and intermediate IDs must be nonzero.
pub const FRAME_TARGET: TextureId = TextureId(0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextureFormat {
    #[default]
    Rgba8,
    Rgba32Float,
    R32Float,
}

#[derive(Clone, Debug)]
pub struct TextureLevel {
    pub width: u32,
    pub height: u32,
    /// Tightly packed rows, top down. Float formats use little-endian f32 bytes. Shared with the cache
    /// that keeps the same pixels (a layer, a mip level) rather than copied for the upload.
    pub bytes: LevelBytes,
}

/// A texture level's bytes: the upload's own, or an allocation a cache keeps too
#[derive(Clone, Debug)]
pub enum LevelBytes {
    Owned(Vec<u8>),
    Shared(std::sync::Arc<[u8]>),
}

impl std::ops::Deref for LevelBytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            LevelBytes::Owned(bytes) => bytes,
            LevelBytes::Shared(bytes) => bytes,
        }
    }
}

impl From<Vec<u8>> for LevelBytes {
    fn from(bytes: Vec<u8>) -> Self {
        LevelBytes::Owned(bytes)
    }
}

impl From<std::sync::Arc<[u8]>> for LevelBytes {
    fn from(bytes: std::sync::Arc<[u8]>) -> Self {
        LevelBytes::Shared(bytes)
    }
}

#[derive(Clone, Debug)]
pub struct TextureUpload {
    pub id: TextureId,
    pub revision: u64,
    pub format: TextureFormat,
    pub levels: Vec<TextureLevel>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlendMode {
    #[default]
    Premultiplied,
    /// Straight image RGB into a CanvasGroup; alpha remains source-over.
    Image,
}

#[derive(Clone, Debug, Default)]
pub struct DrawBatch {
    pub record_offsets: Vec<u32>,
    pub image_slots: [Option<TextureId>; IMAGE_SLOTS],
    pub glyphs: Option<TextureId>,
    pub layer: Option<TextureId>,
    pub lut: Option<TextureId>,
    pub blend: BlendMode,
    pub scissor: Option<[u32; 4]>,
}

#[derive(Clone, Debug)]
pub struct TextureCopy {
    pub source: TextureId,
    pub destination: TextureId,
    pub source_origin: [u32; 2],
    pub destination_origin: [u32; 2],
    pub extent: [u32; 2],
}

#[derive(Clone, Debug)]
pub struct RenderPass {
    pub target: TextureId,
    pub width: u32,
    pub height: u32,
    /// None preserves target contents; values are already premultiplied bytes.
    pub clear: Option<[u8; 4]>,
    pub batches: Vec<DrawBatch>,
    pub copies_after: Vec<TextureCopy>,
}

#[derive(Clone, Debug, Default)]
pub struct RenderPacket {
    pub width: u32,
    pub height: u32,
    /// Texels 0..64 hold the exact float32 byte/255 lookup table.
    pub texels: Vec<[f32; 4]>,
    /// Dependency order: intermediate CanvasGroups precede their consumers.
    pub passes: Vec<RenderPass>,
    pub textures: Vec<TextureUpload>,
    /// Uploaded resources evicted by the owning CPU cache since the previous packet.
    pub retire_textures: Vec<TextureId>,
    /// The scene is unchanged since the previous packet (DrawPreparer's identity check): this packet
    /// carries no records, and the renderer keeps the frame it drew last. A renderer that did not
    /// draw that frame reports an error (the caller then resets its caches).
    pub unchanged: bool,
}

pub fn byte_float_texels() -> Vec<[f32; 4]> {
    (0..64)
        .map(|i| std::array::from_fn(|j| (i * 4 + j) as f32 / 255.0))
        .collect()
}
