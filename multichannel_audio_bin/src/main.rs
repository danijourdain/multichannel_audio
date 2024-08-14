use std::os::windows::thread;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
use std::vec;

use multichannel_audio::audio_class;
use multichannel_audio::methods::generate_gaussian_white_noise;
use multichannel_audio::methods::set_host_and_audio_device;

fn main() {
    set_host_and_audio_device().unwrap();

    let fs = 48000;
    let train_duration = 5;

    let mut training_signal = generate_gaussian_white_noise(train_duration as f32, fs, None);
    training_signal.truncate(60000);
    // let training_file_path =
    // Path::new("C:\\flutter projects\\outdoor_edge_control\\assets\\audio\\white_80-2000.wav");

    let audio_instance = audio_class::AudioInstance::new(fs).unwrap();

    // let mut training_signal = read_wave_file(training_file_path, fs).unwrap();

    // training_signal.truncate((train_duration * fs) as usize);

    // let mut record_iterator = 0;
    // while record_iterator < 200 {
    //     match audio_instance.record(1.0) {
    //         Ok(_) => {}
    //         Err(e) => {
    //             println!("Error recording signal: {}", e);
    //             break;
    //         }
    //     };
    //     record_iterator += 1;
    // }
    // println!("Done with recording i = {}", record_iterator);

    let mut quiet_signal = vec![vec![0; 480000]; 6];
    quiet_signal[0] = training_signal[0..48000].to_vec();

    audio_instance
        .aligned_play_record(training_signal, 1, 4, 6, 6)
        .unwrap();

    // let mut i = 0;
    // while i < 10000 {
    //     println!("Starting record #{}", i);
    //     // let _ = match audio_instance.record(1.0) {
    //     let _ = match audio_instance.play_record(quiet_signal.clone()) {
    //         Ok(_) => {}
    //         Err(e) => {
    //             println!("Error playing signal: {}", e);
    //             break;
    //         }
    //     };
    //     println!("Done record #{}", i);

    //     i += 1;
    // }

    // println!("Done with i = {}", i);
}
