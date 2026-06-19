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
// Bilinear substitution s → (1/k)·(1−z⁻¹)/(1+z⁻¹), k = tan(π·fc/fs):
//   b0 = (1+A·k)/(1+k),  b1 = (A·k−1)/(1+k),  a1 = (k−1)/(1+k)
fn low_shelf_coeffs(gain_db: f32, sample_rate: f32) -> (f64, f64, f64) {
    let a = 10.0_f64.powf(gain_db as f64 / 20.0);
    let k = (std::f64::consts::PI * LOW_SHELF_HZ / sample_rate as f64).tan();
    let denom = 1.0 + k;
    ((1.0 + a * k) / denom, (a * k - 1.0) / denom, (k - 1.0) / denom)
}

// High shelf: H(s) = A·(s + ω₀) / (s + A·ω₀), DC gain = 1, HF gain = A
// Bilinear substitution s → (1/k)·(1−z⁻¹)/(1+z⁻¹), k = tan(π·fc/fs):
//   b0 = A·(1+k)/(1+A·k),  b1 = A·(k−1)/(1+A·k),  a1 = (A·k−1)/(1+A·k)
fn high_shelf_coeffs(gain_db: f32, sample_rate: f32) -> (f64, f64, f64) {
    let a = 10.0_f64.powf(gain_db as f64 / 20.0);
    let k = (std::f64::consts::PI * HIGH_SHELF_HZ / sample_rate as f64).tan();
    let denom = 1.0 + a * k;
    (a * (1.0 + k) / denom, a * (k - 1.0) / denom, (a * k - 1.0) / denom)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const FS: f32 = 44100.0;
    const DB_TOL: f64 = 0.01; // tolerance for exact analytical checks

    // ── helpers ───────────────────────────────────────────────────────────

    fn to_db(linear: f64) -> f64 {
        20.0 * linear.log10()
    }

    // Exact magnitude of H(e^jw) = (b0 + b1·e^−jw) / (1 + a1·e^−jw)
    // Uses the factored complex form to avoid catastrophic cancellation when b0 ≈ −b1.
    fn freq_mag(b0: f64, b1: f64, a1: f64, w: f64) -> f64 {
        let (c, s) = (w.cos(), w.sin());
        let re_n = b0 + b1 * c;
        let im_n = b1 * s;
        let re_d = 1.0 + a1 * c;
        let im_d = a1 * s;
        ((re_n * re_n + im_n * im_n) / (re_d * re_d + im_d * im_d)).sqrt()
    }

    // H(z=1): exact DC gain
    fn dc_mag(b0: f64, b1: f64, a1: f64) -> f64 {
        (b0 + b1) / (1.0 + a1)
    }

    // H(z=−1): exact Nyquist gain
    fn nyquist_mag(b0: f64, b1: f64, a1: f64) -> f64 {
        (b0 - b1) / (1.0 - a1)
    }

    // Drive a filter with a sine at freq_hz; after settling, return RMS(out)/RMS(in)
    fn measure_gain(f: &mut Filter, freq_hz: f64, settle: usize, measure: usize) -> f64 {
        let w = 2.0 * PI * freq_hz / FS as f64;
        for n in 0..settle {
            f.process((w * n as f64).sin());
        }
        let (mut sum_out, mut sum_in) = (0.0_f64, 0.0_f64);
        for n in settle..settle + measure {
            let x = (w * n as f64).sin();
            let y = f.process(x);
            sum_out += y * y;
            sum_in += x * x;
        }
        (sum_out / sum_in).sqrt()
    }

    // ── Coefficient correctness ───────────────────────────────────────────

    #[test]
    fn low_shelf_dc_gain_matches_target() {
        for &gain_db in &[-18.0_f32, -12.0, -6.0, 0.0, 6.0, 12.0, 18.0] {
            let (b0, b1, a1) = low_shelf_coeffs(gain_db, FS);
            let got = to_db(dc_mag(b0, b1, a1));
            assert!(
                (got - gain_db as f64).abs() < DB_TOL,
                "low shelf DC: expected {gain_db} dB, got {got:.4} dB"
            );
        }
    }

    #[test]
    fn low_shelf_nyquist_is_zero_db() {
        for &gain_db in &[-18.0_f32, -6.0, 0.0, 6.0, 18.0] {
            let (b0, b1, a1) = low_shelf_coeffs(gain_db, FS);
            let got = to_db(nyquist_mag(b0, b1, a1));
            assert!(
                got.abs() < DB_TOL,
                "low shelf Nyquist should be 0 dB for gain {gain_db}, got {got:.4} dB"
            );
        }
    }

    #[test]
    fn high_shelf_dc_is_zero_db() {
        for &gain_db in &[-18.0_f32, -6.0, 0.0, 6.0, 18.0] {
            let (b0, b1, a1) = high_shelf_coeffs(gain_db, FS);
            let got = to_db(dc_mag(b0, b1, a1));
            assert!(
                got.abs() < DB_TOL,
                "high shelf DC should be 0 dB for gain {gain_db}, got {got:.4} dB"
            );
        }
    }

    #[test]
    fn high_shelf_nyquist_matches_target() {
        for &gain_db in &[-18.0_f32, -12.0, -6.0, 0.0, 6.0, 12.0, 18.0] {
            let (b0, b1, a1) = high_shelf_coeffs(gain_db, FS);
            let got = to_db(nyquist_mag(b0, b1, a1));
            assert!(
                (got - gain_db as f64).abs() < DB_TOL,
                "high shelf Nyquist: expected {gain_db} dB, got {got:.4} dB"
            );
        }
    }

    // ── Shelf direction ───────────────────────────────────────────────────

    #[test]
    fn low_shelf_direction_correct() {
        // fc/100 = 2 Hz: below A·fc even at ±18 dB, so we're deep inside the shelf → ≈ target
        // Nyquist (exact): well above any shelf frequency → 0 dB (unique to low shelf shape)
        for &gain_db in &[-18.0_f32, -6.0, 6.0, 18.0] {
            let (b0, b1, a1) = low_shelf_coeffs(gain_db, FS);
            let w_inside = 2.0 * PI * (LOW_SHELF_HZ / 100.0) / FS as f64;
            let inside_db = to_db(freq_mag(b0, b1, a1, w_inside));
            let nyq_db    = to_db(nyquist_mag(b0, b1, a1));
            assert!(
                (inside_db - gain_db as f64).abs() < 0.5,
                "low shelf at fc/100: expected {gain_db} dB, got {inside_db:.3} dB"
            );
            assert!(
                nyq_db.abs() < DB_TOL,
                "low shelf at Nyquist: expected 0 dB for gain {gain_db}, got {nyq_db:.4} dB"
            );
        }
    }

    #[test]
    fn high_shelf_direction_correct() {
        // fc/100 = 80 Hz: below A·fc even for large cuts, so well outside the shelf → ≈ 0 dB
        // Nyquist (exact): deep inside the shelf → matches target
        for &gain_db in &[-18.0_f32, -6.0, 6.0, 18.0] {
            let (b0, b1, a1) = high_shelf_coeffs(gain_db, FS);
            let w_outside = 2.0 * PI * (HIGH_SHELF_HZ / 100.0) / FS as f64;
            let outside_db = to_db(freq_mag(b0, b1, a1, w_outside));
            let nyq_db     = to_db(nyquist_mag(b0, b1, a1));
            assert!(
                outside_db.abs() < 0.5,
                "high shelf at fc/100: expected ~0 dB for gain {gain_db}, got {outside_db:.3} dB"
            );
            assert!(
                (nyq_db - gain_db as f64).abs() < DB_TOL,
                "high shelf at Nyquist: expected {gain_db} dB, got {nyq_db:.4} dB"
            );
        }
    }

    // ── Filter::process correctness ───────────────────────────────────────

    #[test]
    fn filter_dc_steady_state_matches_gain() {
        // Feed DC=1.0; output must converge to 10^(gain_db/20)
        for &gain_db in &[-12.0_f32, 0.0, 12.0] {
            let target = 10.0_f64.powf(gain_db as f64 / 20.0);
            let (b0, b1, a1) = low_shelf_coeffs(gain_db, FS);
            let mut f = Filter::default();
            f.set_coeffs(b0, b1, a1);
            for _ in 0..20_000 {
                f.process(1.0);
            }
            let out = f.process(1.0);
            assert!(
                (out - target).abs() < 1e-9,
                "DC steady-state at {gain_db} dB: expected {target:.9}, got {out:.9}"
            );
        }
    }

    #[test]
    fn process_matches_analytical_gain() {
        // Empirically measured gain must match freq_mag() to within 0.01 dB
        for &gain_db in &[-12.0_f32, 0.0, 12.0] {
            let test_freq = 50.0_f64; // well below 200 Hz shelf
            let (b0, b1, a1) = low_shelf_coeffs(gain_db, FS);
            let w = 2.0 * PI * test_freq / FS as f64;
            let expected = freq_mag(b0, b1, a1, w);
            let mut f = Filter::default();
            f.set_coeffs(b0, b1, a1);
            let measured = measure_gain(&mut f, test_freq, 4096, 8192);
            // ~0.5% tolerance: RMS over a non-integer number of cycles has a partial-period
            // bias that doesn't cancel when input and output have different phase offsets.
            assert!(
                (measured - expected).abs() < 5e-3,
                "at {test_freq} Hz, {gain_db} dB: analytical={expected:.6}, measured={measured:.6}"
            );
        }
    }

    // ── State management ──────────────────────────────────────────────────

    #[test]
    fn reset_clears_state() {
        let (b0, b1, a1) = low_shelf_coeffs(18.0, FS);
        let mut f = Filter::default();
        f.set_coeffs(b0, b1, a1);
        for n in 0..1000 {
            f.process((0.1 * n as f64).sin());
        }
        f.reset();
        // Zero input into a zeroed filter must return exactly zero
        assert_eq!(f.process(0.0), 0.0, "x1 and y1 should be zero after reset");
        assert_eq!(f.process(0.0), 0.0);
    }

    #[test]
    fn filters_are_independent_per_channel() {
        let (b0, b1, a1) = low_shelf_coeffs(12.0, FS);
        let mut left = Filter::default();
        let mut right = Filter::default();
        left.set_coeffs(b0, b1, a1);
        right.set_coeffs(b0, b1, a1);

        for n in 0..1000 {
            left.process((0.01 * n as f64).sin());
            right.process(0.0);
        }
        // Right should have zero state; left should not
        assert_eq!(right.process(0.0), 0.0, "silent channel must stay silent");
        assert_ne!(left.x1, 0.0, "active channel must have non-zero state");
    }

    // ── Stability ─────────────────────────────────────────────────────────

    #[test]
    fn no_nan_or_inf_with_full_scale_input() {
        let cases = [
            low_shelf_coeffs(18.0, FS),
            low_shelf_coeffs(-18.0, FS),
            high_shelf_coeffs(18.0, FS),
            high_shelf_coeffs(-18.0, FS),
        ];
        for (b0, b1, a1) in cases {
            let mut f = Filter::default();
            f.set_coeffs(b0, b1, a1);
            for _ in 0..10_000 {
                let y = f.process(1.0);
                assert!(y.is_finite(), "output became non-finite: {y}");
            }
        }
    }

    #[test]
    fn filter_decays_to_silence_after_input_stops() {
        let (b0, b1, a1) = low_shelf_coeffs(12.0, FS);
        let mut f = Filter::default();
        f.set_coeffs(b0, b1, a1);
        for n in 0..1000 {
            f.process((0.01 * n as f64).sin());
        }
        // A stable filter fed silence must converge to zero
        let mut last = 1.0_f64;
        for _ in 0..100_000 {
            last = f.process(0.0);
        }
        assert!(last.abs() < 1e-10, "filter didn't decay to zero: {last:.2e}");
    }
}
