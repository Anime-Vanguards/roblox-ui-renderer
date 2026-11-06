//! `rur sequence`: a story run once and captured frame by frame at exact moments: numbered PNGs, a
//! compact layout trace, a video at real speed and one at quarter speed. The engine half (engine/cli.rs
//! run_sequence) runs in a VM of its own thread, steps the clock from frame to frame and sends each
//! frame's scene delta; this half applies them to a scene, draws it on the GPU, reads back the crop and
//! hands the pixels to PNG writers and ffmpeg, so simulation, drawing, encoding and the disk overlap.
//!
//! Output directory (summary.json is cli/rur.mjs's, from the result [`StoryRenderer::sequence`] returns):
//!   frames/00000.png ...   one per frame, cropped (`--crop`), straight alpha as render.png
//!   trace.json             every node's AbsolutePosition / AbsoluteSize / Rotation at each change ([`Trace`])
//!   video.mp4              the frames at `fps` (H.264, BT.709), video-quarter.mp4 the same at a quarter of it
//!   scenes/00000.json ...  with `--scenes`: each frame's whole scene, as scene.json
//!
//! [`StoryRenderer::sequence`]: crate::story_render::StoryRenderer::sequence
use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};

use crate::{
    assets::AssetStore,
    engine::cli::{Args, Preloaded, SequenceFrame, SequenceMessage, SequenceSpec},
    presentation::ScrollPresentation,
    render::{
        DrawPreparer,
        gpu::{GpuRenderer, PendingReadback},
    },
    resolve::Resolver,
    scene::SceneStore,
};

/// What a sequence writes besides its frames' moments (engine/cli.rs SequenceSpec)
#[derive(Clone, Debug, Default)]
pub struct SequenceOutput {
    /// The capture rate (the videos' frame rate)
    pub fps: f64,
    /// `--crop`: a node (its path, or the end of it) or a logical rect; the whole frame without it
    pub crop: Option<Crop>,
    /// Logical pixels added around the crop
    pub pad: f64,
    /// frames/NNNNN.png
    pub png: bool,
    /// scenes/NNNNN.json
    pub scenes: bool,
    /// video.mp4 and video-quarter.mp4 (when ffmpeg is there)
    pub video: bool,
    /// No downloads: images and fonts from the caches only
    pub offline: bool,
}

#[derive(Clone, Debug)]
pub enum Crop {
    Node(String),
    /// x, y, width, height in logical pixels
    Rect([f64; 4]),
}

impl Crop {
    /// `x,y,w,h` (logical pixels) or a node path
    pub fn parse(text: &str) -> Crop {
        let numbers: Vec<f64> = text.split(',').filter_map(|part| part.trim().parse().ok()).collect();
        if numbers.len() == 4 && text.split(',').count() == 4 && numbers[2] > 0.0 && numbers[3] > 0.0 {
            Crop::Rect([numbers[0], numbers[1], numbers[2], numbers[3]])
        } else {
            Crop::Node(text.trim().to_string())
        }
    }
}

/// The story target every story's nodes sit under: left off the trace's paths
const STORY_ROOT: &str = "CoreGui.RURStory.StoryTarget";

/// Frames the engine half may run ahead of the drawing
const FRAME_QUEUE: usize = 8;

/// A sequence's moments and outputs from the render service's `sequence` object (the CLI's flags
/// make the same): `fps` (60), `from` (0 s), `frames`, else `to` (exclusive: `(to - from) * fps`
/// frames), else `duration` (s), else one second; `crop`, `pad`, `png` (true), `scenes`, `video` (true).
/// Numbers may be strings.
pub fn parse_options(value: &Value, offline: bool) -> Result<(SequenceSpec, SequenceOutput), String> {
    let number = |key: &str| -> Result<Option<f64>, String> {
        match value.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(field) => field
                .as_f64()
                .or_else(|| field.as_str().and_then(|text| text.trim().parse().ok()))
                .filter(|number| number.is_finite())
                .map(Some)
                .ok_or_else(|| format!("--{key}: not a number ({field})")),
        }
    };
    let fps = number("fps")?.unwrap_or(60.0);
    if !(fps > 0.0 && fps <= 1000.0) {
        return Err(format!("--fps must be above 0 and at most 1000 (got {fps})"));
    }
    let from = number("from")?.unwrap_or(0.0);
    if from < 0.0 {
        return Err(format!("--from must not be negative (got {from})"));
    }
    let frames = match (number("frames")?, number("to")?, number("duration")?) {
        (Some(frames), _, _) => frames.round(),
        (None, Some(to), _) => ((to - from) * fps).round(),
        (None, None, Some(duration)) => (duration * fps).round(),
        (None, None, None) => fps.round(),
    };
    if !(1.0..=99_999.0).contains(&frames) {
        return Err(format!("a sequence takes 1 to 99999 frames (got {frames}: check --to / --frames / --duration)"));
    }
    let crop = value.get("crop").and_then(Value::as_str).filter(|text| !text.trim().is_empty()).map(Crop::parse);
    let pad = number("pad")?.unwrap_or(0.0).max(0.0);
    let flag = |key: &str, default: bool| value.get(key).and_then(Value::as_bool).unwrap_or(default);
    Ok((
        SequenceSpec { from, frames: frames as u32, fps },
        SequenceOutput { fps, crop, pad, png: flag("png", true), scenes: flag("scenes", false), video: flag("video", true), offline },
    ))
}

/// The engine half of `args` in a fresh VM, on a thread of its own with a deep stack (as the render
/// service's VMs: deep Luau and layout recursion): its frames
pub fn start_fresh(args: &Args, spec: &SequenceSpec) -> Result<mpsc::Receiver<SequenceMessage>, String> {
    let (send, receive) = mpsc::sync_channel(FRAME_QUEUE);
    let (args, spec) = (args.clone(), spec.clone());
    std::thread::Builder::new()
        .name("rur-sequence-engine".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || match Preloaded::load(&args) {
            Ok(loaded) => loaded.sequence(&args, &spec, &mut |message| send.send(message).is_ok()),
            Err(error) => {
                let _ = send.send(SequenceMessage::End(Err(error)));
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(receive)
}

/// A channel for the render service's standby VMs to send a sequence's frames through
pub fn frame_channel() -> (mpsc::SyncSender<SequenceMessage>, mpsc::Receiver<SequenceMessage>) {
    mpsc::sync_channel(FRAME_QUEUE)
}

/// The physical box `[x, y, width, height]` of `crop` in a `frame`-sized image of `scene` (and the
/// node it found): the node's own rect at this frame (several matches: their union), `pad` logical
/// pixels around it, whole device pixels outward, inside the frame, of even width and height (H.264's
/// 4:2:0 needs both even: a pixel more where the frame has room, else one less).
pub fn crop_box(scene: &SceneStore, crop: &Crop, pad: f64, frame: (u32, u32)) -> Result<([u32; 4], Option<String>)> {
    let scale = scene.viewport().scale;
    let (rect, node) = match crop {
        Crop::Rect(rect) => (*rect, None),
        Crop::Node(query) => {
            let query = query.trim().trim_start_matches('.');
            let matches: Vec<&Value> = scene
                .ordered_nodes()
                .filter(|node| node["path"].as_str().is_some_and(|path| path == query || path.ends_with(&format!(".{query}"))))
                .collect();
            if matches.is_empty() {
                let name = query.rsplit('.').next().unwrap_or(query).to_ascii_lowercase();
                let similar: Vec<&str> = scene
                    .ordered_nodes()
                    .filter_map(|node| node["path"].as_str())
                    .filter(|path| path.rsplit('.').next().is_some_and(|last| last.to_ascii_lowercase().contains(&name)))
                    .take(8)
                    .collect();
                bail!("--crop: no node matches {query:?} in the first frame{}", if similar.is_empty() { String::new() } else { format!(" (nodes named like it: {})", similar.join(", ")) });
            }
            let mut union = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
            for node in &matches {
                let rect: Vec<f64> = node["rect"].as_array().into_iter().flatten().filter_map(Value::as_f64).collect();
                if rect.len() == 4 {
                    union = [union[0].min(rect[0]), union[1].min(rect[1]), union[2].max(rect[0] + rect[2]), union[3].max(rect[1] + rect[3])];
                }
            }
            if !union[0].is_finite() || union[2] <= union[0] || union[3] <= union[1] {
                bail!("--crop: {query:?} has no size in the first frame");
            }
            let path = matches[0]["path"].as_str().unwrap_or(query).to_string();
            ([union[0], union[1], union[2] - union[0], union[3] - union[1]], Some(path))
        }
    };
    let (width, height) = frame;
    let x0 = (((rect[0] - pad) * scale).floor().max(0.0) as u32).min(width.saturating_sub(1));
    let y0 = (((rect[1] - pad) * scale).floor().max(0.0) as u32).min(height.saturating_sub(1));
    let mut x1 = (((rect[0] + rect[2] + pad) * scale).ceil().max(0.0) as u32).min(width);
    let mut y1 = (((rect[1] + rect[3] + pad) * scale).ceil().max(0.0) as u32).min(height);
    if x1 <= x0 || y1 <= y0 {
        bail!("--crop: {:?} is outside the {width}x{height} frame", rect);
    }
    if (x1 - x0) % 2 == 1 {
        if x1 < width { x1 += 1 } else { x1 -= 1 }
    }
    if (y1 - y0) % 2 == 1 {
        if y1 < height { y1 += 1 } else { y1 -= 1 }
    }
    if x1 <= x0 || y1 <= y0 {
        bail!("--crop: {:?} is less than two pixels wide or tall", rect);
    }
    Ok(([x0, y0, x1 - x0, y1 - y0], node))
}

/// The layout trace: for every node, its AbsolutePosition, AbsoluteSize and Rotation in logical pixels
/// as a row `[frame, x, y, width, height, rotation]` at each frame they change (a node keeps its last
/// row until its next), `[frame, null]` from a frame it is not drawn (gone, invisible or culled). Nodes
/// are named by their path under the story target; instances of one path (siblings of one name, a
/// sparkle made again) get `#2`, `#3`... in the order they first appear, and carry their instance `id`
/// (the scenes' node id). Motion is read off the rows without the frames' whole scenes.
#[derive(Default)]
pub struct Trace {
    nodes: Vec<TraceNode>,
    by_id: HashMap<u64, usize>,
    paths: HashMap<String, u32>,
    seen: Vec<u32>,
}

struct TraceNode {
    id: u64,
    path: String,
    class: String,
    rows: Vec<Value>,
    last: Option<[f64; 5]>,
}

impl Trace {
    pub fn record(&mut self, index: u32, scene: &SceneStore) {
        for node in scene.ordered_nodes() {
            let Some(id) = node["id"].as_u64() else { continue };
            let rect: Vec<f64> = node["rect"].as_array().into_iter().flatten().filter_map(Value::as_f64).collect();
            let value = (rect.len() == 4 && node["visible"].as_bool() != Some(false))
                .then(|| [rect[0], rect[1], rect[2], rect[3], node["rotation"].as_f64().unwrap_or(0.0)]);
            let slot = match self.by_id.get(&id) {
                Some(&slot) => slot,
                None => {
                    let full = node["path"].as_str().unwrap_or("?");
                    let path = full.strip_prefix(STORY_ROOT).map(|rest| rest.trim_start_matches('.')).filter(|rest| !rest.is_empty()).unwrap_or(full);
                    let count = self.paths.entry(path.to_string()).or_insert(0);
                    *count += 1;
                    let path = if *count == 1 { path.to_string() } else { format!("{path}#{count}") };
                    self.nodes.push(TraceNode { id, path, class: node["class"].as_str().unwrap_or("").to_string(), rows: Vec::new(), last: None });
                    self.seen.push(u32::MAX);
                    self.by_id.insert(id, self.nodes.len() - 1);
                    self.nodes.len() - 1
                }
            };
            self.seen[slot] = index;
            let entry = &mut self.nodes[slot];
            if entry.rows.is_empty() || entry.last != value {
                entry.rows.push(match value {
                    Some([x, y, w, h, r]) => json!([index, x, y, w, h, r]),
                    None => json!([index, null]),
                });
                entry.last = value;
            }
        }
        for (slot, entry) in self.nodes.iter_mut().enumerate() {
            if self.seen[slot] != index && entry.last.is_some() {
                entry.rows.push(json!([index, null]));
                entry.last = None;
            }
        }
    }

    pub fn to_value(&self, header: Value) -> Value {
        let mut value = header;
        value["units"] = json!("rows [frame, x, y, width, height, rotation]: AbsolutePosition and AbsoluteSize in logical pixels, Rotation in degrees, written at each frame they change (a node keeps its last row until its next); [frame, null]: not drawn from that frame (gone, invisible or culled)");
        value["root"] = json!(STORY_ROOT);
        value["nodes"] = Value::Array(self.nodes.iter().map(|node| json!({"path": node.path, "class": node.class, "id": node.id, "rows": node.rows})).collect());
        value
    }
}

/// A frame's pixels for the writers
struct Pixels {
    index: u32,
    rgba: Arc<Vec<u8>>,
}

/// PNG writers (a few threads: a PNG is encoded on one), the scene writer and the video encoder's feed.
/// Bounded queues: a writer that falls behind holds the drawing up rather than frames piling up.
struct Writers {
    png: Option<(mpsc::SyncSender<Pixels>, Vec<JoinHandle<Result<f64>>>)>,
    scenes: Option<(mpsc::SyncSender<(u32, SceneStore)>, JoinHandle<Result<()>>)>,
    video: Option<Video>,
    video_error: Option<String>,
    size: (u32, u32),
}

/// ffmpeg encoding raw RGBA from its stdin, fed by a thread of its own
struct Video {
    feed: mpsc::SyncSender<Arc<Vec<u8>>>,
    feeder: JoinHandle<Result<(), String>>,
    child: Child,
    errors: JoinHandle<String>,
    file: PathBuf,
    encoder: String,
}

/// PNG writer threads: each encodes a whole frame alone, so they run side by side
const PNG_WRITERS: usize = 3;

fn ffmpeg_program() -> PathBuf {
    std::env::var_os("RUR_FFMPEG").filter(|value| !value.is_empty()).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("ffmpeg"))
}

fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// The libx264 threads for a frame of `pixels`: about one per 0.4 megapixels, 2 to 8 (a UI crop is small;
/// x264's default, one and a half per core, only wakes threads that wait)
fn x264_threads(pixels: u64) -> u32 {
    ((pixels as f64 / 400_000.0).ceil() as u32).clamp(2, 8)
}

impl Writers {
    fn start(out: &Path, options: &SequenceOutput, size: (u32, u32)) -> Result<Writers> {
        let mut writers = Writers { png: None, scenes: None, video: None, video_error: None, size };
        if options.png {
            let directory = out.join("frames");
            std::fs::create_dir_all(&directory)?;
            let (send, receive) = mpsc::sync_channel::<Pixels>(PNG_WRITERS * 2);
            let receive = Arc::new(Mutex::new(receive));
            let threads = (0..PNG_WRITERS)
                .map(|number| {
                    let receive = receive.clone();
                    let directory = directory.clone();
                    std::thread::Builder::new()
                        .name(format!("rur-sequence-png-{number}"))
                        .spawn(move || -> Result<f64> {
                            let mut busy = 0.0;
                            loop {
                                let next = receive.lock().map_err(|_| anyhow!("PNG queue poisoned"))?.recv();
                                let Ok(frame) = next else { return Ok(busy) };
                                let started = Instant::now();
                                crate::story_render::write_png(&directory.join(format!("{:05}.png", frame.index)), &frame.rgba, size.0, size.1)?;
                                busy += started.elapsed().as_secs_f64();
                            }
                        })
                        .context("starting a PNG writer")
                })
                .collect::<Result<Vec<_>>>()?;
            writers.png = Some((send, threads));
        }
        if options.scenes {
            let directory = out.join("scenes");
            std::fs::create_dir_all(&directory)?;
            let (send, receive) = mpsc::sync_channel::<(u32, SceneStore)>(4);
            let thread = std::thread::Builder::new()
                .name("rur-sequence-scenes".into())
                .spawn(move || -> Result<()> {
                    for (index, scene) in receive {
                        let bytes = serde_json::to_vec(&scene.to_value())?;
                        std::fs::write(directory.join(format!("{index:05}.json")), bytes)?;
                    }
                    Ok(())
                })
                .context("starting the scene writer")?;
            writers.scenes = Some((send, thread));
        }
        if options.video {
            match Video::start(out, options.fps, size) {
                Ok(video) => writers.video = Some(video),
                Err(error) => writers.video_error = Some(error),
            }
        }
        Ok(writers)
    }

    /// A frame's scene, while it is the current one (its pixels may come later)
    fn scene(&mut self, index: u32, scene: &SceneStore) -> Result<()> {
        if let Some((send, _)) = &self.scenes {
            send.send((index, scene.clone())).map_err(|_| anyhow!("the scene writer stopped"))?;
        }
        Ok(())
    }

    /// A frame's pixels, in frame order
    fn pixels(&mut self, index: u32, rgba: &Arc<Vec<u8>>) -> Result<()> {
        if let Some((send, _)) = &self.png {
            send.send(Pixels { index, rgba: rgba.clone() }).map_err(|_| anyhow!("the PNG writers stopped"))?;
        }
        if let Some(video) = &self.video {
            if video.feed.send(rgba.clone()).is_err() {
                // ffmpeg stopped taking frames: its error is read when the video is finished
                let video = self.video.take().unwrap();
                self.video_error = Some(video.finish().err().unwrap_or_else(|| "ffmpeg stopped early".into()));
            }
        }
        Ok(())
    }

    /// Waits for every writer; the PNG writers' busy seconds and the videos (or why there are none)
    fn finish(mut self, out: &Path, fps: f64) -> Result<(f64, Value)> {
        let mut png_busy = 0.0;
        if let Some((send, threads)) = self.png.take() {
            drop(send);
            for thread in threads {
                png_busy += thread.join().map_err(|_| anyhow!("a PNG writer panicked"))??;
            }
        }
        if let Some((send, thread)) = self.scenes.take() {
            drop(send);
            thread.join().map_err(|_| anyhow!("the scene writer panicked"))??;
        }
        let video = match self.video.take() {
            Some(video) => {
                let encoder = video.encoder.clone();
                let file = video.file.clone();
                match video.finish() {
                    Ok(()) => match quarter_speed(&file, &out.join("video-quarter.mp4")) {
                        Ok(quarter) => json!({"real": file, "quarter": quarter, "fps": fps, "quarterFps": fps / 4.0, "encoder": encoder, "size": [self.size.0, self.size.1]}),
                        Err(error) => json!({"real": file, "fps": fps, "encoder": encoder, "error": format!("quarter speed: {error}")}),
                    },
                    Err(error) => json!({"error": error}),
                }
            }
            None => match self.video_error.take() {
                Some(error) => json!({"error": error}),
                None => Value::Null,
            },
        };
        Ok((png_busy, video))
    }
}

impl Video {
    fn start(out: &Path, fps: f64, (width, height): (u32, u32)) -> Result<Video, String> {
        let file = out.join("video.mp4");
        let ffmpeg = ffmpeg_program();
        let rate = format!("{}", (fps * 1000.0).round() / 1000.0);
        let threads = x264_threads(u64::from(width) * u64::from(height)).to_string();
        let mut command = Command::new(&ffmpeg);
        command
            .args(["-hide_banner", "-nostats", "-loglevel", "error", "-y", "-filter_threads", "1"])
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{width}x{height}"), "-framerate", &rate, "-i", "-"])
            // The frames' sRGB as BT.709 limited range, tagged so players convert it back alike
            .args(["-vf", "scale=out_color_matrix=bt709:out_range=tv,format=yuv420p"])
            .args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "16", "-threads", &threads])
            .args(["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "iec61966-2-1", "-color_range", "tv"])
            .args(["-movflags", "+faststart", "-an"])
            .arg(&file)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            // ffmpeg downloads nothing: it does not get the asset cookie (cli/assets.mjs withoutCookie)
            .env_remove("ROBLOX_ASSET_DELIVERY_COOKIE")
            .env_remove("ROBLOSECURITY");
        hide_console(&mut command);
        let mut child = command.spawn().map_err(|error| format!("no video: could not start {} ({error}); set RUR_FFMPEG to an ffmpeg", ffmpeg.display()))?;
        let stdin: ChildStdin = child.stdin.take().ok_or("ffmpeg's stdin")?;
        let mut stderr = child.stderr.take().ok_or("ffmpeg's stderr")?;
        let errors = std::thread::spawn(move || {
            let mut text = String::new();
            let _ = std::io::Read::read_to_string(&mut stderr, &mut text);
            text
        });
        // Frames wait here while ffmpeg takes the earlier ones (a frame of the crop is a few MB)
        let (feed, frames) = mpsc::sync_channel::<Arc<Vec<u8>>>(8);
        let feeder = std::thread::Builder::new()
            .name("rur-sequence-video".into())
            .spawn(move || -> Result<(), String> {
                let mut stdin = std::io::BufWriter::with_capacity(1 << 20, stdin);
                for frame in frames {
                    stdin.write_all(&frame).map_err(|error| format!("writing a frame to ffmpeg: {error}"))?;
                }
                stdin.flush().map_err(|error| error.to_string())
            })
            .map_err(|error| error.to_string())?;
        Ok(Video { feed, feeder, child, errors, file, encoder: "libx264".into() })
    }

    fn finish(self) -> Result<(), String> {
        let Video { feed, feeder, mut child, errors, .. } = self;
        drop(feed);
        let fed = feeder.join().map_err(|_| "the video feeder panicked".to_string())?;
        let status = child.wait().map_err(|error| error.to_string())?;
        let text = errors.join().unwrap_or_default();
        if !status.success() {
            return Err(format!("ffmpeg failed ({status}): {}", text.trim()));
        }
        fed
    }
}

/// The video at a quarter of its speed, the same frames each shown four times as long: its packets'
/// timestamps and durations scaled, the stream copied (no second encode). (`-itsscale 4` left the last
/// frame its 1/60 s, 15.95 s for 240 frames; a raw H.264 stream remuxed at a quarter of the rate lost
/// frames to B-frame reordering.)
fn quarter_speed(real: &Path, quarter: &Path) -> Result<PathBuf, String> {
    let mut command = Command::new(ffmpeg_program());
    command
        .args(["-hide_banner", "-nostats", "-loglevel", "error", "-y", "-i"])
        .arg(real)
        .args(["-c", "copy", "-bsf:v", "setts=pts=PTS*4:dts=DTS*4:duration=DURATION*4", "-movflags", "+faststart", "-an"])
        .arg(quarter)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    hide_console(&mut command);
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(quarter.to_path_buf())
}

/// A frame's pixels as a 64-bit hash (repeated frames are counted, never trusted for output)
fn pixel_hash(rgba: &[u8]) -> u64 {
    use std::hash::Hasher;
    let mut hasher = rustc_hash::FxHasher::default();
    hasher.write(rgba);
    hasher.finish()
}

/// Waits until the scene's images and glyphs are ready (as a render waits before it draws)
fn settle_assets(assets: &mut AssetStore, scene: &SceneStore) -> Result<()> {
    assets.request_missing(scene);
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        assets.poll();
        if assets.pending_count() == 0 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("timed out preparing the frame's images and glyphs");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Seconds as the result reports them (ms precision)
fn seconds(value: f64) -> Value {
    json!((value * 1000.0).round() / 1000.0)
}

/// Median of `values` (0 for none)
fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

/// The draw half: every frame of `frames` (the first already received) applied, drawn, read back and
/// written into `out`. Returns the result cli/rur.mjs makes summary.json from.
#[allow(clippy::too_many_arguments)]
pub fn draw_frames(
    gpu: &mut GpuRenderer,
    preparer: &mut DrawPreparer,
    assets: &mut AssetStore,
    resolver: &mut Resolver,
    first: SequenceFrame,
    frames: &mpsc::Receiver<SequenceMessage>,
    out: &Path,
    options: &SequenceOutput,
    started: Instant,
) -> Result<Value> {
    let first_at = started.elapsed().as_secs_f64();
    for directory in ["frames", "scenes"] {
        let path = out.join(directory);
        if path.exists() {
            std::fs::remove_dir_all(&path).with_context(|| format!("clearing {}", path.display()))?;
        }
    }
    let mut scene = SceneStore::default();
    let mut presentation = ScrollPresentation::default();
    let mut trace = Trace::default();
    let mut writers: Option<Writers> = None;
    let mut crop: Option<([u32; 4], Option<String>)> = None;
    let mut resolved: std::collections::HashSet<String> = Default::default();
    let mut missing_images: Vec<String> = Vec::new();
    let mut times: Vec<Value> = Vec::new();
    let mut hashes: Vec<u64> = Vec::new();
    let mut previous: Option<Arc<Vec<u8>>> = None;
    // The last frame drawn, its pixels still on their way out of the GPU: collected once the next frame
    // is prepared and submitted, so the GPU draws and copies while the CPU prepares
    let mut in_flight: Option<(u32, PendingReadback)> = None;
    let (mut unchanged, mut waits, mut draws, mut prepares, mut gpus, mut readbacks) = (0u32, Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut ready = Value::Null;
    let mut next = Some(first);
    let mut waited_at = Instant::now();
    // A frame's pixels, in frame order, to the writers
    fn deliver(writers: &mut Writers, hashes: &mut Vec<u64>, previous: &mut Option<Arc<Vec<u8>>>, index: u32, rgba: Arc<Vec<u8>>) -> Result<()> {
        hashes.push(pixel_hash(&rgba));
        writers.pixels(index, &rgba)?;
        *previous = Some(rgba);
        Ok(())
    }
    let end = loop {
        let frame = match next.take() {
            Some(frame) => frame,
            None => match frames.recv() {
                Ok(SequenceMessage::Frame(frame)) => frame,
                Ok(SequenceMessage::End(Ok(report))) => break report,
                Ok(SequenceMessage::End(Err(error))) => bail!("the sequence stopped: {error}"),
                Err(_) => bail!("the sequence's engine stopped without finishing"),
            },
        };
        let drawn_at = Instant::now();
        waits.push(drawn_at.duration_since(waited_at).as_secs_f64());
        let SequenceFrame { index, time, mut reply } = frame;
        if reply["type"] == "error" {
            bail!("frame {index}: {}", reply["message"].as_str().unwrap_or("the engine failed"));
        }
        let still = index > 0 && reply["unchanged"] == true;
        scene.apply_owned(&mut reply).map_err(|error| anyhow!("frame {index}: {error}"))?;
        if index == 0 {
            assets.begin_scene(&scene);
            presentation.update(&scene, None);
            ready = reply;
        }
        // Images the scene shows for the first time: resolved here (cached and local files in process,
        // downloads through the helper only when online), decoded by the store
        let fresh: Vec<String> = scene.images().iter().filter_map(Value::as_str).filter(|uri| !resolved.contains(*uri)).map(str::to_string).collect();
        if !fresh.is_empty() {
            let answers = resolver.resolve_scene(&scene.to_value(), &fresh);
            for (uri, answer) in &answers {
                if let Err(error) = answer {
                    missing_images.push(format!("{uri}: {error}"));
                }
            }
            assets.provide_images(answers);
            resolved.extend(fresh);
        }
        trace.record(index, &scene);
        times.push(json!(time));
        if still && (previous.is_some() || in_flight.is_some()) {
            // The same pixels as the frame before (collected first if still in flight)
            unchanged += 1;
            let writers = writers.as_mut().context("the first frame drew nothing")?;
            if let Some((at, pending)) = in_flight.take() {
                deliver(writers, &mut hashes, &mut previous, at, Arc::new(gpu.finish_readback(pending)?))?;
            }
            if options.scenes {
                writers.scene(index, &scene)?;
            }
            let same = previous.clone().context("no frame before an unchanged one")?;
            deliver(writers, &mut hashes, &mut previous, index, same)?;
        } else {
            settle_assets(assets, &scene)?;
            presentation.update(&scene, None);
            presentation.advance(&scene, 0.0);
            let presented = presentation.project(&scene);
            let prepared_at = Instant::now();
            let packet = preparer.prepare(&presented, assets)?;
            let gpu_at = Instant::now();
            let rendered = gpu.render(&packet)?;
            if crop.is_none() {
                let size = (rendered.width, rendered.height);
                crop = Some(match &options.crop {
                    Some(spec) => crop_box(&scene, spec, options.pad, size)?,
                    None => {
                        let (width, height) = (size.0 & !1, size.1 & !1);
                        ([0, 0, width, height], None)
                    }
                });
            }
            let region = crop.as_ref().unwrap().0;
            let pending = gpu.begin_readback(Some(region))?;
            let read_at = Instant::now();
            if writers.is_none() {
                writers = Some(Writers::start(out, options, (region[2], region[3]))?);
            }
            let writers = writers.as_mut().unwrap();
            if options.scenes {
                writers.scene(index, &scene)?;
            }
            if let Some((at, previous_pending)) = in_flight.replace((index, pending)) {
                deliver(writers, &mut hashes, &mut previous, at, Arc::new(gpu.finish_readback(previous_pending)?))?;
            }
            prepares.push(gpu_at.duration_since(prepared_at).as_secs_f64());
            gpus.push(read_at.duration_since(gpu_at).as_secs_f64());
            readbacks.push(read_at.elapsed().as_secs_f64());
        }
        draws.push(drawn_at.elapsed().as_secs_f64());
        waited_at = Instant::now();
    };
    if let Some((at, pending)) = in_flight.take() {
        deliver(writers.as_mut().context("the first frame drew nothing")?, &mut hashes, &mut previous, at, Arc::new(gpu.finish_readback(pending)?))?;
    }
    let frames_done = started.elapsed().as_secs_f64();
    let count = times.len() as u32;
    let finishing = Instant::now();
    let writers = writers.context("the sequence captured no frame")?;
    let (png_busy, video) = writers.finish(out, options.fps)?;
    let (region, node) = crop.unwrap_or(([0, 0, 0, 0], None));
    let scale = scene.viewport().scale;
    let header = json!({"version": 1, "fps": options.fps, "frames": count, "times": times, "scale": scale});
    std::fs::write(out.join("trace.json"), serde_json::to_vec(&trace.to_value(header))?).context("writing trace.json")?;
    let finish_seconds = finishing.elapsed().as_secs_f64();
    let repeats: Vec<u32> = hashes.windows(2).enumerate().filter(|(_, pair)| pair[0] == pair[1]).map(|(index, _)| index as u32 + 1).collect();
    let distinct_times = {
        let mut sorted: Vec<f64> = times.iter().filter_map(Value::as_f64).collect();
        sorted.sort_by(f64::total_cmp);
        sorted.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        sorted.len()
    };
    let assets_missing = assets.diagnostics().to_vec();
    Ok(json!({
        "status": end["status"].as_str().map(str::to_string).or_else(|| ready.pointer("/delta/header/status").and_then(Value::as_str).map(str::to_string)).unwrap_or_else(|| "ok".into()),
        "failure": end["failure"],
        "frames": count,
        "fps": options.fps,
        "times": {"first": times.first(), "last": times.last(), "distinct": distinct_times},
        "framesDir": if options.png { Some(out.join("frames")) } else { None },
        "trace": out.join("trace.json"),
        "scenesDir": if options.scenes { Some(out.join("scenes")) } else { None },
        "video": video,
        "crop": {"node": node, "pixels": region, "rect": region.iter().map(|value| f64::from(*value) / scale).collect::<Vec<_>>()},
        "size": [region[2], region[3]],
        "nodes": scene.node_count(),
        "repeats": repeats,
        "unchanged": unchanged,
        "controls": ready["controls"],
        "missingImages": missing_images,
        "assetErrors": assets_missing,
        "diagnostics": if end["diagnostics"].is_null() { ready["diagnostics"].clone() } else { end["diagnostics"].clone() },
        "phases": {
            "mount": seconds(end["engine"]["mount"].as_f64().unwrap_or(0.0)),
            "firstFrame": seconds(first_at),
            "frames": seconds(frames_done - first_at),
            "finish": seconds(finish_seconds),
            "total": seconds(started.elapsed().as_secs_f64()),
        },
        "perFrame": {
            "engine": seconds(end["engine"]["frames"].as_f64().unwrap_or(0.0) / f64::from(count.saturating_sub(1).max(1))),
            "engineWait": seconds(median(waits)),
            "draw": seconds(median(draws)),
            "prepare": seconds(median(prepares)),
            "gpu": seconds(median(gpus)),
            "readback": seconds(median(readbacks)),
            "pngBusy": seconds(if count > 0 { png_busy / f64::from(count) } else { 0.0 }),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::cli::{parse, run_once};

    fn fixture_args(extra: &[&str]) -> Args {
        let repo = crate::engine::default_repo();
        let mut argv: Vec<String> = vec![
            "--repo".into(), repo.to_string_lossy().into_owned(),
            "--roots".into(), repo.join("tools/tests/oracle-root").to_string_lossy().into_owned(),
            "--fixture".into(), repo.join("fixtures/anim-tweens.luau").to_string_lossy().into_owned(),
            "--viewport".into(), "800x520".into(), "--no-snapshots".into(),
        ];
        argv.extend(extra.iter().map(|arg| arg.to_string()));
        parse(&argv).unwrap()
    }

    /// Every node path with its rects and rotations (a path's several instances sorted), as a scene draws them
    fn geometry(nodes: impl Iterator<Item = Value>) -> Vec<(String, Vec<String>)> {
        let mut by_path: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for node in nodes {
            if node["visible"].as_bool() == Some(false) {
                continue;
            }
            let path = node["path"].as_str().unwrap_or("").to_string();
            by_path.entry(path).or_default().push(format!("{} {}", node["rect"], node["rotation"]));
        }
        by_path
            .into_iter()
            .map(|(path, mut rects)| {
                rects.sort();
                (path, rects)
            })
            .collect()
    }

    /// Every message of a sequence of `args`, in order
    fn capture(args: &Args, spec: &SequenceSpec) -> Vec<SequenceMessage> {
        let mut messages = Vec::new();
        Preloaded::load(args).unwrap().sequence(args, spec, &mut |message| {
            messages.push(message);
            true
        });
        messages
    }

    /// Frame `i` of a sequence is `render --time` at its moment: the same nodes at the same places (the
    /// one-shot export also lists nodes their clip hides, which a session culls: those are left out), at
    /// exactly `from + i / fps` s from a whole-frame `from` (0.0167 is frame 1); at 30 fps every second
    /// engine frame. A rate that does not divide 60 steps the display cadence, exactly 1 / fps apart.
    #[test]
    fn a_sequence_frame_is_the_render_at_its_moment() {
        let args = fixture_args(&[]);
        for (from, fps, frames, first_frame, step) in [(0.0167, 60.0, 8u32, 1u32, 1u32), (0.5, 30.0, 4, 30, 2)] {
            let mut scene = SceneStore::default();
            let messages = capture(&args, &SequenceSpec { from, frames, fps });
            assert_eq!(messages.len(), frames as usize + 1, "every frame, then the end");
            for message in messages {
                match message {
                    SequenceMessage::Frame(SequenceFrame { index, time, mut reply }) => {
                        let engine_frame = first_frame + index * step;
                        assert!((time - f64::from(engine_frame) / 60.0).abs() < 1e-9, "frame {index} at {time}");
                        scene.apply_owned(&mut reply).unwrap();
                        let shown: std::collections::HashSet<u64> = scene.ordered_nodes().filter_map(|node| node["id"].as_u64()).collect();
                        let once = run_once(&fixture_args(&["--time", &(f64::from(engine_frame) / 60.0).to_string()])).unwrap();
                        let expected = once.scene["nodes"].as_array().unwrap().iter().filter(|node| node["id"].as_u64().is_some_and(|id| shown.contains(&id))).cloned();
                        assert_eq!(geometry(scene.ordered_nodes().cloned()), geometry(expected), "frame {index} ({fps} fps from {from})");
                    }
                    SequenceMessage::End(end) => assert_eq!(end.unwrap()["status"], "ok"),
                }
            }
        }
        let times: Vec<f64> = capture(&args, &SequenceSpec { from: 0.0, frames: 5, fps: 144.0 })
            .into_iter()
            .filter_map(|message| match message {
                SequenceMessage::Frame(frame) => Some(frame.time),
                SequenceMessage::End(_) => None,
            })
            .collect();
        assert_eq!(times.len(), 5);
        for (index, time) in times.iter().enumerate() {
            assert!((time - index as f64 / 144.0).abs() < 1e-9, "144 fps frame {index} at {time}");
        }
    }

    /// `--time` captures the engine frame nearest it: 1/60 written to a few places is frame 1, not frame 2
    #[test]
    fn a_time_a_hair_past_a_frame_is_that_frame() {
        let scene = |time: &str| geometry(run_once(&fixture_args(&["--time", time])).unwrap().scene["nodes"].as_array().unwrap().iter().cloned());
        let exact = scene("0.016666666666666666");
        assert_eq!(scene("0.0167"), exact);
        assert_eq!(scene("0.017"), exact);
        assert_ne!(scene("0.0333"), exact);
        assert_eq!(scene("0.0333"), scene("0.03333333333333333"));
    }

    #[test]
    fn options_read_moments_and_outputs() {
        let (spec, output) = parse_options(&json!({"from": "0", "to": "4", "fps": 60, "crop": "Stage", "pad": "8"}), false).unwrap();
        assert_eq!((spec.from, spec.frames, spec.fps), (0.0, 240, 60.0));
        assert!(matches!(output.crop, Some(Crop::Node(ref node)) if node == "Stage"));
        assert_eq!((output.pad, output.png, output.video, output.scenes), (8.0, true, true, false));
        assert_eq!(parse_options(&json!({"duration": 2, "fps": 30}), false).unwrap().0.frames, 60);
        assert_eq!(parse_options(&json!({"frames": 7, "to": 9}), false).unwrap().0.frames, 7);
        assert_eq!(parse_options(&json!({}), true).unwrap().0.frames, 60);
        assert!(parse_options(&json!({"from": 2, "to": 1}), false).is_err());
        assert!(parse_options(&json!({"fps": 0}), false).is_err());
        assert!(parse_options(&json!({"to": "soon"}), false).is_err());
        assert!(matches!(Crop::parse("10, 20, 300, 200"), Crop::Rect([x, y, w, h]) if (x, y, w, h) == (10.0, 20.0, 300.0, 200.0)));
        assert!(matches!(Crop::parse("Stage.HoverFrame"), Crop::Node(_)));
    }

    fn scene_of(nodes: Value) -> SceneStore {
        let mut scene = SceneStore::default();
        scene.apply(&json!({"version": 1, "viewport": {"width": 400, "height": 300, "scale": 1.5}, "nodes": nodes, "layers": [], "fonts": [], "images": []})).unwrap();
        scene
    }

    /// A node's own rect in whole device pixels, even-sized for H.264, inside the frame
    #[test]
    fn a_crop_is_the_node_in_even_device_pixels() {
        let scene = scene_of(json!([
            {"id": 1, "class": "Frame", "name": "Stage", "path": "CoreGui.RURStory.StoryTarget.Stage", "rect": [10, 20, 101, 51], "visible": true},
            {"id": 2, "class": "Frame", "name": "Edge", "path": "CoreGui.RURStory.StoryTarget.Edge", "rect": [350, 250, 50, 50], "visible": true},
        ]));
        let (pixels, node) = crop_box(&scene, &Crop::Node("Stage".into()), 0.0, (600, 450)).unwrap();
        assert_eq!(pixels, [15, 30, 152, 78]);
        assert_eq!(node.as_deref(), Some("CoreGui.RURStory.StoryTarget.Stage"));
        assert_eq!(crop_box(&scene, &Crop::Node("Stage".into()), 4.0, (600, 450)).unwrap().0, [9, 24, 164, 90]);
        // At the frame's edge an odd size gives up a pixel instead
        assert_eq!(crop_box(&scene, &Crop::Node("Edge".into()), 1.0, (600, 450)).unwrap().0, [523, 373, 76, 76]);
        assert_eq!(crop_box(&scene, &Crop::Rect([0.0, 0.0, 400.0, 300.0]), 0.0, (600, 450)).unwrap().0, [0, 0, 600, 450]);
        let missing = crop_box(&scene, &Crop::Node("Stag".into()), 0.0, (600, 450)).unwrap_err().to_string();
        assert!(missing.contains("no node matches") && missing.contains("Stage"), "{missing}");
    }

    /// The trace writes a row when a node's rect or rotation changes and `null` when it stops being drawn,
    /// names instances of one path apart, and leaves the story target off their paths
    #[test]
    fn the_trace_keeps_changes_only() {
        let node = |id: u64, name: &str, x: f64, visible: bool| json!({"id": id, "class": "Frame", "name": name, "path": format!("CoreGui.RURStory.StoryTarget.Root.{name}"), "rect": [x, 5, 10, 10], "rotation": 0, "visible": visible});
        let mut trace = Trace::default();
        trace.record(0, &scene_of(json!([node(1, "Bar", 0.0, true), node(2, "Spark", 3.0, true)])));
        trace.record(1, &scene_of(json!([node(1, "Bar", 0.0, true), node(2, "Spark", 4.0, true)])));
        trace.record(2, &scene_of(json!([node(1, "Bar", 2.5, true), node(3, "Spark", 9.0, true)])));
        trace.record(3, &scene_of(json!([node(1, "Bar", 2.5, false), node(3, "Spark", 9.0, true)])));
        let value = trace.to_value(json!({"version": 1}));
        let rows = |path: &str| value["nodes"].as_array().unwrap().iter().find(|node| node["path"] == path).map(|node| node["rows"].clone()).unwrap();
        assert_eq!(rows("Root.Bar"), json!([[0, 0.0, 5.0, 10.0, 10.0, 0.0], [2, 2.5, 5.0, 10.0, 10.0, 0.0], [3, null]]));
        assert_eq!(rows("Root.Spark"), json!([[0, 3.0, 5.0, 10.0, 10.0, 0.0], [1, 4.0, 5.0, 10.0, 10.0, 0.0], [2, null]]));
        assert_eq!(rows("Root.Spark#2"), json!([[2, 9.0, 5.0, 10.0, 10.0, 0.0]]));
        assert_eq!(value["nodes"][2]["id"], 3);
    }
}
