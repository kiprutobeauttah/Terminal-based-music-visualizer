// FFT processing module

use log::{debug, warn};
use ringbuf::traits::Consumer;
use rustfft::{num_complex::Complex, FftPlanner};
use std::f32::consts::PI;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::audio::RingConsumer;

/// FFT size for processing (2048 samples provides good frequency resolution)
pub const FFT_SIZE: usize = 2048;

/// FFT Engine that transforms time-domain audio samples into frequency-domain spectrum
pub struct FftEngine {
    fft_size: usize,
    planner: FftPlanner<f32>,
    window: Vec<f32>,
    input_buffer: Vec<Complex<f32>>,
    output_buffer: Vec<Complex<f32>>,
    sample_source: RingConsumer,
    overlap_buffer: Vec<f32>,
}

impl FftEngine {
    /// Create a new FFT engine with the specified FFT size and sample source
    pub fn new(fft_size: usize, sample_source: RingConsumer) -> Self {
        let planner = FftPlanner::new();
        let window = Self::generate_hann_window(fft_size);
        
        debug!("Initialized FFT engine with size: {}", fft_size);
        
        FftEngine {
            fft_size,
            planner,
            window,
            input_buffer: vec![Complex::new(0.0, 0.0); fft_size],
            output_buffer: vec![Complex::new(0.0, 0.0); fft_size],
            sample_source,
            overlap_buffer: Vec::new(),
        }
    }
    
    /// Generate a Hann window function to reduce spectral leakage
    /// Formula: w(n) = 0.5 * (1 - cos(2πn/N))
    fn generate_hann_window(size: usize) -> Vec<f32> {
        (0..size)
            .map(|n| 0.5 * (1.0 - ((2.0 * PI * n as f32) / (size as f32 - 1.0)).cos()))
            .collect()
    }
    
    /// Process a block of audio samples and return frequency magnitudes in decibels
    /// Returns None if not enough samples are available
    pub fn process_block(&mut self) -> Option<Vec<f32>> {
        // Calculate how many samples we need (50% overlap means we need half FFT size new samples)
        let hop_size = self.fft_size / 2;
        
        // Read samples from ring buffer
        let mut samples = vec![0.0f32; hop_size];
        let read_count = self.sample_source.pop_slice(&mut samples);
        
        if read_count < hop_size {
            // Not enough samples available
            return None;
        }
        
        // Build the full FFT input buffer with 50% overlap
        let mut full_samples = Vec::with_capacity(self.fft_size);
        
        // Add overlap from previous block (second half of previous block)
        if self.overlap_buffer.len() == hop_size {
            full_samples.extend_from_slice(&self.overlap_buffer);
        } else {
            // First block - pad with zeros
            full_samples.resize(hop_size, 0.0);
        }
        
        // Add new samples
        full_samples.extend_from_slice(&samples[..hop_size]);
        
        // Store second half for next overlap
        self.overlap_buffer.clear();
        self.overlap_buffer.extend_from_slice(&full_samples[hop_size..]);
        
        // Apply Hann window to reduce spectral leakage
        self.apply_window(&full_samples);
        
        // Compute FFT
        let fft = self.planner.plan_fft_forward(self.fft_size);
        fft.process(&mut self.input_buffer);
        
        // Convert complex output to magnitude values in decibels
        let magnitudes = self.compute_magnitudes();
        
        Some(magnitudes)
    }
    
    /// Apply Hann window to samples and store in input buffer
    fn apply_window(&mut self, samples: &[f32]) {
        for (i, &sample) in samples.iter().enumerate() {
            self.input_buffer[i] = Complex::new(sample * self.window[i], 0.0);
        }
    }
    
    /// Convert complex FFT output to magnitude values in decibels
    /// Only processes positive frequencies (bins 0 to N/2) since input is real
    fn compute_magnitudes(&self) -> Vec<f32> {
        let num_bins = self.fft_size / 2 + 1;
        let mut magnitudes = Vec::with_capacity(num_bins);
        
        for i in 0..num_bins {
            let complex = self.input_buffer[i];
            let magnitude = (complex.re * complex.re + complex.im * complex.im).sqrt();
            
            // Convert to decibels: 20 * log10(magnitude)
            // Add small epsilon to avoid log(0)
            let db = 20.0 * (magnitude + 1e-10).log10();
            magnitudes.push(db);
        }
        
        magnitudes
    }
}

/// Frequency band for logarithmic binning
#[derive(Debug, Clone)]
struct FrequencyBand {
    start_bin: usize,
    end_bin: usize,
    center_freq: f32,
}

/// Frequency binner that maps FFT bins to logarithmic frequency bands
pub struct FrequencyBinner {
    bands: Vec<FrequencyBand>,
    fft_size: usize,
    sample_rate: f32,
}

impl FrequencyBinner {
    /// Create a new frequency binner with the specified number of bands
    /// Frequency range: 20 Hz to 20 kHz (human hearing range)
    pub fn new(num_bands: usize, fft_size: usize, sample_rate: f32) -> Self {
        let f_min = 20.0; // Minimum frequency (Hz)
        let f_max = 20000.0; // Maximum frequency (Hz)
        
        let bands = Self::calculate_logarithmic_bands(num_bands, f_min, f_max, fft_size, sample_rate);
        
        debug!("Created {} logarithmic frequency bands from {} Hz to {} Hz", 
               num_bands, f_min, f_max);
        
        FrequencyBinner {
            bands,
            fft_size,
            sample_rate,
        }
    }
    
    /// Calculate logarithmic frequency bands
    /// Formula: f(i) = f_min * (f_max/f_min)^(i/N)
    fn calculate_logarithmic_bands(
        num_bands: usize,
        f_min: f32,
        f_max: f32,
        fft_size: usize,
        sample_rate: f32,
    ) -> Vec<FrequencyBand> {
        let mut bands = Vec::with_capacity(num_bands);
        let ratio = (f_max / f_min).powf(1.0 / num_bands as f32);
        
        for i in 0..num_bands {
            // Calculate frequency range for this band
            let freq_start = f_min * ratio.powf(i as f32);
            let freq_end = f_min * ratio.powf((i + 1) as f32);
            let center_freq = (freq_start + freq_end) / 2.0;
            
            // Convert frequencies to FFT bin indices
            // Bin frequency = (bin_index * sample_rate) / fft_size
            let start_bin = ((freq_start * fft_size as f32) / sample_rate).floor() as usize;
            let end_bin = ((freq_end * fft_size as f32) / sample_rate).ceil() as usize;
            
            // Clamp to valid range
            let start_bin = start_bin.min(fft_size / 2);
            let end_bin = end_bin.min(fft_size / 2 + 1).max(start_bin + 1);
            
            bands.push(FrequencyBand {
                start_bin,
                end_bin,
                center_freq,
            });
        }
        
        bands
    }
    
    /// Bin the FFT spectrum into logarithmic frequency bands
    /// Averages multiple FFT bins for each frequency band
    pub fn bin_spectrum(&self, fft_magnitudes: &[f32]) -> Vec<f32> {
        let mut binned = Vec::with_capacity(self.bands.len());
        
        for band in &self.bands {
            // Average all FFT bins in this frequency band
            let mut sum = 0.0;
            let mut count = 0;
            
            for bin_idx in band.start_bin..band.end_bin {
                if bin_idx < fft_magnitudes.len() {
                    sum += fft_magnitudes[bin_idx];
                    count += 1;
                }
            }
            
            let average = if count > 0 {
                sum / count as f32
            } else {
                0.0
            };
            
            binned.push(average);
        }
        
        binned
    }
    
    /// Get the number of bands
    pub fn num_bands(&self) -> usize {
        self.bands.len()
    }
    
    /// Adapt number of bands based on terminal width (32-64 bands)
    pub fn adapt_to_width(terminal_width: usize, fft_size: usize, sample_rate: f32) -> Self {
        // Use terminal width as guide, clamped to reasonable range
        let num_bands = terminal_width.clamp(32, 64);
        Self::new(num_bands, fft_size, sample_rate)
    }
}

/// Spectrum smoother that applies temporal smoothing to reduce visual jitter
pub struct SpectrumSmoother {
    smoothed_values: Vec<f32>,
    peak_values: Vec<f32>,
    peak_decay_rate: f32,
    smoothing_factor: f32,
}

impl SpectrumSmoother {
    /// Create a new spectrum smoother with the specified number of bands
    /// 
    /// # Arguments
    /// * `num_bands` - Number of frequency bands to smooth
    /// * `smoothing_factor` - Exponential moving average factor (0.0-1.0, default 0.7)
    ///   Higher values = more responsive, lower values = smoother
    pub fn new(num_bands: usize, smoothing_factor: f32) -> Self {
        debug!("Initialized SpectrumSmoother with {} bands, smoothing factor: {}", 
               num_bands, smoothing_factor);
        
        SpectrumSmoother {
            smoothed_values: vec![0.0; num_bands],
            peak_values: vec![0.0; num_bands],
            peak_decay_rate: 0.95,
            smoothing_factor: smoothing_factor.clamp(0.0, 1.0),
        }
    }
    
    /// Apply smoothing to new spectrum values
    /// Returns a reference to the smoothed values
    /// 
    /// Uses exponential moving average: smoothed = α * new + (1-α) * old
    /// where α is the smoothing factor (0.7 by default)
    pub fn smooth(&mut self, new_values: &[f32]) -> &[f32] {
        // Ensure buffer sizes match
        if new_values.len() != self.smoothed_values.len() {
            warn!("Spectrum size mismatch: expected {}, got {}", 
                  self.smoothed_values.len(), new_values.len());
            return &self.smoothed_values;
        }
        
        // Apply exponential moving average to each band
        for i in 0..new_values.len() {
            let new_val = new_values[i];
            let old_val = self.smoothed_values[i];
            
            // Exponential moving average: smoothed = α * new + (1-α) * old
            self.smoothed_values[i] = self.smoothing_factor * new_val 
                                     + (1.0 - self.smoothing_factor) * old_val;
        }
        
        // Update peak values (iterate directly to avoid borrow issues)
        for i in 0..self.smoothed_values.len() {
            let current_value = self.smoothed_values[i];
            let current_peak = self.peak_values[i];
            
            // If current value exceeds peak, update peak
            if current_value > current_peak {
                self.peak_values[i] = current_value;
            } else {
                // Otherwise, decay the peak
                self.peak_values[i] = current_peak * self.peak_decay_rate;
            }
        }
        
        &self.smoothed_values
    }
    
    /// Update peak hold values with decay
    /// Peaks decay at 0.95 per frame (5% reduction per frame)
    fn update_peaks(&mut self, values: &[f32]) {
        for i in 0..values.len() {
            let current_value = values[i];
            let current_peak = self.peak_values[i];
            
            // If current value exceeds peak, update peak
            if current_value > current_peak {
                self.peak_values[i] = current_value;
            } else {
                // Otherwise, decay the peak
                self.peak_values[i] = current_peak * self.peak_decay_rate;
            }
        }
    }
    
    /// Get the current smoothed values
    pub fn smoothed_values(&self) -> &[f32] {
        &self.smoothed_values
    }
    
    /// Get the current peak values
    pub fn peak_values(&self) -> &[f32] {
        &self.peak_values
    }
    
    /// Reset all smoothed and peak values to zero
    pub fn reset(&mut self) {
        self.smoothed_values.fill(0.0);
        self.peak_values.fill(0.0);
    }
}

/// Shared spectrum data that is updated by FFT thread and read by render thread
#[derive(Debug, Clone)]
pub struct SpectrumData {
    pub bands: Vec<f32>,
    pub timestamp: Instant,
}

impl SpectrumData {
    /// Create new spectrum data with the specified number of bands
    pub fn new(num_bands: usize) -> Self {
        SpectrumData {
            bands: vec![0.0; num_bands],
            timestamp: Instant::now(),
        }
    }
}

/// Type alias for shared spectrum data buffer
pub type SharedSpectrum = Arc<Mutex<SpectrumData>>;

/// FFT processor that runs on a dedicated thread
pub struct FftProcessor {
    engine: FftEngine,
    binner: FrequencyBinner,
    spectrum_buffer: SharedSpectrum,
    sample_rate: u32,
}

impl FftProcessor {
    /// Create a new FFT processor
    pub fn new(
        sample_source: RingConsumer,
        num_bands: usize,
        sample_rate: u32,
    ) -> (Self, SharedSpectrum) {
        let engine = FftEngine::new(FFT_SIZE, sample_source);
        let binner = FrequencyBinner::new(num_bands, FFT_SIZE, sample_rate as f32);
        let spectrum_buffer = Arc::new(Mutex::new(SpectrumData::new(num_bands)));
        
        let processor = FftProcessor {
            engine,
            binner,
            spectrum_buffer: spectrum_buffer.clone(),
            sample_rate,
        };
        
        (processor, spectrum_buffer)
    }
    
    /// Run the FFT processing loop
    /// Updates spectrum data at 30-60 Hz rate
    pub fn run(mut self) {
        use std::thread;
        use std::time::Duration;
        
        // Target update rate: 60 Hz (16.67ms per update)
        let target_interval = Duration::from_millis(16);
        
        debug!("Starting FFT processing loop");
        
        loop {
            let loop_start = Instant::now();
            
            // Process audio block
            match self.engine.process_block() {
                Some(fft_magnitudes) => {
                    // Bin the spectrum into logarithmic bands
                    let binned_spectrum = self.binner.bin_spectrum(&fft_magnitudes);
                    
                    // Update shared spectrum buffer
                    match self.spectrum_buffer.lock() {
                        Ok(mut spectrum) => {
                            spectrum.bands = binned_spectrum;
                            spectrum.timestamp = Instant::now();
                        }
                        Err(e) => {
                            warn!("Failed to lock spectrum buffer: {}", e);
                        }
                    }
                }
                None => {
                    // Not enough samples available, wait a bit
                    thread::sleep(Duration::from_millis(5));
                }
            }
            
            // Sleep to maintain target update rate
            let elapsed = loop_start.elapsed();
            if elapsed < target_interval {
                thread::sleep(target_interval - elapsed);
            }
        }
    }
}

/// Spawn FFT processing thread
pub fn spawn_fft_thread(
    sample_source: RingConsumer,
    num_bands: usize,
    sample_rate: u32,
) -> (std::thread::JoinHandle<()>, SharedSpectrum) {
    let (processor, spectrum_buffer) = FftProcessor::new(sample_source, num_bands, sample_rate);
    
    let handle = std::thread::spawn(move || {
        processor.run();
    });
    
    (handle, spectrum_buffer)
}
