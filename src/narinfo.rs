use reqwest::r#async::Chunk;
use nom::*;

#[derive(Debug, Clone)]
pub struct NarInfo {
    pub store_path: String,
    pub url: String,
    pub compression: String,
    pub file_hash: String,
    pub file_size: u64,
    pub nar_hash: String,
    pub nar_size: u64,
    pub references: String,
    pub deriver: Option<String>,
    pub sig: String
}

named!(data, alt!(
    terminated!(is_not!("\n"), newline)
  | newline => { |_| &b"\n"[..] }
));

named!(string<String>,
    map!(data, |b| String::from_utf8_lossy(&b).into_owned()));
named!(size<u64>, flat_map!(data, parse_to!(u64)));

named!(narinfo<NarInfo>,
    do_parse!(
        tag!("StorePath: ") >> store_path: string >>
        tag!("URL: ") >> url: string >>
        tag!("Compression: ") >> compression: string >>
        tag!("FileHash: ") >> file_hash: string >>
        tag!("FileSize: ") >> file_size: size >>
        tag!("NarHash: ") >> nar_hash: string >>
        tag!("NarSize: ") >> nar_size: size >>
        tag!("References: ") >> references: string >>
        deriver: opt!(preceded!(tag!("Deriver: "), string)) >>
        tag!("Sig: ") >> sig: string >>
        (NarInfo {
            store_path, url,
            compression,
            file_hash, file_size,
            nar_hash, nar_size,
            references,
            deriver, sig
        })
    )
);

impl NarInfo {
    pub fn from(body: Chunk) -> Option<Self> {
        if &body[..] == b"404" { return None }
        narinfo(&body).ok().map(|(_rest, info)| info)
    }
}

#[test]
fn parse_empty_data() {
    assert!(data(b"\n").is_ok());
    assert_eq!(data(b"  foo\n"), Ok((&b""[..], &b"  foo"[..])));
}

#[test]
fn parse_narinfo() {
    let info = narinfo(include_bytes!("../assets/blender.narinfo")); 
    println!("{:?}", info);
    assert!(info.is_ok());

    let info = narinfo(include_bytes!("../assets/dejagnu.narinfo")); 
    println!("{:?}", info);
    assert!(info.is_ok());
}
