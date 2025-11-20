mod audio;
mod config;
mod fft;
mod modes;
mod render;

use audio::{AudioProcessor, create_ring_buffer};
use config::CliConfig;
use fft::{spawn_fft_thread, SpectrumSmoother};
use log::{error, info};
use modes::{CircularMode, SpectrumBarsMode, WaveformMode};
use render::{ColorScheme, RenderConfig, RenderLoop, TerminalRenderer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn main() {
    // Initialize env_logger for logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();
    
    info!("Terminal Music Visualizer starting...");
    
    // Parse and validate CLI configuration
    let config = CliConfig::parse_args();
    
    // Handle --list-devices flag with early exit
    if config.list_devices {
        println!("Available Audio Input Devices:");
        println!();
        let devices = AudioProcessor::list_devices();
        for device in devices {
            println!("{}", device);
        }
        return;
    }
    
    // Handle --list-modes flag with early exit
    if config.list_modes {
        CliConfig::display_modes();
        return;
    }
    
    // Validate configuration
    if let Err(e) = config.validate() {
        error!("Configuration error: {}", e);
        std::process::exit(1);
    }
    
    // Set up Ctrl+C signal handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    
    ctrlc::set_handler(move || {
        info!("Received Ctrl+C signal, shutting down...");
        running_clone.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl+C handler");
    
    // Register panic hook to ensure terminal restoration
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Try to restore terminal
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show
        );
        
        // Call original panic hook
        original_hook(panic_info);
    }));
    
    // Run the application and handle errors
    if let Err(e) = run_application(config, running) {
        error!("Application error: {}", e);
        std::process::exit(1);
    }
    
    info!("Terminal Music Visualizer exiting");
}

/// Main application logic
fn run_application(config: CliConfig, running: Arc<AtomicBool>) -> Result<(), String> {
    // Create AudioProcessor with specified or default device
    let mut audio_processor = AudioProcessor::new(config.device.as_deref())
        .map_err(|e| format!("Failed to create audio processor: {}", e))?;
    
    let sample_rate = audio_processor.sample_rate();
    info!("Audio sample rate: {} Hz", sample_rate);
    
    // Create ring buffer for audio samples
    let (producer, consumer) = create_ring_buffer();
    
    // Start audio capture
    audio_processor.start(producer)
        .map_err(|e| format!("Failed to start audio capture: {}", e))?;
    
    info!("Audio capture started successfully");
    
    // Determine number of frequency bands based on terminal width
    let (term_width, _) = crossterm::terminal::size()
        .map_err(|e| format!("Failed to get terminal size: {}", e))?;
    let num_bands = (term_width as usize).clamp(32, 64);
    
    info!("Using {} frequency bands", num_bands);
    
    // Spawn FFT processing thread with ring buffer consumer
    let (fft_handle, spectrum_buffer) = spawn_fft_thread(consumer, num_bands, sample_rate);
    
    info!("FFT processing thread started");
    
    // Parse color scheme from CLI config
    let color_names = config.parse_colors();
    let color_scheme = ColorScheme::from_names(&color_names)
        .map_err(|e| format!("Failed to parse colors: {}", e))?;
    
    // Create render configuration
    let render_config = RenderConfig {
        sensitivity: config.sensitivity,
        color_scheme,
        show_peaks: true,
    };
    
    // Initialize selected visualizer mode based on CLI config
    let mode: Box<dyn render::VisualizerMode> = match config.mode.as_str() {
        "spectrum" => Box::new(SpectrumBarsMode::new()),
        "waveform" => Box::new(WaveformMode::new()),
        "circular" => Box::new(CircularMode::new()),
        _ => {
            return Err(format!("Unknown visualizer mode: {}", config.mode));
        }
    };
    
    info!("Initialized {} visualizer mode", mode.name());
    
    // Create TerminalRenderer with mode and render config
    let renderer = TerminalRenderer::new(render_config)
        .map_err(|e| format!("Failed to create terminal renderer: {}", e))?;
    
    // Create render loop
    let mut render_loop = RenderLoop::new(renderer, spectrum_buffer, mode, 60);
    
    // Start main render loop on main thread
    info!("Starting render loop");
    
    // Run render loop (will exit on 'q' or Ctrl+C)
    let render_result = render_loop.run();
    
    // Cleanup sequence
    info!("Initiating cleanup sequence");
    
    // Stop audio capture
    audio_processor.stop();
    info!("Audio capture stopped");
    
    // Note: FFT thread will be terminated when the process exits
    // In a production app, we'd send a signal to gracefully stop it
    drop(fft_handle);
    info!("FFT thread handle dropped");
    
    // Cleanup terminal (handled by Drop trait, but we'll call it explicitly)
    if let Err(e) = render_loop.renderer_mut().cleanup() {
        error!("Failed to cleanup terminal: {}", e);
    }
    
    // Check render result
    render_result.map_err(|e| format!("Render loop error: {}", e))?;
    
    info!("Cleanup completed successfully");
    
    Ok(())
}
