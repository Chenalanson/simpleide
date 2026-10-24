//! Read-only hexadecimal dump model.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const MAX_HEX_BYTES: usize = 32 * 1024 * 1024;
pub const BYTES_PER_ROW: usize = 16;

#[derive(Default)]
pub struct HexDump {
    pub path: PathBuf,
    pub data: Vec<u8>,
}

impl HexDump {
    pub fn open(path: &Path) -> Result<Self, String> {
        let metadata = std::fs::metadata(path).map_err(|err| format!("无法读取文件信息：{err}"))?;
        if metadata.len() > MAX_HEX_BYTES as u64 {
            return Err(format!(
                "文件超过 {} MB，十六进制视图暂不打开",
                MAX_HEX_BYTES / 1024 / 1024
            ));
        }
        let data = std::fs::read(path).map_err(|err| format!("读取文件失败：{err}"))?;
        Ok(Self {
            path: path.to_path_buf(),
            data,
        })
    }

    pub fn row_count(&self) -> usize {
        self.data.len().div_ceil(BYTES_PER_ROW)
    }

    pub fn row(&self, index: usize) -> Option<String> {
        let start = index.checked_mul(BYTES_PER_ROW)?;
        if start >= self.data.len() {
            return None;
        }
        let end = (start + BYTES_PER_ROW).min(self.data.len());
        let bytes = &self.data[start..end];
        let mut hex = String::with_capacity(BYTES_PER_ROW * 3);
        for offset in 0..BYTES_PER_ROW {
            if let Some(byte) = bytes.get(offset) {
                let _ = write!(hex, "{byte:02X} ");
            } else {
                hex.push_str("   ");
            }
        }
        let ascii = bytes
            .iter()
            .map(|byte| {
                if (0x20..=0x7E).contains(byte) {
                    *byte as char
                } else {
                    '.'
                }
            })
            .collect::<String>();
        Some(format!("{start:08X}  {hex} {ascii}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_rows_and_ascii() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.bin");
        std::fs::write(&path, [0x41, 0x42, 0x00, 0xFF]).unwrap();
        let dump = HexDump::open(&path).unwrap();
        assert_eq!(dump.row_count(), 1);
        let row = dump.row(0).unwrap();
        assert!(row.starts_with("00000000  41 42 00 FF "));
        assert!(row.ends_with("AB.."));
        assert!(dump.row(1).is_none());
    }

    #[test]
    fn handles_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.bin");
        std::fs::write(&path, []).unwrap();
        let dump = HexDump::open(&path).unwrap();
        assert_eq!(dump.row_count(), 0);
        assert!(dump.row(0).is_none());
    }
}
