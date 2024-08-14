# Multichannel Rust Audio

This library provides an easy to use audio library to play and record multi-channel audio.

It is inspired by [Python Sounddevice](https://python-sounddevice.readthedocs.io/) and its beginner-friendly functions.

This library is primarily a wrapper around the [CPAL](https://crates.io/crates/cpal) crate. It abstracts the stream creation and provides simple play/record functions.

Currently only **Linux** and **Windows** are supported while using **Focusrite** audio interfaces. More support is planned in the future.

## Getting Started

Add the following to your Cargo.toml file

```toml
[dependencies]
rust_audio = "0.1.0"
```

## How To Use

- Initialize the audio device once at the start of your program.

- Prepare a 2-dimensional audio array with number of columns equal to the number of channels on your audio device. Ex. If playing on a stereo 2-channel device, your array would be 2 by x where x is the number of samples to play.

- Record for a specified duration into a new 2-dimensional array. The same principles apply as playback for the shape of the data.

## Examples
Play White Noise out of channel 1 of a 6-channel audio device at 48kHz sample rate
```rust
set_host_and_audio_device().unwrap();

let signal = generate_gaussian_white_noise(5.0, 48000, None);
let mut multichannel_signal = vec![vec![0; 5 * 48000]; 6];
multichannel_signal[0] = signal;

let audio_instance = audio_class::ZsiAudio::new(48000).unwrap();
audio_instance.play(multichannel_signal).unwrap();
```

Record for 5 seconds
```rust
set_host_and_audio_device().unwrap();

let audio_instance = audio_class::ZsiAudio::new(48000).unwrap();
let recording = audio_instance.record(5.0).unwrap();
```

## Licence

Licensed under the MIT License ([LICENSE](https://github.com/danijourdain/rust-audio/blob/main/LICENSE) or <https://opensource.org/license/MIT>)
