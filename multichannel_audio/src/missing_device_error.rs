use std::{error::Error, fmt};

use cpal::HostUnavailable;

/// Error type for when the audio device is missing.
#[derive(Debug)]
pub enum MissingDeviceError {
    Error(String),
}

impl fmt::Display for MissingDeviceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            MissingDeviceError::Error(ref message) => write!(f, "Error: {}", message),
        }
    }
}

impl From<HostUnavailable> for MissingDeviceError {
    fn from(_error: HostUnavailable) -> Self {
        MissingDeviceError::Error("Failed to connect to Focusrite Host".to_string())
    }
}
impl Error for MissingDeviceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}
