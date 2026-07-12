use crate::engine::types::IfoInfo;

/// 解析 Stardict .ifo 文件内容
pub fn parse_ifo(content: &str) -> Result<IfoInfo, String> {
    let mut info = IfoInfo::default();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "version" => info.version = value.to_string(),
                "bookname" => info.bookname = value.to_string(),
                "wordcount" => info.wordcount = value.parse().unwrap_or(0),
                "synwordcount" => info.synwordcount = Some(value.parse().unwrap_or(0)),
                "idxfilesize" => info.idxfilesize = value.parse().unwrap_or(0),
                "idxoffsetbits" => info.idxoffsetbits = Some(value.parse().unwrap_or(32)),
                "author" => info.author = Some(value.to_string()),
                "email" => info.email = Some(value.to_string()),
                "website" => info.website = Some(value.to_string()),
                "description" => info.description = Some(value.to_string()),
                "date" => info.date = Some(value.to_string()),
                "sametypesequence" => info.sametypesequence = Some(value.to_string()),
                "dicttype" => info.dicttype = Some(value.to_string()),
                _ => {} // 忽略未识别的键
            }
        }
    }

    if info.version != "2.4.2" && info.version != "3.0.0" {
        return Err(format!("Unsupported version: {}", info.version));
    }
    if info.bookname.is_empty() {
        return Err("Missing bookname".to_string());
    }

    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ifo_basic() {
        let content = r#"StarDict's dict ifo file
version=2.4.2
bookname=Test Dictionary
wordcount=1000
idxfilesize=12000
sametypesequence=m
"#;
        let info = parse_ifo(content).unwrap();
        assert_eq!(info.version, "2.4.2");
        assert_eq!(info.bookname, "Test Dictionary");
        assert_eq!(info.wordcount, 1000);
        assert_eq!(info.idxfilesize, 12000);
        assert_eq!(info.sametypesequence, Some("m".to_string()));
    }

    #[test]
    fn test_parse_ifo_30() {
        let content = r#"StarDict's dict ifo file
version=3.0.0
bookname=Modern Dict
wordcount=50000
idxfilesize=800000
idxoffsetbits=64
"#;
        let info = parse_ifo(content).unwrap();
        assert_eq!(info.version, "3.0.0");
        assert_eq!(info.idxoffsetbits, Some(64));
    }

    #[test]
    fn test_parse_ifo_unsupported_version() {
        let content = "version=1.0\nbookname=Old\nwordcount=1\nidxfilesize=10\n";
        assert!(parse_ifo(content).is_err());
    }
}
