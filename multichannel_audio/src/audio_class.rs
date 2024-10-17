use crate::{
    audio_settings::{DEFAULT_SAMPLE_FORMAT, DEFAULT_SAMPLE_RATE},
    methods::set_host_and_audio_device,
    sample_formats::SampleFormat,
    sample_rates::SampleRate,
    stream_controller::StreamController,
};

use super::methods::{DEVICE_NAME, HOST};
use anyhow::Ok;
use cpal::traits::{DeviceTrait, HostTrait};
use std::sync::{Arc, Mutex};

enum StreamControllerType {
    Input,
    Output,
}

pub trait Sample: Send + Sync + Clone {
    fn to_i32(&self) -> i32;
    fn from_i32(value: i32) -> Self;
}

impl Sample for i32 {
    fn to_i32(&self) -> i32 {
        *self
    }

    fn from_i32(value: i32) -> Self {
        value
    }
}

impl Sample for f32 {
    fn to_i32(&self) -> i32 {
        (*self * i32::MAX as f32) as i32
    }

    fn from_i32(value: i32) -> Self {
        value as f32 / i32::MAX as f32
    }
}

#[derive(Clone)]
/// Audio class for handling audio input and output
pub struct AudioInstance<T: Sample> {
    sample_rate: SampleRate,
    sample_format: SampleFormat,

    input_buffer_user: Arc<Mutex<Vec<T>>>,
    output_buffer_user: Arc<Mutex<Vec<T>>>,
    input_buffer_stream: Arc<Mutex<Vec<i32>>>,
    output_buffer_stream: Arc<Mutex<Vec<i32>>>,
    input_stream_controller: Option<StreamController>,
    output_stream_controller: Option<StreamController>,
    play_wait_pair: Arc<(Mutex<bool>, std::sync::Condvar)>,
    record_wait_pair: Arc<(Mutex<bool>, std::sync::Condvar)>,
    number_of_output_channels: u16,
    number_of_input_channels: u16,
}

// TODO: figure out how to wrap streams in a struct to safely implement Send for AudioInstance
unsafe impl<T: Sample> Send for AudioInstance<T> {}
unsafe impl<T: Sample> Sync for AudioInstance<T> {}

impl<T: Sample> AudioInstance<T> {
    /// Create a new audio instance. This will use the device that has already been initialized.
    ///
    /// # Arguments
    /// fs: u32 - the sample rate of the audio device
    ///
    /// # Errors
    /// Returns an error if the audio instance already exists
    /// Returns an error if the host has not been initialized
    /// Returns an error if the device is not found
    pub fn new(
        sample_rate: Option<SampleRate>,
        sample_format: Option<SampleFormat>,
    ) -> Result<Self, anyhow::Error> {
        // audio overhead - set up the audio device
        let mut device_name = DEVICE_NAME.lock().unwrap().clone();
        let mut binding = HOST.lock().unwrap();
        if binding.is_none() || device_name.is_empty() {
            set_host_and_audio_device()?;

            device_name = DEVICE_NAME.lock().unwrap().clone();
            binding = HOST.lock().unwrap();
        }

        let host = binding
            .as_ref()
            .ok_or_else(|| anyhow::Error::msg("Host not initialized"))?;

        let device = host
            .output_devices()?
            .find(|d| d.name().unwrap_or_default() == device_name)
            .ok_or(anyhow::Error::msg("Device not found"))?;

        let sample_rate = sample_rate.unwrap_or(*DEFAULT_SAMPLE_RATE);
        let sample_format = sample_format.unwrap_or(*DEFAULT_SAMPLE_FORMAT);

        let mut output_config = device.default_output_config()?.config();
        output_config.sample_rate = cpal::SampleRate(sample_rate.0);
        let mut input_config = device.default_input_config()?.config();
        input_config.sample_rate = cpal::SampleRate(sample_rate.0);

        // create an instance now to add the streams to later
        let mut audio_instance: AudioInstance<T> = AudioInstance {
            sample_format,
            sample_rate,

            input_buffer_user: Arc::new(Mutex::new(Vec::new())),
            output_buffer_user: Arc::new(Mutex::new(Vec::new())),
            input_buffer_stream: Arc::new(Mutex::new(Vec::new())),
            output_buffer_stream: Arc::new(Mutex::new(Vec::new())),
            input_stream_controller: None,
            output_stream_controller: None,
            play_wait_pair: Arc::new((Mutex::new(true), std::sync::Condvar::new())),
            record_wait_pair: Arc::new((Mutex::new(false), std::sync::Condvar::new())),
            number_of_output_channels: output_config.channels,
            number_of_input_channels: input_config.channels,
        };

        // create the output stream
        let output_buffer_clone = Arc::clone(&audio_instance.output_buffer_stream);
        let play_wait_clone = Arc::clone(&audio_instance.play_wait_pair);

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
        let input_buffer_clone = Arc::clone(&audio_instance.input_buffer_stream);
        let record_wait_clone = Arc::clone(&audio_instance.record_wait_pair);

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
            audio_instance.output_stream_controller = Some(output_stream_controller);
            audio_instance.input_stream_controller = Some(input_stream_controller);
        }

        Ok(audio_instance)
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
    pub fn play(&self, output_data: Vec<Vec<T>>) -> Result<(), anyhow::Error> {
        if self.number_of_output_channels != output_data.len() as u16 {
            return Err(anyhow::Error::msg("Number of channels does not match"));
        }

        // ensure the stream is running
        self.ensure_stream_running(StreamControllerType::Output)?;

        let flattened_output_data = self.flatten_output_data(output_data);

        // initialize the output buffer
        *self.output_buffer_stream.lock().unwrap() = flattened_output_data;

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

    fn ensure_stream_running(
        &self,
        stream_controller_type: StreamControllerType,
    ) -> Result<(), anyhow::Error> {
        // confirm the stream is running
        let stream_controller = match stream_controller_type {
            StreamControllerType::Input => &self.input_stream_controller,
            StreamControllerType::Output => &self.output_stream_controller,
        };

        match stream_controller {
            Some(ref s) => {
                if s.get_state() == super::stream_controller::StreamState::Stopped {
                    s.send_command(super::stream_controller::StreamCommand::Play);
                }
            }
            None => {
                return Err(anyhow::Error::msg("Output stream controller not found"));
            }
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
    pub fn record(&self, duration: f64) -> Result<Vec<Vec<T>>, anyhow::Error> {
        // ensure the stream is running
        self.ensure_stream_running(StreamControllerType::Input)?;

        let sample_rate = self.sample_rate;

        // ensure the buffer is empty
        *self.input_buffer_stream.lock().unwrap() = Vec::<i32>::with_capacity(
            (sample_rate.0 as f64 * duration) as usize * self.number_of_input_channels as usize,
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

        let recorded_data = self.input_buffer_stream.lock().unwrap().clone();

        let channel_recordings = self.convert_to_channel_data(recorded_data);

        return Ok(channel_recordings);
    }

    /// Play and record multiple channels of audio data.
    ///
    /// Play and record simultaneously. See the play and record functions for more details.
    pub fn play_record(&self, output_data: Vec<Vec<T>>) -> Result<Vec<Vec<T>>, anyhow::Error> {
        if self.number_of_output_channels != output_data.len() as u16 {
            return Err(anyhow::Error::msg(format!(
                "Number of channels does not match\n\tExpected: {}, Actual: {}",
                self.number_of_output_channels,
                output_data.len()
            )));
        }

        // ensure the streams are running
        self.ensure_stream_running(StreamControllerType::Output)?;
        self.ensure_stream_running(StreamControllerType::Input)?;

        // get the duration of the playback in seconds
        // this is used for the record section
        // since output_data is a vector of channels, we need the length of one of the channels not the outer length
        let duration = output_data[0].len() as f64 / self.sample_rate.0 as f64;

        // Set up the output buffer
        let flattened_data = self.flatten_output_data(output_data);
        *self.output_buffer_stream.lock().unwrap() = flattened_data;

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
        let input_buffer_capacity = (self.sample_rate.0 as f64 * duration) as usize
            * self.number_of_input_channels as usize;
        *self.input_buffer_stream.lock().unwrap() =
            Vec::<i32>::with_capacity(input_buffer_capacity);

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
        let input_buffer = self.input_buffer_stream.lock().unwrap().clone();
        let channel_recordings = self.convert_to_channel_data(input_buffer);

        Ok(channel_recordings)
    }

    fn flatten_output_data(&self, output_data: Vec<Vec<T>>) -> Vec<i32> {
        // convert from vector of channels to vector of samples
        let mut flattened_output_data: Vec<i32> = Vec::new();
        for sample_index in 0..output_data[0].len() {
            for channel in output_data.iter() {
                flattened_output_data.push(channel[sample_index].to_i32());
            }
        }
        flattened_output_data
    }

    fn convert_to_channel_data(&self, input_buffer: Vec<i32>) -> Vec<Vec<T>> {
        // convert recording to a vector of channels
        let mut channel_recordings: Vec<Vec<T>> =
            vec![Vec::new(); self.number_of_input_channels as usize];
        for chunk in input_buffer.chunks_exact(self.number_of_input_channels as usize) {
            for (channel_index, &sample) in chunk.iter().enumerate() {
                channel_recordings[channel_index].push(T::from_i32(sample));
            }
        }
        channel_recordings
    }
}

// #[cfg(test)]
// mod tests {
//     use crate::methods::set_host_and_audio_device;

//     use super::*;

//     lazy_static::lazy_static! { static ref TESTING_AUDIO_INSTANCE: Mutex<Option<AudioInstance>> = Mutex::new(None);}

//     fn get_audio_instance() -> AudioInstance {
//         let mut audio_instance = TESTING_AUDIO_INSTANCE.lock().unwrap();
//         if audio_instance.is_none() {
//             println!("Creating new audio instance");
//             let sample_rate = 44100;
//             set_host_and_audio_device().unwrap();
//             let new_audio_instance = AudioInstance::new(sample_rate).unwrap();
//             *audio_instance = Some(new_audio_instance);
//         }

//         assert!(audio_instance.is_some());
//         audio_instance.as_mut().unwrap().clone()
//     }

//     #[test]
//     fn test_audio_instance_new() {
//         let sample_rate = 44100;
//         let audio_instance = AudioInstance::new(sample_rate);
//         assert!(audio_instance.is_ok());
//     }

//     #[test]
//     fn test_play() {
//         let sample_rate = 44100;
//         let audio_instance: AudioInstance = get_audio_instance();

//         let output_data =
//             vec![vec![0; sample_rate]; audio_instance.number_of_output_channels as usize];
//         let result = audio_instance.play(output_data);
//         assert!(result.is_ok());
//     }

//     #[test]
//     fn test_record() {
//         let audio_instance: AudioInstance = get_audio_instance();

//         let duration = 1.0;
//         let result = audio_instance.record(duration);
//         assert!(result.is_ok());
//         let recorded_data = result.unwrap();
//         assert_eq!(
//             recorded_data.len(),
//             audio_instance.number_of_input_channels as usize
//         );
//     }

//     #[test]
//     fn test_play_record() {
//         let audio_instance: AudioInstance = get_audio_instance();

//         let output_data = vec![vec![0; 44100]; audio_instance.number_of_output_channels as usize];
//         let result = audio_instance.play_record(output_data);
//         assert!(result.is_ok());
//         let recorded_data = result.unwrap();
//         assert_eq!(
//             recorded_data.len(),
//             audio_instance.number_of_input_channels as usize
//         );
//     }

//     #[test]
//     fn test_play_invalid_channels() {
//         let audio_instance: AudioInstance = get_audio_instance();

//         let output_data =
//             vec![vec![0; 44100]; (audio_instance.number_of_output_channels - 1) as usize];
//         let result = audio_instance.play(output_data);
//         assert!(result.is_err());
//     }

//     #[test]
//     fn test_record_duration() {
//         let sample_rate = 44100;
//         let audio_instance: AudioInstance = get_audio_instance();

//         let duration = 2.0;
//         let result = audio_instance.record(duration);
//         assert!(result.is_ok());
//         let recorded_data = result.unwrap();
//         let expected_samples = (sample_rate as f64 * duration) as usize;
//         assert_eq!(recorded_data[0].len(), expected_samples);
//     }

//     #[test]
//     fn test_drop() {
//         let audio_instance: AudioInstance = get_audio_instance();

//         // drop the instance
//         drop(audio_instance);
//     }
// }
