// Audio capture and processing module

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, Stream, StreamConfig, SupportedStreamConfig};
use log::{debug, error, info, warn};
use ringbuf::{traits::*, HeapRb};
use std::sync::{Arc, Mutex};

/// Default ring buffer capacity (8192 samples = ~185ms at 44.1kHz)
pub const RING_BUFFER_CAPACITY: usize = 8192;

/// Type alias for the ring buffer producer (thread-safe)
pub type RingProducer = Arc<Mutex<ringbuf::HeapProd<f32>>>;

/// Type alias for the ring buffer consumer
pub type RingConsumer = ringbuf::HeapCons<f32>;

/// Create a new ring buffer for audio samples
pub fn create_ring_buffer() -> (RingProducer, RingConsumer) {
    let ring_buffer = HeapRb::<f32>::new(RING_BUFFER_CAPACITY);
    let (producer, consumer) = ring_buffer.split();
    
    info!("Created ring buffer with capacity: {} samples (~{:.1}ms at 44.1kHz)", 
          RING_BUFFER_CAPACITY, 
          (RING_BUFFER_CAPACITY as f32 / 44100.0) * 1000.0);
    
    (Arc::new(Mutex::new(producer)), consumer)
}

/// Audio processor that captures audio from system devices
pub struct AudioProcessor {
    device: Device,
    config: StreamConfig,
    stream: Option<Stream>,
    sample_producer: Option<RingProducer>,
}

impl AudioProcessor {
    /// Create a new AudioProcessor with the specified device name or default device
    pub fn new(device_name: Option<&str>) -> Result<Self, String> {
        let host = cpal::default_host();
        
        // Get the audio device
        let device = if let Some(name) = device_name {
            Self::find_device_by_name(&host, name)?
        } else {
            host.default_input_device()
                .ok_or_else(|| "No default input device available".to_string())?
        };

        info!("Using audio device: {}", device.name().unwrap_or_else(|_| "Unknown".to_string()));

        // Get the default input config
        let supported_config = device
            .default_input_config()
            .map_err(|e| format!("Failed to get default input config: {}", e))?;

        debug!("Supported config: {:?}", supported_config);

        // Create stream config with 44.1kHz sample rate and f32 format
        let config = Self::create_stream_config(supported_config)?;

        info!("Stream config: sample_rate={}, channels={}", 
              config.sample_rate.0, config.channels);

        Ok(AudioProcessor {
            device,
            config,
            stream: None,
            sample_producer: None,
        })
    }

    /// Find a device by name
    fn find_device_by_name(host: &Host, name: &str) -> Result<Device, String> {
        let devices = host.input_devices()
            .map_err(|e| format!("Failed to enumerate input devices: {}", e))?;

        for device in devices {
            if let Ok(device_name) = device.name() {
                if device_name.to_lowercase().contains(&name.to_lowercase()) {
                    return Ok(device);
                }
            }
        }

        // Device not found, list available devices
        let available = Self::list_devices_internal(host);
        Err(format!(
            "Device '{}' not found. Available devices:\n{}",
            name,
            available.join("\n")
        ))
    }

    /// Create a stream config with 44.1kHz sample rate and f32 format
    fn create_stream_config(supported: SupportedStreamConfig) -> Result<StreamConfig, String> {
        let desired_sample_rate = cpal::SampleRate(44100);
        let actual_sample_rate = supported.sample_rate();
        
        // Use 44.1kHz if close to the device's sample rate, otherwise use device default
        let sample_rate = if (actual_sample_rate.0 as i32 - 44100).abs() < 1000 {
            desired_sample_rate
        } else {
            warn!("Device doesn't support 44.1kHz, using default: {}", actual_sample_rate.0);
            actual_sample_rate
        };

        let config = StreamConfig {
            channels: supported.channels(),
            sample_rate,
            buffer_size: cpal::BufferSize::Default,
        };

        Ok(config)
    }

    /// List all available audio input devices
    pub fn list_devices() -> Vec<String> {
        let host = cpal::default_host();
        Self::list_devices_internal(&host)
    }

    /// Internal helper to list devices
    fn list_devices_internal(host: &Host) -> Vec<String> {
        let mut devices = Vec::new();

        if let Ok(input_devices) = host.input_devices() {
            for (i, device) in input_devices.enumerate() {
                if let Ok(name) = device.name() {
                    devices.push(format!("  {}. {}", i + 1, name));
                }
            }
        }

        if devices.is_empty() {
            devices.push("  No input devices found".to_string());
        }

        devices
    }

    /// Start capturing audio with the provided ring buffer producer
    pub fn start(&mut self, producer: RingProducer) -> Result<(), String> {
        self.sample_producer = Some(producer);

        let channels = self.config.channels as usize;
        let producer_clone = self.sample_producer.as_ref().unwrap().clone();

        // Create the input stream
        let stream = self.device
            .build_input_stream(
                &self.config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    Self::audio_callback(data, &producer_clone, channels);
                },
                |err| {
                    error!("Audio stream error: {}", err);
                },
                None,
            )
            .map_err(|e| format!("Failed to build input stream: {}", e))?;

        // Start the stream
        stream.play()
            .map_err(|e| format!("Failed to start audio stream: {}", e))?;

        self.stream = Some(stream);
        info!("Audio capture started");

        Ok(())
    }

    /// Audio callback that writes samples to the ring buffer
    fn audio_callback(data: &[f32], producer: &RingProducer, channels: usize) {
        // Lock the producer to write samples
        let mut producer = match producer.lock() {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to lock ring buffer producer: {}", e);
                return;
            }
        };

        // Convert multi-channel audio to mono by averaging channels
        if channels == 1 {
            // Mono audio - write directly
            let written = producer.push_slice(data);
            if written < data.len() {
                // Buffer overrun - some samples were dropped
                warn!("Ring buffer overrun: dropped {} samples", data.len() - written);
            }
        } else {
            // Multi-channel audio - convert to mono
            let mono_samples: Vec<f32> = data
                .chunks_exact(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                .collect();

            let written = producer.push_slice(&mono_samples);
            if written < mono_samples.len() {
                warn!("Ring buffer overrun: dropped {} samples", mono_samples.len() - written);
            }
        }
    }

    /// Stop capturing audio
    pub fn stop(&mut self) {
        if let Some(stream) = self.stream.take() {
            drop(stream);
            info!("Audio capture stopped");
        }
        self.sample_producer = None;
    }

    /// Get the sample rate
    pub fn sample_rate(&self) -> u32 {
        self.config.sample_rate.0
    }
}

impl Drop for AudioProcessor {
    fn drop(&mut self) {
        self.stop();
    }
}
