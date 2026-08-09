const TARGET_RATE: u32 = 16_000;

pub fn resample_to_16k(samples: &[f32], input_rate: u32) -> Vec<f32> {
    if input_rate == TARGET_RATE || samples.is_empty() {
        return samples.to_vec();
    }

    let ratio = input_rate as f64 / TARGET_RATE as f64;
    let new_len = (samples.len() as f64 / ratio).round() as usize;
    let mut out = Vec::with_capacity(new_len);

    for i in 0..new_len {
        let src_index = i as f64 * ratio;
        let i0 = src_index.floor() as usize;
        let i1 = (i0 + 1).min(samples.len() - 1);
        let frac = (src_index - i0 as f64) as f32;
        out.push(samples[i0] * (1.0 - frac) + samples[i1] * frac);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_through_when_already_16k() {
        let samples = vec![0.1_f32, 0.2, 0.3];
        assert_eq!(resample_to_16k(&samples, 16_000), samples);
    }

    #[test]
    fn halves_length_when_downsampling_by_2x() {
        let samples: Vec<f32> = (0..320).map(|i| i as f32).collect();
        let result = resample_to_16k(&samples, 32_000);
        assert_eq!(result.len(), 160);
    }

    #[test]
    fn empty_input_returns_empty_output() {
        let result = resample_to_16k(&[], 48_000);
        assert!(result.is_empty());
    }
}
