// SPDX-License-Identifier: MIT
//! Bounded synthesized sound effects through CPAL's OS output stream.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use tomo_scene::{Problem, SoundCue};

enum Message {
    Tone(SoundCue),
    Clear,
}
struct Voice {
    frequency: f32,
    volume: f32,
    frame: u32,
    frames: u32,
}
fn sample(voice: &mut Voice, rate: f32) -> f32 {
    let time = voice.frame as f32 / rate;
    let remaining = (voice.frames - voice.frame) as f32 / rate;
    let envelope = (time / 0.005).min(1.) * (remaining / 0.02).min(1.);
    let value =
        (time * voice.frequency * std::f32::consts::TAU).sin() * voice.volume * envelope * 0.2;
    voice.frame += 1;
    value
}
pub struct AudioPlayer {
    _stream: cpal::Stream,
    sender: SyncSender<Message>,
    frames: Arc<AtomicU64>,
    failed: Arc<AtomicBool>,
}
fn stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    receiver: Receiver<Message>,
    frames: Arc<AtomicU64>,
    failed: Arc<AtomicBool>,
) -> Result<cpal::Stream, Problem>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;
    let rate = config.sample_rate.0 as f32;
    let mut voices: Vec<Voice> = Vec::with_capacity(32);
    device
        .build_output_stream(
            config,
            move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
                for _ in 0..64 {
                    match receiver.try_recv() {
                        Ok(Message::Clear) => voices.clear(),
                        Ok(Message::Tone(cue)) => {
                            if voices.len() < 32 {
                                voices.push(Voice {
                                    frequency: cue.frequency,
                                    volume: cue.volume,
                                    frame: 0,
                                    frames: (cue.seconds * rate).ceil() as u32,
                                });
                            }
                        }
                        Err(_) => break,
                    }
                }
                for frame in output.chunks_mut(channels) {
                    let value = voices
                        .iter_mut()
                        .map(|v| sample(v, rate))
                        .sum::<f32>()
                        .clamp(-0.8, 0.8);
                    for channel in frame {
                        *channel = T::from_sample(value);
                    }
                    voices.retain(|v| v.frame < v.frames);
                }
                frames.fetch_add((output.len() / channels) as u64, Ordering::Relaxed);
            },
            move |_| {
                failed.store(true, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|e| Problem::new("AUDIO", "/device", e.to_string()))
}
impl AudioPlayer {
    pub fn new() -> Result<Self, Problem> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| {
                Problem::new(
                    "AUDIO_UNAVAILABLE",
                    "/device",
                    "no default audio output device",
                )
            })?;
        let supported = device
            .default_output_config()
            .map_err(|e| Problem::new("AUDIO", "/device", e.to_string()))?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        if config.channels == 0
            || config.channels > 32
            || !(8000..=384000).contains(&config.sample_rate.0)
        {
            return Err(Problem::new(
                "AUDIO",
                "/device",
                "unsupported output channel count or sample rate",
            ));
        }
        let (sender, receiver) = mpsc::sync_channel(64);
        let frames = Arc::new(AtomicU64::new(0));
        let failed = Arc::new(AtomicBool::new(false));
        let stream = match format {
            cpal::SampleFormat::F32 => {
                stream::<f32>(&device, &config, receiver, frames.clone(), failed.clone())
            }
            cpal::SampleFormat::I16 => {
                stream::<i16>(&device, &config, receiver, frames.clone(), failed.clone())
            }
            cpal::SampleFormat::U16 => {
                stream::<u16>(&device, &config, receiver, frames.clone(), failed.clone())
            }
            _ => Err(Problem::new(
                "AUDIO",
                "/device",
                format!("unsupported output sample format {format:?}"),
            )),
        }?;
        stream
            .play()
            .map_err(|e| Problem::new("AUDIO", "/device", e.to_string()))?;
        Ok(Self {
            _stream: stream,
            sender,
            frames,
            failed,
        })
    }
    pub fn play(&self, cue: &SoundCue) -> Result<(), Problem> {
        if self.failed.load(Ordering::Relaxed) {
            return Err(Problem::new("AUDIO", "/device", "audio output failed"));
        }
        if !cue.frequency.is_finite()
            || !(20. ..=12000.).contains(&cue.frequency)
            || !cue.seconds.is_finite()
            || !(0.01..=5.).contains(&cue.seconds)
            || !cue.volume.is_finite()
            || !(0. ..=1.).contains(&cue.volume)
        {
            return Err(Problem::new("AUDIO_INPUT", "/cue", "invalid tone"));
        }
        self.sender
            .try_send(Message::Tone(cue.clone()))
            .map_err(|e| Problem::new("AUDIO_LIMIT", "/queue", e.to_string()))
    }
    pub fn clear(&self) {
        let _ = self.sender.try_send(Message::Clear);
    }
    pub fn output_frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn envelope_is_finite_bounded_and_reaches_silence() {
        let mut voice = Voice {
            frequency: 440.,
            volume: 1.,
            frame: 0,
            frames: 4800,
        };
        let values: Vec<_> = (0..4800).map(|_| sample(&mut voice, 48000.)).collect();
        assert_eq!(values[0], 0.);
        assert!(values.iter().all(|v| v.is_finite() && v.abs() <= 0.2));
        assert!(values.iter().any(|v| v.abs() > 0.1));
        assert!(values.last().unwrap().abs() < 0.001);
    }
    #[test]
    #[ignore = "requires an actual audio output device"]
    fn real_output_stream_processes_frames() {
        let player = AudioPlayer::new().unwrap();
        player
            .play(&SoundCue {
                id: "test".into(),
                trigger: tomo_scene::CueTrigger::Start {},
                frequency: 440.,
                seconds: 0.05,
                volume: 0.02,
            })
            .unwrap();
        let start = std::time::Instant::now();
        while player.output_frames() < 256 && start.elapsed() < std::time::Duration::from_secs(2) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(player.output_frames() >= 256);
        assert!(!player.failed.load(Ordering::Relaxed));
    }
}
