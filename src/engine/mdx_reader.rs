use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;

use flate2::read::ZlibDecoder;

use crate::engine::types::{ArticleData, SearchResult};

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
    index: Mutex<Option<HashMap<String, (usize, u32, u32)>>>,
    record_blocks: Vec<RecBlock>,
    file: Mutex<File>,
    all_entries: Vec<(String, u64)>, // for lazy index building
    rec_decomp_sizes: Vec<u64>,
}

impl MdxReader {
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut file = File::open(path).map_err(|e| format!("Cannot open: {}", e))?;
        file.seek(SeekFrom::End(0))
            .map_err(|e| format!("Seek: {}", e))?;
        file.seek(SeekFrom::Start(0))
            .map_err(|e| format!("Seek: {}", e))?;

        // ========== 1. Header ==========
        let mut sz = [0u8; 4];
        file.read_exact(&mut sz)
            .map_err(|e| format!("Read hdr size: {}", e))?;
        let hdr_sz = u32::from_be_bytes(sz) as usize;
        let mut hdr = vec![0u8; hdr_sz];
        file.read_exact(&mut hdr)
            .map_err(|e| format!("Read hdr: {}", e))?;
        file.read_exact(&mut [0u8; 4])
            .map_err(|e| format!("Read adler: {}", e))?;

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
        let enc_flag: u32 = encrypted.parse().unwrap_or(0);

        let name = xml_attr(&hdr_text, "Title")
            .filter(|t| t != "Title (No HTML code allowed)")
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown")
                    .to_string()
            });

        // ========== 2. Block Header (v2) ==========
        let block_hdr_start = file
            .seek(SeekFrom::Current(0))
            .map_err(|e| format!("Seek: {}", e))?;
        let mut block_hdr = [0u8; 40];
        file.read_exact(&mut block_hdr)
            .map_err(|e| format!("Read block hdr: {}", e))?;
        let mut pos = 0usize;
        let _num_key_blocks = read_u64_be(&block_hdr, &mut pos);
        let _num_entries = read_u64_be(&block_hdr, &mut pos);
        let _key_info_decomp = read_u64_be(&block_hdr, &mut pos) as usize;
        let key_info_comp = read_u64_be(&block_hdr, &mut pos) as usize;
        let _key_block_total = read_u64_be(&block_hdr, &mut pos);
        file.read_exact(&mut [0u8; 4])
            .map_err(|e| format!("Read block adler: {}", e))?;

        // ========== 3. Key Block Info ==========
        // Try standard format first; if ki validates but decompress fails, retry variant
        let mut ki_data = None;
        let mut ki = vec![0u8; key_info_comp.min(100_000_000)];
        if key_info_comp > 0
            && key_info_comp < 100_000_000
            && file.read_exact(&mut ki).is_ok()
            && ki.len() >= 8
        {
            if enc_flag & 2 != 0 {
                decrypt_headword_index(&mut ki);
            }
            ki_data = decompress_packed_block(&ki[..], _key_info_decomp);
        }

        if ki_data.is_none() {
            // Retry with variant block header (32-byte)
            file.seek(SeekFrom::Start(block_hdr_start))
                .map_err(|e| format!("Seek retry: {}", e))?;
            file.read_exact(&mut block_hdr)
                .map_err(|e| format!("Read block hdr retry: {}", e))?;
            let mut pos = 0usize;
            let _ = read_u32_be(&block_hdr, &mut pos);
            let _ = read_u32_be(&block_hdr, &mut pos);
            let _ = read_u64_be(&block_hdr, &mut pos);
            let key_info_comp = read_u64_be(&block_hdr, &mut pos) as usize;
            file.seek(SeekFrom::Start(block_hdr_start + 32))
                .map_err(|e| format!("Seek retry2: {}", e))?;
            file.read_exact(&mut [0u8; 4])
                .map_err(|e| format!("Read block adler retry: {}", e))?;
            ki = vec![0u8; key_info_comp];
            file.read_exact(&mut ki)
                .map_err(|e| format!("Read key info retry: {}", e))?;
            // Variant might have 8-byte prefix before ki header
            if &ki[0..4] == b"\x02\x00\x00\x00" {
                ki_data = decompress_packed_block(&ki, _key_info_decomp);
            } else if ki.len() >= 16 && &ki[8..12] == b"\x02\x00\x00\x00" {
                // 8-byte prefix before the block
                ki_data = decompress_packed_block(&ki[8..], _key_info_decomp);
            } else {
                return Err("Bad key info header".into());
            }
        }
        let ki_data = ki_data.ok_or_else(|| "Key info decompress failed".to_string())?;
        //        + lwl(u16) + last_word + \0 + comp_size(u64) + decomp_size(u64)
        let mut key_blocks_info: Vec<(usize, usize)> = Vec::new();
        let mut kp = 0usize;
        while kp + 12 <= ki_data.len() {
            let _ne = read_u64_be(&ki_data, &mut kp);
            let fwl = read_u16_be(&ki_data, &mut kp) as usize;
            if kp + fwl + 1 > ki_data.len() {
                break;
            }
            kp += fwl + 1;
            let lwl = read_u16_be(&ki_data, &mut kp) as usize;
            if kp + lwl + 1 > ki_data.len() {
                break;
            }
            kp += lwl + 1;
            if kp + 16 > ki_data.len() {
                break;
            }
            let cs = read_u64_be(&ki_data, &mut kp) as usize;
            let ds = read_u64_be(&ki_data, &mut kp) as usize;
            key_blocks_info.push((cs, ds));
        }

        // ========== 4. Key Blocks: offset(u64) + word\0 ==========
        let mut all_entries: Vec<(String, u64)> = Vec::new();

        for (cs, ds) in &key_blocks_info {
            if *cs > 100_000_000 {
                continue;
            }
            let mut kd = vec![0u8; *cs];
            file.read_exact(&mut kd)
                .map_err(|e| format!("Read key block: {}", e))?;

            // Use type-based decompressor for key blocks
            let decompressed = decompress_packed_block(&kd, *ds);

            if let Some(data) = decompressed {
                let mut dp = 0usize;
                while dp + 9 <= data.len() {
                    let off = read_u64_be(&data, &mut dp);
                    if dp >= data.len() {
                        break;
                    }
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
        let record_start = file
            .seek(SeekFrom::Current(0))
            .map_err(|e| format!("Tell: {}", e))?;

        let mut rec_hdr = [0u8; 40];
        file.read_exact(&mut rec_hdr)
            .map_err(|e| format!("Read rec hdr: {}", e))?;
        let mut rp = 0usize;
        let num_rec_blocks = read_u64_be(&rec_hdr, &mut rp) as usize;
        let _rec_num_entries = read_u64_be(&rec_hdr, &mut rp);
        let rec_info_size = read_u64_be(&rec_hdr, &mut rp) as usize;
        let _rec_info_comp = read_u64_be(&rec_hdr, &mut rp);
        let block0_gap = read_u64_be(&rec_hdr, &mut rp);

        log::info!(
            "MDX: {} record blocks, idx={} bytes",
            num_rec_blocks,
            rec_info_size
        );

        // Read record index: N × 16 bytes, each entry = (decomp_size: u64, next_gap: u64)
        let mut rec_idx = vec![0u8; rec_info_size];
        file.read_exact(&mut rec_idx)
            .map_err(|e| format!("Read rec idx: {}", e))?;

        let mut gaps = vec![block0_gap];
        let mut rec_decomp_sizes: Vec<u64> = Vec::with_capacity(num_rec_blocks);
        for i in 0..num_rec_blocks {
            let entry_start = i * 16;
            if entry_start + 16 > rec_idx.len() {
                break;
            }
            let mut ep = entry_start;
            let decomp_size = read_u64_be(&rec_idx, &mut ep);
            let next_gap = read_u64_be(&rec_idx, &mut ep);
            rec_decomp_sizes.push(decomp_size);
            gaps.push(next_gap);
        }

        // Build record block offsets
        let base = record_start + 40 + rec_info_size as u64;
        let mut rec_blocks: Vec<RecBlock> = Vec::with_capacity(num_rec_blocks);
        let mut current_offset = base;

        for i in 0..num_rec_blocks {
            let gap = gaps[i] as usize;
            let comp_size = gap;
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
            rec_decomp_sizes,
        })
    }

    /// Build word→(block, offset, end_offset) index (call from background thread)
    pub fn build_index(&self) {
        let mut index: HashMap<String, (usize, u32, u32)> = HashMap::new();
        let cum_shadow: Vec<u64> = {
            let mut v = Vec::with_capacity(self.rec_decomp_sizes.len() + 1);
            v.push(0);
            let mut acc: u64 = 0;
            for &ds in &self.rec_decomp_sizes {
                acc = acc.saturating_add(ds);
                v.push(acc);
            }
            v
        };
        let total_decomp: u64 = cum_shadow.last().copied().unwrap_or(0);
        for (entry_idx, (word, offset)) in self.all_entries.iter().enumerate() {
            let bi = match cum_shadow.binary_search(offset) {
                Ok(i) => i,
                Err(0) => continue,
                Err(i) => i - 1,
            };
            if bi >= self.record_blocks.len() {
                continue;
            }
            let offset_in_block = (*offset - cum_shadow[bi]) as u32;
            let end_offset = if entry_idx + 1 < self.all_entries.len() {
                let next_off = self.all_entries[entry_idx + 1].1;
                if next_off <= *offset {
                    (cum_shadow[bi + 1].min(cum_shadow[bi] + self.rec_decomp_sizes[bi])
                        - cum_shadow[bi]) as u32
                } else {
                    (next_off - cum_shadow[bi]) as u32
                }
            } else {
                (total_decomp - cum_shadow[bi]) as u32
            };
            index
                .entry(word.clone())
                .or_insert((bi, offset_in_block, end_offset));
        }
        log::info!(
            "MDX: indexed {} / {} words",
            index.len(),
            self.all_entries.len()
        );
        *self.index.lock().unwrap() = Some(index);
    }

    pub fn index_size(&self) -> usize {
        self.index
            .lock()
            .unwrap()
            .as_ref()
            .map(|m| m.len())
            .unwrap_or(0)
    }

    pub fn sample_words(&self, n: usize) -> Vec<&str> {
        self.all_entries
            .iter()
            .take(n)
            .map(|(w, _)| w.as_str())
            .collect()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn word_count(&self) -> usize {
        self.word_count
    }

    /// Prefix search across all entries (case-insensitive) — linear scan
    pub fn lookup_prefix(&self, prefix: &str, limit: usize) -> Vec<SearchResult> {
        let prefix_lower = prefix.to_lowercase();
        let dict_name = format!("{} (MDX)", self.name);
        self.all_entries
            .iter()
            .filter(|(w, _)| {
                w.len() >= prefix.len()
                    && w.get(..prefix.len())
                        .map(|s| s.to_lowercase() == prefix_lower)
                        .unwrap_or(false)
                    && w.to_lowercase() != prefix_lower
            })
            .take(limit)
            .map(|(w, _)| SearchResult {
                dict_name: dict_name.clone(),
                word: w.clone(),
                score: 0.7,
            })
            .collect()
    }

    /// Fuzzy search across all entries (Levenshtein) — linear scan
    pub fn lookup_fuzzy(
        &self,
        word: &str,
        threshold: usize,
        limit: usize,
    ) -> Vec<SearchResult> {
        use crate::engine::fuzzy_matcher::{levenshtein_distance, similarity};

        let dict_name = format!("{} (MDX)", self.name);
        let word_lower = word.to_lowercase();
        let mut results: Vec<(usize, f64)> = Vec::new();

        for (i, (entry_word, _)) in self.all_entries.iter().enumerate() {
            if entry_word.len() > word.len() + threshold + 3 {
                continue;
            }
            let dist =
                levenshtein_distance(&word_lower, &entry_word.to_lowercase(), threshold);
            if dist != usize::MAX {
                let sim = similarity(&word_lower, &entry_word.to_lowercase());
                results.push((i, sim));
            }
        }

        results
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);

        results
            .into_iter()
            .map(|(idx, score)| SearchResult {
                dict_name: dict_name.clone(),
                word: self.all_entries[idx].0.clone(),
                score: score as f32 * 0.5,
            })
            .collect()
    }

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
                for (key, &(bi, off, end_off)) in map.iter() {
                    if key.to_lowercase() == wl {
                        return self.read_article(bi, off, end_off);
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
        let &(bi, off, end_off) = map.get(word)?;
        self.read_article(bi, off, end_off)
    }

    fn read_article(
        &self,
        block_index: usize,
        offset: u32,
        end_offset: u32,
    ) -> Option<ArticleData> {
        let rb = self.record_blocks.get(block_index)?;

        // Try cache
        {
            let cache = rb.cached.lock().ok()?;
            if let Some(ref data) = *cache {
                return extract_article(data, offset, end_offset, &self.name);
            }
        }

        // Read & decompress
        let mut file = self.file.lock().ok()?;
        file.seek(SeekFrom::Start(rb.file_offset)).ok()?;
        let mut comp = vec![0u8; rb.comp_size];
        file.read_exact(&mut comp).ok()?;
        drop(file);

        let decomp_size = self.rec_decomp_sizes.get(block_index).copied().unwrap_or(0) as usize;
        let data = decompress_packed_block(&comp, decomp_size)
            .or_else(|| zlib_decompress(&comp).ok())
            .or_else(|| raw_deflate_decompress(&comp).ok())?;

        let result = extract_article(&data, offset, end_offset, &self.name);

        if let Ok(mut cache) = rb.cached.lock() {
            *cache = Some(data);
        }

        result
    }
}

/// Extract article text from decompressed block at given offset range
fn extract_article(
    data: &[u8],
    offset: u32,
    end_offset: u32,
    dict_name: &str,
) -> Option<ArticleData> {
    let start = offset as usize;
    let end = (end_offset as usize).min(data.len());
    if start >= data.len() || end <= start {
        return None;
    }
    let raw = &data[start..end];
    // Strip null bytes (crash Pango/GLib) and \\r (breaks ClutterText markup)
    let filtered: Vec<u8> = raw
        .iter()
        .filter(|&&b| b != 0 && b != b'\r')
        .copied()
        .collect();
    let text = String::from_utf8_lossy(&filtered).to_string();
    let is_html = true;
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

fn read_u32_be(data: &[u8], pos: &mut usize) -> u32 {
    let v = u32::from_be_bytes(data[*pos..*pos + 4].try_into().unwrap());
    *pos += 4;
    v
}

fn read_u16_be(data: &[u8], pos: &mut usize) -> u16 {
    let v = u16::from_be_bytes(data[*pos..*pos + 2].try_into().unwrap());
    *pos += 2;
    v
}

/// Goldendict-ng compatible block decompressor
fn decompress_packed_block(data: &[u8], decomp_size: usize) -> Option<Vec<u8>> {
    if data.len() < 8 {
        return None;
    }
    let ctype = u32::from_be_bytes(data[0..4].try_into().ok()?);
    let _checksum = u32::from_be_bytes(data[4..8].try_into().ok()?);
    let buf = &data[8..];
    match ctype {
        0x00000000 => Some(buf.to_vec()),
        0x01000000 => lzo_decompress(buf, decomp_size).ok(),
        0x02000000 => zlib_decompress(buf)
            .ok()
            .or_else(|| raw_deflate_decompress(buf).ok()),
        _ => None,
    }
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
    d.read_to_end(&mut r)
        .map_err(|e| format!("deflate: {}", e))?;
    Ok(r)
}

fn lzo_decompress(data: &[u8], decomp_size: usize) -> Result<Vec<u8>, String> {
    let lzo = minilzo_rs::LZO::init().map_err(|_| "lzo: init failed")?;
    lzo.decompress_safe(data, decomp_size.max(data.len() * 4).min(100_000_000))
        .map_err(|_| "lzo: decompress failed".into())
}

fn decrypt_headword_index(data: &mut [u8]) {
    use ripemd::Digest;
    use ripemd::Ripemd128;
    if data.len() < 8 {
        return;
    }
    let mut hasher = Ripemd128::new();
    hasher.update(&data[4..8]);
    hasher.update(&[0x95, 0x36, 0x00, 0x00]);
    let key = hasher.finalize();
    let mut prev: u8 = 0x36;
    for i in 0..(data.len() - 8) {
        let byte = data[8 + i];
        let mut b = (byte >> 4) | (byte << 4);
        b ^= prev ^ (i as u8) ^ key[i % 16];
        prev = byte;
        data[8 + i] = b;
    }
}

fn utf16le_to_string(data: &[u8]) -> String {
    let mut r = String::with_capacity(data.len() / 2);
    for c in data.chunks_exact(2) {
        let code = u16::from_le_bytes([c[0], c[1]]);
        if let Some(ch) = char::from_u32(code as u32) {
            r.push(ch);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_oxford_lookup_good() {
        let _ = env_logger::try_init();
        let path = Path::new("/home/tiger/Documents/test-dict/[英-汉] 【2012.7.1】牛津高阶学习词典英汉双解第7版【OALD 8风格重新排版】.mdx");
        if !path.exists() {
            eprintln!("SKIP: file not found");
            return;
        }
        let reader = MdxReader::open(path).expect("open");
        assert!(reader.word_count() > 0);
        reader.build_index();
        assert!(reader.index_size() > 0);
        assert!(reader.lookup_exact("good").is_some());
    }
}
