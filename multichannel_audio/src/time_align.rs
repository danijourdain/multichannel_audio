use crate::audio_class::AudioInstance;

use super::methods;
use anyhow::Result;

impl AudioInstance {
    /// Play and record simultaneously with loopback timing signal.
    ///
    /// See the play and record functions for more details.
    pub fn aligned_play_record(
        &self,
        training_signal: Vec<i32>,
        training_channel: usize,
        timing_channel_out: usize,
        timing_channel_in: usize,
        number_of_output_channels: usize,
    ) -> Result<Vec<Vec<i32>>, anyhow::Error> {
        let duration = training_signal.len() as f64 / self.sample_rate as f64;
        let output_data = self
            .assemble_signal_with_loopback(
                &training_signal,
                duration as usize,
                training_channel,
                timing_channel_out,
                self.sample_rate,
                number_of_output_channels,
            )
            .unwrap();
        let mut recorded_data = self.play_record(output_data)?;
        let aligned_data = self.align_with_loopback(&mut recorded_data, timing_channel_in)?;
        Ok(aligned_data)
    }

    fn assemble_signal_with_loopback(
        &self,
        training_signal: &Vec<i32>,
        duration: usize,
        training_channel: usize,
        timing_output: usize,
        fs: u32,
        number_of_output_channels: usize,
    ) -> Result<Vec<Vec<i32>>, anyhow::Error> {
        let timing_index = timing_output - 1;
        let training_index = training_channel - 1;

        let mut training_vec = vec![vec![0i32; duration * fs as usize]; number_of_output_channels];

        // loop the training signal to fill the duration
        let mut training_signal = training_signal.clone();
        if training_signal.len() < duration * fs as usize {
            let mut training_signal_loop = training_signal.clone();
            while training_signal_loop.len() < duration * fs as usize {
                training_signal_loop.extend(training_signal.iter());
            }
            training_signal = training_signal_loop;
        }

        // Populate outer_vec[0] with as much of signal as possible
        for (i, &value) in training_signal.iter().enumerate() {
            if i < duration * fs as usize {
                training_vec[training_index as usize][i] = value as i32;
            } else {
                break;
            }
        }

        // Read chirp from wave file
        let chirp_bytes = include_bytes!("../assets/chirp.wav").to_vec();
        let chirp = methods::read_wave_file_dart(chirp_bytes, fs).unwrap();

        // Format chirp for multichannel
        let mut chirp_vec =
            methods::format_signal_for_multichannel(chirp, timing_index, number_of_output_channels);

        // Create a vector of zeros of size fs. We need this to null of any noise when the recording initializes
        let mut gap = vec![vec![0i32; fs as usize / 2]; number_of_output_channels];

        // assemble the final training signal
        // Check that all vectors have the same number of channels
        assert_eq!(gap.len(), chirp_vec.len());
        assert_eq!(gap.len(), training_vec.len());

        // Append chirp_vec and training_vec to the inner vectors of gap
        for ((gap_channel, chirp_channel), training_channel) in
            gap.iter_mut().zip(&mut chirp_vec).zip(&mut training_vec)
        {
            gap_channel.append(chirp_channel);
            gap_channel.append(training_channel);
        }

        return Ok(gap);
    }

    fn find_start(&self, loopback: &mut Vec<i32>) -> Result<usize, anyhow::Error> {
        // Convert loopback to f64 values for normalization
        let mut loopback_f64: Vec<f64> = loopback.iter().map(|&x| x as f64).collect();

        // Remove any noise at the start
        for val in loopback_f64.iter_mut().take(24000) {
            *val = 0.0;
        }

        // Normalize loopback
        let max = loopback_f64.iter().cloned().fold(f64::NAN, f64::max);
        for val in &mut loopback_f64 {
            *val /= max;
        }

        // Find indices of values greater than 0.2
        let trigger: Vec<usize> = loopback_f64
            .iter()
            .enumerate()
            .filter_map(|(i, &val)| if val >= 0.2 { Some(i) } else { None })
            .collect();

        // if trigger is later than 2 seconds in, signal is corrupted
        if trigger.len() == 0 || trigger[trigger.len() - 1] > 96000 {
            return Err(anyhow::anyhow!(
            "Timing trigger is later than 2 seconds. Signal is corrupted likely due to timing channel assign error."
        ));
        }

        // Calculate start sample
        let start_sample = trigger[trigger.len() - 1] + 15; // Add 15 samples to ensure we are at the start of the signal
        println!("start_sample: {}", start_sample);

        Ok(start_sample)
    }

    fn align_with_loopback(
        &self,
        array: &mut Vec<Vec<i32>>,
        timing_channel: usize,
    ) -> Result<Vec<Vec<i32>>, anyhow::Error> {
        // Subtract 1 from timing_channel as Rust uses 0-based indexing
        let timing_channel = timing_channel
            .checked_sub(1)
            .ok_or(anyhow::anyhow!("timing_channel must be greater than 0"))?;

        // Find the start sample
        let start_sample = self.find_start(&mut array[timing_channel])?;
        // println!("Start sample: {}", start_sample);

        // Remove the first start_sample elements from each channel
        for channel in array.iter_mut() {
            channel.drain(..start_sample);
        }

        Ok(array.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assemble_signal_with_loopback() {
        let audio_instance = AudioInstance::new(48000).unwrap();
        let training_signal = vec![1, 2, 3, 4, 5];
        let duration = 5;
        let training_channel = 1;
        let timing_output = 2;
        let fs = 48000;
        let number_of_output_channels = 2;

        let result = audio_instance
            .assemble_signal_with_loopback(
                &training_signal,
                duration,
                training_channel,
                timing_output,
                fs,
                number_of_output_channels,
            )
            .unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].len(), 240000);
        assert_eq!(result[1].len(), 240000);
    }
}
