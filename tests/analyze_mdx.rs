/// MDX 二进制结构分析工具
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

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

fn main() {
    let _ = env_logger::try_init();
    let path = Path::new("/home/tiger/Downloads/test-dict/21世纪大英汉词典.mdx");
    println!("=== MDX File Analysis: {} ===\n", path.display());

    let mut file = File::open(path).expect("Cannot open");
    let file_len = file.seek(SeekFrom::End(0)).unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    println!("File size: {} bytes ({:.2} MB)", file_len, file_len as f64 / 1_048_576.0);

    // 1. Header
    let mut sz = [0u8; 4];
    file.read_exact(&mut sz).unwrap();
    let hdr_sz = u32::from_be_bytes(sz) as usize;
    println!("\n--- Header ---");
    println!("Header size: {} bytes (raw u32 BE: {:02x?})", hdr_sz, sz);

    let mut hdr = vec![0u8; hdr_sz];
    file.read_exact(&mut hdr).unwrap();

    // Adler32
    let mut ab = [0u8; 4];
    file.read_exact(&mut ab).unwrap();
    println!("Header Adler32: {:02x?}", ab);

    let hdr_text = if hdr.len() >= 2 && hdr[hdr.len() - 2..] == [0, 0] {
        utf16le_to_string(&hdr[..hdr.len() - 2])
    } else {
        utf16le_to_string(&hdr)
    };
    println!("Header XML:\n{}\n", hdr_text);

    let version: f32 = xml_attr(&hdr_text, "GeneratedByEngineVersion")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let encrypted = xml_attr(&hdr_text, "Encrypted").unwrap_or_default();
    let description = xml_attr(&hdr_text, "Description").unwrap_or_default();
    let encoding = xml_attr(&hdr_text, "Encoding").unwrap_or_default();
    let key_case = xml_attr(&hdr_text, "KeyCaseSensitive").unwrap_or_default();
    let strip_key = xml_attr(&hdr_text, "StripKey").unwrap_or_default();
    let compact = xml_attr(&hdr_text, "Compact").unwrap_or_default();

    println!("Version:     {}", version);
    println!("Encrypted:   '{}'", encrypted);
    println!("Description: '{}'", description);
    println!("Encoding:    '{}'", encoding);
    println!("KeyCase:     '{}'", key_case);
    println!("StripKey:    '{}'", strip_key);
    println!("Compact:     '{}'", compact);

    let actual_encrypted = encrypted == "Yes" || encrypted == "1";

    // 2. Check if version 2.0+ format
    if version >= 2.0 {
        println!("\n--- Binary Block Header (v2.0) ---");
        let mut block_hdr = [0u8; 40];
        file.read_exact(&mut block_hdr).unwrap();
        let mut pos = 0usize;
        let num_key_blocks = read_u64_be(&block_hdr, &mut pos);
        let num_entries = read_u64_be(&block_hdr, &mut pos);
        let key_info_decomp = read_u64_be(&block_hdr, &mut pos);
        let key_info_comp = read_u64_be(&block_hdr, &mut pos);
        let key_block_total = read_u64_be(&block_hdr, &mut pos);

        println!("num_key_blocks:    {}", num_key_blocks);
        println!("num_entries:       {}", num_entries);
        println!("key_info_decomp:   {} bytes", key_info_decomp);
        println!("key_info_comp:     {} bytes", key_info_comp);
        println!("key_block_total:   {} bytes", key_block_total);

        // Adler32
        let mut ba = [0u8; 4];
        file.read_exact(&mut ba).unwrap();
        println!("Block Adler32:     {:02x?}", ba);

        // 3. Key block info
        println!("\n--- Key Block Info ---");
        let mut ki = vec![0u8; key_info_comp as usize];
        file.read_exact(&mut ki).unwrap();
        println!("Raw size: {} bytes", ki.len());
        println!("First 12 bytes: {:02x?}", &ki[..12.min(ki.len())]);

        if ki.len() >= 8 && &ki[0..4] == b"\x02\x00\x00\x00" {
            println!("Magic: \\x02\\x00\\x00\\x00 ✓");
            // Strip 8 bytes header (4 magic + 4 adler32), then zlib decompress
            match zlib_decompress(&ki[8..]) {
                Ok(ki_data) => {
                    println!("Decompressed size: {} bytes", ki_data.len());
                    let expected_entries = key_info_decomp as usize / 28; // 28 per entry in v2
                    println!("Expected entries (key_info_decomp/28): {}", expected_entries);

                    let num_width: usize = 8; // v2
                    let mut kp = 0usize;
                    let mut block_count = 0;
                    let mut total_cs = 0usize;
                    let mut total_ds = 0usize;
                    let mut block_min_cs = usize::MAX;
                    let mut block_max_cs = 0usize;
                    let mut block_min_ds = usize::MAX;
                    let mut block_max_ds = 0usize;

                    while kp + 28 <= ki_data.len() {
                        kp += num_width; // num_entries (skip)
                        kp += 2; // first_word_len
                        kp += 2; // last_word_len
                        let cs = read_u64_be(&ki_data, &mut kp) as usize;
                        let ds = read_u64_be(&ki_data, &mut kp) as usize;
                        total_cs += cs;
                        total_ds += ds;
                        if cs < block_min_cs {
                            block_min_cs = cs;
                        }
                        if cs > block_max_cs {
                            block_max_cs = cs;
                        }
                        if ds < block_min_ds {
                            block_min_ds = ds;
                        }
                        if ds > block_max_ds {
                            block_max_ds = ds;
                        }
                        block_count += 1;
                    }
                    println!("Parsed blocks: {}", block_count);
                    println!("Total compressed:   {} bytes ({:.2} MB)", total_cs, total_cs as f64 / 1_048_576.0);
                    println!("Total decompressed: {} bytes ({:.2} MB)", total_ds, total_ds as f64 / 1_048_576.0);
                    println!("Block cs range: {} ~ {}", block_min_cs, block_max_cs);
                    println!("Block ds range: {} ~ {}", block_min_ds, block_max_ds);

                    // 4. Sample first key block
                    if block_count > 0 {
                        println!("\n--- First Key Block ---");
                        let block_start = file.seek(SeekFrom::Current(0)).unwrap();
                        println!("Block start offset: {} (0x{:X})", block_start, block_start);

                        // Read first block (use first block's cs)
                        let (first_cs, _first_ds) = {
                            let mut kp2 = 0usize;
                            kp2 += 8; // num_entries
                            kp2 += 2; // first_word_len
                            kp2 += 2; // last_word_len
                            let cs = read_u64_be(&ki_data, &mut kp2) as usize;
                            let ds = read_u64_be(&ki_data, &mut kp2) as usize;
                            (cs, ds)
                        };

                        let mut kd = vec![0u8; first_cs];
                        file.read_exact(&mut kd).unwrap();
                        println!("Compressed size: {} bytes", first_cs);
                        println!("First 16 compressed bytes: {:02x?}", &kd[..16.min(kd.len())]);

                        match raw_deflate_decompress(&kd) {
                            Ok(data) => {
                                println!("Decompressed size: {} bytes", data.len());
                                // Parse first few entries
                                let mut dp = 0usize;
                                let mut entry_count = 0;
                                let max_show = 5;
                                while dp + 17 < data.len() && entry_count < max_show {
                                    let end = data[dp..]
                                        .iter()
                                        .position(|&b| b == 0)
                                        .map(|p| dp + p)
                                        .unwrap_or(data.len());
                                    let word = String::from_utf8_lossy(&data[dp..end]).to_string();
                                    dp = end + 1;
                                    if dp + 16 > data.len() {
                                        break;
                                    }
                                    let _off = read_u64_be(&data, &mut dp);
                                    let _cs2 = read_u32_be(&data, &mut dp);
                                    let _ds2 = read_u32_be(&data, &mut dp);
                                    println!(
                                        "  Entry {}: word='{}', off={}, cs={}, ds={}",
                                        entry_count + 1,
                                        word,
                                        _off,
                                        _cs2,
                                        _ds2
                                    );
                                    entry_count += 1;
                                }
                                // Count total
                                dp = 0usize;
                                let mut total = 0u64;
                                while dp + 17 < data.len() {
                                    let end = data[dp..]
                                        .iter()
                                        .position(|&b| b == 0)
                                        .map(|p| dp + p)
                                        .unwrap_or(data.len());
                                    dp = end + 1;
                                    if dp + 16 > data.len() {
                                        break;
                                    }
                                    dp += 8; // off
                                    dp += 4; // cs
                                    dp += 4; // ds
                                    total += 1;
                                }
                                println!("  Total entries in first block: {}", total);
                            }
                            Err(e) => println!("Failed to decompress first block: {}", e),
                        }
                    }
                }
                Err(e) => println!("Zlib decompress failed: {}", e),
            }
        } else {
            println!("Magic mismatch! First 4 bytes: {:02x?}", &ki[..4.min(ki.len())]);
        }

        // Calculate record start offset
        let record_start = file.seek(SeekFrom::Current(0)).unwrap();
        // Actually we need to skip all key blocks to get record_start...
        // Let's calculate from known data
        println!("\n--- Position Summary ---");
        println!("After reading first key block, at offset: {} (0x{:X})", record_start, record_start);
    }
}

fn zlib_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    use flate2::read::ZlibDecoder;
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
