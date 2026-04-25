#[derive(Clone, Copy)]
pub struct BenchmarkConfig {
    pub warmup_frames: u32,
    pub sample_frames: u32,
}

pub struct BenchmarkState {
    config: BenchmarkConfig,
    frame_index: u32,
    sampled_seconds: f64,
}

impl BenchmarkState {
    pub fn new(config: BenchmarkConfig) -> Self {
        Self {
            config,
            frame_index: 0,
            sampled_seconds: 0.0,
        }
    }

    #[inline]
    pub fn config(&self) -> BenchmarkConfig {
        self.config
    }

    pub fn record_frame(&mut self, dt: f32) -> Option<BenchmarkResult> {
        self.frame_index += 1;
        if self.frame_index > self.config.warmup_frames {
            self.sampled_seconds += dt as f64;
        }

        let total_target = self.config.warmup_frames + self.config.sample_frames;
        if self.frame_index >= total_target {
            Some(BenchmarkResult {
                sample_frames: self.config.sample_frames,
                total_seconds: self.sampled_seconds,
            })
        } else {
            None
        }
    }
}

pub struct BenchmarkResult {
    pub sample_frames: u32,
    total_seconds: f64,
}

impl BenchmarkResult {
    pub fn fps(&self) -> f64 {
        self.sample_frames as f64 / self.total_seconds.max(f64::EPSILON)
    }

    pub fn frame_ms(&self) -> f64 {
        self.total_seconds * 1000.0 / self.sample_frames.max(1) as f64
    }
}
