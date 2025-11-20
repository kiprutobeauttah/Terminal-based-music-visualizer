// Terminal rendering module

use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    style::{Color, ResetColor, SetForegroundColor},
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::{debug, error, info, warn};
use std::io::{self, Stdout, Write};
use std::time::{Duration, Instant};

use crate::fft::SharedSpectrum;

/// Canvas for internal frame buffer representation
#[derive(Debug, Clone)]
pub struct Canvas {
    width: usize,
    height: usize,
    buffer: Vec<Vec<Cell>>,
}

/// A single cell in the canvas
#[derive(Debug, Clone)]
pub struct Cell {
    pub character: char,
    pub color: Color,
}

impl Cell {
    /// Create a new cell with the specified character and color
    pub fn new(character: char, color: Color) -> Self {
        Cell { character, color }
    }
    
    /// Create an empty cell (space with default color)
    pub fn empty() -> Self {
        Cell {
            character: ' ',
            color: Color::Reset,
        }
    }
}

impl Canvas {
    /// Create a new canvas with the specified dimensions
    pub fn new(width: usize, height: usize) -> Self {
        let buffer = vec![vec![Cell::empty(); width]; height];
        Canvas {
            width,
            height,
            buffer,
        }
    }
    
    /// Get the width of the canvas
    pub fn width(&self) -> usize {
        self.width
    }
    
    /// Get the height of the canvas
    pub fn height(&self) -> usize {
        self.height
    }
    
    /// Set a cell at the specified position
    pub fn set_cell(&mut self, x: usize, y: usize, cell: Cell) {
        if x < self.width && y < self.height {
            self.buffer[y][x] = cell;
        }
    }
    
    /// Get a cell at the specified position
    pub fn get_cell(&self, x: usize, y: usize) -> Option<&Cell> {
        if x < self.width && y < self.height {
            Some(&self.buffer[y][x])
        } else {
            None
        }
    }
    
    /// Clear the canvas (fill with empty cells)
    pub fn clear(&mut self) {
        for row in &mut self.buffer {
            for cell in row {
                *cell = Cell::empty();
            }
        }
    }
    
    /// Resize the canvas to new dimensions
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.buffer = vec![vec![Cell::empty(); width]; height];
    }
    
    /// Get a reference to the buffer
    pub fn buffer(&self) -> &Vec<Vec<Cell>> {
        &self.buffer
    }
}

/// Configuration for rendering
#[derive(Debug, Clone)]
pub struct RenderConfig {
    pub sensitivity: f32,
    pub color_scheme: ColorScheme,
    pub show_peaks: bool,
}

impl Default for RenderConfig {
    fn default() -> Self {
        RenderConfig {
            sensitivity: 1.0,
            color_scheme: ColorScheme::default(),
            show_peaks: true,
        }
    }
}

/// Color scheme for visualization
#[derive(Debug, Clone)]
pub struct ColorScheme {
    colors: Vec<Color>,
}

impl ColorScheme {
    /// Create a new color scheme with the specified colors
    pub fn new(colors: Vec<Color>) -> Self {
        ColorScheme { colors }
    }
    
    /// Create a gradient color scheme from a list of colors
    pub fn gradient(colors: Vec<Color>) -> Self {
        if colors.is_empty() {
            Self::default()
        } else {
            ColorScheme { colors }
        }
    }
    
    /// Parse color names from CLI arguments
    /// Supports: red, yellow, green, cyan, blue, magenta, white, black, 
    ///           dark_red, dark_yellow, dark_green, dark_cyan, dark_blue, dark_magenta, grey
    pub fn from_names(color_names: &[String]) -> Result<Self, String> {
        if color_names.is_empty() {
            return Ok(Self::default());
        }
        
        let mut colors = Vec::new();
        
        for name in color_names {
            let color = Self::parse_color_name(name)?;
            colors.push(color);
        }
        
        Ok(ColorScheme { colors })
    }
    
    /// Parse a single color name to a Color
    fn parse_color_name(name: &str) -> Result<Color, String> {
        match name.to_lowercase().as_str() {
            "red" => Ok(Color::Red),
            "yellow" => Ok(Color::Yellow),
            "green" => Ok(Color::Green),
            "cyan" => Ok(Color::Cyan),
            "blue" => Ok(Color::Blue),
            "magenta" => Ok(Color::Magenta),
            "white" => Ok(Color::White),
            "black" => Ok(Color::Black),
            "dark_red" | "darkred" => Ok(Color::DarkRed),
            "dark_yellow" | "darkyellow" => Ok(Color::DarkYellow),
            "dark_green" | "darkgreen" => Ok(Color::DarkGreen),
            "dark_cyan" | "darkcyan" => Ok(Color::DarkCyan),
            "dark_blue" | "darkblue" => Ok(Color::DarkBlue),
            "dark_magenta" | "darkmagenta" => Ok(Color::DarkMagenta),
            "grey" | "gray" => Ok(Color::Grey),
            _ => Err(format!("Unknown color name: {}", name)),
        }
    }
    
    /// Get the color for a specific band index
    /// Interpolates between colors in the gradient
    /// Maps frequency bands to colors:
    /// - Low frequencies (bass): warm colors (red, orange)
    /// - Mid frequencies: neutral colors (yellow, green)
    /// - High frequencies (treble): cool colors (cyan, blue)
    pub fn get_color(&self, band_index: usize, num_bands: usize) -> Color {
        if self.colors.is_empty() {
            return Color::White;
        }
        
        if self.colors.len() == 1 {
            return self.colors[0];
        }
        
        // Calculate position in gradient (0.0 to 1.0)
        let position = if num_bands > 1 {
            band_index as f32 / (num_bands - 1) as f32
        } else {
            0.0
        };
        
        // Find which two colors to interpolate between
        let segment_size = 1.0 / (self.colors.len() - 1) as f32;
        let segment_index = (position / segment_size).floor() as usize;
        let segment_index = segment_index.min(self.colors.len() - 2);
        
        // For now, just return the nearest color (no interpolation for terminal colors)
        // Full RGB interpolation would require TrueColor support
        let local_position = (position - segment_index as f32 * segment_size) / segment_size;
        
        if local_position < 0.5 {
            self.colors[segment_index]
        } else {
            self.colors[segment_index + 1]
        }
    }
    
    /// Get the list of colors in this scheme
    pub fn colors(&self) -> &[Color] {
        &self.colors
    }
}

impl Default for ColorScheme {
    /// Default gradient: Red → Yellow → Green → Cyan → Blue
    /// Maps low frequencies (bass) to warm colors and high frequencies (treble) to cool colors
    fn default() -> Self {
        ColorScheme {
            colors: vec![
                Color::Red,
                Color::Yellow,
                Color::Green,
                Color::Cyan,
                Color::Blue,
            ],
        }
    }
}

/// Terminal renderer that manages terminal state and coordinates visualization rendering
pub struct TerminalRenderer {
    stdout: Stdout,
    canvas: Canvas,
    config: RenderConfig,
    last_size: (u16, u16),
}

impl TerminalRenderer {
    /// Create a new terminal renderer with the specified configuration
    pub fn new(config: RenderConfig) -> io::Result<Self> {
        let mut stdout = io::stdout();
        
        // Enable raw mode
        terminal::enable_raw_mode()?;
        
        // Enter alternate screen
        execute!(stdout, EnterAlternateScreen)?;
        
        // Hide cursor
        execute!(stdout, cursor::Hide)?;
        
        // Get initial terminal size
        let (width, height) = terminal::size()?;
        let canvas = Canvas::new(width as usize, height as usize);
        
        info!("Initialized terminal renderer: {}x{}", width, height);
        
        Ok(TerminalRenderer {
            stdout,
            canvas,
            config,
            last_size: (width, height),
        })
    }
    
    /// Check for terminal resize and update canvas if needed
    pub fn check_resize(&mut self) -> io::Result<bool> {
        let (width, height) = terminal::size()?;
        
        if (width, height) != self.last_size {
            debug!("Terminal resized: {}x{} -> {}x{}", 
                   self.last_size.0, self.last_size.1, width, height);
            
            self.canvas.resize(width as usize, height as usize);
            self.last_size = (width, height);
            
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    /// Get a reference to the canvas
    pub fn canvas(&self) -> &Canvas {
        &self.canvas
    }
    
    /// Get a mutable reference to the canvas
    pub fn canvas_mut(&mut self) -> &mut Canvas {
        &mut self.canvas
    }
    
    /// Get the render configuration
    pub fn config(&self) -> &RenderConfig {
        &self.config
    }
    
    /// Flush the canvas to the terminal display
    pub fn flush(&mut self) -> io::Result<()> {
        // Move cursor to top-left
        execute!(self.stdout, cursor::MoveTo(0, 0))?;
        
        // Render each row
        for (y, row) in self.canvas.buffer().iter().enumerate() {
            // Move to start of row
            execute!(self.stdout, cursor::MoveTo(0, y as u16))?;
            
            let mut current_color = Color::Reset;
            
            for cell in row {
                // Only change color if it's different from current
                if cell.color != current_color {
                    execute!(self.stdout, SetForegroundColor(cell.color))?;
                    current_color = cell.color;
                }
                
                // Print character
                write!(self.stdout, "{}", cell.character)?;
            }
        }
        
        // Reset color
        execute!(self.stdout, ResetColor)?;
        
        // Flush to display
        self.stdout.flush()?;
        
        Ok(())
    }
    
    /// Clear the terminal screen
    pub fn clear(&mut self) -> io::Result<()> {
        execute!(
            self.stdout,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )?;
        self.canvas.clear();
        Ok(())
    }
    
    /// Cleanup and restore terminal state
    pub fn cleanup(&mut self) -> io::Result<()> {
        // Show cursor
        execute!(self.stdout, cursor::Show)?;
        
        // Leave alternate screen
        execute!(self.stdout, LeaveAlternateScreen)?;
        
        // Disable raw mode
        terminal::disable_raw_mode()?;
        
        info!("Terminal renderer cleaned up");
        
        Ok(())
    }
}

impl Drop for TerminalRenderer {
    /// Ensure terminal is restored even on panic
    fn drop(&mut self) {
        if let Err(e) = self.cleanup() {
            error!("Failed to cleanup terminal: {}", e);
        }
    }
}

/// Trait for visualizer modes
pub trait VisualizerMode: Send {
    /// Render the spectrum data to the canvas
    fn render(&self, spectrum: &[f32], canvas: &mut Canvas, config: &RenderConfig);
    
    /// Get the name of this visualizer mode
    fn name(&self) -> &str;
}

/// Main rendering loop that runs at 30-60 FPS
pub struct RenderLoop {
    renderer: TerminalRenderer,
    spectrum_buffer: SharedSpectrum,
    mode: Box<dyn VisualizerMode>,
    target_fps: u32,
}

impl RenderLoop {
    /// Create a new render loop
    pub fn new(
        renderer: TerminalRenderer,
        spectrum_buffer: SharedSpectrum,
        mode: Box<dyn VisualizerMode>,
        target_fps: u32,
    ) -> Self {
        let target_fps = target_fps.clamp(30, 60);
        
        info!("Initialized render loop with {} mode at {} FPS", 
              mode.name(), target_fps);
        
        RenderLoop {
            renderer,
            spectrum_buffer,
            mode,
            target_fps,
        }
    }
    
    /// Run the main rendering loop
    /// Returns when user presses 'q' or Ctrl+C
    pub fn run(&mut self) -> io::Result<()> {
        let frame_duration = Duration::from_millis(1000 / self.target_fps as u64);
        
        info!("Starting render loop");
        
        loop {
            let frame_start = Instant::now();
            
            // Check for resize
            self.renderer.check_resize()?;
            
            // Check for user input (non-blocking)
            if event::poll(Duration::from_millis(0))? {
                if let Event::Key(key_event) = event::read()? {
                    match key_event.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            info!("User requested exit");
                            break;
                        }
                        KeyCode::Char('c') if key_event.modifiers.contains(event::KeyModifiers::CONTROL) => {
                            info!("Ctrl+C pressed");
                            break;
                        }
                        _ => {}
                    }
                }
            }
            
            // Read spectrum data from shared buffer
            let spectrum = match self.spectrum_buffer.lock() {
                Ok(data) => data.bands.clone(),
                Err(e) => {
                    warn!("Failed to lock spectrum buffer: {}", e);
                    vec![0.0; 32] // Fallback to empty spectrum
                }
            };
            
            // Apply sensitivity scaling to spectrum values
            let scaled_spectrum: Vec<f32> = spectrum
                .iter()
                .map(|&val| val * self.renderer.config.sensitivity)
                .collect();
            
            // Clear canvas
            self.renderer.canvas_mut().clear();
            
            // Clone config to avoid borrow checker issues
            let config = self.renderer.config.clone();
            
            // Delegate rendering to active visualizer mode
            self.mode.render(&scaled_spectrum, self.renderer.canvas_mut(), &config);
            
            // Flush canvas to terminal display
            self.renderer.flush()?;
            
            // Sleep to maintain target frame rate
            let elapsed = frame_start.elapsed();
            if elapsed < frame_duration {
                std::thread::sleep(frame_duration - elapsed);
            } else {
                debug!("Frame took longer than target: {:?}", elapsed);
            }
        }
        
        Ok(())
    }
    
    /// Get a mutable reference to the renderer for cleanup
    pub fn renderer_mut(&mut self) -> &mut TerminalRenderer {
        &mut self.renderer
    }
}
