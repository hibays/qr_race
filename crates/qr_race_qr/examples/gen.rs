use std::io::Write;

fn main() {
    let mut args = std::env::args().skip(1);
    let text = args.next().expect("text");
    let path = args.next().expect("output path");
    let bytes = qr_race_qr::qr_webp(&text).expect("encode");
    let mut file = std::fs::File::create(path).expect("create");
    file.write_all(&bytes).expect("write");
}
