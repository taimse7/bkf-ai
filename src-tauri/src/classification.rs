use std::fs::OpenOptions;
use std::io::{self, Read};
use std::path::Path;

pub const PREFIX_LIMIT: u64 = 16;
const BKC_MAGIC: &[u8] = b"BKC";
const BKF_MAGIC: &[u8] = b"BKF";

pub fn classify_file(path: &Path) -> io::Result<&'static str> {
    let mut file = OpenOptions::new().read(true).write(false).open(path)?;
    classify_reader(&mut file)
}

pub fn classify_reader(reader: &mut impl Read) -> io::Result<&'static str> {
    let mut prefix = Vec::with_capacity(PREFIX_LIMIT as usize);
    reader.take(PREFIX_LIMIT).read_to_end(&mut prefix)?;
    Ok(if prefix.starts_with(BKC_MAGIC) {
        "BKC"
    } else if prefix.starts_with(BKF_MAGIC) {
        "BKF"
    } else {
        "Unknown"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn classifies_real_fixture_files_by_magic_not_extension() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        assert_eq!(
            classify_file(&fixtures.join("sample-bkc.bin")).unwrap(),
            "BKC"
        );
        assert_eq!(
            classify_file(&fixtures.join("sample-bkf.bin")).unwrap(),
            "BKF"
        );
        assert_eq!(
            classify_file(&fixtures.join("sample-unknown.book")).unwrap(),
            "Unknown"
        );
    }

    #[test]
    fn reads_only_the_required_16_byte_prefix() {
        let data = [b'X'; 4096];
        let mut cursor = Cursor::new(data);
        assert_eq!(classify_reader(&mut cursor).unwrap(), "Unknown");
        assert_eq!(cursor.position(), 16);
    }

    #[test]
    fn extension_does_not_override_magic() {
        let mut fake_bkc_extension = Cursor::new(b"not a container".to_vec());
        assert_eq!(classify_reader(&mut fake_bkc_extension).unwrap(), "Unknown");
    }
}
