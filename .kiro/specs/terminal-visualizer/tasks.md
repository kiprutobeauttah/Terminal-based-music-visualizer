# Implementation Plan

- [x] 1. Initialize Rust project and configure dependencies
  - Create new Rust project with `cargo init`
  - Add dependencies to Cargo.toml: cpal, rustfft, crossterm, clap, ringbuf, log, env_logger
  - Configure project structure with modules: audio, fft, render, modes, config
  - _Requirements: All requirements depend on proper project setup_

- [x] 2. Implement CLI argument parsing and configuration
  - Create `config.rs` module with CliConfig struct using clap derive macros
  - Implement argument parsing for device, mode, sensitivity, colors, list-modes, and help flags
  - Add validation logic for sensitivity range (0.1-5.0) and color parsing
  - Implement list-modes functionality to display available visualizer modes
  - _Requirements: 8.1, 8.2, 8.3, 8.4, 8.5, 8.6_

- [x] 3. Implement audio capture system
- [x] 3.1 Create AudioProcessor component
  - Create `audio.rs` module with AudioProcessor struct
  - Implement device enumeration using cpal to list available audio devices
  - Implement audio device initialization with specified or default device
  - Configure audio stream with 44.1kHz sample rate and f32 sample format
  - _Requirements: 1.1, 1.2, 1.4, 1.5_

- [x] 3.2 Set up ring buffer for audio samples
  - Initialize lock-free ring buffer with 8192 sample capacity using ringbuf crate
  - Implement audio callback that writes samples to ring buffer
  - Add buffer overrun detection and logging
  - Handle multi-channel to mono conversion in audio callback
  - _Requirements: 1.3_

- [x] 4. Implement FFT processing engine




- [x] 4.1 Create FftEngine component


  - Create `fft.rs` module with FftEngine struct
  - Initialize FFT planner with size 2048 using rustfft crate
  - Implement Hann window function generation
  - Create input and output buffers for FFT processing
  - _Requirements: 2.1, 2.2_

- [x] 4.2 Implement FFT processing loop


  - Read 2048 sample blocks from ring buffer consumer
  - Apply Hann window to samples before FFT
  - Compute FFT and convert complex output to magnitude values in decibels
  - Implement 50% overlap between processing blocks
  - _Requirements: 2.1, 2.2, 2.3_

- [x] 4.3 Implement frequency binning


  - Create FrequencyBinner struct to map FFT bins to logarithmic bands
  - Calculate logarithmic frequency bands from 20 Hz to 20 kHz
  - Implement bin averaging for each frequency band
  - Adapt number of bands based on terminal width (32-64 bands)
  - _Requirements: 2.4_

- [x] 4.4 Set up FFT processing thread


  - Create dedicated thread for FFT processing
  - Implement shared spectrum data buffer with Arc<Mutex<SpectrumData>>
  - Update spectrum data at 30-60 Hz rate
  - Add error handling for corrupted audio data
  - _Requirements: 2.5, 9.3, 10.2_

- [x] 5. Implement spectrum smoothing




- [x] 5.1 Create SpectrumSmoother component


  - Create SpectrumSmoother struct in `fft.rs` module
  - Implement exponential moving average with smoothing factor 0.7
  - Implement peak hold tracking with 0.95 decay rate per frame
  - Initialize smoothing buffers based on number of frequency bands
  - _Requirements: 7.1, 7.2, 7.3, 7.4_

- [x] 6. Implement terminal rendering system




- [x] 6.1 Create TerminalRenderer component


  - Create `render.rs` module with TerminalRenderer struct
  - Initialize crossterm terminal with raw mode enabled
  - Implement terminal size detection and resize handling
  - Create Canvas struct for internal frame buffer representation
  - Implement cleanup method to restore terminal state on exit
  - _Requirements: 3.1, 3.3, 3.5_

- [x] 6.2 Implement rendering loop


  - Create main render loop running at 30-60 FPS
  - Read spectrum data from shared buffer
  - Apply sensitivity scaling to spectrum values
  - Delegate rendering to active visualizer mode
  - Flush canvas to terminal display
  - _Requirements: 3.2, 6.2, 10.4_

- [x] 6.3 Implement ColorScheme component


  - Create ColorScheme struct in `render.rs` module
  - Implement gradient color interpolation
  - Map frequency bands to colors (low=red/orange, mid=yellow/green, high=cyan/blue)
  - Support custom color schemes from CLI arguments
  - _Requirements: 5.1, 5.2, 5.3, 5.4, 5.5_

- [x] 7. Implement visualizer modes





- [x] 7.1 Create VisualizerMode trait


  - Create `modes.rs` module with VisualizerMode trait
  - Define render method interface accepting spectrum data and canvas
  - Add name method for mode identification
  - _Requirements: 4.1, 4.2, 4.3_


- [x] 7.2 Implement SpectrumBarsMode


  - Create SpectrumBarsMode struct implementing VisualizerMode
  - Render vertical bars using Unicode block characters (▁▂▃▄▅▆▇█)
  - Map each frequency band to a vertical bar
  - Apply color gradient from bass (red) to treble (blue)
  - Add optional peak dots above bars
  - _Requirements: 4.1, 4.4, 5.1_



- [x] 7.3 Implement WaveformMode

  - Create WaveformMode struct with history buffer
  - Render horizontal scrolling waveform display
  - Calculate amplitude as RMS of all frequency bands
  - Use line-drawing characters for waveform
  - Implement right-to-left scrolling

  - _Requirements: 4.2, 4.4_


- [x] 7.4 Implement CircularMode

  - Create CircularMode struct implementing VisualizerMode
  - Render radial display with frequency bands as spokes
  - Calculate spoke positions using polar coordinates
  - Apply rotating color gradient around circle
  - Display overall amplitude in center
  - _Requirements: 4.3, 4.4_
- [-] 8. Integrate components in main application


- [ ] 8. Integrate components in main application


- [ ] 8.1 Implement main function logic
  - Initialize env_logger for logging
  - Parse and validate CLI configuration
  - Handle --list-devices and --list-modes flags with early exit
  - Create AudioProcessor with specified or default device
  - Create ring buffer and start audio capture
  - _Requirements: 8.1, 8.2, 8.5, 8.6, 9.1_

- [ ] 8.2 Set up application threads
  - Spawn FFT processing thread with ring buffer consumer
  - Initialize selected visualizer mode based on CLI config
  - Create TerminalRenderer with mode and render config
  - Start main render loop on main thread
  - _Requirements: 10.2, 10.3_

- [ ] 8.3 Implement signal handling and cleanup
  - Register Ctrl+C signal handler using ctrlc crate (add to dependencies)
  - Implement graceful shutdown sequence: stop audio, stop FFT thread, cleanup terminal
  - Register panic hook to ensure terminal restoration
  - Ensure cleanup completes within 1 second
  - _Requirements: 3.5, 9.4_

- [ ] 9. Implement error handling and logging
- [ ] 9.1 Create error types
  - Define VisualizerError enum with variants for different error types
  - Implement Display and Error traits for VisualizerError
  - Add helpful error messages with context
  - _Requirements: 9.1, 9.2_

- [ ] 9.2 Add error handling throughout application
  - Handle audio device errors with device listing
  - Handle terminal initialization errors with requirement messages
  - Add logging at appropriate levels (ERROR, WARN, INFO, DEBUG)
  - Implement frame skipping for corrupted audio data
  - _Requirements: 9.1, 9.2, 9.3_

- [ ] 10. Performance optimization and validation
- [ ] 10.1 Implement double buffering
  - Use crossterm's internal buffer for double buffering
  - Ensure no visual tearing during frame updates
  - _Requirements: 10.3_

- [ ] 10.2 Add performance monitoring
  - Add debug logging for frame timing
  - Log buffer states and overruns
  - Monitor FFT processing time
  - _Requirements: 10.1_

- [ ]* 10.3 Performance testing and profiling
  - Test CPU usage during operation (target < 15%)
  - Verify stable 60 FPS frame rate
  - Test with different audio sources and devices
  - Profile with cargo flamegraph if needed
  - _Requirements: 10.1, 10.4, 10.5_