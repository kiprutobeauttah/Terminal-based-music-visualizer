// Configuration and CLI parsing module

use clap::Parser;

/// Terminal Music Visualizer - Real-time audio visualization in your terminal
#[derive(Parser, Debug)]
#[command(name = "termsonic")]
#[command(author, version, about, long_about = None)]
pub struct CliConfig {
    /// Audio input device name (use --list-devices to see available devices)
    #[arg(short, long)]
    pub device: Option<String>,

    /// Visualizer mode: spectrum, waveform, or circular
    #[arg(short, long, default_value = "spectrum")]
    pub mode: String,

    /// Sensitivity multiplier (0.1 - 5.0)
    #[arg(short, long, default_value = "1.0")]
    pub sensitivity: f32,

    /// Color scheme as comma-separated color names (e.g., red,yellow,green,cyan,blue)
    #[arg(short, long)]
    pub colors: Option<String>,

    /// List available visualizer modes and exit
    #[arg(long)]
    pub list_modes: bool,

    /// List available audio devices and exit
    #[arg(long)]
    pub list_devices: bool,
}

impl CliConfig {
    /// Parse command-line arguments
    pub fn parse_args() -> Self {
        Self::parse()
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        // Validate sensitivity range
        if self.sensitivity < 0.1 || self.sensitivity > 5.0 {
            return Err(format!(
                "Sensitivity must be between 0.1 and 5.0, got: {}",
                self.sensitivity
            ));
        }

        // Validate mode
        let valid_modes = ["spectrum", "waveform", "circular"];
        if !valid_modes.contains(&self.mode.as_str()) {
            return Err(format!(
                "Invalid mode '{}'. Valid modes are: {}",
                self.mode,
                valid_modes.join(", ")
            ));
        }

        // Validate colors if provided
        if let Some(ref colors) = self.colors {
            self.validate_colors(colors)?;
        }

        Ok(())
    }

    /// Validate color string format
    fn validate_colors(&self, colors: &str) -> Result<(), String> {
        let valid_colors = [
            "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
            "dark_grey", "light_red", "light_green", "light_yellow", "light_blue",
            "light_magenta", "light_cyan", "grey",
        ];

        for color in colors.split(',') {
            let color = color.trim().to_lowercase();
            if !valid_colors.contains(&color.as_str()) {
                return Err(format!(
                    "Invalid color '{}'. Valid colors are: {}",
                    color,
                    valid_colors.join(", ")
                ));
            }
        }

        Ok(())
    }

    /// Parse colors from the color string
    pub fn parse_colors(&self) -> Vec<String> {
        if let Some(ref colors) = self.colors {
            colors
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .collect()
        } else {
            // Default color gradient: red -> yellow -> green -> cyan -> blue
            vec![
                "red".to_string(),
                "yellow".to_string(),
                "green".to_string(),
                "cyan".to_string(),
                "blue".to_string(),
            ]
        }
    }

    /// Display available visualizer modes
    pub fn display_modes() {
        println!("Available Visualizer Modes:");
        println!();
        println!("  spectrum   - Vertical bars displaying frequency spectrum");
        println!("               Best for seeing individual frequency ranges");
        println!();
        println!("  waveform   - Horizontal scrolling waveform display");
        println!("               Best for seeing audio amplitude over time");
        println!();
        println!("  circular   - Radial spectrum display in circular pattern");
        println!("               Best for aesthetic circular visualization");
        println!();
        println!("Usage: termsonic --mode <MODE>");
        println!("Example: termsonic --mode spectrum");
    }
}
