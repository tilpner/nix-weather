use std::{ fs, path::Path };

use nom::{
    IResult,
    sequence::{
        delimited, preceded,
        separated_pair,
        tuple
    },
    combinator::{ map, value },
    branch::alt,
    multi::separated_list,
    bytes::streaming::{
        escaped_transform,
        is_not,
        tag
    },
    character::complete::char
};
use log::trace;

use crate::{ StoreHash, StoreItem, StoreCache };

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drv {
    pub outputs: Vec<DrvOutput>,
    pub input_drvs: Vec<InputDrv>,
    pub input_srcs: Vec<String>,
    pub platform: String,
    pub builder: String,
    pub builder_args: Vec<String>,
    pub env: Vec<(String, String)>
}

impl Drv {
    pub fn read_from<P: AsRef<Path>>(path: P) -> Self {
        trace!("reading derivation {}", path.as_ref().display());
        let file_content = fs::read(path).expect("Unable to read derivation");
        let (rest, drv) = drv(&file_content).expect("Unable to parse derivation");
        assert!(rest.is_empty(), "Less than the entire drv was parsed");
        drv
    }

    pub fn find_name(&self) -> String {
        self.env.iter()
            .find(|(k, _)| k == "name")
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| String::from("unknown"))
    }

    pub fn find_output(&self, key: &str) -> Option<&DrvOutput> {
        self.outputs.iter()
            .find(|output| output.key == key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrvOutput {
    pub key: String,
    pub path: String,
    pub hash_algo: String,
    pub hash: String
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDrv {
    pub path: String,
    pub outputs: Vec<String>
}

impl InputDrv {
    // TODO: don't lifetime *everything*?
    pub fn resolve<'a>(&'a self, drvs: &'a StoreCache) -> impl Iterator<Item = &'a str> + 'a {
        let hash = StoreHash::from_path(&self.path);
        if let Some(StoreItem::Drv(drv)) = drvs.get(&hash) {
            self.outputs.iter()
                .flat_map(move |output| drv.find_output(output))
                .map(|output| &output.path[..])
        } else { panic!() }
    }
}

fn string(i: &[u8]) -> IResult<&[u8], String> {
    delimited(
        char('"'),
        map(
            escaped_transform(is_not("\\\""), '\\', alt((
              value(&b"\\"[..], char('\\')),
              value(&b"\""[..], char('"')),
              value(&b"\n"[..], char('n')),
              value(&b"\t"[..], char('t'))
            ))),
            |bytes| String::from_utf8_lossy(&bytes).into_owned()
        ),
        char('"')
    )(i)
}

fn pair<A, OA, B, OB>(a: A, b: B) -> impl Fn(&[u8]) -> IResult<&[u8], (OA, OB)>
where A: Fn(&[u8]) -> IResult<&[u8], OA> + Copy,
      B: Fn(&[u8]) -> IResult<&[u8], OB> + Copy {
    in_parens(
        move |i| separated_pair(a, char(','), b)(i)
    )
}

fn list_of<P, O>(p: P) -> impl Fn(&[u8]) -> IResult<&[u8], Vec<O>>
where P: Fn(&[u8]) -> IResult<&[u8], O> + Copy {
    move |i| {
        delimited(
            char('['),
            separated_list(char(','), p),
            char(']')
        )(i)
    }
}

fn comma(i: &[u8]) -> IResult<&[u8], char> { char(',')(i) }

fn in_parens<P, OP>(p: P) -> impl Fn(&[u8]) -> IResult<&[u8], OP>
where P: Fn(&[u8]) -> IResult<&[u8], OP> + Copy {
    move |i| delimited(char('('), p, char(')'))(i)
}

fn drv_output(i: &[u8]) -> IResult<&[u8], DrvOutput> {
    in_parens(
        move |i| {
            let (i, (key, _, path, _, hash_algo, _, hash)) =
                tuple((string, comma, string, comma, string, comma, string))(i)?;
            Ok((i, DrvOutput { key, path, hash_algo, hash }))
        },
    )(i)
}

fn input_drv(i: &[u8]) -> IResult<&[u8], InputDrv> {
    in_parens(
        move |i| {
            let (i, (path, _, outputs)) =
                tuple((string, comma, list_of(string)))(i)?;
            Ok((i, InputDrv { path, outputs }))
        },
    )(i)
}

fn drv(i: &[u8]) -> IResult<&[u8], Drv> {
    fn pair_string_string(i: &[u8]) -> IResult<&[u8], (String, String)> {
        pair(string, string)(i)
    }
    preceded(
        tag("Derive"),
        in_parens(move |i| {
            let (i, (outputs, _, input_drvs, _,
                     input_srcs, _, platform, _,
                     builder, _, builder_args, _, env)) =
                tuple((list_of(drv_output), comma, list_of(input_drv), comma,
                       list_of(string), comma, string, comma,
                       string, comma, list_of(string), comma, list_of(pair_string_string)))(i)?;
            Ok((i, Drv { outputs, input_drvs, input_srcs, platform, builder, builder_args, env }))
        })
    )(i)
}

#[test]
fn parse_string() {
    assert_eq!(string(br#""foo""#), Ok((&b""[..], String::from("foo"))));
    assert_eq!(string(br#""""#), Ok((&b""[..], String::from(""))));
    assert_eq!(string(br#""foo/bar""#), Ok((&b""[..], String::from("foo/bar"))));
    assert_eq!(string(br#""\"""#), Ok((&b""[..], String::from("\""))));
    assert_eq!(string(br#""\\""#), Ok((&b""[..], String::from("\\"))));
    assert_eq!(string(br#""\t""#), Ok((&b""[..], String::from("\t"))));
    assert_eq!(string(br#""\n""#), Ok((&b""[..], String::from("\n"))));
}

#[test]
fn parse_pair() {
    let tuple_string_string = pair(string, string);
    assert_eq!(tuple_string_string(b"(\"foo\",\"bar\")"), Ok((&b""[..],
               (String::from("foo"), String::from("bar")))));
}

#[test]
fn parse_list_of() {
    let string_list = list_of(string);
    assert_eq!(string_list(b"[\"foo\"]"), Ok((&b""[..], vec![String::from("foo")])));
}

#[test]
fn parse_drv_output() {
    assert_eq!(drv_output(br#"("out","/nix/store/rgmc4d3spji36n2l1sicm80yq79dpcc2-hello-2.10","","")"#),
        Ok((&b""[..], DrvOutput {
            key: String::from("out"),
            path: String::from("/nix/store/rgmc4d3spji36n2l1sicm80yq79dpcc2-hello-2.10"),
            hash_algo: String::new(),
            hash: String::new()
        })));
}

#[test]
fn parse_input_drv() {
    assert_eq!(input_drv(br#"("/nix/store/cif7s5k57iwcxwgcv01myyiypw1skz99-stdenv-linux.drv",["out"])"#),
        Ok((&b""[..], InputDrv {
            path: String::from("/nix/store/cif7s5k57iwcxwgcv01myyiypw1skz99-stdenv-linux.drv"),
            outputs: vec![String::from("out")]
        })));
}

#[test]
fn parse_derivation() {
    let drv_hello = drv(include_bytes!("../assets/hello.drv"));
    println!("{:?}", drv_hello);
    assert!(drv_hello.is_ok());

    let drv_blender = drv(include_bytes!("../assets/blender.drv"));
    assert!(drv_blender.is_ok());

    let drv_xz = drv(include_bytes!("../assets/xz.tar.bz2.drv"));
    println!("{:?}", drv_xz);
    assert!(drv_xz.is_ok());
}
