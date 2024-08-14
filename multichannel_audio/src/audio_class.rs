use crate::stream_controller::StreamController;

use super::methods::{DEVICE_NAME, HOST};
use anyhow::Ok;
use cpal::traits::{DeviceTrait, HostTrait};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
/// Audio class for handling audio input and output
pub struct AudioInstance {
    input_buffer: Arc<Mutex<Vec<i32>>>,
    output_buffer: Arc<Mutex<Vec<i32>>>,
    input_stream_controller: Option<StreamController>,
    output_stream_controller: Option<StreamController>,
    play_wait_pair: Arc<(Mutex<bool>, std::sync::Condvar)>,
    pub(super) sample_rate: u32,
    record_wait_pair: Arc<(Mutex<bool>, std::sync::Condvar)>,
    number_of_output_channels: u16,
    number_of_input_channels: u16,
}

// TODO: figure out how to wrap streams in a struct to safely implement Send for AudioInstance
unsafe impl Send for AudioInstance {}
unsafe impl Sync for AudioInstance {}

impl Drop for AudioInstance {
    fn drop(&mut self) {
        // stop the streams
        if let Some(ref output_stream_controller) = self.output_stream_controller {
            output_stream_controller.send_command(super::stream_controller::StreamCommand::Stop);
        }
        if let Some(ref input_stream_controller) = self.input_stream_controller {
            input_stream_controller.send_command(super::stream_controller::StreamCommand::Stop);
        }
    }
}

impl AudioInstance {
    /// Create a new audio instance. This will use the device that has already been initialized.
    ///
    /// # Arguments
    /// fs: u32 - the sample rate of the audio device
    ///
    /// # Panics
    /// Panics if the host has not been initialized
    /// Panics if the device is not found
    pub fn new(fs: u32) -> Result<Self, anyhow::Error> {
        // audio overhead - set up the audio device
        let device_name = DEVICE_NAME.lock().unwrap().clone();
        let binding = HOST.lock().unwrap();
        let host = binding
            .as_ref()
            .ok_or_else(|| anyhow::Error::msg("Host not initialized"))?;

        let device = host
            .output_devices()?
            .find(|d| d.name().unwrap_or_default() == device_name)
            .ok_or(anyhow::Error::msg("Device not found"))?;

        let mut output_config = device.default_output_config()?.config();
        output_config.sample_rate = cpal::SampleRate(fs);
        let mut input_config = device.default_input_config()?.config();
        input_config.sample_rate = cpal::SampleRate(fs);

        // create an instance now to add the streams to later
        let mut zsi_audio_instance = AudioInstance {
            input_buffer: Arc::new(Mutex::new(Vec::new())),
            output_buffer: Arc::new(Mutex::new(Vec::new())),
            input_stream_controller: None,
            output_stream_controller: None,
            play_wait_pair: Arc::new((Mutex::new(true), std::sync::Condvar::new())),
            sample_rate: fs,
            record_wait_pair: Arc::new((Mutex::new(false), std::sync::Condvar::new())),
            number_of_output_channels: output_config.channels,
            number_of_input_channels: input_config.channels,
        };

        // create the output stream
        let output_buffer_clone = Arc::clone(&zsi_audio_instance.output_buffer);
        let play_wait_clone = Arc::clone(&zsi_audio_instance.play_wait_pair);

        let output_stream_controller = StreamController::new(
            super::stream_controller::StreamType::Output {
                output_buffer: output_buffer_clone,
                play_wait: play_wait_clone,
            },
            device.clone(),
            output_config,
        );
        output_stream_controller.send_command(super::stream_controller::StreamCommand::Play);

        // create the input stream
        let input_buffer_clone = Arc::clone(&zsi_audio_instance.input_buffer);
        let record_wait_clone = Arc::clone(&zsi_audio_instance.record_wait_pair);

        let input_stream_controller = StreamController::new(
            super::stream_controller::StreamType::Input {
                input_buffer: input_buffer_clone,
                record_wait: record_wait_clone,
            },
            device,
            input_config,
        );
        input_stream_controller.send_command(super::stream_controller::StreamCommand::Play);

        // add the streams to the instance
        {
            zsi_audio_instance.output_stream_controller = Some(output_stream_controller);
            zsi_audio_instance.input_stream_controller = Some(input_stream_controller);
        }

        Ok(zsi_audio_instance)
    }

    /// Play multiple channels of audio data.
    ///
    /// The number of channels must match the number of output channels of the audio device.
    /// The length of each channel must be the same.
    ///
    /// The audio data is played in the order of the channels.
    /// This function blocks until the audio has finished playing.
    ///
    /// # Arguments
    /// output_data: Vec<Vec<i32> - the audio data to play. The outer vector represents the channels and the inner vector represents the samples.
    pub fn play(&self, output_data: Vec<Vec<i32>>) -> Result<(), anyhow::Error> {
        if self.number_of_output_channels != output_data.len() as u16 {
            return Err(anyhow::Error::msg("Number of channels does not match"));
        }

        let flattened_output_data = self.flatten_output_data(output_data);

        // initialize the output buffer
        *self.output_buffer.lock().unwrap() = flattened_output_data;

        let play_wait_pair_clone = Arc::clone(&self.play_wait_pair);
        let (lock, cvar) = &*play_wait_pair_clone;
        let mut play_wait = lock.lock().unwrap();

        // start playing audio
        *play_wait = true;
        while *play_wait {
            play_wait = cvar.wait(play_wait).unwrap();
        }

        Ok(())
    }

    /// Record multiple channels of audio data.
    ///
    /// This function blocks until the audio has finished recording.
    ///
    /// # Arguments
    /// duration: f64 - the duration of the recording in seconds
    ///
    /// # Returns
    /// A vector of channels where each channel is a vector of samples
    pub fn record(&self, duration: f64) -> Result<Vec<Vec<i32>>, anyhow::Error> {
        let sample_rate = self.sample_rate;

        // ensure the buffer is empty
        *self.input_buffer.lock().unwrap() = Vec::<i32>::with_capacity(
            (sample_rate as f64 * duration) as usize * self.number_of_input_channels as usize,
        );

        let record_wait_pair_clone = Arc::clone(&self.record_wait_pair);
        let (lock, cvar) = &*record_wait_pair_clone;
        let mut start_recording = lock.lock().unwrap();
        // start recording audio
        *start_recording = true;

        // wait until start_recording is set to false
        while *start_recording {
            start_recording = cvar.wait(start_recording).unwrap();
        }
        drop(start_recording);

        let recorded_data = self.input_buffer.lock().unwrap().clone();

        let channel_recordings = self.convert_to_channel_data(recorded_data);

        return Ok(channel_recordings);
    }

    /// Play and record multiple channels of audio data.
    ///
    /// Play and record simultaneously. See the play and record functions for more details.
    pub fn play_record(&self, output_data: Vec<Vec<i32>>) -> Result<Vec<Vec<i32>>, anyhow::Error> {
        if self.number_of_output_channels != output_data.len() as u16 {
            return Err(anyhow::Error::msg(format!(
                "Number of channels does not match\n\tExpected: {}, Actual: {}",
                self.number_of_output_channels,
                output_data.len()
            )));
        }

        // get the duration of the playback in seconds
        // this is used for the record section
        // since output_data is a vector of channels, we need the length of one of the channels not the outer length
        let duration = output_data[0].len() as f64 / self.sample_rate as f64;

        // Set up the output buffer
        let flattened_data = self.flatten_output_data(output_data);
        *self.output_buffer.lock().unwrap() = flattened_data;

        // Start playback in a separate thread
        let play_handle = {
            let play_wait_clone = Arc::clone(&self.play_wait_pair);

            std::thread::spawn(move || {
                let (lock, cvar) = &*play_wait_clone;
                let mut play_wait = lock.lock().unwrap();
                *play_wait = true;

                while *play_wait {
                    play_wait = cvar.wait(play_wait).unwrap();
                }
            })
        };

        // Set up the input buffer
        let input_buffer_capacity =
            (self.sample_rate as f64 * duration) as usize * self.number_of_input_channels as usize;
        *self.input_buffer.lock().unwrap() = Vec::<i32>::with_capacity(input_buffer_capacity);

        // Create condition variables to synchronize play and record
        let record_wait_pair_clone = Arc::clone(&self.record_wait_pair);

        // Start recording in a separate thread
        let record_handle = {
            std::thread::spawn(move || {
                let (lock, cvar) = &*record_wait_pair_clone;
                let mut record_wait = lock.lock().unwrap();
                *record_wait = true;

                while *record_wait {
                    record_wait = cvar.wait(record_wait).unwrap();
                }
            })
        };

        // Wait for both threads to complete
        play_handle.join().unwrap();
        record_handle.join().unwrap();

        // Get the recorded data
        let input_buffer = self.input_buffer.lock().unwrap().clone();
        let channel_recordings = self.convert_to_channel_data(input_buffer);

        Ok(channel_recordings)
    }

    fn flatten_output_data(&self, output_data: Vec<Vec<i32>>) -> Vec<i32> {
        // convert from vector of channels to vector of samples
        let mut flattened_output_data: Vec<i32> = Vec::new();
        for sample_index in 0..output_data[0].len() {
            for channel in output_data.iter() {
                flattened_output_data.push(channel[sample_index]);
            }
        }
        flattened_output_data
    }

    fn convert_to_channel_data(&self, input_buffer: Vec<i32>) -> Vec<Vec<i32>> {
        // convert recording to a vector of channels
        let mut channel_recordings: Vec<Vec<i32>> =
            vec![Vec::new(); self.number_of_input_channels as usize];
        for chunk in input_buffer.chunks_exact(self.number_of_input_channels as usize) {
            for (channel_index, &sample) in chunk.iter().enumerate() {
                channel_recordings[channel_index].push(sample);
            }
        }
        channel_recordings
    }
}
