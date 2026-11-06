pub mod coverage;
pub mod geometry;
pub mod gpu;
pub mod gradients;
mod groups;
mod image_records;
pub mod images;
mod node_records;
pub mod packet;
pub mod paint;
pub mod sampling;
mod scene_draw;
pub mod text;
pub mod trig;
pub use scene_draw::{DrawError, DrawPreparer, DrawStats, LAYER_CACHE_MISMATCHES, LAYER_CACHE_VERIFIED};

/// Drops `value` on a background thread: releasing a superseded scene (a scroll frame's moved nodes,
/// thousands of small allocations) costs about as much as building it. Inline when no thread starts.
pub(crate) fn drop_later<T: Send + 'static>(value: T) {
    use std::sync::{Mutex, OnceLock, mpsc};
    type Garbage = Box<dyn Send>;
    static DROPPER: OnceLock<Option<Mutex<mpsc::Sender<Garbage>>>> = OnceLock::new();
    let sender = DROPPER.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<Garbage>();
        std::thread::Builder::new()
            .name("draw-dropper".into())
            .spawn(move || while let Ok(garbage) = receiver.recv() { drop(garbage) })
            .ok()
            .map(|_| Mutex::new(sender))
    });
    let garbage: Garbage = Box::new(value);
    match sender.as_ref().and_then(|s| s.lock().ok()) {
        Some(sender) => {
            let _ = sender.send(garbage);
        }
        None => drop(garbage),
    }
}
