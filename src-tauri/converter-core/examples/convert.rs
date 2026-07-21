use bkf_converter_core::convert_bkc;
use std::path::Path;

fn main() {
    let mut arguments = std::env::args_os().skip(1);
    let input = arguments.next().expect("usage: convert <input.book> <output.pdf>");
    let output = arguments.next().expect("usage: convert <input.book> <output.pdf>");
    if arguments.next().is_some() {
        panic!("usage: convert <input.book> <output.pdf>");
    }
    let report = convert_bkc(Path::new(&input), Path::new(&output))
        .unwrap_or_else(|error| panic!("conversion failed: {error}"));
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
