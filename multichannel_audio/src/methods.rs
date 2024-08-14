#![allow(dead_code)]

use cpal::traits::{DeviceTrait, HostTrait};
use hound::{self, SampleFormat};
use lazy_static::lazy_static;
use std::f32::consts::PI;
use std::io::Cursor;
use std::path::Path;
use std::sync::Mutex;

use crate::missing_device_error::MissingDeviceError;

lazy_static! {
    pub static ref HOST: Mutex<Option<cpal::Host>> = Mutex::new(None);
    pub static ref DEVICE_NAME: Mutex<String> = Mutex::new(String::new());
}

/// Set the host and audio device to use for audio I/O
///
/// Currently defaults to Focusrite with no way to change it.
/// This will be updated in the future to allow the user to select the audio device.
///
/// On Windows, defaults to ASIO and on Linux the default host is used.
pub fn set_host_and_audio_device() -> Result<(), MissingDeviceError> {
    #[cfg(target_os = "windows")]
    {
        let host =
            cpal::host_from_id(cpal::HostId::Asio).map_err(|e| MissingDeviceError::from(e))?; // Convert cpal error to MissingDeviceError
        *HOST.lock().unwrap() = Some(host);
        *DEVICE_NAME.lock().unwrap() = "Focusrite USB ASIO".to_string();
    }
    #[cfg(target_os = "linux")]
    {
        let host = cpal::default_host();
        *HOST.lock().unwrap() = Some(host);
        *DEVICE_NAME.lock().unwrap() = "hw:CARD=USB,DEV=0".to_string();
    }

    let device_exists = match (*HOST)
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .devices()
        .map_err(|_| "failed to get devices".to_string())
    {
        Ok(mut devices) => devices.any(|d| {
            d.name()
                .map_or(false, |name: String| name == *DEVICE_NAME.lock().unwrap())
        }),
        Err(_) => false,
    };

    if !device_exists {
        return Err(MissingDeviceError::Error("Device not found".to_string()));
    }

    Ok(())
}

/// Generate a sine wave signal.
pub fn generate_sine_wave(frequency: u32, duration: f32, fs: u32) -> Vec<i32> {
    let signal: Vec<f32> = (0..(fs as f32 * duration) as usize)
        .map(|i| ((i as u32 * frequency * 2) as f32 * PI / fs as f32).sin() as f32)
        .collect();

    let signal: Vec<i32> = signal
        .iter()
        .map(|&x| (x * i32::MAX as f32) as i32)
        .collect(); // Convert the signal to i32
                    //return the signal
    signal
}

/// Generate a white noise signal.
pub fn generate_gaussian_white_noise(
    duration_seconds: f32,
    fs: u32,
    _scalar: Option<f32>,
) -> Vec<i32> {
    let wav_file_contents = include_bytes!("../assets/full_spectrum_white_noise.wav");

    // read the white noise file
    let white_noise = read_wave_file_data(Cursor::new(wav_file_contents.to_vec()), fs).unwrap();

    // trim the white noise to the desired duration
    let white_noise: Vec<i32> = white_noise
        .iter()
        .take((duration_seconds * fs as f32) as usize)
        .cloned()
        .collect();

    white_noise
}

pub(crate) fn print_devices() -> Result<(), Box<dyn std::error::Error>> {
    let binding = HOST.lock().unwrap();
    let host = binding.as_ref().ok_or("Host not initialized")?;

    println!("Input devices:");
    let input_devices = host.input_devices()?;
    for device in input_devices {
        let config = device.default_input_config()?;
        println!(
            "Device: {}, input channels: {}",
            device.name()?,
            config.channels()
        );
    }

    println!("Output devices:");
    let output_devices = host.output_devices()?;
    for device in output_devices {
        let config = device.default_output_config()?;
        println!(
            "Device: {}, output channels: {}",
            device.name()?,
            config.channels()
        );
    }

    Ok(())
}

/// Convert from a single channel signal to a multi-channel signal.
///
/// This is useful for playing a single channel signal on a multi-channel audio interface.
///
/// Puts the signal in specified playback_index channel and nothing in all other channels.
pub fn format_signal_for_multichannel(
    signal: Vec<i32>,
    playback_index: usize,
    output_channels: usize,
) -> Vec<Vec<i32>> {
    let mut multi_channel_data = vec![vec![0; signal.len()]; output_channels];
    multi_channel_data[playback_index] = signal;
    multi_channel_data
}

/// Save a signal to a WAV file.
pub fn save_to_wav(data: &Vec<i32>, filename: &str, sample_rate: u32) -> Result<(), anyhow::Error> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(filename, spec)?;
    let sliced_data = data.as_slice();

    for &sample in sliced_data {
        writer.write_sample(sample)?;
    }

    writer.finalize()?;

    Ok(())
}

/// Read a WAV file from a byte array.
pub fn read_wave_file_dart(byte_data: Vec<u8>, fs: u32) -> Result<Vec<i32>, hound::Error> {
    let cursor = Cursor::new(byte_data);
    read_wave_file_data(cursor, fs)
}

fn read_wave_file_data<R: std::io::Read + std::io::Seek>(
    reader: R,
    fs: u32,
) -> Result<Vec<i32>, hound::Error> {
    let mut reader = hound::WavReader::new(reader)?;
    let spec = reader.spec();

    assert_eq!(spec.sample_rate, fs, "Sample rate of WAV file does not match the sample rate of the audio interface.\n\tWAV file sample rate: {}\n\tAudio interface sample rate: {}", spec.sample_rate, fs);

    let samples: Vec<i32> = match (spec.sample_format, spec.bits_per_sample) {
        // get int samples (any int format with 32 bits or less)
        (SampleFormat::Int, _) => reader.samples::<i32>().collect::<Result<Vec<_>, _>>()?,

        // get float samples and convert them to int32
        (SampleFormat::Float, 32) => {
            let float_samples: Vec<f32> = reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?;
            float_samples
                .into_iter()
                .map(|sample| (sample * std::i32::MAX as f32) as i32)
                .collect()
        }
        _ => return Err(hound::Error::Unsupported),
    };

    Ok(samples)
}

/// Read a WAV file from a file path.
pub fn read_wave_file(filepath: &Path, fs: u32) -> Result<Vec<i32>, hound::Error> {
    let mut reader = hound::WavReader::open(filepath)?;
    let spec = reader.spec();

    assert_eq!(spec.sample_rate, fs, "Sample rate of WAV file does not match the sample rate of the audio interface.\n\tWAV file sample rate: {}\n\tAudio interface sample rate: {}", spec.sample_rate, fs);

    let samples: Vec<i32> = match (spec.sample_format, spec.bits_per_sample) {
        // get int samples (any int format with 32 bits or less)
        (SampleFormat::Int, _) => reader.samples::<i32>().collect::<Result<Vec<_>, _>>()?,

        // get float samples and convert them to int32
        (SampleFormat::Float, 32) => {
            let float_samples: Vec<f32> = reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?;
            float_samples
                .into_iter()
                .map(|sample| (sample * std::i32::MAX as f32) as i32)
                .collect()
        }
        _ => return Err(hound::Error::Unsupported),
    };

    Ok(samples)
}
