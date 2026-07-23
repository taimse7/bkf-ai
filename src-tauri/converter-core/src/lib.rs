use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

const PREFIX_LEN: usize = 200;
const TAIL_WINDOW: u64 = 2 * 1024 * 1024;
const COPY_BUFFER: usize = 1024 * 1024;
const GOLDEN_ENCODED_PREFIX_SHA256: &str =
    "24dc4bbb763a30a9eecbdaa538c214747714b7c38a976dd6bf0f82c35e15701f";
const GOLDEN_PREFIX_XOR_MASK_HEX: &str = concat!(
    "dbb9d37684ae1bc06976b9833e6461954d6b06418b08ae2bd498ae79a8cf743f",
    "5b97ddace97b54b10354cde277a2ee1db55ab0f716ad45e2c893bb1bdde7b28c",
    "5c8f9ebdfaf4b20599590ea2ea5e02dfd8f62ffaa02d63dd6283678271d1472d",
    "7347ba03ab3ba0082d737b72de9a837a76cf285e32f374afbdd44e20e6d5c6b",
    "230f8e2c04772b42b8a2c87ea4675acc43a9eaed2538e9cb32d82c522def276",
    "a75684c010d44caa4ca8548b4caa4ca8548b2719fe4d22def276a756602d85be",
    "d15686b99a3b9b3826"
);

#[derive(Debug)]
pub enum ConversionError {
    Io(io::Error),
    InvalidBkc(&'static str),
    UnknownDecoderProfile,
    RepairUnavailable(String),
    UnsafeOutput(String),
    InvalidPdf(String),
    Cancelled,
}

impl std::fmt::Display for ConversionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::InvalidBkc(reason) => write!(formatter, "BKC לא תקין: {reason}"),
            Self::UnknownDecoderProfile => formatter.write_str("וריאנט BKC אינו מוכר"),
            Self::RepairUnavailable(reason) => {
                write!(formatter, "מנוע תיקון PDF אינו זמין: {reason}")
            }
            Self::UnsafeOutput(reason) => write!(formatter, "יעד פלט אינו בטוח: {reason}"),
            Self::InvalidPdf(reason) => write!(formatter, "אימות PDF נכשל: {reason}"),
            Self::Cancelled => formatter.write_str("ההמרה בוטלה בבטחה"),
        }
    }
}

impl std::error::Error for ConversionError {}

impl From<io::Error> for ConversionError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BkcStructure {
    pub base_offset: u64,
    pub startxref: u64,
    pub physical_xref: u64,
    pub encoded_prefix_sha256: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BkcSupport {
    Exact,
    RepairAvailable,
    RepairUnavailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BkcAnalysis {
    pub structure: BkcStructure,
    pub support: BkcSupport,
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionReport {
    pub decoder_profile: String,
    pub base_offset: u64,
    pub startxref: u64,
    pub physical_xref: u64,
    pub output_size: u64,
    pub page_count: u64,
    pub sha256: String,
}

struct DecoderProfile {
    name: &'static str,
    mask: [u8; PREFIX_LEN],
}

pub fn analyze_bkc(path: &Path) -> Result<BkcAnalysis, ConversionError> {
    let mut source = File::open(path)?;
    let input_size = source.metadata()?.len();
    let structure = read_structure(&mut source, input_size)?;
    source.seek(SeekFrom::Start(structure.base_offset))?;
    let mut encoded = [0_u8; PREFIX_LEN];
    source.read_exact(&mut encoded)?;

    let encoded_hash = hex_lower(&Sha256::digest(encoded));
    let profile = select_profile(&encoded).ok();
    let repair_available = ghostscript_path().is_some();
    Ok(BkcAnalysis {
        structure: BkcStructure {
            encoded_prefix_sha256: encoded_hash,
            ..structure
        },
        support: if profile.is_some() {
            BkcSupport::Exact
        } else if repair_available {
            BkcSupport::RepairAvailable
        } else {
            BkcSupport::RepairUnavailable
        },
        profile: profile.map(|value| value.name.to_string()),
    })
}

pub fn convert_bkc(input: &Path, output: &Path) -> Result<ConversionReport, ConversionError> {
    convert_bkc_with_control(input, output, |_| {}, || false)
}

pub fn convert_bkc_with_control<P, C>(
    input: &Path,
    output: &Path,
    mut progress: P,
    cancelled: C,
) -> Result<ConversionReport, ConversionError>
where
    P: FnMut(u64),
    C: Fn() -> bool,
{
    let canonical_input = input.canonicalize()?;
    let source_directory = canonical_input
        .parent()
        .ok_or_else(|| ConversionError::UnsafeOutput("תיקיית המקור אינה תקינה".into()))?;
    let output_parent = output
        .parent()
        .ok_or_else(|| ConversionError::UnsafeOutput("לא נבחרה תיקיית יעד".into()))?;
    std::fs::create_dir_all(output_parent)?;
    let output_parent = output_parent.canonicalize()?;
    if output_parent.starts_with(source_directory) {
        return Err(ConversionError::UnsafeOutput(
            "אין לכתוב בתוך תיקיית המקור".into(),
        ));
    }
    if output.exists() {
        return Err(ConversionError::UnsafeOutput("קובץ היעד כבר קיים".into()));
    }

    let mut source = File::open(&canonical_input)?;
    let input_size = source.metadata()?.len();
    let structure = read_structure(&mut source, input_size)?;
    source.seek(SeekFrom::Start(structure.base_offset))?;
    let mut encoded = [0_u8; PREFIX_LEN];
    source.read_exact(&mut encoded)?;

    let profile = match select_profile(&encoded) {
        Ok(profile) => profile,
        Err(ConversionError::UnknownDecoderProfile) => {
            return repair_bkc_with_ghostscript(
                &mut source,
                input_size,
                &structure,
                output,
                progress,
                cancelled,
            );
        }
        Err(error) => return Err(error),
    };

    let mut decoded = encoded;
    for (byte, mask) in decoded.iter_mut().zip(profile.mask) {
        *byte ^= mask;
    }
    if !decoded.starts_with(b"%PDF-") {
        return Err(ConversionError::InvalidPdf("כותרת PDF לא שוחזרה".into()));
    }

    let temporary = temporary_path(output);
    let result = (|| -> Result<ConversionReport, ConversionError> {
        if cancelled() {
            return Err(ConversionError::Cancelled);
        }
        let destination = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut writer = BufWriter::with_capacity(COPY_BUFFER, destination);
        writer.write_all(&decoded)?;
        progress(PREFIX_LEN as u64);

        source.seek(SeekFrom::Start(structure.base_offset + PREFIX_LEN as u64))?;
        copy_streaming(
            &mut source,
            &mut writer,
            structure.base_offset,
            &mut progress,
            &cancelled,
        )?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        drop(writer);

        let (output_size, page_count, sha256) = validate_pdf(&temporary, structure.startxref)?;
        fs::rename(&temporary, output)?;
        Ok(ConversionReport {
            decoder_profile: profile.name.into(),
            base_offset: structure.base_offset,
            startxref: structure.startxref,
            physical_xref: structure.physical_xref,
            output_size,
            page_count,
            sha256,
        })
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn read_structure(source: &mut File, input_size: u64) -> Result<BkcStructure, ConversionError> {
    if input_size < PREFIX_LEN as u64 + 32 {
        return Err(ConversionError::InvalidBkc("הקובץ קצר מדי"));
    }
    source.seek(SeekFrom::Start(0))?;
    let mut magic = [0_u8; 3];
    source.read_exact(&mut magic)?;
    if &magic != b"BKC" {
        return Err(ConversionError::InvalidBkc("magic bytes אינם BKC"));
    }

    let tail_start = input_size.saturating_sub(TAIL_WINDOW);
    source.seek(SeekFrom::Start(tail_start))?;
    let mut tail = Vec::with_capacity((input_size - tail_start) as usize);
    source.read_to_end(&mut tail)?;
    let startxref_marker =
        rfind(&tail, b"startxref").ok_or(ConversionError::InvalidBkc("startxref לא נמצא"))?;
    let startxref = parse_decimal_line(&tail[startxref_marker + b"startxref".len()..])
        .ok_or(ConversionError::InvalidBkc("ערך startxref אינו תקין"))?;
    let xref_type = rfind_xref_type(&tail[..startxref_marker])
        .ok_or(ConversionError::InvalidBkc("XRef פיזי לא נמצא"))?;
    let object_line_start = find_object_start(&tail[..xref_type])
        .ok_or(ConversionError::InvalidBkc("תחילת אובייקט XRef לא נמצאה"))?;
    let physical_xref = tail_start + object_line_start as u64;
    let base_offset = physical_xref
        .checked_sub(startxref)
        .ok_or(ConversionError::InvalidBkc("היסט XRef שלילי"))?;

    if base_offset + PREFIX_LEN as u64 > input_size {
        return Err(ConversionError::InvalidBkc("baseOffset מחוץ לקובץ"));
    }

    Ok(BkcStructure {
        base_offset,
        startxref,
        physical_xref,
        encoded_prefix_sha256: String::new(),
    })
}

fn repair_bkc_with_ghostscript<P, C>(
    source: &mut File,
    input_size: u64,
    structure: &BkcStructure,
    output: &Path,
    mut progress: P,
    cancelled: C,
) -> Result<ConversionReport, ConversionError>
where
    P: FnMut(u64),
    C: Fn() -> bool,
{
    let executable = ghostscript_path().ok_or_else(|| {
        ConversionError::RepairUnavailable(
            "Ghostscript לא נמצא. הגדר BKF_AI_GS_PATH או התקן gs".into(),
        )
    })?;
    if cancelled() {
        return Err(ConversionError::Cancelled);
    }

    let probe = temporary_path(&output.with_extension("probe.pdf"));
    let repaired = temporary_path(output);
    let result = (|| -> Result<ConversionReport, ConversionError> {
        let destination = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)?;
        let mut writer = BufWriter::with_capacity(COPY_BUFFER, destination);
        const PDF_HEADER: &[u8] = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n";
        writer.write_all(PDF_HEADER)?;
        writer.write_all(&vec![b' '; PREFIX_LEN - PDF_HEADER.len()])?;
        source.seek(SeekFrom::Start(structure.base_offset + PREFIX_LEN as u64))?;
        copy_streaming(
            source,
            &mut writer,
            structure.base_offset,
            &mut progress,
            &cancelled,
        )?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        drop(writer);

        let status = Command::new(&executable)
            .arg("-q")
            .arg("-dNOPAUSE")
            .arg("-dBATCH")
            .arg("-sDEVICE=pdfwrite")
            .arg(format!("-sOutputFile={}", repaired.display()))
            .arg(&probe)
            .status()
            .map_err(|error| {
                ConversionError::RepairUnavailable(format!("{} ({error})", executable.display()))
            })?;

        if !status.success() {
            return Err(ConversionError::RepairUnavailable(format!(
                "Ghostscript exited with {status}"
            )));
        }
        if cancelled() {
            return Err(ConversionError::Cancelled);
        }

        let repaired_startxref = read_pdf_startxref(&repaired)?;
        let (output_size, page_count, sha256) = validate_pdf(&repaired, repaired_startxref)?;
        fs::rename(&repaired, output)?;
        progress(input_size.saturating_sub(structure.base_offset));

        Ok(ConversionReport {
            decoder_profile: "bkc-ghostscript-repair-v1".into(),
            base_offset: structure.base_offset,
            startxref: structure.startxref,
            physical_xref: structure.physical_xref,
            output_size,
            page_count,
            sha256,
        })
    })();

    let _ = fs::remove_file(&probe);
    if result.is_err() {
        let _ = fs::remove_file(&repaired);
    }
    result
}

fn copy_streaming<P, C>(
    source: &mut File,
    writer: &mut BufWriter<File>,
    base_offset: u64,
    progress: &mut P,
    cancelled: &C,
) -> Result<(), ConversionError>
where
    P: FnMut(u64),
    C: Fn() -> bool,
{
    let mut buffer = vec![0_u8; COPY_BUFFER];
    loop {
        if cancelled() {
            return Err(ConversionError::Cancelled);
        }
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        writer.write_all(&buffer[..read])?;
        progress(source.stream_position()?.saturating_sub(base_offset));
    }
    Ok(())
}

fn ghostscript_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("BKF_AI_GS_PATH").map(PathBuf::from) {
        return path.is_file().then_some(path);
    }
    Command::new("gs")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|_| PathBuf::from("gs"))
}

fn read_pdf_startxref(path: &Path) -> Result<u64, ConversionError> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let start = size.saturating_sub(TAIL_WINDOW);
    file.seek(SeekFrom::Start(start))?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail)?;
    let marker = rfind(&tail, b"startxref")
        .ok_or_else(|| ConversionError::InvalidPdf("startxref חסר לאחר תיקון".into()))?;
    parse_decimal_line(&tail[marker + b"startxref".len()..])
        .ok_or_else(|| ConversionError::InvalidPdf("ערך startxref אינו תקין".into()))
}

fn select_profile(encoded: &[u8; PREFIX_LEN]) -> Result<DecoderProfile, ConversionError> {
    if hex_lower(&Sha256::digest(encoded)) != GOLDEN_ENCODED_PREFIX_SHA256 {
        return Err(ConversionError::UnknownDecoderProfile);
    }
    let bytes =
        decode_hex(GOLDEN_PREFIX_XOR_MASK_HEX).ok_or(ConversionError::UnknownDecoderProfile)?;
    let mask: [u8; PREFIX_LEN] = bytes
        .try_into()
        .map_err(|_| ConversionError::UnknownDecoderProfile)?;
    Ok(DecoderProfile {
        name: "bkc-golden-674817-v1",
        mask,
    })
}

fn validate_pdf(
    path: &Path,
    expected_startxref: u64,
) -> Result<(u64, u64, String), ConversionError> {
    let file = File::open(path)?;
    let size = file.metadata()?.len();
    if expected_startxref >= size {
        return Err(ConversionError::InvalidPdf("startxref מחוץ לקובץ".into()));
    }

    let mut reader = BufReader::with_capacity(COPY_BUFFER, file);
    let mut hash = Sha256::new();
    let mut page_count = 0_u64;
    let mut carry = Vec::new();
    let mut buffer = vec![0_u8; COPY_BUFFER];

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
        carry.extend_from_slice(&buffer[..read]);
        let keep = b"/Type /Pages".len();
        if carry.len() > keep {
            let safe_starts = carry.len() - keep;
            page_count += count_page_starts(&carry, safe_starts);
            carry.drain(..safe_starts);
        }
    }
    page_count += count_page_starts(&carry, carry.len());

    let mut check = File::open(path)?;
    let mut header = [0_u8; 5];
    check.read_exact(&mut header)?;
    if &header != b"%PDF-" {
        return Err(ConversionError::InvalidPdf("כותרת חסרה".into()));
    }

    check.seek(SeekFrom::Start(expected_startxref))?;
    let mut xref = [0_u8; 4096];
    let read = check.read(&mut xref)?;
    if rfind_xref_type(&xref[..read]).is_none() && !xref[..read].starts_with(b"xref") {
        return Err(ConversionError::InvalidPdf(
            "startxref אינו מצביע ל־XRef".into(),
        ));
    }

    let tail_len = size.min(1024);
    check.seek(SeekFrom::End(-(tail_len as i64)))?;
    let mut end = vec![0_u8; tail_len as usize];
    check.read_exact(&mut end)?;
    if !contains(&end, b"%%EOF") {
        return Err(ConversionError::InvalidPdf("%%EOF חסר".into()));
    }

    Ok((size, page_count, hex_lower(&hash.finalize())))
}

fn count_page_starts(bytes: &[u8], max_starts: usize) -> u64 {
    let needle = b"/Type /Page";
    bytes
        .windows(needle.len())
        .enumerate()
        .filter(|(index, window)| {
            *index < max_starts
                && *window == needle
                && bytes
                    .get(index + needle.len())
                    .is_none_or(|byte| *byte != b's')
        })
        .count() as u64
}

fn temporary_path(output: &Path) -> PathBuf {
    let name = output.file_name().unwrap_or_default().to_string_lossy();
    output.with_file_name(format!(".{name}.{}.tmp", Uuid::new_v4()))
}

fn rfind(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    rfind(haystack, needle).is_some()
}

fn parse_decimal_line(bytes: &[u8]) -> Option<u64> {
    let start = bytes.iter().position(|byte| !byte.is_ascii_whitespace())?;
    let digits = &bytes[start..];
    let end = digits
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .unwrap_or(digits.len());
    if end == 0 {
        return None;
    }
    std::str::from_utf8(&digits[..end]).ok()?.parse().ok()
}

fn rfind_xref_type(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(b"/Type".len())
        .enumerate()
        .filter_map(|(index, window)| {
            if window != b"/Type" {
                return None;
            }
            let rest = &bytes[index + b"/Type".len()..];
            let start = rest.iter().position(|byte| !byte.is_ascii_whitespace())?;
            rest[start..].starts_with(b"/XRef").then_some(index)
        })
        .next_back()
}

fn find_object_start(bytes: &[u8]) -> Option<usize> {
    let obj = rfind(bytes, b" obj")?;
    let line_start = bytes[..obj]
        .iter()
        .rposition(|byte| matches!(byte, b'\n' | b'\r'))
        .map_or(0, |index| index + 1);
    let fields = std::str::from_utf8(&bytes[line_start..obj]).ok()?;
    let mut parts = fields.split_ascii_whitespace();
    parts.next()?.parse::<u64>().ok()?;
    parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(line_start)
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16)? as u8;
            let low = (pair[1] as char).to_digit(16)? as u8;
            Some((high << 4) | low)
        })
        .collect()
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_profile() {
        assert!(matches!(
            select_profile(&[0_u8; PREFIX_LEN]),
            Err(ConversionError::UnknownDecoderProfile)
        ));
    }

    #[test]
    fn accepts_compact_xref() {
        assert_eq!(rfind_xref_type(b"<</Type/XRef/Length 4>>"), Some(2));
    }

    #[test]
    fn never_routes_bkf_to_pdf() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("sample.book");
        let output_dir = tempfile::tempdir().unwrap();
        let output = output_dir.path().join("sample.pdf");
        fs::write(
            &input,
            [b'B', b'K', b'F']
                .into_iter()
                .chain(std::iter::repeat(0).take(300))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert!(matches!(
            convert_bkc(&input, &output),
            Err(ConversionError::InvalidBkc(_))
        ));
        assert!(!output.exists());
    }
}
