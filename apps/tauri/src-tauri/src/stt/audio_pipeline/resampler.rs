pub(super) struct LinearResampler {
    input_rate: u32,
    output_rate: u32,
    ratio: f32,
}

impl LinearResampler {
    pub(super) fn new(input_rate: u32, output_rate: u32) -> Self {
        let ratio = if output_rate == 0 {
            1.0
        } else {
            input_rate as f32 / output_rate as f32
        };
        Self {
            input_rate,
            output_rate,
            ratio,
        }
    }

    pub(super) fn needs_resample(&self) -> bool {
        self.input_rate != self.output_rate
    }

    pub(super) fn process(&self, input: &[f32]) -> Vec<f32> {
        if !self.needs_resample() {
            return input.to_vec();
        }

        if input.is_empty() {
            return Vec::new();
        }

        let output_len = ((input.len() as f32) / self.ratio).ceil() as usize;
        let mut output = Vec::with_capacity(output_len);

        for i in 0..output_len {
            let src_pos = i as f32 * self.ratio;
            let lower_idx = src_pos.floor() as usize;
            let upper_idx = std::cmp::min(lower_idx + 1, input.len() - 1);
            let frac = src_pos - lower_idx as f32;
            let lower = input[lower_idx];
            let upper = input[upper_idx];
            let interpolated = lower + (upper - lower) * frac;
            output.push(interpolated);
        }

        output
    }
}
