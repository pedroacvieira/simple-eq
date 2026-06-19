use nih_plug::prelude::*;
use std::sync::Arc;

const LOW_SHELF_HZ: f64 = 200.0;
const HIGH_SHELF_HZ: f64 = 8000.0;

// First-order IIR: H(z) = (b0 + b1·z⁻¹) / (1 + a1·z⁻¹)
#[derive(Clone, Copy, Default)]
struct Filter {
    b0: f64,
    b1: f64,
    a1: f64,
    x1: f64,
    y1: f64,
}

impl Filter {
    fn set_coeffs(&mut self, b0: f64, b1: f64, a1: f64) {
        self.b0 = b0;
        self.b1 = b1;
        self.a1 = a1;
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 - self.a1 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

// Low shelf: H(s) = (s + A·ω₀) / (s + ω₀), DC gain = A, HF gain = 1
// Bilinear transform with K = tan(π·fc/fs):
//   b0 = (K+A)/(K+1),  b1 = (A-K)/(K+1),  a1 = (1-K)/(K+1)
fn low_shelf_coeffs(gain_db: f32, sample_rate: f32) -> (f64, f64, f64) {
    let a = 10.0_f64.powf(gain_db as f64 / 20.0);
    let k = (std::f64::consts::PI * LOW_SHELF_HZ / sample_rate as f64).tan();
    let denom = k + 1.0;
    ((k + a) / denom, (a - k) / denom, (1.0 - k) / denom)
}

// High shelf: H(s) = A·(s + ω₀) / (s + A·ω₀), DC gain = 1, HF gain = A
// Bilinear transform with K = tan(π·fc/fs):
//   b0 = A·(K+1)/(K+A),  b1 = A·(1-K)/(K+A),  a1 = (A-K)/(K+A)
fn high_shelf_coeffs(gain_db: f32, sample_rate: f32) -> (f64, f64, f64) {
    let a = 10.0_f64.powf(gain_db as f64 / 20.0);
    let k = (std::f64::consts::PI * HIGH_SHELF_HZ / sample_rate as f64).tan();
    let denom = k + a;
    (a * (k + 1.0) / denom, a * (1.0 - k) / denom, (a - k) / denom)
}

struct SimpleEq {
    params: Arc<SimpleEqParams>,
    sample_rate: f32,
    low_filters: Vec<Filter>,
    high_filters: Vec<Filter>,
    // Sentinels to detect coefficient changes without recomputing tan() every sample
    prev_lows_db: f32,
    prev_highs_db: f32,
}

#[derive(Params)]
struct SimpleEqParams {
    /// Low shelf gain at 200 Hz, 6 dB/octave
    #[id = "lows"]
    pub lows_db: FloatParam,

    /// High shelf gain at 8 kHz, 6 dB/octave
    #[id = "highs"]
    pub highs_db: FloatParam,
}

impl Default for SimpleEqParams {
    fn default() -> Self {
        let range = FloatRange::Linear {
            min: -18.0,
            max: 18.0,
        };
        Self {
            lows_db: FloatParam::new("Lows", 0.0, range)
                .with_unit(" dB")
                .with_step_size(0.1)
                .with_smoother(SmoothingStyle::Linear(20.0)),
            highs_db: FloatParam::new("Highs", 0.0, range)
                .with_unit(" dB")
                .with_step_size(0.1)
                .with_smoother(SmoothingStyle::Linear(20.0)),
        }
    }
}

impl Default for SimpleEq {
    fn default() -> Self {
        Self {
            params: Arc::new(SimpleEqParams::default()),
            sample_rate: 44100.0,
            low_filters: vec![Filter::default(); 2],
            high_filters: vec![Filter::default(); 2],
            prev_lows_db: f32::MAX,
            prev_highs_db: f32::MAX,
        }
    }
}

impl Plugin for SimpleEq {
    const NAME: &'static str = "Simple EQ";
    const VENDOR: &'static str = "Eigenblue";
    const URL: &'static str = env!("CARGO_PKG_HOMEPAGE");
    const EMAIL: &'static str = "pedro.vieira@eigenblue.ai";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        let num_channels = audio_io_layout
            .main_input_channels
            .map(|c| c.get() as usize)
            .unwrap_or(2);
        self.low_filters = vec![Filter::default(); num_channels];
        self.high_filters = vec![Filter::default(); num_channels];
        self.prev_lows_db = f32::MAX;
        self.prev_highs_db = f32::MAX;
        true
    }

    fn reset(&mut self) {
        self.low_filters.iter_mut().for_each(|f| f.reset());
        self.high_filters.iter_mut().for_each(|f| f.reset());
        self.prev_lows_db = f32::MAX;
        self.prev_highs_db = f32::MAX;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        for channel_samples in buffer.iter_samples() {
            let lows_db = self.params.lows_db.smoothed.next();
            let highs_db = self.params.highs_db.smoothed.next();

            if lows_db != self.prev_lows_db {
                let (b0, b1, a1) = low_shelf_coeffs(lows_db, self.sample_rate);
                self.low_filters.iter_mut().for_each(|f| f.set_coeffs(b0, b1, a1));
                self.prev_lows_db = lows_db;
            }
            if highs_db != self.prev_highs_db {
                let (b0, b1, a1) = high_shelf_coeffs(highs_db, self.sample_rate);
                self.high_filters.iter_mut().for_each(|f| f.set_coeffs(b0, b1, a1));
                self.prev_highs_db = highs_db;
            }

            for (ch, sample) in channel_samples.into_iter().enumerate() {
                if ch >= self.low_filters.len() {
                    break;
                }
                let x = *sample as f64;
                let y = self.low_filters[ch].process(x);
                let y = self.high_filters[ch].process(y);
                *sample = y as f32;
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for SimpleEq {
    const CLAP_ID: &'static str = "ai.eigenblue.simple-eq";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Two-band shelving EQ");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Equalizer,
    ];
}

impl Vst3Plugin for SimpleEq {
    // Must be exactly 16 bytes; keep stable across releases to preserve DAW sessions
    const VST3_CLASS_ID: [u8; 16] = *b"SimpleEQEigenbl_";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Eq];
}

nih_export_clap!(SimpleEq);
nih_export_vst3!(SimpleEq);
