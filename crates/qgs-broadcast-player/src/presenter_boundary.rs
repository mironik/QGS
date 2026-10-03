//! Presenter / monitor boundary brick.
//! Displays pixels the video_payload brick already produced (PPM or Wayland blit).
//! Not a Wayland+Vulkan swapchain presenter; does not claim real_display.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::monitor_preview::{MonitorPreview, MonitorPreviewFrame};
use crate::video_payload::EnginePixels;

/// Re-export letterbox blit for acceptance tests without absorbing decode.
pub use crate::monitor_preview::blit_rgb8_letterbox;

/// Monitor sinks only display pixels the engine already produced.
pub enum LiveMonitorSink {
    Files {
        output_dir: PathBuf,
        preview_every: u64,
        output_index: u64,
    },
    Window {
        window: MonitorPreview,
    },
}

impl LiveMonitorSink {
    pub fn open_files(
        output_dir: &Path,
        preview_every: u64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        std::fs::create_dir_all(output_dir)?;
        Ok(Self::Files {
            output_dir: output_dir.to_path_buf(),
            preview_every: preview_every.max(1),
            output_index: 0,
        })
    }

    pub fn open_window() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self::Window {
            window: MonitorPreview::open()?,
        })
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Files { .. } => "diagnostic preview files",
            Self::Window { .. } => "monitor preview window",
        }
    }

    pub fn output_dir(&self) -> Option<&Path> {
        match self {
            Self::Files { output_dir, .. } => Some(output_dir.as_path()),
            Self::Window { .. } => None,
        }
    }

    pub fn preview_every(&self) -> u64 {
        match self {
            Self::Files { preview_every, .. } => *preview_every,
            Self::Window { .. } => 1,
        }
    }

    pub fn is_window_closed(&self) -> bool {
        match self {
            Self::Window { window } => window.is_closed(),
            Self::Files { .. } => false,
        }
    }

    /// Present engine pixels. For file sinks, `make_json(frame_file_name, image_bytes)`
    /// builds the public-safe latest.json body after the PPM size is known.
    pub fn present(
        &mut self,
        pixels: &EnginePixels,
        written_label: &'static str,
        make_json: impl FnOnce(&str, usize) -> String,
    ) -> &'static str {
        match self {
            Self::Files {
                output_dir,
                output_index,
                ..
            } => match write_frame_artifacts(output_dir, *output_index, pixels, make_json) {
                Ok(()) => {
                    *output_index = output_index.saturating_add(1);
                    written_label
                }
                Err(_) => "failed",
            },
            Self::Window { window } => {
                let rgb8 =
                    match rgba_u16_to_rgb8_bytes(&pixels.rgba_u16, pixels.width, pixels.height) {
                        Ok(rgb8) => rgb8,
                        Err(_) => return "failed",
                    };
                let _ = make_json("", 0);
                if window.present(MonitorPreviewFrame {
                    width: pixels.width,
                    height: pixels.height,
                    rgb8,
                }) {
                    "preview-presented"
                } else {
                    "window-closed"
                }
            }
        }
    }
}

fn write_frame_artifacts(
    output_dir: &Path,
    output_index: u64,
    pixels: &EnginePixels,
    make_json: impl FnOnce(&str, usize) -> String,
) -> Result<(), Box<dyn std::error::Error>> {
    let ppm_bytes = rgba_u16_to_ppm_p6_rgb8(&pixels.rgba_u16, pixels.width, pixels.height)?;
    let frame_name = format!("frame_{output_index:06}.ppm");
    File::create(output_dir.join(&frame_name))?.write_all(&ppm_bytes)?;
    File::create(output_dir.join("latest.ppm"))?.write_all(&ppm_bytes)?;
    let latest_json = make_json(&frame_name, ppm_bytes.len());
    File::create(output_dir.join("latest.json"))?.write_all(latest_json.as_bytes())?;
    Ok(())
}

fn rgba_u16_to_rgb8_bytes(
    rgba_u16: &[u16],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let pixel_count = usize::try_from(width)?
        .checked_mul(usize::try_from(height)?)
        .ok_or("readback image dimensions overflow")?;
    let expected_samples = pixel_count
        .checked_mul(4)
        .ok_or("readback RGBA sample count overflow")?;
    if rgba_u16.len() != expected_samples {
        return Err(format!(
            "readback RGBA sample count mismatch: expected {expected_samples}, got {}",
            rgba_u16.len()
        )
        .into());
    }
    let mut rgb = Vec::with_capacity(
        pixel_count
            .checked_mul(3)
            .ok_or("readback RGB byte count overflow")?,
    );
    for pixel in rgba_u16.chunks_exact(4) {
        rgb.push((pixel[0] >> 8) as u8);
        rgb.push((pixel[1] >> 8) as u8);
        rgb.push((pixel[2] >> 8) as u8);
    }
    Ok(rgb)
}

fn rgba_u16_to_ppm_p6_rgb8(
    rgba_u16: &[u16],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let rgb = rgba_u16_to_rgb8_bytes(rgba_u16, width, height)?;
    let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
    ppm.extend_from_slice(&rgb);
    Ok(ppm)
}
