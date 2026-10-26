//! A small GLES2-compatible GPU painter. Rust owns vector glyphs and the draw list;
//! SDL only supplies the context. No browser, LVGL or Qt runtime.
use crate::{
    font::Font,
    raster::{Size, Viewport},
    state::Rect,
    typography::{HeadingFont, TextFont},
    vector,
};
use gl::types::*;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    ffi::{CStr, CString},
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::Instant,
};

pub type Color = [f32; 4];

type DecodedStill = Option<(image::RgbaImage, Option<image::RgbaImage>)>;

/// Decodes wallpaper-clip stills and camera snapshots off the GL thread, so a
/// clip swap or a fresh snapshot never stalls a frame on a large JPEG; the GL
/// thread only uploads the pixels.
struct StillLoader {
    /// Path, and whether a frost copy is wanted too.
    requests: mpsc::Sender<(String, bool)>,
    replies: mpsc::Receiver<(String, DecodedStill)>,
    pending: HashSet<String>,
    /// Stills (and their frost) kept resident regardless of the texture budget.
    pinned: HashSet<String>,
}
impl StillLoader {
    fn new() -> Self {
        let (requests, jobs) = mpsc::channel::<(String, bool)>();
        let (done, replies) = mpsc::channel();
        // If the thread cannot start, sends fail and stills decode inline.
        let _ = std::thread::Builder::new()
            .name("luma-stills".into())
            .spawn(move || {
                for (path, frost) in jobs {
                    let decoded = open_image(&path).ok().map(|image| {
                        let frost = frost.then(|| frost_pixels(&image));
                        // Nothing on screen is wider than 1920 logical pixels.
                        let image = if image.width() > 1920 {
                            image.resize(1920, 1920, image::imageops::FilterType::Triangle)
                        } else {
                            image
                        };
                        (image.into_rgba8(), frost)
                    });
                    if done.send((path, decoded)).is_err() {
                        break;
                    }
                }
            });
        Self {
            requests,
            replies,
            pending: HashSet::new(),
            pinned: HashSet::new(),
        }
    }
}
/// Covers drawn as `path#async` decode on two worker threads: the image
/// draws nothing until its pixels are in, so a screen of new covers (a
/// Library tab) never stalls the frame on a dozen JPEG decodes.
struct PosterLoader {
    requests: mpsc::Sender<String>,
    replies: mpsc::Receiver<(String, Option<image::RgbaImage>)>,
    pending: HashSet<String>,
}
impl PosterLoader {
    fn new() -> Self {
        let (requests, jobs) = mpsc::channel::<String>();
        let (done, replies) = mpsc::channel();
        let jobs = std::sync::Arc::new(std::sync::Mutex::new(jobs));
        for n in 0..2 {
            let (jobs, done) = (jobs.clone(), done.clone());
            let _ = std::thread::Builder::new()
                .name(format!("luma-covers-{n}"))
                .spawn(move || {
                    loop {
                        let next = jobs.lock().unwrap().recv();
                        let Ok(path) = next else {
                            break;
                        };
                        let started = std::time::Instant::now();
                        let decoded = open_image(&path).ok().map(|i| {
                            // Nothing on screen is wider than 1920 logical pixels.
                            if i.width() > 1920 {
                                i.thumbnail(1920, 1920).into_rgba8()
                            } else {
                                i.into_rgba8()
                            }
                        });
                        if is_poster_cache(&path) {
                            log_image_timing(&path, started, decoded.as_ref());
                        }
                        if done.send((path, decoded)).is_err() {
                            break;
                        }
                    }
                });
        }
        Self {
            requests,
            replies,
            pending: HashSet::new(),
        }
    }
}
pub const WHITE: Color = [0.96, 0.96, 0.96, 1.];
const GLYPH_GEOMETRY_BUDGET: usize = 8 * 1024 * 1024;
const TEXT_GEOMETRY_BUDGET: usize = 32 * 1024 * 1024;
const TEXTURE_BUDGET: usize = 48 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub weight: u16,
    /// CSS letter-spacing in em, not pixels.
    pub tracking: f32,
    pub line_height: f32,
    pub align_right: bool,
    pub heading_override: Option<HeadingFont>,
    pub text_override: Option<TextFont>,
    pub display: Option<bool>,
    pub tabular_nums: bool,
}
impl TextStyle {
    pub const fn new(size: f32) -> Self {
        Self {
            size,
            weight: 400,
            tracking: 0.,
            line_height: 1.2,
            align_right: false,
            heading_override: None,
            text_override: None,
            display: None,
            tabular_nums: true,
        }
    }
    pub const fn weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }
    pub const fn tracking(mut self, tracking: f32) -> Self {
        self.tracking = tracking;
        self
    }
    pub const fn leading(mut self, height: f32) -> Self {
        self.line_height = height;
        self
    }
    pub const fn display(mut self, display: bool) -> Self {
        self.display = Some(display);
        self
    }
    pub const fn heading(mut self, font: HeadingFont) -> Self {
        self.heading_override = Some(font);
        self
    }
    pub const fn body(mut self, font: TextFont) -> Self {
        self.text_override = Some(font);
        self
    }
    pub const fn tabular(mut self) -> Self {
        self.tabular_nums = true;
        self
    }
    pub const fn right(mut self) -> Self {
        self.align_right = true;
        self
    }
}

#[derive(Clone, PartialEq)]
pub enum Draw {
    Shape(Rect, f32, f32, Color),
    RoundedGradient(Rect, f32, Color, Color),
    DiagonalGradient(Rect, f32, Color, Color),
    /// Left-to-right gradient (rect, radius, left, right).
    HGradient(Rect, f32, Color, Color),
    DiagonalGradientBorder(Rect, f32, f32, Color, Color),
    Shadow(Rect, f32, f32, Color),
    /// Diagonal light sweep across a rounded card: (card, radius, progress 0..1, peak color).
    Sheen(Rect, f32, f32, Color),
    Text(String, Rect, TextStyle, Color, bool),
    Image(String, Rect, f32, bool),
    TintedImage(String, Rect, f32, bool, Color),
    AppImage(Arc<crate::app_artwork::Artwork>, Rect, Color),
    Clip(Option<Rect>),
    /// Rounded mask for textured paint (card, radius): sprites inside a
    /// rounded card never show in its corners.
    RoundClip(Option<(Rect, f32)>),
    EdgeFade(Option<[f32; 4]>),
}

struct Texture {
    id: GLuint,
    width: u32,
    height: u32,
    frame: u64,
}
impl Drop for Texture {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &self.id);
        }
    }
}

struct Glyph {
    geometry: Vec<luma_vector_geometry::Vertex>,
    frame: u64,
}

/// The two-stop colour ramp of a quad.
#[derive(Clone, Copy)]
enum Ramp {
    Vertical(Color, Color),
    /// 135°: the colour follows the NW-to-SE axis.
    Diagonal(Color, Color),
    Horizontal(Color, Color),
}
impl Ramp {
    fn colors(self) -> [Color; 2] {
        match self {
            Self::Vertical(a, b) | Self::Diagonal(a, b) | Self::Horizontal(a, b) => [a, b],
        }
    }
    /// Position along the ramp of the unit-quad point (`x`, `y`).
    fn progress(self, x: f32, y: f32) -> f32 {
        match self {
            Self::Vertical(..) => y,
            Self::Diagonal(..) => (x + y) * 0.5,
            Self::Horizontal(..) => x,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct TextKey {
    text: String,
    weight: u16,
    font_id: u16,
    fallback_id: u16,
    display: bool,
    // Logical size, tracking, line height, available width and height.
    layout: [u32; 5],
    flags: u8,
}

struct TextRun {
    mesh: Option<vector::Mesh>,
    missing_glyphs: usize,
    failures: usize,
    frame: u64,
}

#[derive(Clone, Serialize)]
pub struct GraphicsInfo {
    pub vendor: String,
    pub renderer: String,
    pub version: String,
    pub max_texture_size: u32,
    pub max_viewport: [i32; 2],
    pub fragment_highp: bool,
}

#[derive(Clone, Serialize)]
pub struct RenderStats {
    pub target: Size,
    pub viewport: Viewport,
    pub font_antialiasing: &'static str,
    pub multisample_samples: u32,
    pub max_logical_font_px: f32,
    pub max_physical_font_px: f32,
    pub cached_glyphs: usize,
    pub glyph_tessellations: usize,
    pub glyph_failures: usize,
    pub missing_glyphs: usize,
    pub missing_images: usize,
    pub glyph_geometry_bytes: usize,
    pub cached_text_runs: usize,
    pub text_geometry_bytes: usize,
    pub text_triangles: usize,
    pub artwork_bytes: usize,
    pub vector_meshes: usize,
    pub vector_bytes: usize,
    pub vector_triangles: usize,
    pub high_resolution_wallpaper: bool,
    pub draw_calls: usize,
    pub cpu_submission_ms: f64,
    /// Part of `cpu_submission_ms` spent turning commands into vertices
    /// (tessellation, image decode and upload), before any GL draw.
    pub build_ms: f64,
    pub cached_effect: bool,
    pub effect_render_ms: f64,
    pub crisp_render_ms: f64,
}
impl RenderStats {
    fn new(target: Size) -> Self {
        Self {
            target,
            viewport: Viewport::new(target),
            font_antialiasing: "vector grayscale coverage",
            multisample_samples: 0,
            max_logical_font_px: 0.,
            max_physical_font_px: 0.,
            cached_glyphs: 0,
            glyph_tessellations: 0,
            glyph_failures: 0,
            missing_glyphs: 0,
            missing_images: 0,
            glyph_geometry_bytes: 0,
            cached_text_runs: 0,
            text_geometry_bytes: 0,
            text_triangles: 0,
            artwork_bytes: 0,
            vector_meshes: 0,
            vector_bytes: 0,
            vector_triangles: 0,
            high_resolution_wallpaper: false,
            draw_calls: 0,
            cpu_submission_ms: 0.,
            build_ms: 0.,
            cached_effect: false,
            effect_render_ms: 0.,
            crisp_render_ms: 0.,
        }
    }
}

enum BatchKind {
    Paint {
        texture: GLuint,
        start: usize,
        count: usize,
        crisp: bool,
        // Full-screen wallpaper/still must be behind cached glass and scrim.
        underlay: bool,
    },
    Vector {
        path: String,
        rect: Rect,
        tint: Color,
    },
    Text {
        key: TextKey,
        origin: [f32; 2],
        tint: Color,
    },
}

struct Batch {
    kind: BatchKind,
    clip: Option<Rect>,
    fade: Option<[f32; 4]>,
    round: Option<(Rect, f32)>,
}

pub struct Renderer {
    program: GLuint,
    vector_program: GLuint,
    text_program: GLuint,
    buffer: GLuint,
    vao: GLuint,
    white: Texture,
    aerial_texture: Option<Texture>,
    font_directory: PathBuf,
    effects_fbo: GLuint,
    effects_texture: GLuint,
    effects_stamp: Option<u64>,
    active_heading: HeadingFont,
    active_text: TextFont,
    heading_pct: u8,
    text_pct: u8,
    fonts: HashMap<u16, Font>,
    failed_fonts: HashSet<u16>,
    glyphs: HashMap<(char, u16), Glyph>,
    text_runs: HashMap<TextKey, TextRun>,
    textures: HashMap<String, Option<Texture>>,
    stills: StillLoader,
    posters: PosterLoader,
    vectors: HashMap<String, Option<vector::Mesh>>,
    artwork_variants: HashMap<String, String>,
    vertices: Vec<f32>,
    batches: Vec<Batch>,
    clip: Option<Rect>,
    fade: Option<[f32; 4]>,
    round: Option<(Rect, f32)>,
    frame: u64,
    stats: RenderStats,
    pub info: GraphicsInfo,
    // Diagnostic-only layer mask from LUMA_PROF_SKIP (1 shapes, 2 textures,
    // 4 text, 8 vectors, 16 full-screen underlays). Zero in normal sessions.
    prof_skip: u8,
    prof_finish: bool,
    // LUMA_PROF_OLD: draw without the fill-rate savings, for A/B timing.
    prof_old: bool,
    // Areas that opaque draws later in this frame will overwrite (see render).
    holes: Vec<Rect>,
    // Blurred 240x135 RGBA of each frost source, for baked plate variants.
    frost_pixels: HashMap<String, Vec<u8>>,
    // Latest uploaded snapshot path of each Home Assistant camera.
    cameras: HashMap<String, String>,
    // A camera snapshot was uploaded since main last asked.
    fresh: bool,
    /// When each title-card picture finished decoding: it fades in instead
    /// of popping onto the screen.
    appeared: HashMap<String, std::time::Instant>,
}

impl Renderer {
    pub fn new(font: &Path, gles: bool) -> Result<Self, String> {
        let directory = font.parent().ok_or("Invalid font path")?;
        let mut fonts = HashMap::new();
        // Open only the common face. Metrics and outlines stay lazy even when
        // the much larger CJK fallback is needed later.
        fonts.insert(400, load_font(directory, 400)?);
        let mut max_texture_size = 0;
        let mut max_viewport = [0; 2];
        let mut precision = 0;
        unsafe {
            gl::GetIntegerv(gl::MAX_TEXTURE_SIZE, &mut max_texture_size);
            gl::GetIntegerv(gl::MAX_VIEWPORT_DIMS, max_viewport.as_mut_ptr());
            if gles && gl::GetShaderPrecisionFormat::is_loaded() {
                let mut range = [0; 2];
                gl::GetShaderPrecisionFormat(
                    gl::FRAGMENT_SHADER,
                    gl::HIGH_FLOAT,
                    range.as_mut_ptr(),
                    &mut precision,
                );
            }
        }
        let fragment_highp = !gles || precision > 0;
        let qualifier = if fragment_highp { "highp" } else { "mediump" };
        let vertex_prefix = if gles {
            format!("#version 100\nprecision highp float;\n#define PAINT_PRECISION {qualifier}\n")
        } else {
            "#version 150\n#define attribute in\n#define varying out\n#define PAINT_PRECISION\n"
                .into()
        };
        let fragment_prefix = if gles {
            format!(
                "#version 100\nprecision {qualifier} float;\n#define PAINT_PRECISION {qualifier}\n"
            )
        } else {
            "#version 150\n#define varying in\n#define texture2D texture\nout vec4 out_color;\n#define gl_FragColor out_color\n#define PAINT_PRECISION\n".into()
        };
        let program = link(
            &(vertex_prefix.clone() + include_str!("shaders/paint.vert")),
            &(fragment_prefix.clone() + include_str!("shaders/paint.frag")),
            &[c"a_position", c"a_uv", c"a_color", c"a_shape", c"a_params"],
        )?;
        let vector_program = match link(
            &(vertex_prefix.clone() + include_str!("shaders/vector.vert")),
            &(fragment_prefix.clone() + include_str!("shaders/vector.frag")),
            &[c"a_position", c"a_offset", c"a_color", c"a_coverage"],
        ) {
            Ok(program) => program,
            Err(error) => {
                unsafe { gl::DeleteProgram(program) };
                return Err(error);
            }
        };
        let text_program = match link(
            &(vertex_prefix + include_str!("shaders/text.vert")),
            &(fragment_prefix + include_str!("shaders/vector.frag")),
            &[c"a_position", c"a_offset", c"a_origin", c"a_coverage"],
        ) {
            Ok(program) => program,
            Err(error) => {
                unsafe {
                    gl::DeleteProgram(program);
                    gl::DeleteProgram(vector_program);
                }
                return Err(error);
            }
        };
        let mut buffer = 0;
        let mut vao = 0;
        unsafe {
            if !gles {
                gl::GenVertexArrays(1, &mut vao);
                gl::BindVertexArray(vao);
            }
            gl::GenBuffers(1, &mut buffer);
            gl::Enable(gl::BLEND);
            // Preserve an opaque destination alpha. Using BlendFunc for both
            // channels makes translucent UI darken again in PNG/window compositing.
            gl::BlendFuncSeparate(
                gl::SRC_ALPHA,
                gl::ONE_MINUS_SRC_ALPHA,
                gl::ONE,
                gl::ONE_MINUS_SRC_ALPHA,
            );
            gl::Disable(gl::DEPTH_TEST);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
        }
        let white = texture(1, 1, &[255; 4]);
        Ok(Self {
            program,
            vector_program,
            text_program,
            buffer,
            vao,
            white,
            aerial_texture: None,
            font_directory: directory.into(),
            effects_fbo: 0,
            effects_texture: 0,
            effects_stamp: None,
            active_heading: HeadingFont::default(),
            active_text: TextFont::default(),
            heading_pct: 100,
            text_pct: 100,
            fonts,
            failed_fonts: HashSet::new(),
            glyphs: HashMap::new(),
            text_runs: HashMap::new(),
            textures: HashMap::new(),
            stills: StillLoader::new(),
            posters: PosterLoader::new(),
            vectors: HashMap::new(),
            artwork_variants: HashMap::new(),
            vertices: Vec::with_capacity(65536),
            batches: Vec::new(),
            clip: None,
            fade: None,
            round: None,
            frame: 0,
            stats: RenderStats::new(Size::HD),
            info: GraphicsInfo {
                vendor: gl_string(gl::VENDOR),
                renderer: gl_string(gl::RENDERER),
                version: gl_string(gl::VERSION),
                max_texture_size: max_texture_size.max(0) as u32,
                max_viewport,
                fragment_highp,
            },
            prof_skip: std::env::var("LUMA_PROF_SKIP")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            prof_finish: std::env::var_os("LUMA_PROF_FINISH").is_some_and(|v| v == "1"),
            prof_old: std::env::var_os("LUMA_PROF_OLD").is_some_and(|v| v == "1"),
            holes: Vec::new(),
            frost_pixels: HashMap::new(),
            cameras: HashMap::new(),
            fresh: false,
            appeared: HashMap::new(),
        })
    }

    pub fn set_fonts(
        &mut self,
        heading: HeadingFont,
        text: TextFont,
        heading_pct: u8,
        text_pct: u8,
    ) {
        self.active_heading = heading;
        self.active_text = text;
        self.heading_pct = heading_pct.clamp(85, 170);
        self.text_pct = text_pct.clamp(90, 150);
    }
    fn ensure_font(&mut self, id: u16) -> bool {
        if self.fonts.contains_key(&id) {
            return true;
        }
        if self.failed_fonts.contains(&id) {
            return false;
        }
        match load_font(&self.font_directory, id) {
            Ok(font) => {
                self.fonts.insert(id, font);
                true
            }
            Err(error) => {
                eprintln!("{error}");
                self.failed_fonts.insert(id);
                false
            }
        }
    }

    fn glyph(&mut self, c: char, font: u16) -> Result<&[luma_vector_geometry::Vertex], String> {
        let key = (c, font);
        if !self.glyphs.contains_key(&key) {
            let geometry = self.fonts[&font].geometry(c)?;
            self.glyphs.insert(
                key,
                Glyph {
                    geometry,
                    frame: self.frame,
                },
            );
            self.stats.glyph_tessellations += 1;
        }
        let glyph = self.glyphs.get_mut(&key).unwrap();
        glyph.frame = self.frame;
        Ok(&glyph.geometry)
    }

    fn trim_text_geometry(&mut self) {
        let glyph_bytes =
            |glyph: &Glyph| glyph.geometry.len() * size_of::<luma_vector_geometry::Vertex>();
        let mut bytes: usize = self.glyphs.values().map(glyph_bytes).sum();
        if bytes > GLYPH_GEOMETRY_BUDGET || self.glyphs.len() > 4096 {
            let mut old: Vec<_> = self
                .glyphs
                .iter()
                .filter(|(_, glyph)| glyph.frame != self.frame)
                .map(|(key, glyph)| (*key, glyph.frame))
                .collect();
            old.sort_by_key(|(_, frame)| *frame);
            for (key, _) in old {
                if bytes <= GLYPH_GEOMETRY_BUDGET && self.glyphs.len() <= 4096 {
                    break;
                }
                if let Some(glyph) = self.glyphs.remove(&key) {
                    bytes -= glyph_bytes(&glyph);
                }
            }
        }
        let run_bytes = |run: &TextRun| run.mesh.as_ref().map_or(0, |mesh| mesh.bytes);
        let mut bytes: usize = self.text_runs.values().map(run_bytes).sum();
        if bytes > TEXT_GEOMETRY_BUDGET || self.text_runs.len() > 512 {
            let mut old: Vec<_> = self
                .text_runs
                .iter()
                .filter(|(_, run)| run.frame != self.frame)
                .map(|(key, run)| (key.clone(), run.frame))
                .collect();
            old.sort_by_key(|(_, frame)| *frame);
            for (key, _) in old {
                if bytes <= TEXT_GEOMETRY_BUDGET && self.text_runs.len() <= 512 {
                    break;
                }
                if let Some(run) = self.text_runs.remove(&key) {
                    bytes -= run_bytes(&run);
                }
            }
        }
        self.stats.cached_glyphs = self.glyphs.len();
        self.stats.glyph_geometry_bytes = self.glyphs.values().map(glyph_bytes).sum();
        self.stats.cached_text_runs = self.text_runs.len();
        self.stats.text_geometry_bytes = self.text_runs.values().map(run_bytes).sum();
    }

    fn quad(
        &mut self,
        rect: Rect,
        uv: [f32; 4],
        radius: f32,
        border: f32,
        color: Color,
        texture: GLuint,
    ) {
        self.gradient_quad(
            rect,
            uv,
            radius,
            border,
            Ramp::Vertical(color, color),
            texture,
        );
    }

    fn gradient_quad(
        &mut self,
        rect: Rect,
        uv: [f32; 4],
        radius: f32,
        border: f32,
        ramp: Ramp,
        texture: GLuint,
    ) {
        let colors = ramp.colors();
        if rect.w <= 0. || rect.h <= 0. {
            return;
        }
        // A fully transparent quad changes neither colour nor alpha; at 4K
        // every skipped fragment is fill-rate the GPU gets back.
        if colors[0][3] <= 0.001 && colors[1][3] <= 0.001 && !self.prof_old {
            return;
        }
        let underlay = texture != self.white.id
            && rect.x <= 0.
            && rect.y <= 0.
            && rect.w >= 1900.
            && rect.h >= 1000.;
        let start = self.vertices.len() / 14;
        // An outline only covers a ring along the edge. Rasterize four bands
        // (rounded corners included) instead of the whole interior, whose
        // fragments all have zero coverage. The SDF per fragment is unchanged.
        let band = radius.max(border) + 2.;
        let ring;
        let cells;
        let parts: &[[f32; 4]] = if self.prof_old {
            &[[0., 0., 1., 1.]]
        } else if border > 0. && rect.w > band * 4. && rect.h > band * 4. {
            let (bw, bh) = (band / rect.w, band / rect.h);
            ring = [
                [0., 0., 1., bh],
                [0., 1. - bh, 1., 1.],
                [0., bh, bw, 1. - bh],
                [1. - bw, bh, 1., 1. - bh],
            ];
            &ring
        } else if border <= 0. && !self.holes.is_empty() {
            cells = subtract_holes(rect, &self.holes);
            &cells
        } else {
            &[[0., 0., 1., 1.]]
        };
        for part in parts.iter().filter(|p| p[2] > p[0] && p[3] > p[1]) {
            for (cx, cy) in [(0, 1), (2, 1), (0, 3), (0, 3), (2, 1), (2, 3)] {
                let (x, y) = (part[cx], part[cy]);
                let progress = ramp.progress(x, y);
                let color: Color =
                    [0, 1, 2, 3].map(|i| colors[0][i] * (1. - progress) + colors[1][i] * progress);
                self.vertices.extend_from_slice(&[
                    rect.x + x * rect.w,
                    rect.y + y * rect.h,
                    uv[0] + x * (uv[2] - uv[0]),
                    uv[1] + y * (uv[3] - uv[1]),
                    color[0],
                    color[1],
                    color[2],
                    color[3],
                    (x - 0.5) * rect.w,
                    (y - 0.5) * rect.h,
                    rect.w / 2.,
                    rect.h / 2.,
                    radius,
                    border,
                ]);
            }
        }
        let added = self.vertices.len() / 14 - start;
        if added == 0 {
            return;
        }
        if let Some(batch) = self.batches.last_mut() {
            let same_clip = match (batch.clip, self.clip) {
                (None, None) => true,
                (Some(a), Some(b)) => a.x == b.x && a.y == b.y && a.w == b.w && a.h == b.h,
                _ => false,
            };
            if same_clip
                && batch.fade == self.fade
                && batch.round == self.round
                && let BatchKind::Paint {
                    texture: previous,
                    count,
                    crisp: previous_crisp,
                    underlay: previous_underlay,
                    ..
                } = &mut batch.kind
                && *previous == texture
                && *previous_crisp == (border > 0. || texture != self.white.id)
                && *previous_underlay == underlay
            {
                *count += added;
                return;
            }
        }
        self.batches.push(Batch {
            kind: BatchKind::Paint {
                texture,
                start,
                count: added,
                crisp: border > 0. || texture != self.white.id,
                underlay,
            },
            clip: self.clip,
            fade: self.fade,
            round: self.round,
        });
    }

    /// Two slanted half-bands (clear → peak → clear). Every vertex carries the
    /// CARD's local frame, so the rounded-rect SDF trims the band to the card.
    fn sheen(&mut self, card: Rect, radius: f32, progress: f32, color: Color) {
        if card.w <= 0. || card.h <= 0. || progress <= 0. || progress >= 1. {
            return;
        }
        let slant = card.h * 0.55;
        let half = card.w * 0.22;
        let travel = card.w + 2. * half + slant;
        let center = card.x - half + progress * travel;
        let clear = [color[0], color[1], color[2], 0.];
        let (cx, cy) = (card.x + card.w / 2., card.y + card.h / 2.);
        let start = self.vertices.len() / 14;
        for (x0, x1, c0, c1) in [
            (center - half, center, clear, color),
            (center, center + half, color, clear),
        ] {
            let corners = [
                (x0, card.y, c0),
                (x1, card.y, c1),
                (x0 - slant, card.y + card.h, c0),
                (x0 - slant, card.y + card.h, c0),
                (x1, card.y, c1),
                (x1 - slant, card.y + card.h, c1),
            ];
            for (x, y, c) in corners {
                self.vertices.extend_from_slice(&[
                    x,
                    y,
                    0.5,
                    0.5,
                    c[0],
                    c[1],
                    c[2],
                    c[3],
                    x - cx,
                    y - cy,
                    card.w / 2.,
                    card.h / 2.,
                    radius,
                    0.,
                ]);
            }
        }
        self.batches.push(Batch {
            kind: BatchKind::Paint {
                texture: self.white.id,
                start,
                count: 12,
                crisp: false,
                underlay: false,
            },
            clip: self.clip,
            fade: self.fade,
            round: self.round,
        });
    }

    fn text(&mut self, text: &str, rect: Rect, style: TextStyle, color: Color, centered: bool) {
        // Layout remains logical. Reuse the same run geometry at every density;
        // its shader snaps insertion origins and computes pixel-sized coverage.
        // Display is semantic: size must never turn nav pills or buttons into headlines.
        let display = style.display.unwrap_or(false);
        let heading = style.heading_override.unwrap_or(self.active_heading);
        let body = style.text_override.unwrap_or(self.active_text);
        let chosen_id = catalog_font_id(
            if display { heading.id() } else { body.id() },
            display,
            style.weight,
        );
        let size_f = style.size
            * if display {
                heading.scale() * self.heading_pct as f32 / 100.
            } else {
                body.scale() * self.text_pct as f32 / 100.
            };
        let viewport = self.stats.viewport;
        self.stats.max_logical_font_px = self.stats.max_logical_font_px.max(size_f);
        self.stats.max_physical_font_px = self
            .stats
            .max_physical_font_px
            .max(size_f * viewport.text_scale());
        let padded = rect.inset(-size_f);
        if padded.x + padded.w < 0.
            || padded.y + padded.h < 0.
            || padded.x > 1920.
            || padded.y > 1080.
            || self.clip.is_some_and(|clip| {
                padded.x + padded.w < clip.x
                    || padded.y + padded.h < clip.y
                    || padded.x > clip.x + clip.w
                    || padded.y > clip.y + clip.h
            })
        {
            return;
        }
        let origin = [rect.x, rect.y];
        let text_id = catalog_font_id(body.id(), false, style.weight);
        let key = TextKey {
            text: text.into(),
            weight: style.weight,
            font_id: chosen_id,
            fallback_id: text_id,
            display,
            layout: [
                size_f.to_bits(),
                style.tracking.to_bits(),
                style.line_height.to_bits(),
                rect.w.to_bits(),
                rect.h.to_bits(),
            ],
            flags: u8::from(style.align_right)
                | (u8::from(centered) << 1)
                | (u8::from(self.stats.multisample_samples > 1) << 2)
                | (u8::from(style.tabular_nums) << 3),
        };
        if self.text_runs.contains_key(&key) {
            self.submit_text(key, origin, color);
            return;
        }
        let rect = Rect::new(0., 0., rect.w, rect.h);
        let legacy_id = style.weight + if display { 1000 } else { 0 };
        let requested_id = chosen_id;
        let font_id = if self.ensure_font(requested_id) {
            requested_id
        } else {
            legacy_id
        };
        let text_ready = display && self.ensure_font(text_id);
        let legacy_ready = font_id != legacy_id && self.ensure_font(legacy_id);
        let cjk_id = if style.weight >= 500 { 2500 } else { 2400 };
        let hangul_id = cjk_id + 1000;
        if !self.ensure_font(font_id) {
            self.stats.glyph_failures += text.chars().count();
            return;
        }
        let (mut needs_cjk, mut needs_hangul) = (false, false);
        for c in text.chars() {
            if !c.is_ascii_control()
                && self.fonts[&font_id].lookup_glyph_index(c) == 0
                && (!text_ready || self.fonts[&text_id].lookup_glyph_index(c) == 0)
                && (!legacy_ready || self.fonts[&legacy_id].lookup_glyph_index(c) == 0)
            {
                if is_hangul(c) {
                    needs_hangul = true;
                } else {
                    needs_cjk = true;
                }
            }
        }
        // Each fallback loads only when a run needs it, so Korean text never
        // pulls in the larger Chinese font and vice versa.
        let cjk_ready = needs_cjk && self.ensure_font(cjk_id);
        let hangul_ready = needs_hangul && self.ensure_font(hangul_id);
        let fonts = &self.fonts;
        let choose_font = |c: char| {
            if fonts[&font_id].lookup_glyph_index(c) != 0 {
                font_id
            } else if text_ready && fonts[&text_id].lookup_glyph_index(c) != 0 {
                text_id
            } else if legacy_ready && fonts[&legacy_id].lookup_glyph_index(c) != 0 {
                legacy_id
            } else if is_hangul(c) {
                if hangul_ready { hangul_id } else { font_id }
            } else if cjk_ready {
                cjk_id
            } else {
                font_id
            }
        };
        let font = &fonts[&font_id];
        // Fixed by semantic text role, so switching families never changes row height.
        let line_height = style.size * style.line_height;
        let baseline = font.horizontal_line_metrics(size_f).map_or(size_f, |m| {
            (line_height - m.ascent + m.descent) / 2. + m.ascent
        });
        let tracking = style.tracking * size_f;
        let max_digit = if style.tabular_nums {
            "0123456789"
                .chars()
                .map(|c| fonts[&choose_font(c)].metrics(c, size_f).advance_width)
                .fold(0.0_f32, f32::max)
        } else {
            0.
        };
        let advance = |c: char| {
            if c == '\n' {
                0.
            } else {
                if style.tabular_nums && c.is_ascii_digit() {
                    max_digit + tracking
                } else {
                    fonts[&choose_font(c)].metrics(c, size_f).advance_width + tracking
                }
            }
        };
        let mut lines: Vec<String> = Vec::new();
        let mut line = String::new();
        let mut width = 0.;
        // Word wrapping, with character fallback for a title wider than the tile.
        for word in text.split_inclusive(char::is_whitespace) {
            let word_width: f32 = word.chars().map(advance).sum();
            if width > 0. && width + word_width > rect.w {
                lines.push(line.trim_end().into());
                line.clear();
                width = 0.;
            }
            for c in word.chars() {
                if c == '\n' {
                    lines.push(line.trim_end().into());
                    line.clear();
                    width = 0.;
                    continue;
                }
                let w = advance(c);
                if width > 0. && width + w > rect.w {
                    lines.push(line.trim_end().into());
                    line.clear();
                    width = 0.;
                }
                line.push(c);
                width += w;
            }
        }
        if !line.is_empty() {
            lines.push(line.trim_end().into());
        }
        // Replace clipped trailing text with a measured ellipsis, not half a glyph.
        // Row height is fixed by the semantic role, never by the chosen font.
        let visible_lines = (rect.h / line_height).floor().max(1.) as usize;
        if lines.len() > visible_lines {
            lines.truncate(visible_lines);
            if let Some(last) = lines.last_mut() {
                let ellipsis_width = advance('…');
                let mut used: f32 = last.chars().map(advance).sum();
                while used + ellipsis_width > rect.w && !last.is_empty() {
                    let removed = last.pop().unwrap();
                    used -= advance(removed);
                }
                if ellipsis_width <= rect.w {
                    last.push('…');
                }
            }
        }
        let mut placed = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            let top = rect.y + index as f32 * line_height;
            if index > 0 && top + line_height > rect.y + rect.h + 1. {
                break;
            }
            let mut previous = None;
            let width: f32 = line
                .chars()
                .map(|c| {
                    let kern = previous
                        .and_then(|p| font.horizontal_kern(p, c, size_f))
                        .unwrap_or(0.);
                    previous = Some(c);
                    advance(c) + kern
                })
                .sum();
            let mut x = rect.x
                + if style.align_right {
                    rect.w - width
                } else if centered {
                    (rect.w - width) / 2.
                } else {
                    0.
                };
            let mut previous = None;
            for c in line.chars() {
                if let Some(previous) = previous {
                    x += font.horizontal_kern(previous, c, size_f).unwrap_or(0.);
                }
                let selected = choose_font(c);
                let baseline = if selected == font_id {
                    baseline
                } else {
                    fonts[&selected]
                        .horizontal_line_metrics(size_f)
                        .map_or(baseline, |m| {
                            (line_height - m.ascent + m.descent) / 2. + m.ascent
                        })
                };
                let digit_offset = if style.tabular_nums && c.is_ascii_digit() {
                    (max_digit - fonts[&selected].metrics(c, size_f).advance_width) / 2.
                } else {
                    0.
                };
                placed.push((c, x + digit_offset, top + baseline, selected));
                x += advance(c);
                previous = Some(c);
            }
        }
        let mut vertices = Vec::new();
        let mut missing_glyphs = 0;
        let mut failures = 0;
        let multisampled = self.stats.multisample_samples > 1;
        for (c, x, baseline, selected) in placed {
            if self.fonts[&selected].lookup_glyph_index(c) == 0 && !c.is_whitespace() {
                missing_glyphs += 1;
            }
            match self.glyph(c, selected) {
                Ok(geometry) => {
                    for triangle in geometry.chunks_exact(3) {
                        // True sample coverage needs only the original fill.
                        // Exclude the boundary fringe, not just its alpha.
                        if multisampled && triangle.iter().any(|vertex| vertex.coverage < 1.) {
                            continue;
                        }
                        for vertex in triangle {
                            vertices.extend_from_slice(&[
                                vertex.position.x * size_f,
                                vertex.position.y * size_f,
                                if multisampled { 0. } else { vertex.offset[0] },
                                if multisampled { 0. } else { vertex.offset[1] },
                                x,
                                baseline,
                                0.,
                                0.,
                                vertex.coverage,
                            ]);
                        }
                    }
                }
                Err(error) => {
                    eprintln!("Luma glyph {c:?}: {error}");
                    failures += 1;
                }
            }
        }
        let mesh = if vertices.is_empty() {
            None
        } else {
            match vector::Mesh::upload(1., 1., &vertices) {
                Ok(mesh) => Some(mesh),
                Err(error) => {
                    eprintln!("Luma text: {error}");
                    failures += 1;
                    None
                }
            }
        };
        self.text_runs.insert(
            key.clone(),
            TextRun {
                mesh,
                missing_glyphs,
                failures,
                frame: self.frame,
            },
        );
        self.submit_text(key, origin, color);
    }

    fn submit_text(&mut self, key: TextKey, origin: [f32; 2], tint: Color) {
        let run = self.text_runs.get_mut(&key).unwrap();
        run.frame = self.frame;
        self.stats.missing_glyphs += run.missing_glyphs;
        self.stats.glyph_failures += run.failures;
        if let Some(mesh) = &run.mesh {
            self.stats.text_triangles += mesh.vertices as usize / 3;
            self.batches.push(Batch {
                kind: BatchKind::Text { key, origin, tint },
                clip: self.clip,
                fade: self.fade,
                round: self.round,
            });
        }
    }

    fn image(&mut self, path: &str, mut rect: Rect, radius: f32, contain: bool, tint: Color) {
        if let Some(source) = path.strip_suffix("#frost") {
            self.frost(source, rect, radius, tint, None);
            return;
        }
        if let Some(source) = path.strip_suffix("#async") {
            self.upload_posters();
            if !self.textures.contains_key(source) {
                if !self.posters.pending.contains(source)
                    && self.posters.requests.send(source.to_owned()).is_ok()
                {
                    self.posters.pending.insert(source.to_owned());
                }
                if self.posters.pending.contains(source) {
                    return;
                }
            }
            self.image(source, rect, radius, contain, tint);
            return;
        }
        if path.ends_with(".lvec") {
            self.vector_image(path, rect, contain, tint);
            return;
        }
        let path = if self.stats.viewport.scale > 1.
            && self.info.max_texture_size >= 3840
            && Path::new(path)
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|p| p == "wallpapers")
        {
            self.artwork_variants
                .entry(path.into())
                .or_insert_with(|| {
                    let original = Path::new(path);
                    let variant = original.with_file_name(format!(
                        "{}@2x.jpg",
                        original.file_stem().unwrap_or_default().to_string_lossy()
                    ));
                    if variant.is_file() {
                        variant.to_string_lossy().into_owned()
                    } else {
                        path.into()
                    }
                })
                .clone()
        } else {
            path.into()
        };
        if path.ends_with("@2x.jpg") {
            self.stats.high_resolution_wallpaper = true;
        }
        let path = match camera_key(&path) {
            // Snapshots decode off the GL thread; until the new one is in, the
            // camera keeps showing its previous snapshot.
            Some(camera) if !self.textures.contains_key(&path) => {
                if !self.stills.pending.contains(&path)
                    && self.stills.requests.send((path.clone(), false)).is_ok()
                {
                    self.stills.pending.insert(path.clone());
                }
                match self.cameras.get(camera) {
                    Some(previous) if self.textures.contains_key(previous) => previous.clone(),
                    _ => return,
                }
            }
            // media posters, banners and stills decode off the GL thread
            // too: a title card never freezes its entrance on a big JPEG. They
            // use the two cover threads, not the stills queue, so a wallpaper
            // still decoding never holds a card's banner back.
            _ if is_poster_cache(&path) && !self.textures.contains_key(&path) => {
                self.upload_posters();
                if !self.textures.contains_key(&path) {
                    if !self.posters.pending.contains(&path)
                        && self.posters.requests.send(path.clone()).is_ok()
                    {
                        self.posters.pending.insert(path.clone());
                    }
                    if self.posters.pending.contains(&path) {
                        return;
                    }
                }
                path
            }
            _ => path,
        };
        if !self.textures.contains_key(&path) && self.stills.pending.contains(&path) {
            self.upload_stills(Some(&path));
        }
        let tint = match self.appeared.get(&path) {
            Some(at) => {
                let k = (at.elapsed().as_secs_f32() / IMAGE_FADE).min(1.);
                if k < 1. {
                    self.fresh = true;
                } else {
                    self.appeared.remove(&path);
                }
                [tint[0], tint[1], tint[2], tint[3] * k * k * (3. - 2. * k)]
            }
            None => tint,
        };
        let frame = self.frame;
        let entry = self
            .textures
            .entry(path.clone())
            .or_insert_with(|| match open_image(&path) {
                Ok(image) => {
                    let image = image.into_rgba8();
                    Some(texture(image.width(), image.height(), image.as_raw()))
                }
                Err(error) => {
                    eprintln!("Artwork {path}: {error}");
                    None
                }
            });
        let Some(texture) = entry else {
            self.stats.missing_images += 1;
            return;
        };
        texture.frame = frame;
        let id = texture.id;
        if contain {
            let scale = (rect.w / texture.width as f32).min(rect.h / texture.height as f32);
            let w = texture.width as f32 * scale;
            let h = texture.height as f32 * scale;
            rect = Rect::new(rect.x + (rect.w - w) / 2., rect.y + (rect.h - h) / 2., w, h);
        }
        // Server artwork never stretches: a picture of another shape fills
        // the card from its centre and the overflow is cut off.
        let uv = if !contain && is_poster_cache(&path) {
            cover_uv(
                texture.width as f32 / texture.height as f32,
                rect.w / rect.h,
            )
        } else {
            [0., 0., 1., 1.]
        };
        self.quad(rect, uv, radius, 0., tint, id);
    }

    fn upload_posters(&mut self) {
        while let Ok((path, decoded)) = self.posters.replies.try_recv() {
            self.posters.pending.remove(&path);
            let uploaded = decoded.map(|pixels| {
                let mut t = texture(pixels.width(), pixels.height(), pixels.as_raw());
                t.frame = self.frame;
                t
            });
            if uploaded.is_none() {
                eprintln!("Artwork {path}: cannot decode");
            }
            if uploaded.is_some() && is_poster_cache(&path) {
                self.appeared
                    .insert(path.clone(), std::time::Instant::now());
                self.fresh = true;
            }
            self.textures.insert(path, uploaded);
        }
    }

    /// Keep these clip stills decoded and resident: the current clip and the
    /// ones a Next/Previous press or the automatic swap would show next.
    pub fn preload_stills(&mut self, paths: &[String]) {
        self.stills.pinned = paths
            .iter()
            .flat_map(|path| [path.clone(), format!("{path}#frost")])
            .collect();
        for path in paths {
            let resident = self.textures.contains_key(path)
                && self.textures.contains_key(&format!("{path}#frost"));
            if resident || self.stills.pending.contains(path) {
                continue;
            }
            if self.stills.requests.send((path.clone(), true)).is_ok() {
                self.stills.pending.insert(path.clone());
            }
        }
    }

    /// Upload stills the loader has finished; with `wait_for`, block until
    /// that one is in (it is already decoding, so this beats decoding again).
    fn upload_stills(&mut self, wait_for: Option<&str>) {
        loop {
            let reply = match wait_for {
                Some(path) if self.stills.pending.contains(path) => self.stills.replies.recv().ok(),
                _ => self.stills.replies.try_recv().ok(),
            };
            let Some((path, decoded)) = reply else {
                break;
            };
            self.stills.pending.remove(&path);
            let Some((full, frost)) = decoded else {
                continue;
            };
            if let Some(camera) = camera_key(&path) {
                // A newer snapshot replaces the camera's previous one.
                if let Some(old) = self.cameras.insert(camera.to_owned(), path.clone())
                    && old != path
                {
                    self.textures.remove(&old);
                }
                self.fresh = true;
            }
            if is_poster_cache(&path) {
                self.fresh = true;
            }
            if let Some(frost) = frost {
                self.frost_pixels
                    .insert(path.clone(), frost.as_raw().clone());
                let mut uploaded = texture(frost.width(), frost.height(), frost.as_raw());
                uploaded.frame = self.frame;
                self.textures
                    .insert(format!("{path}#frost"), Some(uploaded));
            }
            let mut uploaded = texture(full.width(), full.height(), full.as_raw());
            uploaded.frame = self.frame;
            self.textures.insert(path, Some(uploaded));
        }
    }

    /// Build the text runs, icons and artwork of `commands` without drawing
    /// them, so the first frame that shows them only submits cached geometry.
    pub fn prepare(&mut self, commands: &[Draw]) {
        let saved = (self.clip.take(), self.fade.take(), self.round.take());
        let vertices = self.vertices.len();
        let batches = self.batches.len();
        for command in commands {
            match command {
                Draw::Text(text, rect, style, color, centered) => {
                    self.text(text, *rect, *style, *color, *centered)
                }
                Draw::Image(path, rect, radius, contain) if camera_key(path).is_none() => {
                    self.image(path, *rect, *radius, *contain, [1.; 4])
                }
                Draw::TintedImage(path, rect, radius, contain, tint)
                    if camera_key(path).is_none() =>
                {
                    self.image(path, *rect, *radius, *contain, *tint)
                }
                Draw::AppImage(image, rect, tint) => self.app_image(image, *rect, *tint),
                _ => {}
            }
        }
        self.vertices.truncate(vertices);
        self.batches.truncate(batches);
        (self.clip, self.fade, self.round) = saved;
    }

    /// Camera snapshots or `#async` covers are still decoding off the GL thread.
    pub fn decoding(&self) -> bool {
        !self.stills.pending.is_empty() || !self.posters.pending.is_empty()
    }

    /// A camera snapshot arrived since the last call: the scene needs a redraw.
    pub fn take_fresh(&mut self) -> bool {
        self.upload_stills(None);
        self.upload_posters();
        std::mem::take(&mut self.fresh)
    }

    /// Frosted glass under a plate: the full-screen image `source`, blurred
    /// once at 240x135 and stretched back, shown only inside `rect`.
    fn frost(&mut self, source: &str, rect: Rect, radius: f32, tint: Color, fill: Option<Color>) {
        let key = format!("{source}#frost");
        if !self.textures.contains_key(&key) && self.stills.pending.contains(source) {
            self.upload_stills(Some(source));
        }
        let frame = self.frame;
        let plain = self
            .textures
            .entry(key)
            .or_insert_with(|| match open_image(source) {
                Ok(image) => {
                    let blurred = frost_pixels(&image);
                    let uploaded = texture(blurred.width(), blurred.height(), blurred.as_raw());
                    self.frost_pixels.insert(source.into(), blurred.into_raw());
                    Some(uploaded)
                }
                Err(error) => {
                    eprintln!("Frost {source}: {error}");
                    None
                }
            });
        let Some(plain) = plain else {
            return;
        };
        plain.frame = frame;
        let plain_id = plain.id;
        let uv = [
            rect.x / 1920.,
            rect.y / 1080.,
            (rect.x + rect.w) / 1920.,
            (rect.y + rect.h) / 1080.,
        ];
        let Some(fill) = fill else {
            self.quad(rect, uv, radius, 0., tint, plain_id);
            return;
        };
        // A baked plate stores frost * tint * (1 - a) + fill * a, exactly
        // what the two blended layers produced, and draws untinted.
        let q = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
        let baked_key = format!(
            "{source}#frost#{:02x}{:02x}{:02x}#{:02x}{:02x}{:02x}{:02x}",
            q(tint[0]),
            q(tint[1]),
            q(tint[2]),
            q(fill[0]),
            q(fill[1]),
            q(fill[2]),
            q(fill[3])
        );
        if !self.textures.contains_key(&baked_key) {
            let Some(pixels) = self.frost_pixels.get(source) else {
                // No CPU copy (yet): the two original layers.
                self.quad(rect, uv, radius, 0., tint, plain_id);
                self.quad(rect, [0., 0., 1., 1.], radius, 0., fill, self.white.id);
                return;
            };
            let mut baked = pixels.clone();
            let a = fill[3].clamp(0., 1.);
            for pixel in baked.chunks_exact_mut(4) {
                for i in 0..3 {
                    let frost = f32::from(pixel[i]) * tint[i];
                    pixel[i] = (frost * (1. - a) + fill[i] * 255. * a)
                        .round()
                        .clamp(0., 255.) as u8;
                }
            }
            self.textures
                .insert(baked_key.clone(), Some(texture(240, 135, &baked)));
        }
        let Some(Some(baked)) = self.textures.get_mut(&baked_key) else {
            return;
        };
        baked.frame = frame;
        let id = baked.id;
        self.quad(rect, uv, radius, 0., [1.; 4], id);
    }

    fn vector_image(&mut self, path: &str, mut rect: Rect, contain: bool, tint: Color) {
        if rect.w <= 0. || rect.h <= 0. {
            return;
        }
        let mesh = self.vectors.entry(path.into()).or_insert_with(|| {
            match vector::Mesh::load(Path::new(path)) {
                Ok(mesh) => Some(mesh),
                Err(error) => {
                    eprintln!("{error}");
                    None
                }
            }
        });
        let Some(mesh) = mesh else {
            self.stats.missing_images += 1;
            return;
        };
        if contain {
            let scale = (rect.w / mesh.width).min(rect.h / mesh.height);
            let width = mesh.width * scale;
            let height = mesh.height * scale;
            rect = Rect::new(
                rect.x + (rect.w - width) / 2.,
                rect.y + (rect.h - height) / 2.,
                width,
                height,
            );
        }
        let clip = if let Some(clip) = self.clip {
            let x = rect.x.max(clip.x);
            let y = rect.y.max(clip.y);
            Rect::new(
                x,
                y,
                (rect.x + rect.w).min(clip.x + clip.w).max(x) - x,
                (rect.y + rect.h).min(clip.y + clip.h).max(y) - y,
            )
        } else {
            rect
        };
        if clip.w <= 0. || clip.h <= 0. {
            return;
        }
        self.stats.vector_triangles += mesh.vertices as usize / 3;
        self.batches.push(Batch {
            kind: BatchKind::Vector {
                path: path.into(),
                rect,
                tint,
            },
            clip: Some(clip),
            fade: self.fade,
            round: self.round,
        });
    }

    fn app_image(&mut self, image: &crate::app_artwork::Artwork, rect: Rect, tint: Color) {
        let entry = self.textures.entry(image.key.clone()).or_insert_with(|| {
            Some(texture(
                image.pixels.width(),
                image.pixels.height(),
                image.pixels.as_raw(),
            ))
        });
        let Some(texture) = entry else {
            return;
        };
        texture.frame = self.frame;
        let scale = (rect.w / texture.width as f32).min(rect.h / texture.height as f32);
        let w = texture.width as f32 * scale;
        let h = texture.height as f32 * scale;
        let id = texture.id;
        self.quad(
            Rect::new(rect.x + (rect.w - w) / 2., rect.y + (rect.h - h) / 2., w, h),
            [0., 0., 1., 1.],
            0.,
            0.,
            tint,
            id,
        );
    }

    /// Upload one hardware-decoded RGB frame into a reused GPU texture, on the GL thread.
    pub fn upload_aerial(&mut self, frame: &[u8]) {
        use crate::aerial_video::{BYTES_PER_FRAME, HEIGHT, WIDTH};
        if frame.len() != BYTES_PER_FRAME {
            return;
        }
        unsafe {
            if self.aerial_texture.is_none() {
                let mut id = 0;
                gl::GenTextures(1, &mut id);
                gl::BindTexture(gl::TEXTURE_2D, id);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
                gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
                gl::TexImage2D(
                    gl::TEXTURE_2D,
                    0,
                    gl::RGB as i32,
                    WIDTH as i32,
                    HEIGHT as i32,
                    0,
                    gl::RGB,
                    gl::UNSIGNED_BYTE,
                    frame.as_ptr().cast(),
                );
                self.aerial_texture = Some(Texture {
                    id,
                    width: WIDTH as u32,
                    height: HEIGHT as u32,
                    frame: self.frame,
                });
            } else if let Some(texture) = &self.aerial_texture {
                gl::BindTexture(gl::TEXTURE_2D, texture.id);
                gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
                gl::TexSubImage2D(
                    gl::TEXTURE_2D,
                    0,
                    0,
                    0,
                    WIDTH as i32,
                    HEIGHT as i32,
                    gl::RGB,
                    gl::UNSIGNED_BYTE,
                    frame.as_ptr().cast(),
                );
            }
        }
    }
    fn effects_signature(&self) -> u64 {
        // Stable and conservative: includes every paint vertex and batch clip.
        // Never re-use a cached FBO across animated geometry or a changing image ID.
        let mut hash = 0xcbf29ce484222325u64;
        for &n in &self.vertices {
            hash ^= n.to_bits() as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        for batch in &self.batches {
            hash ^= batch.clip.map_or(0, |c| {
                c.x.to_bits() as u64
                    ^ (c.y.to_bits() as u64).rotate_left(11)
                    ^ (c.w.to_bits() as u64).rotate_left(23)
                    ^ (c.h.to_bits() as u64).rotate_left(37)
            });
            hash = hash.wrapping_mul(0x100000001b3);
            if let Some((r, radius)) = batch.round {
                for item in [r.x, r.y, r.w, r.h, radius] {
                    hash ^= item.to_bits() as u64;
                    hash = hash.wrapping_mul(0x100000001b3);
                }
            }
            if let Some(f) = batch.fade {
                for item in f {
                    hash ^= item.to_bits() as u64;
                    hash = hash.wrapping_mul(0x100000001b3);
                }
            }
        }
        hash ^ self.batches.len() as u64
    }
    unsafe fn effects_blit(&self, screen: crate::raster::Viewport) {
        // Vertices in UI coordinates; a negative radius flags simple textured sampling.
        let mut vertices = Vec::with_capacity(6 * 14);
        for (x, y, u, v) in [
            (0., 0., 0., 1.),
            (1920., 0., 1., 1.),
            (0., 1080., 0., 0.),
            (0., 1080., 0., 0.),
            (1920., 0., 1., 1.),
            (1920., 1080., 1., 0.),
        ] {
            vertices.extend_from_slice(&[x, y, u, v, 1., 1., 1., 1., 0., 0., 0., 0., -1., 0.]);
        }
        unsafe {
            gl::UseProgram(self.program);
            gl::Uniform1i(
                gl::GetUniformLocation(self.program, c"u_texture".as_ptr()),
                0,
            );
            gl::Uniform4f(
                gl::GetUniformLocation(self.program, c"u_edge_fade".as_ptr()),
                0.,
                0.,
                0.,
                0.,
            );
            gl::Uniform4f(
                gl::GetUniformLocation(self.program, c"u_round_clip".as_ptr()),
                0.,
                0.,
                0.,
                0.,
            );
            gl::Uniform1f(
                gl::GetUniformLocation(self.program, c"u_aa".as_ptr()),
                0.425,
            );
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.effects_texture);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.buffer);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (vertices.len() * 4) as isize,
                vertices.as_ptr().cast(),
                gl::DYNAMIC_DRAW,
            );
            let mut offset = 0;
            for (index, size) in [2, 2, 4, 4, 2].into_iter().enumerate() {
                gl::EnableVertexAttribArray(index as u32);
                gl::VertexAttribPointer(
                    index as u32,
                    size,
                    gl::FLOAT,
                    gl::FALSE,
                    56,
                    (offset * 4) as *const _,
                );
                offset += size as usize;
            }
            gl::Viewport(screen.x, screen.y, screen.width, screen.height);
            gl::Disable(gl::SCISSOR_TEST);
            // Offscreen framebuffer already holds premultiplied RGBA.
            gl::BlendFuncSeparate(
                gl::ONE,
                gl::ONE_MINUS_SRC_ALPHA,
                gl::ONE,
                gl::ONE_MINUS_SRC_ALPHA,
            );
            gl::DrawArrays(gl::TRIANGLES, 0, 6);
            gl::BlendFuncSeparate(
                gl::SRC_ALPHA,
                gl::ONE_MINUS_SRC_ALPHA,
                gl::ONE,
                gl::ONE_MINUS_SRC_ALPHA,
            );
            // Restore native vertex buffer before drawing 4K text/icons/borders.
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (self.vertices.len() * 4) as isize,
                self.vertices.as_ptr().cast(),
                gl::DYNAMIC_DRAW,
            );
        }
    }

    pub fn render(&mut self, commands: &[Draw], width: u32, height: u32, video: bool) {
        let started = Instant::now();
        self.stats = RenderStats::new(Size::new(width, height));
        // Captures can bind a single-sample FBO even when the actual TV window
        // is multisampled, so query the active target rather than assume.
        let mut samples = 0;
        unsafe { gl::GetIntegerv(gl::SAMPLES, &mut samples) };
        self.stats.multisample_samples = samples.max(0) as u32;
        if samples > 1 {
            self.stats.font_antialiasing = "multisample vector coverage";
        }
        self.frame += 1;
        self.upload_stills(None);
        self.vertices.clear();
        self.batches.clear();
        self.clip = None;
        self.fade = None;
        self.round = None;
        // Composited before every UI draw: video can no longer disappear behind
        // an unrelated window/plane, and is clipped by the exact same framebuffer.
        if video
            && !cfg!(all(
                feature = "webos",
                target_os = "linux",
                target_arch = "arm"
            ))
            && let Some(ref aerial) = self.aerial_texture
        {
            let id = aerial.id;
            self.quad(
                Rect::new(0., 0., 1920., 1080.),
                [0., 0., 1., 1.],
                0.,
                0.,
                [1.; 4],
                id,
            );
        }
        let covers = if self.prof_old {
            Vec::new()
        } else {
            // Only artwork already uploaded counts: a missing or still
            // loading photo draws nothing, and must not uncover a hole.
            opaque_covers(commands, |path| {
                if let Some(source) = opaque_frost(path) {
                    return matches!(self.textures.get(&format!("{source}#frost")), Some(Some(_)));
                }
                let path = self.artwork_variants.get(path).map_or(path, String::as_str);
                matches!(self.textures.get(path), Some(Some(_)))
            })
        };
        let mut merged_next = false;
        for (index, command) in commands.iter().enumerate() {
            if std::mem::take(&mut merged_next) {
                continue;
            }
            // Paint of this command under a later opaque cover is overwritten
            // anyway: skip those fragments (text and icons are unaffected).
            self.holes.clear();
            if matches!(
                command,
                Draw::Shape(..)
                    | Draw::RoundedGradient(..)
                    | Draw::DiagonalGradient(..)
                    | Draw::HGradient(..)
                    | Draw::Shadow(..)
            ) {
                let reach = command_bounds(command);
                let mut found: Vec<Rect> = covers
                    .iter()
                    .filter(|(at, cover)| *at > index && reach.is_some_and(|r| overlaps(r, *cover)))
                    .map(|(_, cover)| *cover)
                    .collect();
                found.sort_by(|a, b| (b.w * b.h).total_cmp(&(a.w * a.h)));
                found.truncate(3);
                self.holes = found;
            }
            // Fill-rate peepholes for 4K: identical pixels, fewer fragments.
            let plain = self.fade.is_none() && self.round.is_none() && !self.prof_old;
            match (command, commands.get(index + 1)) {
                // Two flat full-screen tints (video dim + Settings scrim) are
                // one tint: the same "over" result in a single screen fill.
                // Radius -1 selects the plain shader path: no edge AA, so the
                // split Home bands below meet without a seam.
                (Draw::Shape(a, 0., 0., under), Some(Draw::Shape(b, 0., 0., over)))
                    if plain && covers_screen(*a) && covers_screen(*b) =>
                {
                    self.quad(
                        Rect::new(0., 0., 1920., 1080.),
                        [0., 0., 1., 1.],
                        -1.,
                        0.,
                        tint_over(*under, *over),
                        self.white.id,
                    );
                    merged_next = true;
                    continue;
                }
                // A full-screen tint under a full-width bottom gradient (Home
                // floor): tint alone above it, both composed into one gradient below.
                (
                    Draw::Shape(a, 0., 0., under),
                    Some(Draw::RoundedGradient(floor, 0., top, bottom)),
                ) if plain
                    && covers_screen(*a)
                    && floor.x <= 0.
                    && floor.x + floor.w >= 1920.
                    && floor.y > 0.
                    && floor.y + floor.h >= 1080. =>
                {
                    let white = self.white.id;
                    self.quad(
                        Rect::new(0., 0., 1920., floor.y),
                        [0., 0., 1., 1.],
                        -1.,
                        0.,
                        *under,
                        white,
                    );
                    self.gradient_quad(
                        Rect::new(0., floor.y, 1920., 1080. - floor.y),
                        [0., 0., 1., 1.],
                        -1.,
                        0.,
                        Ramp::Vertical(tint_over(*under, *top), tint_over(*under, *bottom)),
                        white,
                    );
                    merged_next = true;
                    continue;
                }
                // Frost + the plate's translucent fill become one opaque quad
                // with the fill baked into the 240x135 frost texture.
                (
                    Draw::TintedImage(path, rect, radius, false, tint),
                    Some(Draw::Shape(fill_rect, fill_radius, border, fill)),
                ) if plain
                    && *border == 0.
                    && fill_rect == rect
                    && fill_radius == radius
                    && tint[3] >= 0.999
                    && let Some(source) = opaque_frost(path) =>
                {
                    self.frost(source, *rect, *radius, *tint, Some(*fill));
                    merged_next = true;
                    continue;
                }
                // An opaque full-screen photo under a black scrim: darken the
                // photo itself instead of filling the whole screen twice.
                (
                    Draw::Image(path, rect, radius, false),
                    Some(Draw::Shape(scrim, 0., border, color)),
                ) if plain
                    && *border == 0.
                    && covers_screen(*rect)
                    && covers_screen(*scrim)
                    && color[..3].iter().all(|c| *c <= 0.001)
                    && is_jpeg(path) =>
                {
                    let keep = 1. - color[3].clamp(0., 1.);
                    self.image(path, *rect, *radius, false, [keep, keep, keep, 1.]);
                    merged_next = true;
                    continue;
                }
                _ => {}
            }
            match command {
                Draw::Shape(rect, radius, border, color) => self.quad(
                    *rect,
                    [0., 0., 1., 1.],
                    *radius,
                    *border,
                    *color,
                    self.white.id,
                ),
                Draw::RoundedGradient(rect, radius, top, bottom) => self.gradient_quad(
                    *rect,
                    [0., 0., 1., 1.],
                    *radius,
                    0.,
                    Ramp::Vertical(*top, *bottom),
                    self.white.id,
                ),
                Draw::HGradient(rect, radius, left, right) => self.gradient_quad(
                    *rect,
                    [0., 0., 1., 1.],
                    *radius,
                    0.,
                    Ramp::Horizontal(*left, *right),
                    self.white.id,
                ),
                Draw::DiagonalGradient(rect, radius, first, second) => self.gradient_quad(
                    *rect,
                    [0., 0., 1., 1.],
                    *radius,
                    0.,
                    Ramp::Diagonal(*first, *second),
                    self.white.id,
                ),
                Draw::DiagonalGradientBorder(rect, radius, border, first, second) => self
                    .gradient_quad(
                        *rect,
                        [0., 0., 1., 1.],
                        *radius,
                        *border,
                        Ramp::Diagonal(*first, *second),
                        self.white.id,
                    ),
                Draw::Sheen(rect, radius, progress, color) => {
                    self.sheen(*rect, *radius, *progress, *color)
                }
                Draw::Shadow(rect, radius, blur, color) => self.quad(
                    rect.inset(-*blur),
                    [0., 0., 1., 1.],
                    *radius,
                    -*blur,
                    *color,
                    self.white.id,
                ),
                Draw::Text(text, rect, size, color, centered) => {
                    self.text(text, *rect, *size, *color, *centered)
                }
                Draw::Image(path, rect, radius, contain) => {
                    self.image(path, *rect, *radius, *contain, [1.; 4])
                }
                Draw::TintedImage(path, rect, radius, contain, tint) => {
                    self.image(path, *rect, *radius, *contain, *tint)
                }
                Draw::AppImage(image, rect, tint) => self.app_image(image, *rect, *tint),
                Draw::Clip(clip) => self.clip = *clip,
                Draw::RoundClip(round) => self.round = *round,
                Draw::EdgeFade(fade) => self.fade = *fade,
            }
        }
        self.stats.build_ms = started.elapsed().as_secs_f64() * 1000.;
        let Viewport {
            x: vx,
            y: vy,
            width: vw,
            height: vh,
            scale,
        } = self.stats.viewport;
        // 2.2.44 hotfix safety fallback. Even the three-stage compositor may
        // reorder intermediate photos against glass in mixed scenes. Native
        // 4K single-pass painting respects the original Draw batch sequence,
        // including every opaque wallpaper, transparent scrim, card, and icon.
        // Do not re-enable hybrid until ordering and FBO alpha are proven on TV.
        let hybrid = false;
        let stamp = if hybrid {
            Some(self.effects_signature())
        } else {
            None
        };
        let refresh = stamp.is_some() && stamp != self.effects_stamp;
        // Explicit opt-in marker: only diagnostic runs serialize the GPU with glFinish.
        // Never incur synchronization stalls in a normal TV session.
        let trace_gl = self.prof_finish
            || (cfg!(feature = "webos")
                && std::path::Path::new("/media/developer/com.denisefremov.luma/profile-gl")
                    .exists());
        self.stats.cached_effect = hybrid && !refresh;
        let mut original_fbo = 0;
        self.stats.effect_render_ms = 0.;
        self.stats.crisp_render_ms = 0.;
        unsafe {
            gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut original_fbo);
            // Three ordered stages preserve the wallpaper -> glass -> text z-order.
            // Stage 0: native-resolution underlay; 1: 1080p static effects FBO;
            // 2: cached effects composite followed by crisp icons and glyphs.
            for pass in 0..if hybrid { 3 } else { 1 } {
                if hybrid && pass == 1 && !refresh {
                    continue;
                }
                let layer_start = Instant::now();
                let (px, py, pw, ph, pixel_scale) = if hybrid && pass == 1 {
                    gl::BindFramebuffer(gl::FRAMEBUFFER, self.effects_fbo);
                    gl::Viewport(0, 0, 1920, 1080);
                    gl::ClearColor(0., 0., 0., 0.);
                    gl::Clear(gl::COLOR_BUFFER_BIT);
                    (0, 0, 1920, 1080, 1.)
                } else {
                    gl::BindFramebuffer(gl::FRAMEBUFFER, original_fbo as u32);
                    gl::Viewport(0, 0, width as i32, height as i32);
                    if !hybrid || pass == 0 {
                        if video {
                            gl::ClearColor(0., 0., 0., 0.);
                        } else {
                            gl::ClearColor(0.04, 0.04, 0.05, 1.);
                        }
                        gl::Clear(gl::COLOR_BUFFER_BIT);
                    }
                    if hybrid && pass == 2 {
                        self.effects_blit(self.stats.viewport);
                    }
                    (vx, vy, vw, vh, scale)
                };
                gl::Viewport(px, py, pw, ph);
                if self.vao != 0 {
                    gl::BindVertexArray(self.vao);
                }
                gl::BindBuffer(gl::ARRAY_BUFFER, self.buffer);
                gl::BufferData(
                    gl::ARRAY_BUFFER,
                    (self.vertices.len() * size_of::<f32>()) as isize,
                    self.vertices.as_ptr().cast(),
                    gl::DYNAMIC_DRAW,
                );
                gl::UseProgram(self.program);
                gl::Uniform1f(
                    gl::GetUniformLocation(self.program, c"u_aa".as_ptr()),
                    0.85 / pixel_scale.max(0.01),
                );
                gl::Uniform1i(
                    gl::GetUniformLocation(self.program, c"u_texture".as_ptr()),
                    0,
                );
                gl::ActiveTexture(gl::TEXTURE0);
                let mut paint_bound = false;
                let mut current_fade = None;
                for batch in &self.batches {
                    if hybrid {
                        let belongs = match pass {
                            0 => matches!(batch.kind, BatchKind::Paint { underlay: true, .. }),
                            1 => matches!(batch.kind, BatchKind::Paint { crisp: false, .. }),
                            _ => !matches!(
                                batch.kind,
                                BatchKind::Paint { crisp: false, .. }
                                    | BatchKind::Paint { underlay: true, .. }
                            ),
                        };
                        if !belongs {
                            continue;
                        }
                    }
                    if self.prof_skip != 0 {
                        let bit = match &batch.kind {
                            BatchKind::Paint { underlay: true, .. } => 16,
                            BatchKind::Paint { texture, .. } if *texture == self.white.id => 1,
                            BatchKind::Paint { .. } => 2,
                            BatchKind::Text { .. } => 4,
                            BatchKind::Vector { .. } => 8,
                        };
                        if self.prof_skip & bit != 0 {
                            continue;
                        }
                    }
                    if current_fade != Some((batch.fade, batch.round)) {
                        // The same cheap alpha mask applies to shapes, textures,
                        // icons and text; the unmasked wallpaper remains beneath.
                        let fade = batch.fade.unwrap_or([0., 0., 0., 0.]);
                        for program in [self.program, self.vector_program, self.text_program] {
                            gl::UseProgram(program);
                            gl::Uniform4f(
                                gl::GetUniformLocation(program, c"u_edge_fade".as_ptr()),
                                fade[0],
                                fade[1],
                                fade[2],
                                fade[3],
                            );
                        }
                        let (round, radius) = batch
                            .round
                            .map_or(([0.; 4], 0.), |(r, radius)| ([r.x, r.y, r.w, r.h], radius));
                        gl::UseProgram(self.program);
                        gl::Uniform4f(
                            gl::GetUniformLocation(self.program, c"u_round_clip".as_ptr()),
                            round[0],
                            round[1],
                            round[2],
                            round[3],
                        );
                        gl::Uniform1f(
                            gl::GetUniformLocation(self.program, c"u_round_radius".as_ptr()),
                            radius,
                        );
                        current_fade = Some((batch.fade, batch.round));
                        paint_bound = false;
                    }
                    if let Some(clip) = batch.clip {
                        gl::Enable(gl::SCISSOR_TEST);
                        gl::Scissor(
                            px + (clip.x * pw as f32 / 1920.).round() as i32,
                            py + ((1080. - clip.y - clip.h) * ph as f32 / 1080.).round() as i32,
                            (clip.w * pw as f32 / 1920.).round() as i32,
                            (clip.h * ph as f32 / 1080.).round() as i32,
                        );
                    } else {
                        gl::Disable(gl::SCISSOR_TEST);
                    }
                    match &batch.kind {
                        BatchKind::Paint {
                            texture,
                            start,
                            count,
                            ..
                        } => {
                            if !paint_bound {
                                gl::UseProgram(self.program);
                                gl::BindBuffer(gl::ARRAY_BUFFER, self.buffer);
                                let mut offset = 0;
                                for (index, size) in [2, 2, 4, 4, 2].into_iter().enumerate() {
                                    gl::EnableVertexAttribArray(index as u32);
                                    gl::VertexAttribPointer(
                                        index as u32,
                                        size,
                                        gl::FLOAT,
                                        gl::FALSE,
                                        56,
                                        (offset * size_of::<f32>()) as *const _,
                                    );
                                    offset += size as usize;
                                }
                                paint_bound = true;
                            }
                            gl::BindTexture(gl::TEXTURE_2D, *texture);
                            gl::DrawArrays(gl::TRIANGLES, *start as i32, *count as i32);
                        }
                        BatchKind::Vector { path, rect, tint } => {
                            let Some(mesh) = self.vectors.get(path).and_then(Option::as_ref) else {
                                continue;
                            };
                            self.draw_mesh(
                                self.vector_program,
                                mesh,
                                [rect.w / mesh.width, rect.h / mesh.height, rect.x, rect.y],
                                *tint,
                            );
                            paint_bound = false;
                        }
                        BatchKind::Text { key, origin, tint } => {
                            let Some(mesh) =
                                self.text_runs.get(key).and_then(|run| run.mesh.as_ref())
                            else {
                                continue;
                            };
                            self.draw_mesh(
                                self.text_program,
                                mesh,
                                [1., 1., origin[0], origin[1]],
                                *tint,
                            );
                            paint_bound = false;
                        }
                    }
                }
                gl::Disable(gl::SCISSOR_TEST);
                if trace_gl {
                    gl::Finish();
                }
                if hybrid && pass == 1 {
                    self.effects_stamp = stamp;
                    self.stats.effect_render_ms = layer_start.elapsed().as_secs_f64() * 1000.;
                } else {
                    self.stats.crisp_render_ms += layer_start.elapsed().as_secs_f64() * 1000.;
                }
            }
            gl::BindFramebuffer(gl::FRAMEBUFFER, original_fbo as u32);
            gl::Viewport(vx, vy, vw, vh);
        }
        // Keep the current scene, evict old full-resolution wallpapers before memory grows.
        let mut bytes: usize = self
            .textures
            .values()
            .flatten()
            .map(|t| t.width as usize * t.height as usize * 4)
            .sum();
        if bytes > TEXTURE_BUDGET {
            let mut old: Vec<_> = self
                .textures
                .iter()
                .filter(|(path, _)| !self.stills.pinned.contains(*path))
                .filter(|(path, _)| !self.cameras.values().any(|current| current == *path))
                .filter_map(|(path, t)| {
                    t.as_ref()
                        .filter(|t| t.frame != self.frame)
                        .map(|t| (path.clone(), t.frame))
                })
                .collect();
            old.sort_by_key(|(_, frame)| *frame);
            for (path, _) in old {
                if bytes <= TEXTURE_BUDGET {
                    break;
                }
                if let Some(Some(t)) = self.textures.remove(&path) {
                    bytes -= t.width as usize * t.height as usize * 4;
                }
            }
        }
        self.trim_text_geometry();
        self.stats.vector_meshes = self.vectors.values().flatten().count();
        self.stats.vector_bytes = self.vectors.values().flatten().map(|mesh| mesh.bytes).sum();
        self.stats.artwork_bytes = bytes + self.stats.vector_bytes;
        self.stats.draw_calls = self.batches.len();
        self.stats.cpu_submission_ms = started.elapsed().as_secs_f64() * 1000.;
        if self.prof_finish
            && (self.frame.is_multiple_of(30) || std::env::var_os("LUMA_PROF_EVERY").is_some())
        {
            // Logical fill area per layer kind, in screens (1.0 = one full 1920x1080).
            let mut fill = [0f32; 5];
            for batch in &self.batches {
                if let BatchKind::Paint {
                    texture,
                    start,
                    count,
                    underlay,
                    ..
                } = &batch.kind
                {
                    let slot = if *underlay {
                        4
                    } else if *texture == self.white.id {
                        0
                    } else {
                        1
                    };
                    for quad in (*start..*start + *count).step_by(6) {
                        let v = &self.vertices[quad * 14..];
                        let (w, h) = ((v[70] - v[0]).abs(), (v[71] - v[1]).abs());
                        let mut area = w * h;
                        if let Some(c) = batch.clip {
                            area = area.min(c.w * c.h);
                        }
                        fill[slot] += area / (1920. * 1080.);
                        if area > 0.002 * 1920. * 1080.
                            && std::env::var_os("LUMA_PROF_DUMP").is_some()
                        {
                            eprintln!(
                                "LUMA_QUAD slot={slot} x={:.0} y={:.0} w={w:.0} h={h:.0} r={:.0} border={:.0} rgba=[{:.2},{:.2},{:.2},{:.2}] screens={:.2}",
                                v[0],
                                v[1],
                                v[12],
                                v[13],
                                v[4],
                                v[5],
                                v[6],
                                v[7],
                                area / (1920. * 1080.)
                            );
                        }
                    }
                }
            }
            eprintln!(
                "LUMA_FILL frame={} shapes={:.2} textures={:.2} underlay={:.2} batches={}",
                self.frame,
                fill[0],
                fill[1],
                fill[4],
                self.batches.len()
            );
        }
        if trace_gl && self.frame.is_multiple_of(30) {
            eprintln!(
                "LUMA_GL_PROFILE frame={} target={}x{} effect_ms={:.3} crisp_ms={:.3} total_ms={:.3} cached={} calls={}",
                self.frame,
                width,
                height,
                self.stats.effect_render_ms,
                self.stats.crisp_render_ms,
                self.stats.cpu_submission_ms,
                self.stats.cached_effect,
                self.stats.draw_calls
            );
        }
    }

    fn draw_mesh(&self, program: GLuint, mesh: &vector::Mesh, transform: [f32; 4], tint: Color) {
        let viewport = self.stats.viewport;
        unsafe {
            gl::UseProgram(program);
            if program == self.text_program {
                gl::Uniform1f(
                    gl::GetUniformLocation(program, c"u_fringe".as_ptr()),
                    if self.stats.multisample_samples > 1 {
                        0.
                    } else {
                        0.5
                    },
                );
            }
            gl::BindBuffer(gl::ARRAY_BUFFER, mesh.buffer);
            gl::DisableVertexAttribArray(4);
            let mut offset = 0;
            for (index, size) in [2, 2, 4, 1].into_iter().enumerate() {
                gl::EnableVertexAttribArray(index as u32);
                gl::VertexAttribPointer(
                    index as u32,
                    size,
                    gl::FLOAT,
                    gl::FALSE,
                    36,
                    (offset * size_of::<f32>()) as *const _,
                );
                offset += size as usize;
            }
            gl::Uniform4f(
                gl::GetUniformLocation(program, c"u_transform".as_ptr()),
                transform[0],
                transform[1],
                transform[2],
                transform[3],
            );
            gl::Uniform2f(
                gl::GetUniformLocation(program, c"u_pixel_scale".as_ptr()),
                viewport.width as f32 / 1920.,
                viewport.height as f32 / 1080.,
            );
            gl::Uniform4f(
                gl::GetUniformLocation(program, c"u_tint".as_ptr()),
                tint[0],
                tint[1],
                tint[2],
                tint[3],
            );
            gl::DrawArrays(gl::TRIANGLES, 0, mesh.vertices);
        }
    }

    pub fn stats(&self) -> &RenderStats {
        &self.stats
    }

    pub fn screenshot(&self, path: &Path, width: u32, height: u32) -> Result<(), String> {
        let mut pixels = vec![0; width as usize * height as usize * 4];
        unsafe {
            gl::ReadPixels(
                0,
                0,
                width as i32,
                height as i32,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                pixels.as_mut_ptr().cast(),
            );
        }
        let mut image = image::RgbaImage::from_raw(width, height, pixels)
            .ok_or("Invalid framebuffer dimensions")?;
        image::imageops::flip_vertical_in_place(&mut image);
        image.save(path).map_err(|e| e.to_string())
    }

    /// Capture the authored TV surface, independent of desktop HiDPI/window limits.
    pub fn capture(
        &mut self,
        commands: &[Draw],
        path: &Path,
        size: Size,
    ) -> Result<RenderStats, String> {
        if size.width > self.info.max_texture_size
            || size.height > self.info.max_texture_size
            || size.width > self.info.max_viewport[0] as u32
            || size.height > self.info.max_viewport[1] as u32
        {
            return Err(format!(
                "Capture {}x{} exceeds GPU limits: {}px texture, {:?} viewport",
                size.width, size.height, self.info.max_texture_size, self.info.max_viewport
            ));
        }
        let image = texture(size.width, size.height, &[]);
        let mut framebuffer = 0;
        unsafe {
            gl::GenFramebuffers(1, &mut framebuffer);
            gl::BindFramebuffer(gl::FRAMEBUFFER, framebuffer);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                image.id,
                0,
            );
            if gl::CheckFramebufferStatus(gl::FRAMEBUFFER) != gl::FRAMEBUFFER_COMPLETE {
                gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
                gl::DeleteFramebuffers(1, &framebuffer);
                return Err("Screenshot framebuffer is incomplete".into());
            }
        }
        self.render(commands, size.width, size.height, false);
        let stats = self.stats.clone();
        let result = self.screenshot(path, size.width, size.height);
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::DeleteFramebuffers(1, &framebuffer);
        }
        result.map(|_| stats)
    }
}

// Noto Sans SC (font-cjk) has no Hangul syllables; these go to font-kr instead.
// Keep in sync with is_hangul() in tools/font-weights.py, which builds font-kr.
/// Rects that an opaque draw will fully overwrite, with the index of that draw.
/// Only photos (JPEG) and frost plates drawn unfaded, unmasked and untinted
/// in alpha count; rounded corners and the anti-aliased edge are excluded.
fn opaque_covers(commands: &[Draw], loaded: impl Fn(&str) -> bool) -> Vec<(usize, Rect)> {
    let (mut clip, mut faded, mut rounded) = (None::<Rect>, false, false);
    let mut covers = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        let cover = match command {
            Draw::Clip(c) => {
                clip = *c;
                None
            }
            Draw::EdgeFade(f) => {
                faded = f.is_some();
                None
            }
            Draw::RoundClip(r) => {
                rounded = r.is_some();
                None
            }
            Draw::Image(path, rect, radius, false) if is_jpeg(path) && loaded(path) => {
                Some((*rect, *radius))
            }
            Draw::TintedImage(path, rect, radius, false, tint)
                if tint[3] >= 0.999
                    && (is_jpeg(path) || opaque_frost(path).is_some())
                    && loaded(path) =>
            {
                Some((*rect, *radius))
            }
            _ => None,
        };
        if let Some((rect, radius)) = cover
            && !faded
            && !rounded
        {
            let mut inner = rect.inset(radius.max(0.) + 1.);
            if let Some(c) = clip {
                inner = intersect(inner, c);
            }
            if inner.w > 32. && inner.h > 32. {
                covers.push((index, inner));
            }
        }
    }
    covers
}
fn command_bounds(command: &Draw) -> Option<Rect> {
    match command {
        Draw::Shape(r, ..)
        | Draw::RoundedGradient(r, ..)
        | Draw::DiagonalGradient(r, ..)
        | Draw::HGradient(r, ..) => Some(*r),
        Draw::Shadow(r, _, blur, _) => Some(r.inset(-*blur)),
        _ => None,
    }
}
fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}
fn intersect(a: Rect, b: Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    Rect::new(
        x,
        y,
        (a.x + a.w).min(b.x + b.w) - x,
        (a.y + a.h).min(b.y + b.h) - y,
    )
}
/// `rect` minus `holes` as row-merged grid cells in rect-normalized coordinates.
fn subtract_holes(rect: Rect, holes: &[Rect]) -> Vec<[f32; 4]> {
    let norm: Vec<[f32; 4]> = holes
        .iter()
        .map(|h| {
            [
                ((h.x - rect.x) / rect.w).clamp(0., 1.),
                ((h.y - rect.y) / rect.h).clamp(0., 1.),
                ((h.x + h.w - rect.x) / rect.w).clamp(0., 1.),
                ((h.y + h.h - rect.y) / rect.h).clamp(0., 1.),
            ]
        })
        .filter(|h| h[2] > h[0] && h[3] > h[1])
        .collect();
    let mut xs = vec![0., 1.];
    let mut ys = vec![0., 1.];
    for h in &norm {
        xs.extend([h[0], h[2]]);
        ys.extend([h[1], h[3]]);
    }
    xs.sort_by(f32::total_cmp);
    xs.dedup();
    ys.sort_by(f32::total_cmp);
    ys.dedup();
    let mut cells = Vec::new();
    for row in ys.windows(2) {
        let mut open: Option<f32> = None;
        for col in xs.windows(2) {
            let (cx, cy) = ((col[0] + col[1]) * 0.5, (row[0] + row[1]) * 0.5);
            let hidden = norm
                .iter()
                .any(|h| cx > h[0] && cx < h[2] && cy > h[1] && cy < h[3]);
            match (hidden, open) {
                (false, None) => open = Some(col[0]),
                (true, Some(start)) => {
                    cells.push([start, row[0], col[0], row[1]]);
                    open = None;
                }
                _ => {}
            }
        }
        if let Some(start) = open {
            cells.push([start, row[0], 1., row[1]]);
        }
    }
    cells
}
/// `over` composited over `under`, as one non-premultiplied colour.
fn tint_over(under: Color, over: Color) -> Color {
    let alpha = over[3] + under[3] * (1. - over[3]);
    if alpha <= 0. {
        return [0.; 4];
    }
    let mix = |i: usize| (under[i] * under[3] * (1. - over[3]) + over[i] * over[3]) / alpha;
    [mix(0), mix(1), mix(2), alpha]
}
/// The blurred source of a frost path, when that source is an opaque photo.
fn opaque_frost(path: &str) -> Option<&str> {
    path.strip_suffix("#frost").filter(|source| is_jpeg(source))
}
fn is_jpeg(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".jpg") || lower.ends_with(".jpeg")
}
/// `.../ha-camera-cache/camera.door-7.jpg` -> `camera.door`.
fn camera_key(path: &str) -> Option<&str> {
    let (folder, file) = path.rsplit_once('/')?;
    if !folder.ends_with("ha-camera-cache") {
        return None;
    }
    Some(file.rsplit_once('-')?.0)
}
/// Seconds a freshly decoded title-card picture takes to fade in.
const IMAGE_FADE: f32 = 0.3;

/// One line per decoded title-card picture in luma-images.log next to the
/// poster cache, so slow banners on the TV can be measured, not guessed.
fn log_image_timing(path: &str, started: std::time::Instant, decoded: Option<&image::RgbaImage>) {
    use std::io::Write;
    let Some(folder) = Path::new(path).parent().and_then(Path::parent) else {
        return;
    };
    let Ok(mut log) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(folder.join("luma-images.log"))
    else {
        return;
    };
    let size = decoded.map_or("failed".to_owned(), |i| {
        format!("{}x{}", i.width(), i.height())
    });
    let name = Path::new(path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let _ = writeln!(
        log,
        "decode {} ms {size} {name}",
        started.elapsed().as_millis()
    );
}
fn is_poster_cache(path: &str) -> bool {
    path.rsplit_once('/')
        .is_some_and(|(folder, _)| folder.ends_with("posters"))
}
/// Texture coordinates that crop an image of aspect `image` to fill a box of
/// aspect `card` without distortion; within 2 % the whole image is kept.
fn cover_uv(image: f32, card: f32) -> [f32; 4] {
    if !(image.is_finite() && card.is_finite()) || (image / card - 1.).abs() < 0.02 {
        return [0., 0., 1., 1.];
    }
    if image > card {
        let cut = (1. - card / image) / 2.;
        [cut, 0., 1. - cut, 1.]
    } else {
        let cut = (1. - image / card) / 2.;
        [0., cut, 1., 1. - cut]
    }
}
fn covers_screen(rect: Rect) -> bool {
    rect.x <= 0. && rect.y <= 0. && rect.x + rect.w >= 1920. && rect.y + rect.h >= 1080.
}
fn is_hangul(c: char) -> bool {
    matches!(
        c,
        '\u{1100}'..='\u{11FF}'
            | '\u{3130}'..='\u{318F}'
            | '\u{A960}'..='\u{A97F}'
            | '\u{AC00}'..='\u{D7A3}'
            | '\u{D7B0}'..='\u{D7FF}'
    )
}

/// Encoding stays disjoint from the original Inter and Noto CJK IDs.
// Native ID mapping for individual OFL/Apache-licensed source subsets.
fn catalog_font_id(family: &str, heading: bool, weight: u16) -> u16 {
    const FAMILIES: &[&str] = &[
        "archivo",
        "inter",
        "unbounded",
        "montserrat",
        "manrope",
        "onest",
        "rubik",
        "exo2",
        "raleway",
        "oswald",
        "russoone",
        "jost",
        "playfairdisplay",
        "prata",
        "fraunces",
        "spacegrotesk",
        "syne",
        "bebasneue",
        "golostext",
        "ibmplexsans",
        "notosans",
        "sourcesans3",
        "roboto",
        "ptsans",
        "opensans",
        "nunitosans",
        "nunito",
    ];
    let index = FAMILIES
        .iter()
        .position(|&name| name == family)
        .unwrap_or(1) as u16;
    let face = if heading {
        if family == "nunito" {
            if weight >= 850 { 900 } else { 800 }
        } else if matches!(family, "archivo" | "inter" | "unbounded") {
            800
        } else {
            700
        }
    } else if weight >= 550 {
        600
    } else if weight >= 450 {
        500
    } else {
        400
    };
    20000 + index * 1000 + face
}

fn load_font(directory: &Path, id: u16) -> Result<Font, String> {
    let name = if id >= 20000 {
        let index = ((id - 20000) / 1000) as usize;
        let fam = [
            "archivo",
            "inter",
            "unbounded",
            "montserrat",
            "manrope",
            "onest",
            "rubik",
            "exo2",
            "raleway",
            "oswald",
            "russoone",
            "jost",
            "playfairdisplay",
            "prata",
            "fraunces",
            "spacegrotesk",
            "syne",
            "bebasneue",
            "golostext",
            "ibmplexsans",
            "notosans",
            "sourcesans3",
            "roboto",
            "ptsans",
            "opensans",
            "nunitosans",
            "nunito",
        ]
        .get(index)
        .ok_or_else(|| format!("Unknown catalog font id {id}"))?;
        let weight = (id - 20000) % 1000;
        let role = if weight >= 700 { "heading" } else { "text" };
        format!("font-catalog-{fam}-{role}-{weight}.ttf")
    } else if id >= 3000 {
        format!("font-kr-{}.ttf", id - 3000)
    } else if id >= 2000 {
        format!("font-cjk-{}.ttf", id - 2000)
    } else if id >= 1000 {
        format!("font-display-{}.ttf", id - 1000)
    } else {
        format!("font-{id}.ttf")
    };
    let path = directory.join(&name);
    let started = Instant::now();
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("Font {}: {e}. Regenerate native assets.", path.display()))?;
    let font = Font::from_bytes(bytes).map_err(|e| format!("Font {}: {e}", path.display()))?;
    if cfg!(feature = "webos") {
        eprintln!(
            "Luma font: {name} loaded in {} ms",
            started.elapsed().as_millis()
        );
    }
    Ok(font)
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            if self.effects_fbo != 0 {
                gl::DeleteFramebuffers(1, &self.effects_fbo);
            }
            if self.effects_texture != 0 {
                gl::DeleteTextures(1, &self.effects_texture);
            }
            gl::DeleteBuffers(1, &self.buffer);
            gl::DeleteProgram(self.program);
            gl::DeleteProgram(self.vector_program);
            gl::DeleteProgram(self.text_program);
            if self.vao != 0 {
                gl::DeleteVertexArrays(1, &self.vao);
            }
        }
    }
}

/// The frost texture of a full-screen image: 240x135, blurred, with its
/// colour pushed (a plain blur of a smooth sky reads as a grey fill).
fn frost_pixels(image: &image::DynamicImage) -> image::RgbaImage {
    let small = image
        .resize_exact(240, 135, image::imageops::FilterType::Triangle)
        .into_rgba8();
    let mut blurred = image::imageops::blur(&small, 3.);
    for pixel in blurred.pixels_mut() {
        let [r, g, b, _] = pixel.0.map(f32::from);
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        for (channel, value) in pixel.0[..3].iter_mut().zip([r, g, b]) {
            *channel = (luma + (value - luma) * 1.7).clamp(0., 255.) as u8;
        }
    }
    blurred
}

fn texture(width: u32, height: u32, pixels: &[u8]) -> Texture {
    let mut id = 0;
    unsafe {
        gl::GenTextures(1, &mut id);
        gl::BindTexture(gl::TEXTURE_2D, id);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
        gl::TexImage2D(
            gl::TEXTURE_2D,
            0,
            gl::RGBA as i32,
            width as i32,
            height as i32,
            0,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            if pixels.is_empty() {
                std::ptr::null()
            } else {
                pixels.as_ptr().cast()
            },
        );
    }
    Texture {
        id,
        width,
        height,
        frame: 0,
    }
}

fn gl_string(name: GLenum) -> String {
    unsafe {
        let value = gl::GetString(name);
        if value.is_null() {
            "unknown".into()
        } else {
            CStr::from_ptr(value.cast()).to_string_lossy().into_owned()
        }
    }
}

fn compile(kind: GLenum, source: &str) -> Result<GLuint, String> {
    let source = CString::new(source).map_err(|e| e.to_string())?;
    unsafe {
        let shader = gl::CreateShader(kind);
        gl::ShaderSource(shader, 1, &source.as_ptr(), std::ptr::null());
        gl::CompileShader(shader);
        let mut success = 0;
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut success);
        if success == 0 {
            let mut log = vec![0u8; 4096];
            let mut len = 0;
            gl::GetShaderInfoLog(shader, log.len() as i32, &mut len, log.as_mut_ptr().cast());
            gl::DeleteShader(shader);
            return Err(format!(
                "Shader compilation: {}",
                String::from_utf8_lossy(&log[..len as usize])
            ));
        }
        Ok(shader)
    }
}

fn link(vertex: &str, fragment: &str, attributes: &[&CStr]) -> Result<GLuint, String> {
    let vertex = compile(gl::VERTEX_SHADER, vertex)?;
    let fragment = match compile(gl::FRAGMENT_SHADER, fragment) {
        Ok(fragment) => fragment,
        Err(error) => {
            unsafe {
                gl::DeleteShader(vertex);
            }
            return Err(error);
        }
    };
    unsafe {
        let program = gl::CreateProgram();
        gl::AttachShader(program, vertex);
        gl::AttachShader(program, fragment);
        for (index, name) in attributes.iter().enumerate() {
            gl::BindAttribLocation(program, index as u32, name.as_ptr());
        }
        gl::LinkProgram(program);
        gl::DeleteShader(vertex);
        gl::DeleteShader(fragment);
        let mut success = 0;
        gl::GetProgramiv(program, gl::LINK_STATUS, &mut success);
        if success == 0 {
            let mut log = vec![0u8; 4096];
            let mut len = 0;
            gl::GetProgramInfoLog(program, log.len() as i32, &mut len, log.as_mut_ptr().cast());
            gl::DeleteProgram(program);
            return Err(format!(
                "Shader linking: {}",
                String::from_utf8_lossy(&log[..len as usize])
            ));
        }
        Ok(program)
    }
}

/// Opens a picture by its content, not its extension: media serves some
/// wide art as WebP under a .jpg name.
pub(crate) fn open_image(path: impl AsRef<Path>) -> image::ImageResult<image::DynamicImage> {
    image::ImageReader::open(path)?
        .with_guessed_format()?
        .decode()
}
