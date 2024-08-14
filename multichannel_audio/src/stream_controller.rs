use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{InputCallbackInfo, OutputCallbackInfo, Stream};

pub enum StreamCommand {
    Play,
    Stop,
}

#[derive(Clone)]
pub enum StreamType {
    Input {
        record_wait: Arc<(Mutex<bool>, std::sync::Condvar)>,
        input_buffer: Arc<Mutex<Vec<i32>>>,
    },
    Output {
        output_buffer: Arc<Mutex<Vec<i32>>>,
        play_wait: Arc<(Mutex<bool>, std::sync::Condvar)>,
    },
}

#[derive(Clone)]
pub(crate) struct StreamController {
    command_sender: mpsc::Sender<StreamCommand>,
}

impl StreamController {
    pub fn new(stream_type: StreamType, device: cpal::Device, config: cpal::StreamConfig) -> Self {
        let (sender, receiver) = mpsc::channel();
        let config_clone = config.clone(); // Clone output_config

        thread::spawn(move || {
            let mut stream: Option<Stream> = None; // Initially, there's no stream

            for command in receiver {
                match command {
                    StreamCommand::Play => {
                        if stream.is_none() {
                            // Create the stream here, if not already created
                            match stream_type {
                                StreamType::Input {
                                    ref record_wait,
                                    ref input_buffer,
                                } => {
                                    let new_stream = create_input_stream(
                                        device.clone(),
                                        config.clone(),
                                        Arc::clone(&record_wait.clone()),
                                        Arc::clone(&input_buffer.clone()),
                                    );
                                    stream = Some(new_stream.unwrap());
                                }
                                StreamType::Output {
                                    ref output_buffer,
                                    ref play_wait,
                                } => {
                                    let new_stream = create_output_stream(
                                        &device,
                                        config_clone.clone(),
                                        Arc::clone(&output_buffer.clone()),
                                        Arc::clone(&play_wait.clone()),
                                    );
                                    stream = Some(new_stream.unwrap());
                                }
                            }
                        }
                        if let Some(ref s) = stream {
                            s.play().unwrap();
                        }
                    }
                    StreamCommand::Stop => {
                        if let Some(ref s) = stream {
                            s.pause().unwrap();
                        }
                    }
                }
            }
        });

        StreamController {
            command_sender: sender,
        }
    }

    pub fn send_command(&self, command: StreamCommand) {
        self.command_sender.send(command).unwrap();
    }
}

fn create_input_stream(
    device: cpal::Device,
    input_config: cpal::StreamConfig,
    record_wait_clone: Arc<(Mutex<bool>, std::sync::Condvar)>,
    input_buffer_clone: Arc<Mutex<Vec<i32>>>,
) -> Result<Stream, anyhow::Error> {
    let temp_input_stream = device.build_input_stream(
        &input_config,
        move |data: &[i32], _: &InputCallbackInfo| {
            let (record_wait, cvar) = &*record_wait_clone;
            // if we are not currently recording, don't do anything
            // this is so we don't continually record data and fill up the buffer unnecessarily
            if !(*record_wait.lock().unwrap()) {
                return;
            }
            let mut input_buffer = input_buffer_clone.lock().unwrap();

            if input_buffer.len() + data.len() < input_buffer.capacity() {
                // if we have room, keep recording
                input_buffer.extend_from_slice(data);
            } else if input_buffer.capacity() > 0 {
                // add as much as we can to the buffer
                let remaining_capacity = input_buffer.capacity() - input_buffer.len();
                input_buffer.extend_from_slice(&data[..remaining_capacity]);
                // we are done with input_buffer, drop it to prevent deadlock
                drop(input_buffer);

                // we have recorded all we need, notify the main thread
                *record_wait.lock().unwrap() = false;
                cvar.notify_all();
            }
        },
        err_fn,
        None,
    )?;
    Ok(temp_input_stream)
}

fn create_output_stream(
    device: &cpal::Device,
    output_config: cpal::StreamConfig,
    output_buffer: Arc<Mutex<Vec<i32>>>,
    play_wait: Arc<(Mutex<bool>, std::sync::Condvar)>,
) -> Result<Stream, anyhow::Error> {
    // create a local buffer for the callback to avoid locking the mutex buffer so much
    let mut callback_output_buffer = Vec::<i32>::new();
    let mut output_buffer_iterator = 0;

    let temp_output_stream = device.build_output_stream(
        &output_config,
        move |data: &mut [i32], _: &OutputCallbackInfo| {
            let (play_wait_bool, _) = &*play_wait;
            // if we aren't currently playing, don't do anything
            if !(*play_wait_bool.lock().unwrap()) {
                for i in 0..data.len() {
                    data[i] = 0;
                }
            }

            // check if we have enough data in the callback buffer
            if callback_output_buffer.is_empty() {
                // we don't have enough data, we need to get new data from the buffer
                callback_output_buffer = output_buffer.lock().unwrap().clone();

                // reset the output buffer iterator
                output_buffer_iterator = 0;
            }

            // iterate over the chunk and the corresponding channel of data
            let mut to_clear_buffer = false;

            // get the next chunk of data to write
            let number_of_samples = std::cmp::min(
                data.len(),
                callback_output_buffer.len() - output_buffer_iterator,
            );
            let end_index = output_buffer_iterator + number_of_samples;

            let chunk_data = &callback_output_buffer[output_buffer_iterator..end_index];

            for i in 0..data.len() {
                if i >= chunk_data.len() {
                    // we have reached the end of the signal, signal that we should stop
                    data[i] = 0;

                    // only send the signal to stop playing if we are currently playing
                    let (play_wait, cvar) = &*play_wait;
                    let mut play_wait = play_wait.lock().unwrap();
                    if *play_wait {
                        *play_wait = false;
                        cvar.notify_all();

                        // clear the local buffer
                        to_clear_buffer = true;
                    }
                } else {
                    // just write as normal
                    data[i] = chunk_data[i];
                }
            }

            output_buffer_iterator += number_of_samples;

            // clear the buffer if we have reached the end of the signal
            if to_clear_buffer {
                callback_output_buffer.clear();
                output_buffer_iterator = 0;

                let empty_vector = Vec::new();
                *output_buffer.lock().unwrap() = empty_vector;
            }
        },
        err_fn,
        None,
    )?;
    Ok(temp_output_stream)
}

fn err_fn(err: cpal::StreamError) {
    println!("an error occurred on stream: {}", err);
}
