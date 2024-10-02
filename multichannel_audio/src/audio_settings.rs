use super::sample_formats::SampleFormat;
use super::sample_rates::SampleRate;
use lazy_static::lazy_static;

lazy_static! {
    /// The sample format for audio streams. Deafults to `SampleFormat::F32`.
    pub static ref DEFAULT_SAMPLE_FORMAT: SampleFormat = SampleFormat::F32;

    /// The sample rate for audio streams. Defaults to `44100`.
    pub static ref DEFAULT_SAMPLE_RATE: SampleRate = SampleRate(44100);
}
