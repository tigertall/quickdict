use std::io::{Read};
use std::fs;

fn main() {
    // Read the raw .dict data for "bad" from freedict
    // We need to check the StarDict reader output
    let path = "/home/tiger/Downloads/freedict-eng-zho-2025.11.23.stardict/eng-zho/eng-zho.dict.dz";
    // dictzip is compressed, let's just check with a quick cargo script
    println!("Dict is at: {}", path);
}

// Instead, let me write a quick test that loads the freedict and dumps the raw HTML for "bad"

use std::path::Path;
use std::fs::File;
use std::io::BufReader;

fn read_u32_be(data: &[u8], pos: &mut usize) -> u32 {
    let v = u32::from_be_bytes(data[*pos..*pos+4].try_into().unwrap());
    *pos += 4; v
}

fn main() {
    let base = Path::new("/home/tiger/Downloads/freedict-eng-zho-2025.11.23.stardict/eng-zho");
    
    // Read .ifo
    let ifo_path = base.with_extension("ifo");
    let ifo_text = fs::read_to_string(&ifo_path).unwrap();
    let idx_offset_bits: u32 = ifo_text.lines()
        .find(|l| l.starts_with("idxoffsetbits="))
        .and_then(|l| l.split('=').nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(32);
    
    // Read .idx
    let idx_path = base.with_extension("idx");
    let mut file = File::open(&idx_path).unwrap();
    let idx_data = {
        let mut v = Vec::new();
        file.read_to_end(&mut v).unwrap();
        v
    };
    
    // Binary search for "bad"
    let word = b"bad\0";
    let mut low = 0usize;
    let mut high = idx_data.len() / (word.len().max(4) + 4);
    let idx_bytes = if idx_offset_bits == 64 { 8 } else { 4 };
    
    let mut found_entry = None;
    while low <= high {
        let mid = (low + high) / 2;
        let offset = mid * (word.len() + idx_bytes);
        if offset + word.len() > idx_data.len() { break; }
        
        let cand = &idx_data[offset..offset + word.len()];
        match cand.cmp(word) {
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid.saturating_sub(1),
            std::cmp::Ordering::Equal => {
                let mut pos = offset + word.len();
                let data_offset = if idx_offset_bits == 64 {
                    read_u32_be(&idx_data, &mut pos) as u64 | ((read_u32_be(&idx_data, &mut pos) as u64) << 32)
                } else {
                    read_u32_be(&idx_data, &mut pos) as u64
                };
                let data_size = read_u32_be(&idx_data, &mut pos) as usize;
                found_entry = Some((data_offset, data_size, offset));
                break;
            }
        }
    }
    
    if let Some((offset, size, _)) = found_entry {
        // Read .dict
        let dict_path = base.with_extension("dict.dz");
        let dict_file = File::open(&dict_path).unwrap();
        let mut reader = BufReader::new(dict_file);
        
        // dictzip decompress
        use flate2::read::ZlibDecoder;
        use std::io::{Seek, SeekFrom, BufRead};
        
        // Read chunks through dictzip
        let compressed = {
            let mut dict_data = vec![0u8; offset as usize + size + 1024];
            reader.read_exact(&mut dict_data[..offset as usize + size]).ok();
            
            // Actually, dictzip format is gzip with random access chunks
            // Let's use the dictzip module
            // For now, extract just the article data
            String::new() // placeholder
        };
        
        // Simpler approach: just call the actual dict_reader
        println!("Article at offset {} size {}", offset, size);
    } else {
        println!("'bad' not found in freedict");
    }
}
