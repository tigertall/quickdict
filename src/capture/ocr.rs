/// OCR for hover-to-translate.
/// The GNOME Shell extension captures a screen region around the cursor
/// (via Shell.Screenshot), sends it as base64 PNG over D-Bus;
/// we run tesseract on it and find the word under the cursor offset
/// (relative to the region's top-left corner).
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;

/// OCR a captured region and return the word under the cursor offset.
/// cursor_x/cursor_y are in logical coordinates; cap_w/cap_h are the
/// logical size of the captured region. The PNG may be scaled up by the
/// display scale factor (e.g. 200% HiDPI), so we convert the cursor offset
/// to the PNG's physical pixel coordinates before box matching.
pub fn ocr_word_at(image_base64: &str, cursor_x: i32, cursor_y: i32, cap_w: i32, cap_h: i32) -> Option<String> {
    let png = decode_base64(image_base64)?;
    let (png_w, png_h) = png_size(&png)?;
    let scale_x = png_w as f64 / cap_w.max(1) as f64;
    let scale_y = png_h as f64 / cap_h.max(1) as f64;
    let phys_x = (cursor_x as f64 * scale_x).round() as i32;
    let phys_y = (cursor_y as f64 * scale_y).round() as i32;
    let tmp_path = write_temp_png(&png)?;
    let tsv = run_tesseract(&tmp_path)?;
    let _ = std::fs::remove_file(&tmp_path);
    find_word_at(&tsv, phys_x, phys_y)
}

/// Parse PNG IHDR to get the pixel dimensions (no extra dependency).
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    // signature(8) | IHDR len(4) | "IHDR"(4) | width(4) | height(4)
    if png.len() < 24 || &png[0..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let h = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    Some((w, h))
}

fn decode_base64(b64: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

fn write_temp_png(png: &[u8]) -> Option<std::path::PathBuf> {
    let dir = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir.join(format!("quickdict-ocr-{}-{}.png", std::process::id(), nanos));
    let mut f = std::fs::File::create(&path).ok()?;
    f.write_all(png).ok()?;
    Some(path)
}

/// Run tesseract with TSV output (word boxes) on the PNG file.
fn run_tesseract(png_path: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new("tesseract")
        .arg(png_path)
        .arg("stdout")
        .arg("--psm")
        .arg("6") // uniform block of text, best for small line captures
        .arg("-l")
        .arg("eng+chi_sim")
        .arg("tsv")
        .output()
        .ok()?;
    if !output.status.success() {
        log::warn!("[ocr] tesseract failed: {}", String::from_utf8_lossy(&output.stderr));
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// Parse tesseract TSV and return the word whose box contains (x, y).
fn find_word_at(tsv: &str, x: i32, y: i32) -> Option<String> {
    let mut best: Option<(i32, String)> = None;
    for line in tsv.lines().skip(1) {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 12 || fields[0] != "5" {
            continue; // level 5 = word
        }
        let (left, top, width, height) = match (
            fields[6].trim().parse::<i32>(),
            fields[7].trim().parse::<i32>(),
            fields[8].trim().parse::<i32>(),
            fields[9].trim().parse::<i32>(),
        ) {
            (Ok(l), Ok(t), Ok(w), Ok(h)) => (l, t, w, h),
            _ => continue,
        };
        if x >= left && x <= left + width && y >= top && y <= top + height {
            let text = fields[11].trim();
            if text.is_empty() {
                continue;
            }
            // Prefer the closest word center if multiple boxes overlap
            let cx = left + width / 2;
            let cy = top + height / 2;
            let dist = (cx - x).abs() + (cy - y).abs();
            if best.as_ref().map_or(true, |(d, _)| dist < *d) {
                best = Some((dist, text.to_string()));
            }
        }
    }
    best.map(|(_, w)| w)
}
