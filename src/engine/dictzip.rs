use std::path::Path;

/// 将整个 .dict.dz 完全解压到内存
pub fn decompress_dictdz(path: &Path) -> Result<Vec<u8>, String> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    let file = std::fs::File::open(path)
        .map_err(|e| format!("Cannot open: {}", e))?;

    let mut decoder = GzDecoder::new(file);
    let mut buf = Vec::new();
    decoder
        .read_to_end(&mut buf)
        .map_err(|e| format!("Decompress error: {}", e))?;

    Ok(buf)
}
