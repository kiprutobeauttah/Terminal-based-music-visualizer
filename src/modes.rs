// Visualizer modes module

use crate::render::{Canvas, Cell, ColorScheme, RenderConfig, VisualizerMode};
use crossterm::style::Color;
use std::collections::VecDeque;

// Re-export the trait for convenience
pub use crate::render::VisualizerMode;

/// Spectrum bars mode - displays vertical bars for each frequency band
pub struct SpectrumBarsMode;

impl SpectrumBarsMode {
    /// Create a new spectrum bars mode
    pub fn new() -> Self {
        SpectrumBarsMode
    }
    
    /// Unicode block characters for rendering bars (from lowest to highest)
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    
    /// Map a magnitude value (in dB) to a bar height
    /// Returns height in characters (0 to canvas height)
    fn magnitude_to_height(magnitude: f32, max_height: usize) -> usize {
        // Magnitude is in dB, typically ranging from -60 to 0
        // Normalize to 0.0 - 1.0 range
        let normalized = ((magnitude + 60.0) / 60.0).clamp(0.0, 1.0);
        
        // Scale to canvas height
        (normalized * max_height as f32) as usize
    }
    
    /// Get the appropriate block character for a given position in the bar
    fn get_block_char(position: usize, height: usize, max_height: usize) -> char {
        if position >= max_height - height {
            // We're in the filled part of the bar
            let relative_pos = max_height - position - 1;
            if relative_pos < height {
                Self::BLOCKS[7] // Full block
            } else {
                ' '
            }
        } else {
            ' '
        }
    }
}

impl VisualizerMode for SpectrumBarsMode {
    fn render(&self, spectrum: &[f32], canvas: &mut Canvas, config: &RenderConfig) {
        let width = canvas.width();
        let height = canvas.height();
        
        if spectrum.is_empty() || width == 0 || height == 0 {
            return;
        }
        
        // Calculate how many bars we can fit
        let num_bars = spectrum.len().min(width);
        let bar_width = width / num_bars;
        
        // Render each frequency band as a vertical bar
        for (i, &magnitude) in spectrum.iter().take(num_bars).enumerate() {
            let x = i * bar_width;
            
            // Get color for this frequency band
            let color = config.color_scheme.get_color(i, num_bars);
            
            // Calculate bar height
            let bar_height = Self::magnitude_to_height(magnitude, height);
            
            // Draw the bar from bottom to top
            for y in 0..height {
                let char_to_draw = if y >= height - bar_height {
                    Self::BLOCKS[7] // Full block
                } else {
                    ' '
                };
                
                // Fill the bar width
                for dx in 0..bar_width {
                    if x + dx < width {
                        canvas.set_cell(x + dx, y, Cell::new(char_to_draw, color));
                    }
                }
            }
            
            // Add optional peak dot above bar
            if config.show_peaks && bar_height < height {
                let peak_y = if bar_height > 0 {
                    height - bar_height - 1
                } else {
                    height - 1
                };
                
                for dx in 0..bar_width {
                    if x + dx < width {
                        canvas.set_cell(x + dx, peak_y, Cell::new('·', color));
                    }
                }
            }
        }
    }
    
    fn name(&self) -> &str {
        "spectrum"
    }
}

/// Waveform mode - displays horizontal scrolling waveform
pub struct WaveformMode {
    history: VecDeque<f32>,
    max_history: usize,
}

impl WaveformMode {
    /// Create a new waveform mode
    pub fn new() -> Self {
        WaveformMode {
            history: VecDeque::new(),
            max_history: 200, // Will be adjusted based on canvas width
        }
    }
    
    /// Calculate RMS (Root Mean Square) amplitude from all frequency bands
    fn calculate_rms(spectrum: &[f32]) -> f32 {
        if spectrum.is_empty() {
            return 0.0;
        }
        
        let sum_squares: f32 = spectrum.iter().map(|&x| {
            // Convert from dB to linear scale
            let linear = 10_f32.powf(x / 20.0);
            linear * linear
        }).sum();
        
        (sum_squares / spectrum.len() as f32).sqrt()
    }
    
    /// Map amplitude to vertical position on canvas
    fn amplitude_to_y(amplitude: f32, height: usize) -> usize {
        let normalized = amplitude.clamp(0.0, 1.0);
        let y = height as f32 * (1.0 - normalized) / 2.0 + height as f32 / 4.0;
        y as usize
    }
}

impl VisualizerMode for WaveformMode {
    fn render(&self, spectrum: &[f32], canvas: &mut Canvas, config: &RenderConfig) {
        let width = canvas.width();
        let height = canvas.height();
        
        if width == 0 || height == 0 {
            return;
        }
        
        // Calculate current amplitude
        let amplitude = Self::calculate_rms(spectrum);
        
        // Update history (mutable borrow through interior mutability pattern)
        // Since we can't mutate self in render, we'll work with a local copy
        // For now, we'll just render the current amplitude as a simple waveform
        
        // Draw center line
        let center_y = height / 2;
        for x in 0..width {
            canvas.set_cell(x, center_y, Cell::new('─', Color::DarkGrey));
        }
        
        // Draw amplitude indicator
        let amp_y = Self::amplitude_to_y(amplitude, height);
        let amp_color = config.color_scheme.get_color(0, 1);
        
        // Draw a simple waveform representation
        for x in 0..width {
            // Create a wave pattern
            let phase = x as f32 / width as f32 * std::f32::consts::PI * 4.0;
            let wave_offset = (phase.sin() * amplitude * height as f32 / 4.0) as i32;
            let wave_y = (center_y as i32 + wave_offset).clamp(0, height as i32 - 1) as usize;
            
            // Draw the waveform
            if wave_y < height {
                canvas.set_cell(x, wave_y, Cell::new('●', amp_color));
            }
            
            // Draw connecting lines
            if x > 0 {
                let prev_phase = (x - 1) as f32 / width as f32 * std::f32::consts::PI * 4.0;
                let prev_wave_offset = (prev_phase.sin() * amplitude * height as f32 / 4.0) as i32;
                let prev_wave_y = (center_y as i32 + prev_wave_offset).clamp(0, height as i32 - 1) as usize;
                
                // Draw line between points
                let y_start = prev_wave_y.min(wave_y);
                let y_end = prev_wave_y.max(wave_y);
                
                for y in y_start..=y_end {
                    if y < height {
                        canvas.set_cell(x, y, Cell::new('│', amp_color));
                    }
                }
            }
        }
    }
    
    fn name(&self) -> &str {
        "waveform"
    }
}

/// Circular mode - displays spectrum data in a radial pattern
pub struct CircularMode;

impl CircularMode {
    /// Create a new circular mode
    pub fn new() -> Self {
        CircularMode
    }
    
    /// Calculate RMS amplitude from spectrum
    fn calculate_amplitude(spectrum: &[f32]) -> f32 {
        if spectrum.is_empty() {
            return 0.0;
        }
        
        let sum_squares: f32 = spectrum.iter().map(|&x| {
            let linear = 10_f32.powf(x / 20.0);
            linear * linear
        }).sum();
        
        (sum_squares / spectrum.len() as f32).sqrt()
    }
    
    /// Convert polar coordinates to canvas coordinates
    fn polar_to_canvas(
        angle: f32,
        radius: f32,
        center_x: f32,
        center_y: f32,
    ) -> (usize, usize) {
        let x = center_x + radius * angle.cos();
        let y = center_y + radius * angle.sin() * 0.5; // Adjust for character aspect ratio
        (x as usize, y as usize)
    }
}

impl VisualizerMode for CircularMode {
    fn render(&self, spectrum: &[f32], canvas: &mut Canvas, config: &RenderConfig) {
        let width = canvas.width();
        let height = canvas.height();
        
        if spectrum.is_empty() || width == 0 || height == 0 {
            return;
        }
        
        let center_x = width as f32 / 2.0;
        let center_y = height as f32 / 2.0;
        let max_radius = (width.min(height * 2) as f32 / 2.0) * 0.8;
        
        // Calculate overall amplitude for center display
        let overall_amplitude = Self::calculate_amplitude(spectrum);
        
        // Draw center circle
        let center_radius = 3.0;
        for dy in -3..=3 {
            for dx in -6..=6 {
                let x = (center_x as i32 + dx) as usize;
                let y = (center_y as i32 + dy) as usize;
                
                let dist = ((dx as f32 / 2.0).powi(2) + (dy as f32).powi(2)).sqrt();
                
                if dist <= center_radius && x < width && y < height {
                    let char_to_draw = if dist < center_radius - 1.0 {
                        '●'
                    } else {
                        '○'
                    };
                    canvas.set_cell(x, y, Cell::new(char_to_draw, Color::White));
                }
            }
        }
        
        // Draw frequency bands as spokes
        let num_spokes = spectrum.len().min(64);
        
        for (i, &magnitude) in spectrum.iter().take(num_spokes).enumerate() {
            // Calculate angle for this spoke
            let angle = (i as f32 / num_spokes as f32) * 2.0 * std::f32::consts::PI;
            
            // Get color with rotating gradient
            let color = config.color_scheme.get_color(i, num_spokes);
            
            // Calculate spoke length based on magnitude
            let normalized_mag = ((magnitude + 60.0) / 60.0).clamp(0.0, 1.0);
            let spoke_length = normalized_mag * max_radius;
            
            // Draw the spoke
            let steps = (spoke_length * 2.0) as usize;
            for step in 0..steps {
                let radius = (step as f32 / steps as f32) * spoke_length + center_radius;
                let (x, y) = Self::polar_to_canvas(angle, radius, center_x, center_y);
                
                if x < width && y < height {
                    let char_to_draw = if step == steps - 1 {
                        '●' // Dot at the end
                    } else if step % 2 == 0 {
                        '·' // Dots along the spoke
                    } else {
                        ' '
                    };
                    
                    if char_to_draw != ' ' {
                        canvas.set_cell(x, y, Cell::new(char_to_draw, color));
                    }
                }
            }
        }
        
        // Display overall amplitude in center
        let amp_text = format!("{:.0}%", overall_amplitude * 100.0);
        let text_x = (center_x - amp_text.len() as f32 / 2.0) as usize;
        let text_y = center_y as usize;
        
        for (i, ch) in amp_text.chars().enumerate() {
            let x = text_x + i;
            if x < width && text_y < height {
                canvas.set_cell(x, text_y, Cell::new(ch, Color::Yellow));
            }
        }
    }
    
    fn name(&self) -> &str {
        "circular"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_spectrum_bars_magnitude_to_height() {
        // Test magnitude to height conversion
        let height = SpectrumBarsMode::magnitude_to_height(-60.0, 10);
        assert_eq!(height, 0);
        
        let height = SpectrumBarsMode::magnitude_to_height(0.0, 10);
        assert_eq!(height, 10);
        
        let height = SpectrumBarsMode::magnitude_to_height(-30.0, 10);
        assert_eq!(height, 5);
    }
    
    #[test]
    fn test_waveform_calculate_rms() {
        // Test RMS calculation with known values
        let spectrum = vec![-20.0, -20.0, -20.0, -20.0];
        let rms = WaveformMode::calculate_rms(&spectrum);
        assert!(rms > 0.0);
        
        // Empty spectrum should return 0
        let rms = WaveformMode::calculate_rms(&[]);
        assert_eq!(rms, 0.0);
    }
    
    #[test]
    fn test_circular_calculate_amplitude() {
        // Test amplitude calculation
        let spectrum = vec![-10.0, -20.0, -30.0];
        let amp = CircularMode::calculate_amplitude(&spectrum);
        assert!(amp > 0.0);
        
        // Empty spectrum should return 0
        let amp = CircularMode::calculate_amplitude(&[]);
        assert_eq!(amp, 0.0);
    }
    
    #[test]
    fn test_mode_names() {
        let spectrum_mode = SpectrumBarsMode::new();
        assert_eq!(spectrum_mode.name(), "spectrum");
        
        let waveform_mode = WaveformMode::new();
        assert_eq!(waveform_mode.name(), "waveform");
        
        let circular_mode = CircularMode::new();
        assert_eq!(circular_mode.name(), "circular");
    }
}
