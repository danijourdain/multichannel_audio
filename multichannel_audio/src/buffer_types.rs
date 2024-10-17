use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub(crate) enum Buffer {
    F32(Arc<Mutex<Vec<f32>>>),
    I32(Arc<Mutex<Vec<i32>>>),
    // TODO: Add other types
}
