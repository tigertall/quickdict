use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;

use flate2::read::ZlibDecoder;

use crate::engine::types::ArticleData;

/// Record block with lazy decompression cache
struct RecBlock {
    file_offset: u64,
    comp_size: usize,
    cached: Mutex<Option<Vec<u8>>>,
}

/// MDX 词典阅读器（Mdict v2.0 格式）
pub struct MdxReader {
    name: String,
    word_count: usize,
    /// word → (record_block_index, byte_offset_in_decompressed_block)
    index: Mutex<Option<HashMap<String, (usize, u32)>>>,
    record_blocks: Vec<RecBlock>,
    file: Mutex<File>,
    all_entries: Vec<(String, u64)>,  // for lazy index building
}

impl MdxReader {
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut file = File::open(path).map_err(|e| format!("Cannot open: {}", e))?;
        file.seek(SeekFrom::End(0)).map_err(|e| format!("Seek: {}", e))?;
        file.seek(SeekFrom::Start(0)).map_err(|e| format!("Seek: {}", e))?;

        // ========== 1. Header ==========
        let mut sz = [0u8; 4];
        file.read_exact(&mut sz).map_err(|e| format!("Read hdr size: {}", e))?;
        let hdr_sz = u32::from_be_bytes(sz) as usize;
        let mut hdr = vec![0u8; hdr_sz];
        file.read_exact(&mut hdr).map_err(|e| format!("Read hdr: {}", e))?;
        file.read_exact(&mut [0u8; 4]).map_err(|e| format!("Read adler: {}", e))?;

        let hdr_text = if hdr.len() >= 2 && hdr[hdr.len() - 2..] == [0, 0] {
            utf16le_to_string(&hdr[..hdr.len() - 2])
        } else {
            utf16le_to_string(&hdr)
        };

        let version: f32 = xml_attr(&hdr_text, "GeneratedByEngineVersion")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.0);
        if version < 2.0 {
            return Err("Only MDX v2.0+ supported".into());
        }

        let encrypted = xml_attr(&hdr_text, "Encrypted").unwrap_or_default();
        if encrypted == "Yes" || encrypted == "1" {
            if encrypted != "2" {
                return Err("Encrypted MDX not supported".into());
            }
        }

        let name = xml_attr(&hdr_text, "Title")
            .or_else(|| xml_attr(&hdr_text, "Description").map(|d| strip_html_tags(&d)))
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown")
                    .to_string()
            });

        // ========== 2. Block Header (v2) ==========
        let mut block_hdr = [0u8; 40];
        file.read_exact(&mut block_hdr).map_err(|e| format!("Read block hdr: {}", e))?;
        let mut pos = 0usize;
        let _num_key_blocks = read_u64_be(&block_hdr, &mut pos);
        let _num_entries = read_u64_be(&block_hdr, &mut pos);
        let _key_info_decomp = read_u64_be(&block_hdr, &mut pos);
        let key_info_comp = read_u64_be(&block_hdr, &mut pos) as usize;
        let _key_block_total = read_u64_be(&block_hdr, &mut pos);
        file.read_exact(&mut [0u8; 4]).map_err(|e| format!("Read block adler: {}", e))?;

        // ========== 3. Key Block Info (variable-length entries) ==========
        let mut ki = vec![0u8; key_info_comp];
        file.read_exact(&mut ki).map_err(|e| format!("Read key info: {}", e))?;

        if ki.len() < 8 || &ki[0..4] != b"\x02\x00\x00\x00" {
            return Err("Bad key info header".into());
        }
        let ki_data = zlib_decompress(&ki[8..])?;

        // Parse: num_entries(u64) + fwl(u16) + first_word + \0
        //        + lwl(u16) + last_word + \0 + comp_size(u64) + decomp_size(u64)
        let mut key_blocks_info: Vec<(usize, usize)> = Vec::new();
        let mut kp = 0usize;
        while kp + 12 <= ki_data.len() {
            let _ne = read_u64_be(&ki_data, &mut kp);
            let fwl = read_u16_be(&ki_data, &mut kp) as usize;
            if kp + fwl + 1 > ki_data.len() { break; }
            kp += fwl + 1;
            let lwl = read_u16_be(&ki_data, &mut kp) as usize;
            if kp + lwl + 1 > ki_data.len() { break; }
            kp += lwl + 1;
            if kp + 16 > ki_data.len() { break; }
            let cs = read_u64_be(&ki_data, &mut kp) as usize;
            let ds = read_u64_be(&ki_data, &mut kp) as usize;
            key_blocks_info.push((cs, ds));
        }

        // ========== 4. Key Blocks: offset(u64) + word\0 ==========
        let mut all_entries: Vec<(String, u64)> = Vec::new();

        for (cs, _ds) in &key_blocks_info {
            if *cs > 100_000_000 { continue; }
            let mut kd = vec![0u8; *cs];
            file.read_exact(&mut kd).map_err(|e| format!("Read key block: {}", e))?;

            // Key blocks: \x02\x00\x00\x00 + adler32(4) + zlib(data)
            let decompressed = if kd.len() >= 8 && &kd[0..4] == b"\x02\x00\x00\x00" {
                zlib_decompress(&kd[8..]).ok()
            } else {
                None
            };
            let decompressed = decompressed.or_else(|| {
                if kd.len() >= 8 && &kd[0..4] == b"\x02\x00\x00\x00" {
                    raw_deflate_decompress(&kd[8..]).ok()
                } else {
                    raw_deflate_decompress(&kd).ok()
                }
            });

            if let Some(data) = decompressed {
                let mut dp = 0usize;
                while dp + 9 <= data.len() {
                    let off = read_u64_be(&data, &mut dp);
                    if dp >= data.len() { break; }
                    let end = data[dp..]
                        .iter()
                        .position(|&b| b == 0)
                        .map(|p| dp + p)
                        .unwrap_or(data.len());
                    let word = String::from_utf8_lossy(&data[dp..end]).to_string();
                    dp = end + 1;
                    all_entries.push((word, off));
                }
            }
        }

        log::info!("MDX: {} key entries", all_entries.len());
        let word_count = all_entries.len();

        // ========== 5. Record Section ==========
        // Record header: 40 bytes (5 × u64 BE), NO Adler32
        let record_start = file.seek(SeekFrom::Current(0))
            .map_err(|e| format!("Tell: {}", e))?;

        let mut rec_hdr = [0u8; 40];
        file.read_exact(&mut rec_hdr).map_err(|e| format!("Read rec hdr: {}", e))?;
        let mut rp = 0usize;
        let num_rec_blocks = read_u64_be(&rec_hdr, &mut rp) as usize;
        let _rec_num_entries = read_u64_be(&rec_hdr, &mut rp);
        let rec_info_size = read_u64_be(&rec_hdr, &mut rp) as usize;
        let _rec_info_comp = read_u64_be(&rec_hdr, &mut rp);
        let block0_gap = read_u64_be(&rec_hdr, &mut rp);

        log::info!("MDX: {} record blocks, idx={} bytes", num_rec_blocks, rec_info_size);

        // Read record index: N × 16 bytes, each entry = (decomp_size: u64, next_gap: u64)
        let mut rec_idx = vec![0u8; rec_info_size];
        file.read_exact(&mut rec_idx).map_err(|e| format!("Read rec idx: {}", e))?;

        let mut gaps = vec![block0_gap];
        for i in 0..num_rec_blocks {
            let entry_start = i * 16;
            if entry_start + 16 > rec_idx.len() { break; }
            let mut ep = entry_start;
            let _ds = read_u64_be(&rec_idx, &mut ep);
            let next_gap = read_u64_be(&rec_idx, &mut ep);
            gaps.push(next_gap);
        }

        // Build record block offsets
        let base = record_start + 40 + rec_info_size as u64;
        let mut rec_blocks: Vec<RecBlock> = Vec::with_capacity(num_rec_blocks);
        let mut current_offset = base;

        for i in 0..num_rec_blocks {
            let gap = gaps[i] as usize;
            let comp_size = if gap >= 8 { gap - 8 } else { gap };
            rec_blocks.push(RecBlock {
                file_offset: current_offset,
                comp_size,
                cached: Mutex::new(None),
            });
            current_offset += gap as u64;
        }

        log::info!("MDX: built {} record block offsets", rec_blocks.len());

        Ok(Self {
            name,
            word_count,
            index: Mutex::new(None),
            record_blocks: rec_blocks,
            file: Mutex::new(file),
            all_entries,
        })
    }

    /// Build word→(block, offset) index (call from background thread)
    pub fn build_index(&self) {
        let mut index: HashMap<String, (usize, u32)> = HashMap::new();
        let mut article_idx: u64 = 0;
        let mut file = self.file.lock().unwrap();

        for (bi, rb) in self.record_blocks.iter().enumerate() {
            if rb.comp_size == 0 { continue; }

            file.seek(SeekFrom::Start(rb.file_offset)).ok();
            let mut comp = vec![0u8; rb.comp_size];
            file.read_exact(&mut comp).ok();

            let decompressed = {
                let mut d = ZlibDecoder::new(&comp[..]);
                let mut r = Vec::new();
                d.read_to_end(&mut r).ok().map(|_| r)
            };

            if let Some(data) = decompressed {
                let bounds = find_article_boundaries(&data);
                for (start, _end) in &bounds {
                    if let Some((ref word, _)) = self.all_entries.get(article_idx as usize) {
                        index.entry(word.clone()).or_insert((bi, *start));
                    }
                    article_idx += 1;
                }
            }
        }
        log::info!("MDX: indexed {} / {} words", index.len(), self.all_entries.len());
        *self.index.lock().unwrap() = Some(index);
    }

    pub fn name(&self) -> &str { &self.name }

    pub fn word_count(&self) -> usize { self.word_count }

    pub fn lookup_exact(&self, word: &str) -> Option<ArticleData> {
        if let Some(result) = self.lookup_inner(word) {
            return Some(result);
        }
        let wl = word.to_lowercase();
        if wl != word {
            if let Some(result) = self.lookup_inner(&wl) {
                return Some(result);
            }
        }
        // Linear case-insensitive fallback
        if let Some(idx) = self.index.lock().ok() {
            if let Some(ref map) = *idx {
                for (key, &(bi, off)) in map {
                    if key.to_lowercase() == wl {
                        return self.read_article(bi, off);
                    }
                }
            }
        }
        // Abbreviation fallback: try query with trailing period (e.g. "test" → "test.")
        let with_dot = format!("{}.", word);
        if let Some(result) = self.lookup_inner(&with_dot) {
            return Some(result);
        }
        let wl_dot = format!("{}.", wl);
        if wl_dot != with_dot {
            if let Some(result) = self.lookup_inner(&wl_dot) {
                return Some(result);
            }
        }
        None
    }

    fn lookup_inner(&self, word: &str) -> Option<ArticleData> {
        let idx = self.index.lock().ok()?;
        let map = idx.as_ref()?;
        let &(bi, off) = map.get(word)?;
        self.read_article(bi, off)
    }

    fn read_article(&self, block_index: usize, offset: u32) -> Option<ArticleData> {
        let rb = self.record_blocks.get(block_index)?;

        // Try cache
        {
            let cache = rb.cached.lock().ok()?;
            if let Some(ref data) = *cache {
                return extract_article(data, offset, &self.name);
            }
        }

        // Read & decompress
        let mut file = self.file.lock().ok()?;
        file.seek(SeekFrom::Start(rb.file_offset)).ok()?;
        let mut comp = vec![0u8; rb.comp_size];
        file.read_exact(&mut comp).ok()?;
        drop(file);

        let data = zlib_decompress(&comp)
            .or_else(|_| raw_deflate_decompress(&comp))
            .ok()?;

        let result = extract_article(&data, offset, &self.name);

        if let Ok(mut cache) = rb.cached.lock() {
            *cache = Some(data);
        }

        result
    }
}

/// Find article boundaries in decompressed HTML.
/// Each article starts with: <link rel="stylesheet" ... sf_ecce.css"/>
fn find_article_boundaries(data: &[u8]) -> Vec<(u32, u32)> {
    let mut boundaries = Vec::new();
    if data.is_empty() { return boundaries; }

    let link_prefix = b"<link ";
    let css_suffix = b"sf_ecce.css";

    let mut i = 0;
    while i + 6 < data.len() {
        if &data[i..i+6] == link_prefix {
            let end = data[i..].iter().position(|&b| b == b'>').map(|p| i + p).unwrap_or(data.len());
            if data[i..end].windows(css_suffix.len()).any(|w| w == css_suffix) {
                if let Some(last) = boundaries.last_mut() {
                    last.1 = i as u32;
                }
                boundaries.push((i as u32, data.len() as u32));
            }
        }
        i += 1;
    }

    boundaries
}

/// Extract article text from decompressed block at given offset
fn extract_article(data: &[u8], offset: u32, dict_name: &str) -> Option<ArticleData> {
    let start = offset as usize;
    if start >= data.len() { return None; }

    // Find end: next <link ... sf_ecce.css or end of data
    let mut end = data.len();
    let link_prefix = b"<link ";
    let css_suffix = b"sf_ecce.css";

    let mut i = start + 1;
    while i + 6 < data.len() {
        if &data[i..i+6] == link_prefix {
            let link_end = data[i..].iter().position(|&b| b == b'>').map(|p| i + p).unwrap_or(data.len());
            if data[i..link_end].windows(css_suffix.len()).any(|w| w == css_suffix) {
                end = i;
                break;
            }
        }
        i += 1;
    }

    let raw = &data[start..end];
    // Strip interior null bytes which would crash Pango/GLib,
    // then decode as UTF-8 (preserving multi-byte sequences)
    let filtered: Vec<u8> = raw.iter().filter(|&&b| b != 0).copied().collect();
    let text = String::from_utf8_lossy(&filtered).to_string();
    let is_html = !text.is_empty();

    Some(ArticleData {
        dict_name: format!("{} (MDX)", dict_name),
        raw_text: text,
        is_html,
    })
}

// ========== Helper functions ==========

fn read_u64_be(data: &[u8], pos: &mut usize) -> u64 {
    let v = u64::from_be_bytes(data[*pos..*pos + 8].try_into().unwrap());
    *pos += 8;
    v
}

fn read_u16_be(data: &[u8], pos: &mut usize) -> u16 {
    let v = u16::from_be_bytes(data[*pos..*pos + 2].try_into().unwrap());
    *pos += 2;
    v
}

fn zlib_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut d = ZlibDecoder::new(data);
    let mut r = Vec::new();
    d.read_to_end(&mut r).map_err(|e| format!("zlib: {}", e))?;
    Ok(r)
}

fn raw_deflate_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    use flate2::bufread::DeflateDecoder;
    let mut d = DeflateDecoder::new(data);
    let mut r = Vec::new();
    d.read_to_end(&mut r).map_err(|e| format!("deflate: {}", e))?;
    Ok(r)
}

fn utf16le_to_string(data: &[u8]) -> String {
    let mut r = String::with_capacity(data.len() / 2);
    for c in data.chunks_exact(2) {
        let code = u16::from_le_bytes([c[0], c[1]]);
        if let Some(ch) = char::from_u32(code as u32) { r.push(ch); }
    }
    r
}

fn xml_attr(xml: &str, name: &str) -> Option<String> {
    let s = format!("{}=\"", name);
    if let Some(p) = xml.find(&s) {
        let start = p + s.len();
        if let Some(end) = xml[start..].find('"') {
            return Some(xml[start..start + end].to_string());
        }
    }
    None
}

/// Strip HTML tags and decode entities from an HTML string
fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for c in html.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(c);
        }
    }

    // Decode common HTML entities
    result = result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    // Collapse whitespace
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}
