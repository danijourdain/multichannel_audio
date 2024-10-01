use std::fmt;

pub enum SampleRate {
    /// 44.1KHz sample rate.
    SR44100,

    /// 48KHz sample rate.
    SR48000,

    /// 96KHz sample rate.
    SR96000,

    /// 192KHz sample rate.
    SR192000,
}

impl fmt::Display for SampleRate {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                SampleRate::SR44100 => "44.1KHz",
                SampleRate::SR48000 => "48KHz",
                SampleRate::SR96000 => "96KHz",
                SampleRate::SR192000 => "192KHz",
            }
        )
    }
}
