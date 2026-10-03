use std::path::{Path, PathBuf};

use qgs_codec_h264::H264DecoderState;
use qgs_h264_422p10::{H264422P10Decoder, H264422P10Profile};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from("tests/fixtures/h264/professional-422-10bit-long-gop-128x72.h264")
        });
    let bytes = std::fs::read(&input)?;
    let access_units = split_h264_annex_b_access_units(&bytes)?;
    let first = access_units.first().ok_or("no H.264 access units found")?;
    let parsed = qgs_codec_h264::parse_annex_b_access_unit(first)?;
    let profile = H264422P10Profile {
        profile: parsed.profile,
        bit_depth: parsed.desc.bit_depth.get(),
        chroma: parsed.desc.chroma,
        coded_width: parsed.desc.coded_width,
        coded_height: parsed.desc.coded_height,
        visible_width: parsed.desc.visible_region.width,
        visible_height: parsed.desc.visible_region.height,
        scan_mode: parsed.desc.scan_mode,
    };
    let mut decoder = H264422P10Decoder::new(profile.clone());
    let mut report_state = H264DecoderState::new();
    let mut output_frames = 0_usize;
    let mut output_bytes = 0_usize;

    println!("QGS native H.264 4:2:2 10-bit decompressor demo");
    println!("input: {}", display_path(&input));
    println!("access units: {}", access_units.len());
    println!(
        "profile: {:?} {}bit {:?} coded={}x{} visible={}x{}",
        profile.profile,
        profile.bit_depth,
        profile.chroma,
        profile.coded_width,
        profile.coded_height,
        profile.visible_width,
        profile.visible_height
    );
    println!("ffmpeg/rsmpeg/libav: not used");

    for (index, access_unit) in access_units.iter().enumerate() {
        let report = report_state.parse_access_unit(access_unit)?;
        println!(
            "submit AU {index:02}: slices={} kind={:?} first_mb={} cabac_init={} l0_refs={} l1_refs={}",
            report.slices.len(),
            report.slices[0].kind,
            report.slices[0].first_mb_in_slice,
            report.slices[0].cabac_init_idc,
            report.slices[0].ref_pic_list0.len(),
            report.slices[0].ref_pic_list1.len()
        );
        match decoder.submit_access_unit(access_unit) {
            Ok(outputs) => {
                println!("  output frames={}", outputs.len());
                for output in outputs {
                    output.validate_layout()?;
                    output_bytes = output_bytes.saturating_add(output.owned_bytes());
                    output_frames = output_frames.saturating_add(1);
                    println!(
                        "    frame {:02}: y={} cb={} cr={} bytes={}",
                        output.presentation_index,
                        output.y.data.len(),
                        output.cb.data.len(),
                        output.cr.data.len(),
                        output.owned_bytes()
                    );
                }
                let _ = report_state.finish_picture(&report)?;
            }
            Err(error) => {
                println!("  decoder boundary: {error}");
                println!("decoded frames before boundary: {output_frames}");
                return Err(Box::new(error));
            }
        }
    }

    for output in decoder.flush()? {
        output.validate_layout()?;
        output_bytes = output_bytes.saturating_add(output.owned_bytes());
        output_frames = output_frames.saturating_add(1);
        println!(
            "  flush frame {:02}: y={} cb={} cr={} bytes={}",
            output.presentation_index,
            output.y.data.len(),
            output.cb.data.len(),
            output.cr.data.len(),
            output.owned_bytes()
        );
    }

    println!("decoded frames: {output_frames}");
    println!("decoded owned yuv422p10le bytes: {output_bytes}");
    Ok(())
}

fn display_path(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("<input>")
        .to_string()
}

fn split_h264_annex_b_access_units(
    data: &[u8],
) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i..].starts_with(&[0, 0, 1]) {
            starts.push((i, 3));
            i += 3;
        } else if i + 4 <= data.len() && data[i..].starts_with(&[0, 0, 0, 1]) {
            starts.push((i, 4));
            i += 4;
        } else {
            i += 1;
        }
    }
    if starts.is_empty() {
        return Err("input is not Annex B H.264".into());
    }

    let mut access_units = Vec::new();
    let mut current = Vec::new();
    let mut seen_vcl = false;
    for (index, (start, prefix_len)) in starts.iter().copied().enumerate() {
        let nal_start = start + prefix_len;
        let nal_end = starts
            .get(index + 1)
            .map(|(next, _)| *next)
            .unwrap_or(data.len());
        if nal_start >= nal_end {
            return Err("malformed Annex B NAL".into());
        }
        let nal_type = data[nal_start] & 0x1f;
        let is_vcl = nal_type == 1 || nal_type == 5;
        if is_vcl && seen_vcl && !current.is_empty() {
            access_units.push(std::mem::take(&mut current));
        }
        current.extend_from_slice(&data[start..nal_end]);
        if is_vcl {
            seen_vcl = true;
        }
    }
    if !current.is_empty() {
        access_units.push(current);
    }
    Ok(access_units)
}
