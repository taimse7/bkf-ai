use bkf_converter_core::convert_bkc;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;
use tempfile::tempdir;

fn sha256(path: &std::path::Path) -> String {
    let mut reader = BufReader::new(File::open(path).unwrap());
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer).unwrap();
        if read == 0 { break; }
        digest.update(&buffer[..read]);
    }
    digest.finalize().iter().map(|byte| format!("{byte:02X}")).collect()
}

#[test]
#[ignore = "requires the external 230 MB golden fixtures; run with BKF_GOLDEN_DIR"]
fn converts_674817_byte_for_byte() {
    let fixtures = PathBuf::from(std::env::var("BKF_GOLDEN_DIR").expect("BKF_GOLDEN_DIR is required"));
    let input = fixtures.join("674817.book");
    let expected = fixtures.join("674817_recovered.pdf");
    let temp = tempdir().unwrap();
    let output = temp.path().join("674817.pdf");
    let report = convert_bkc(&input, &output).unwrap();
    println!("{report:?}");
    assert_eq!(report.base_offset, 7105);
    assert_eq!(report.output_size, 115_172_663);
    assert_eq!(report.page_count, 506);
    assert_eq!(report.sha256.to_uppercase(), "030B0E2B93270B96EF24D63F1C5254D41BA2B54C9E0232C428F2D9E254E3B165");
    assert_eq!(sha256(&output), sha256(&expected));
    let mut generated = BufReader::new(File::open(&output).unwrap());
    let mut golden = BufReader::new(File::open(&expected).unwrap());
    let mut generated_chunk = vec![0_u8; 1024 * 1024];
    let mut golden_chunk = vec![0_u8; 1024 * 1024];
    loop {
        let generated_read = generated.read(&mut generated_chunk).unwrap();
        let golden_read = golden.read(&mut golden_chunk).unwrap();
        assert_eq!(generated_read, golden_read);
        if generated_read == 0 { break; }
        assert_eq!(&generated_chunk[..generated_read], &golden_chunk[..golden_read]);
    }
    println!("binary comparison: identical");
}
