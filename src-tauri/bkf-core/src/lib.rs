use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const PREFIX_LEN: usize = 200;
const MAGIC: &[u8; 8] = b"BKFPFX01";
const RECORD_SIZE: usize = 256;

#[derive(Debug)]
pub enum BkfError {
    Io(io::Error),
    InvalidSidecar(String),
    Mismatch(String),
    MissingPage(u32),
}

impl std::fmt::Display for BkfError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::InvalidSidecar(error) => formatter.write_str(error),
            Self::Mismatch(error) => formatter.write_str(error),
            Self::MissingPage(page) => write!(formatter, "עמוד {page} אינו קיים ב־Sidecar"),
        }
    }
}

impl std::error::Error for BkfError {}

impl From<io::Error> for BkfError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BkfSidecarHeader {
    pub version: u32,
    pub page_count: u32,
    pub source_sha256: String,
    pub source_size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BkfPageRecord {
    pub page_index: u32,
    pub segment_offset: u64,
    pub segment_length: u64,
    #[serde(skip)]
    pub decoded_prefix: [u8; PREFIX_LEN],
    pub decoded_prefix_sha256: String,
}

#[derive(Debug, Clone)]
pub struct BkfSidecar {
    pub path: PathBuf,
    pub header: BkfSidecarHeader,
    pub pages: Vec<BkfPageRecord>,
}

pub trait BkfPrefixProvider: Send + Sync {
    fn provider_name(&self) -> &'static str;
    fn page_count(&self) -> u32;
    fn prefix_for_page(&self, page_index: u32) -> Result<[u8; PREFIX_LEN], BkfError>;
    fn segment_for_page(&self, page_index: u32) -> Result<(u64, u64), BkfError>;
}

impl BkfPrefixProvider for BkfSidecar {
    fn provider_name(&self) -> &'static str {
        "bkf-sidecar-v1"
    }

    fn page_count(&self) -> u32 {
        self.header.page_count
    }

    fn prefix_for_page(&self, page_index: u32) -> Result<[u8; PREFIX_LEN], BkfError> {
        self.pages
            .iter()
            .find(|page| page.page_index == page_index)
            .map(|page| page.decoded_prefix)
            .ok_or(BkfError::MissingPage(page_index))
    }

    fn segment_for_page(&self, page_index: u32) -> Result<(u64, u64), BkfError> {
        self.pages
            .iter()
            .find(|page| page.page_index == page_index)
            .map(|page| (page.segment_offset, page.segment_length))
            .ok_or(BkfError::MissingPage(page_index))
    }
}

pub fn discover_sidecar(book_path: &Path) -> Option<PathBuf> {
    let candidates = [
        book_path.with_extension("bkf-prefixes"),
        PathBuf::from(format!("{}.bkf-prefixes", book_path.display())),
    ];
    candidates.into_iter().find(|path| path.is_file())
}

pub fn read_sidecar(path: &Path) -> Result<BkfSidecar, BkfError> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut magic = [0_u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(BkfError::InvalidSidecar("Magic של Sidecar אינו תקין".into()));
    }

    let version = read_u32(&mut reader)?;
    if version != 1 {
        return Err(BkfError::InvalidSidecar(format!(
            "גרסת Sidecar אינה נתמכת: {version}"
        )));
    }
    let page_count = read_u32(&mut reader)?;
    let mut source_hash = [0_u8; 32];
    reader.read_exact(&mut source_hash)?;
    let source_size = read_u64(&mut reader)?;
    let mut reserved = [0_u8; 8];
    reader.read_exact(&mut reserved)?;

    let mut pages = Vec::with_capacity(page_count as usize);
    for _ in 0..page_count {
        let page_index = read_u32(&mut reader)?;
        let segment_offset = read_u64(&mut reader)?;
        let segment_length = read_u64(&mut reader)?;
        let prefix_length = read_u32(&mut reader)?;
        if prefix_length != PREFIX_LEN as u32 {
            return Err(BkfError::InvalidSidecar(format!(
                "אורך prefix שגוי בעמוד {page_index}: {prefix_length}"
            )));
        }
        let mut expected_hash = [0_u8; 32];
        reader.read_exact(&mut expected_hash)?;
        let mut decoded_prefix = [0_u8; PREFIX_LEN];
        reader.read_exact(&mut decoded_prefix)?;

        let actual_hash: [u8; 32] = Sha256::digest(decoded_prefix).into();
        if actual_hash != expected_hash {
            return Err(BkfError::InvalidSidecar(format!(
                "Checksum של prefix נכשל בעמוד {page_index}"
            )));
        }

        pages.push(BkfPageRecord {
            page_index,
            segment_offset,
            segment_length,
            decoded_prefix,
            decoded_prefix_sha256: hex_lower(&actual_hash),
        });
    }

    Ok(BkfSidecar {
        path: path.to_path_buf(),
        header: BkfSidecarHeader {
            version,
            page_count,
            source_sha256: hex_lower(&source_hash),
            source_size,
        },
        pages,
    })
}

pub fn validate_sidecar_for_book(
    sidecar: &BkfSidecar,
    book_path: &Path,
    verify_full_sha256: bool,
) -> Result<(), BkfError> {
    let metadata = book_path.metadata()?;
    if metadata.len() != sidecar.header.source_size {
        return Err(BkfError::Mismatch(
            "גודל קובץ המקור אינו תואם ל־Sidecar".into(),
        ));
    }

    if verify_full_sha256 {
        let actual = sha256_file(book_path)?;
        if actual != sidecar.header.source_sha256 {
            return Err(BkfError::Mismatch(
                "SHA-256 של קובץ המקור אינו תואם ל־Sidecar".into(),
            ));
        }
    }

    for page in &sidecar.pages {
        if page.segment_length < PREFIX_LEN as u64 {
            return Err(BkfError::InvalidSidecar(format!(
                "מקטע עמוד {} קצר מ־200 בתים",
                page.page_index
            )));
        }
        let end = page
            .segment_offset
            .checked_add(page.segment_length)
            .ok_or_else(|| BkfError::InvalidSidecar("גלישת offset".into()))?;
        if end > metadata.len() {
            return Err(BkfError::InvalidSidecar(format!(
                "מקטע עמוד {} מחוץ לקובץ",
                page.page_index
            )));
        }
    }

    Ok(())
}

pub fn rebuild_page_to_writer(
    book_path: &Path,
    provider: &dyn BkfPrefixProvider,
    page_index: u32,
    output: &mut dyn Write,
) -> Result<u64, BkfError> {
    let (offset, length) = provider.segment_for_page(page_index)?;
    if length < PREFIX_LEN as u64 {
        return Err(BkfError::InvalidSidecar("מקטע קצר מדי".into()));
    }
    let prefix = provider.prefix_for_page(page_index)?;
    output.write_all(&prefix)?;

    let mut source = File::open(book_path)?;
    source.seek(SeekFrom::Start(offset + PREFIX_LEN as u64))?;
    let mut remaining = length - PREFIX_LEN as u64;
    let mut buffer = vec![0_u8; 1024 * 1024];

    while remaining > 0 {
        let requested = remaining.min(buffer.len() as u64) as usize;
        let read = source.read(&mut buffer[..requested])?;
        if read == 0 {
            return Err(BkfError::InvalidSidecar(
                "קובץ המקור הסתיים באמצע עמוד".into(),
            ));
        }
        output.write_all(&buffer[..read])?;
        remaining -= read as u64;
    }

    Ok(length)
}

pub fn rebuild_page_to_vec(
    book_path: &Path,
    provider: &dyn BkfPrefixProvider,
    page_index: u32,
) -> Result<Vec<u8>, BkfError> {
    let (_, length) = provider.segment_for_page(page_index)?;
    let capacity = usize::try_from(length)
        .map_err(|_| BkfError::InvalidSidecar("העמוד גדול מדי לזיכרון".into()))?;
    let mut output = Vec::with_capacity(capacity);
    rebuild_page_to_writer(book_path, provider, page_index, &mut output)?;
    Ok(output)
}

pub fn write_sidecar(
    path: &Path,
    source_sha256: [u8; 32],
    source_size: u64,
    pages: &[(u32, u64, u64, [u8; PREFIX_LEN])],
) -> Result<(), BkfError> {
    let mut writer = File::create(path)?;
    writer.write_all(MAGIC)?;
    writer.write_all(&1_u32.to_le_bytes())?;
    writer.write_all(&(pages.len() as u32).to_le_bytes())?;
    writer.write_all(&source_sha256)?;
    writer.write_all(&source_size.to_le_bytes())?;
    writer.write_all(&[0_u8; 8])?;

    for (page_index, offset, length, prefix) in pages {
        writer.write_all(&page_index.to_le_bytes())?;
        writer.write_all(&offset.to_le_bytes())?;
        writer.write_all(&length.to_le_bytes())?;
        writer.write_all(&(PREFIX_LEN as u32).to_le_bytes())?;
        let hash: [u8; 32] = Sha256::digest(prefix).into();
        writer.write_all(&hash)?;
        writer.write_all(prefix)?;
        debug_assert_eq!(4 + 8 + 8 + 4 + 32 + PREFIX_LEN, RECORD_SIZE);
    }
    writer.sync_all()?;
    Ok(())
}

fn read_u32(reader: &mut impl Read) -> Result<u32, io::Error> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(reader: &mut impl Read) -> Result<u64, io::Error> {
    let mut bytes = [0_u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn sha256_file(path: &Path) -> Result<String, BkfError> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(hex_lower(&hash.finalize()))
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn writes_reads_and_rebuilds_a_page() {
        let temp = tempdir().unwrap();
        let book = temp.path().join("sample.book");
        let sidecar_path = temp.path().join("sample.bkf-prefixes");

        let mut protected = vec![0x55; 500];
        protected[..3].copy_from_slice(b"BKF");
        std::fs::write(&book, &protected).unwrap();

        let mut prefix = [0_u8; PREFIX_LEN];
        prefix[..8].copy_from_slice(b"AT&TFORM");
        let source_hash: [u8; 32] = Sha256::digest(&protected).into();
        write_sidecar(
            &sidecar_path,
            source_hash,
            protected.len() as u64,
            &[(0, 0, 500, prefix)],
        )
        .unwrap();

        let sidecar = read_sidecar(&sidecar_path).unwrap();
        validate_sidecar_for_book(&sidecar, &book, true).unwrap();
        let rebuilt = rebuild_page_to_vec(&book, &sidecar, 0).unwrap();
        assert_eq!(&rebuilt[..8], b"AT&TFORM");
        assert_eq!(&rebuilt[200..], &protected[200..]);
    }
}
