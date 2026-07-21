use serde::Serialize;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

const MAGIC_LEN: usize = 16;
const TAIL_WINDOW: u64 = 2 * 1024 * 1024;
const HEAD_EVIDENCE_WINDOW: usize = 64 * 1024;

#[derive(Debug)]
pub enum ProbeError {
    Io(io::Error),
    Malformed(&'static str),
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Malformed(reason) => write!(f, "container structure is malformed: {reason}"),
        }
    }
}

impl std::error::Error for ProbeError {}

impl From<io::Error> for ProbeError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerKind {
    Bkc,
    Bkf,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLevel {
    Proven,
    Hypothesis,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceItem {
    pub level: EvidenceLevel,
    pub code: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BkcStructure {
    pub startxref: u64,
    pub physical_xref: u64,
    pub base_offset: u64,
    pub xref_object_number: u64,
    pub eof_physical_offset: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BkfStructure {
    pub standard_djvu_signature_visible: bool,
    pub page_index_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeReport {
    pub format_version: u32,
    pub kind: ContainerKind,
    pub file_size: u64,
    pub bkc: Option<BkcStructure>,
    pub bkf: Option<BkfStructure>,
    pub decoder_available: bool,
    pub evidence: Vec<EvidenceItem>,
}

pub fn probe_path(path: &Path) -> Result<ProbeReport, ProbeError> {
    let mut file = File::open(path)?;
    probe_reader(&mut file)
}

pub fn probe_reader<R: Read + Seek>(reader: &mut R) -> Result<ProbeReport, ProbeError> {
    let file_size = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;
    let mut magic = [0_u8; MAGIC_LEN];
    let magic_read = reader.read(&mut magic)?;

    if magic_read >= 3 && &magic[..3] == b"BKC" {
        return probe_bkc(reader, file_size);
    }
    if magic_read >= 3 && &magic[..3] == b"BKF" {
        return probe_bkf(reader, file_size);
    }

    Ok(ProbeReport {
        format_version: 1,
        kind: ContainerKind::Unknown,
        file_size,
        bkc: None,
        bkf: None,
        decoder_available: false,
        evidence: vec![evidence(
            EvidenceLevel::Proven,
            "unknown_magic",
            "The first three bytes are neither BKC nor BKF.",
        )],
    })
}

fn probe_bkc<R: Read + Seek>(reader: &mut R, file_size: u64) -> Result<ProbeReport, ProbeError> {
    let tail_start = file_size.saturating_sub(TAIL_WINDOW);
    reader.seek(SeekFrom::Start(tail_start))?;
    let mut tail = Vec::with_capacity((file_size - tail_start) as usize);
    reader.read_to_end(&mut tail)?;

    let startxref_marker = rfind(&tail, b"startxref")
        .ok_or(ProbeError::Malformed("last startxref was not found"))?;
    let startxref = parse_decimal(&tail[startxref_marker + b"startxref".len()..])
        .ok_or(ProbeError::Malformed("last startxref value is invalid"))?;
    let eof_relative = tail[startxref_marker..]
        .windows(b"%%EOF".len())
        .position(|window| window == b"%%EOF")
        .map(|relative| startxref_marker + relative)
        .ok_or(ProbeError::Malformed("matching EOF marker was not found"))?;

    let xref_type = rfind_xref_type(&tail[..startxref_marker])
        .ok_or(ProbeError::Malformed("physical XRef stream was not found"))?;
    let object = find_object_header(&tail[..xref_type])
        .ok_or(ProbeError::Malformed("XRef object header was not found"))?;
    let physical_xref = tail_start + object.offset as u64;
    let base_offset = physical_xref
        .checked_sub(startxref)
        .ok_or(ProbeError::Malformed("computed base offset is negative"))?;
    if base_offset >= file_size {
        return Err(ProbeError::Malformed("computed base offset is outside the file"));
    }

    let bkc = BkcStructure {
        startxref,
        physical_xref,
        base_offset,
        xref_object_number: object.number,
        eof_physical_offset: tail_start + eof_relative as u64,
    };
    Ok(ProbeReport {
        format_version: 1,
        kind: ContainerKind::Bkc,
        file_size,
        bkc: Some(bkc),
        bkf: None,
        decoder_available: false,
        evidence: vec![
            evidence(EvidenceLevel::Proven, "bkc_magic", "The file begins with BKC."),
            evidence(
                EvidenceLevel::Proven,
                "bkc_last_xref",
                "The last startxref, physical XRef stream and following EOF are structurally consistent.",
            ),
            evidence(
                EvidenceLevel::Unknown,
                "decoder_profile",
                "A structural probe cannot determine the header decoder profile.",
            ),
        ],
    })
}

fn probe_bkf<R: Read + Seek>(reader: &mut R, file_size: u64) -> Result<ProbeReport, ProbeError> {
    reader.seek(SeekFrom::Start(0))?;
    let mut head = vec![0_u8; HEAD_EVIDENCE_WINDOW.min(file_size as usize)];
    reader.read_exact(&mut head)?;
    let djvu_visible = contains(&head, b"AT&TFORM") || contains(&head, b"DJVU");

    Ok(ProbeReport {
        format_version: 1,
        kind: ContainerKind::Bkf,
        file_size,
        bkc: None,
        bkf: Some(BkfStructure {
            standard_djvu_signature_visible: djvu_visible,
            page_index_status: "unknown".into(),
        }),
        decoder_available: false,
        evidence: vec![
            evidence(EvidenceLevel::Proven, "bkf_magic", "The file begins with BKF."),
            evidence(
                EvidenceLevel::Proven,
                if djvu_visible { "djvu_signature_visible" } else { "djvu_signature_not_visible_in_head" },
                if djvu_visible {
                    "A standard DjVu signature is visible in the bounded head window."
                } else {
                    "No standard DjVu signature is visible in the bounded head window."
                },
            ),
            evidence(
                EvidenceLevel::Unknown,
                "bkf_page_index",
                "Page boundaries and the page-index format have not been proven.",
            ),
        ],
    })
}

fn evidence(level: EvidenceLevel, code: &str, detail: &str) -> EvidenceItem {
    EvidenceItem { level, code: code.into(), detail: detail.into() }
}

#[derive(Debug)]
struct ObjectHeader {
    offset: usize,
    number: u64,
}

fn find_object_header(bytes: &[u8]) -> Option<ObjectHeader> {
    let obj = rfind(bytes, b" obj")?;
    let line_start = bytes[..obj]
        .iter()
        .rposition(|byte| matches!(byte, b'\n' | b'\r'))
        .map_or(0, |index| index + 1);
    let fields = std::str::from_utf8(&bytes[line_start..obj]).ok()?;
    let mut fields = fields.split_ascii_whitespace();
    let number = fields.next()?.parse().ok()?;
    let generation: u64 = fields.next()?.parse().ok()?;
    if generation > u32::MAX as u64 || fields.next().is_some() {
        return None;
    }
    Some(ObjectHeader { offset: line_start, number })
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
        .last()
}

fn parse_decimal(bytes: &[u8]) -> Option<u64> {
    let start = bytes.iter().position(|byte| !byte.is_ascii_whitespace())?;
    let digits = &bytes[start..];
    let end = digits.iter().position(|byte| !byte.is_ascii_digit()).unwrap_or(digits.len());
    if end == 0 {
        return None;
    }
    std::str::from_utf8(&digits[..end]).ok()?.parse().ok()
}

fn rfind(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|window| window == needle)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn classifies_unknown_by_content_not_extension() {
        let mut input = Cursor::new(b"not a book".to_vec());
        let report = probe_reader(&mut input).unwrap();
        assert_eq!(report.kind, ContainerKind::Unknown);
    }

    #[test]
    fn probes_bkf_without_claiming_page_boundaries() {
        let mut input = Cursor::new(b"BKF\0encoded payload".to_vec());
        let report = probe_reader(&mut input).unwrap();
        assert_eq!(report.kind, ContainerKind::Bkf);
        assert!(!report.decoder_available);
        assert_eq!(report.bkf.unwrap().page_index_status, "unknown");
    }

    #[test]
    fn probes_last_bkc_xref_with_crlf_and_compact_type() {
        let prefix = b"BKC encoded\r\n";
        let logical_xref = 40_u64;
        let physical_xref = 80_u64;
        let mut bytes = prefix.to_vec();
        bytes.resize(physical_xref as usize, b'x');
        bytes.extend_from_slice(b"9 0 obj\r<</Type/XRef/Length 0>>\rstream\r\rendstream\rendobj\rstartxref\r40\r%%EOF");
        let mut input = Cursor::new(bytes);
        let report = probe_reader(&mut input).unwrap();
        let bkc = report.bkc.unwrap();
        assert_eq!(bkc.startxref, logical_xref);
        assert_eq!(bkc.physical_xref, physical_xref);
        assert_eq!(bkc.base_offset, 40);
        assert_eq!(bkc.xref_object_number, 9);
    }

    #[test]
    fn rejects_bkc_without_structural_evidence() {
        let mut input = Cursor::new(b"BKC only".to_vec());
        assert!(matches!(probe_reader(&mut input), Err(ProbeError::Malformed(_))));
    }
}
