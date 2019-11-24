use std::str;

use nom::{
    parse_to,
    IResult,
    branch::alt,
    sequence::{ preceded, terminated, tuple },
    combinator::{ map, map_parser, opt, value },
    bytes::streaming::{ tag, is_not },
    character::streaming::newline
};

#[derive(Debug, Clone)]
pub struct NarInfo {
    pub store_path: String,
    pub url: String,
    pub compression: String,
    pub file_hash: String,
    pub file_size: u64,
    pub nar_hash: String,
    pub nar_size: u64,
    pub references: Vec<String>,
    pub deriver: Option<String>,
    pub sig: String
}

fn data(i: &[u8]) -> IResult<&[u8], &[u8]> {
    alt((terminated(is_not("\n"), newline),
         value(&b"\n"[..], newline)))(i)
}

fn string(i: &[u8]) -> IResult<&[u8], String> {
    map(data, |b| String::from_utf8_lossy(&b).into_owned())(i)
}

fn string_list(i: &[u8]) -> IResult<&[u8], Vec<String>> {
    map(string, |s| s.split_whitespace().map(str::to_owned).collect())(i)
}

fn size(i: &[u8]) -> IResult<&[u8], u64> {
    map_parser(data, |i| parse_to!(i, u64))(i)
}

fn narinfo(i: &[u8]) -> IResult<&[u8], NarInfo> {
    let (i,
         (_, store_path, _, url, _, compression,
          _, file_hash, _, file_size,
          _, nar_hash, _, nar_size,
          _, references, deriver,  _, sig)) =
         tuple((tag("StorePath: "), string,
                tag("URL: "), string,
                tag("Compression: "), string,
                tag("FileHash: "), string,
                tag("FileSize: "), size,
                tag("NarHash: "), string,
                tag("NarSize: "), size,
                tag("References: "), string_list,
                opt(preceded(tag("Deriver: "), string)),
                tag("Sig: "), string))(i)?;
    Ok((i, NarInfo { store_path, url, compression,
       file_hash, file_size, nar_hash, nar_size,
       references, deriver, sig }))
}

/*
named!(data, alt!(
    terminated!(is_not!("\n"), newline)
  | newline => { |_| &b"\n"[..] }
));

named!(string<String>,
    map!(data, |b| String::from_utf8_lossy(&b).into_owned()));
named!(string_list<Vec<String> >,
    map!(string, |s| s.split_whitespace().map(str::to_owned).collect()));
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
        tag!("References: ") >> references: string_list >>
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
);*/

impl NarInfo {
    pub fn from(body: &[u8]) -> Option<Self> {
        if &body[..] == b"404" { return None }
        narinfo(body).ok().map(|(_rest, info)| info)
    }
}

#[test]
fn parse_empty_data() {
    assert!(data(b"\n").is_ok());
    assert_eq!(data(b"  foo\n"), Ok((&b""[..], &b"  foo"[..])));
}

#[test]
fn parse_size() {
    assert_eq!(size(b"20971\n"), Ok((&b""[..], 20971)));
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
