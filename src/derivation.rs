use nom::*;

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

named!(
    string<String>,
    delimited!(
        char!('"'),
        map!(
            escaped_transform!(is_not!("\\\""), '\\', alt!(
                char!('\\') => { |_| &b"\\"[..] }
              | char!('"')  => { |_| &b"\""[..] }
              | char!('n')  => { |_| &b"\n"[..] }
              | char!('t')  => { |_| &b"\t"[..] }
            )),
            |bytes| String::from_utf8_lossy(&bytes).into_owned()
        ),
        char!('"')
    )
);

macro_rules! tuple_of {
    ($i:expr, $( $name:ident : $parser:ident ),* ) => {
        do_parse!($i,
            char!('(') >>
            $(
                $name : $parser >> opt!(comma) >>
            )*
            char!(')') >>
            ( $( $name ),* )
        )
    }
}

macro_rules! struct_of {
    ($i:expr, $st_name:ident { $( $name:ident : $parser:ident ),* } ) => {
        do_parse!($i,
            char!('(') >>
            $(
                $name : $parser >> opt!(comma) >>
            )*
            char!(')') >>
            ( $st_name { $( $name ),* } )
        )
    }
}

macro_rules! list_of {
    ($i:expr, $element:ident) => {
        delimited!($i,
            char!('['),
            separated_list_complete!(
                char!(','),
                $element
            ),
            char!(']')
        )
    }
}

named!(drv_output<DrvOutput>,
    struct_of!(
        DrvOutput {
            key: string,
            path: string,
            hash_algo: string,
            hash: string
        }
    )
);

named!(input_drv<InputDrv>,
    struct_of!(
        InputDrv {
            path: string,
            outputs: string_list
        }
    )
);

named!(comma<char>, char!(','));

// Why define lists here?
// 1. My macros only accepts idents, not submacros
// 2. It might help reduce overall code size
named!(string_list<Vec<String> >, list_of!(string));
named!(drv_output_list<Vec<DrvOutput> >, list_of!(drv_output));
named!(input_drv_list<Vec<InputDrv> >, list_of!(input_drv));
named!(tuple_string_string<(String, String)>, tuple_of!(a: string, b: string));
named!(tuple_string_string_list<Vec<(String, String)> >, list_of!(tuple_string_string));

named!(pub drv<Drv>,
    preceded!(
        tag!("Derive"),
        struct_of!(
            Drv {
                outputs: drv_output_list,
                input_drvs: input_drv_list,
                input_srcs: string_list,
                platform: string,
                builder: string,
                builder_args: string_list,
                env: tuple_string_string_list
            }
        )
    )
);


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
    named!(pair<(String, String)>, tuple_of!(foo: string, bar: string));
    assert_eq!(pair(b"(\"foo\",\"bar\")"), Ok((&b""[..],
               (String::from("foo"), String::from("bar")))));
}

#[test]
fn parse_list_of() {
    named!(string_list<Vec<String> >, list_of!(string));
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
    assert!(drv_hello.is_ok());

    let drv_blender = drv(include_bytes!("../assets/blender.drv"));
    assert!(drv_blender.is_ok());

    let drv_xz = drv(include_bytes!("../assets/xz.tar.bz2.drv"));
    println!("{:?}", drv_xz);
    assert!(drv_xz.is_ok());
}
