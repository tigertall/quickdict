use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::engine::dictzip::decompress_dictdz;

/// .dict 或 .dict.dz 数据文件的内存中读取器
///
/// 将所有数据加载到内存以简化随机访问。
/// 对于大词典（>200MB），后续可改为 mmap 或 chunked 方案。

pub struct DictDataReader {
    data: Vec<u8>,
}


impl DictDataReader {
    /// 打开 .dict 或 .dict.dz 文件
    pub fn open(dict_path: &Path) -> Result<Self, String> {
        // 先尝试 .dict.dz
        let dz_path = dict_path.with_extension("dict.dz");
        if dz_path.exists() {
            let data = decompress_dictdz(&dz_path)?;
            log::info!("Loaded dict.dz: {} bytes uncompressed", data.len());
            return Ok(DictDataReader { data });
        }

        // 尝试 .dict
        let plain_path = dict_path.with_extension("dict");
        if plain_path.exists() {
            let mut file = File::open(&plain_path)
                .map_err(|e| format!("Cannot open .dict: {}", e))?;
            let mut data = Vec::new();
            file.read_to_end(&mut data)
                .map_err(|e| format!("Cannot read .dict: {}", e))?;
            log::info!("Loaded .dict: {} bytes", data.len());
            return Ok(DictDataReader { data });
        }

        Err(format!(
            "Neither .dict nor .dict.dz found for {}",
            dict_path.display()
        ))
    }

    /// 从指定偏移读取指定大小的数据
    pub fn read_at(&self, offset: u64, size: u64) -> Result<Vec<u8>, String> {
        let start = offset as usize;
        let end = (offset + size) as usize;
        if end > self.data.len() {
            return Err(format!(
                "Offset+size ({}) exceeds data length ({})",
                end,
                self.data.len()
            ));
        }
        Ok(self.data[start..end].to_vec())
    }
}
