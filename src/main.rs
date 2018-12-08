pub mod derivation;
pub mod narinfo;

use std::{
    env, fs, str, mem,
    path::Path,
    collections::HashMap
};

use futures::{ stream, Future, Stream };
use tokio::runtime::Runtime;
use reqwest::r#async::Client;

use number_prefix::{ binary_prefix, Standalone, Prefixed };

use crate::{ derivation::*, narinfo::* };

const NIX_HASH_LENGTH: usize = 32;

fn read_drv<P: AsRef<Path>>(path: P) -> Drv {
    let file_content = fs::read(path).expect("Unable to read derivation");
    let (rest, drv) = derivation::drv(&file_content).expect("Unable to parse derivation");
    assert!(rest.is_empty(), "Less than the entire drv was parsed");
    drv
}

fn gather_closure(hash: StoreHash, drv: &Drv) -> HashMap<StoreHash, Drv> {
    fn add_to_closure(out: &mut HashMap<StoreHash, Drv>, hash: StoreHash, drv: &Drv) {
        if out.contains_key(&hash) { return; }
        out.insert(hash, drv.clone());

        for InputDrv { path, .. } in &drv.input_drvs {
            // check cache to avoid unnecessary IO/parsing
            let input_drv_hash = StoreHash::from_path(path);
            let input_drv = if out.contains_key(&input_drv_hash) {
                out[&input_drv_hash].clone()
            } else {
                read_drv(&path)
            };

            add_to_closure(out, input_drv_hash, &input_drv);
        }
    }

    let mut closure = HashMap::new();
    add_to_closure(&mut closure, hash, &drv);
    closure
}

// Nix store hashes are the first 160 bits of a sha256 hash, base32 encoded.
// That base32 representation could be decoded into a [u32; 5], but then
// we'd depend on Nix's exact character set and encoding/decoding rules.
//
// sizeof [u8; 32] = 256 bit > 160 bit = sizeof [u32; 5]
//
// This means our representation is quite a bit less compact than it could be,
// but with a typical system closure of 10k paths, that's still less than a megabyte,
// the derivations themselves are much more expensive.
#[derive(Hash, Clone, PartialEq, Eq)]
struct StoreHash([u8; NIX_HASH_LENGTH]);
impl StoreHash {
    fn from_path<P: AsRef<Path>>(path: P) -> Self {
        let hash = &path.as_ref()
            .file_name().expect("Not a file")
            .to_str().expect("Invalid characters in filename")
            .as_bytes()[..NIX_HASH_LENGTH];

        let mut arr: [u8; NIX_HASH_LENGTH] = unsafe { mem::uninitialized() };
        arr.copy_from_slice(hash);

        StoreHash(arr)
    }

    fn to_str(&self) -> &str {
        // no need to check for utf8-ness, content can only be created by from_path
        unsafe { str::from_utf8_unchecked(&self.0) }
    }
}

fn fetch_narinfo<I>(outputs: I) -> Vec<Option<NarInfo>>
    // Runtime::block_on needs Send + 'static
    where I: Iterator<Item = StoreHash> + Send + 'static
{
    let client = Client::new();
    let narinfos = stream::iter_ok(outputs)
        .map(move |output| {
            let url = format!("http://cache.nixos.org/{}.narinfo", output.to_str());
            client.get(&url)
                  .send()
                  .and_then(|res| res.into_body().concat2().from_err())
        })
        .buffer_unordered(128)
        .map(|bytes| {
            let b: &[u8] = bytes.as_ref();
            NarInfo::from(b)
        })
        .collect();

    let mut runtime = Runtime::new().expect("Unable to initialise tokio runtime");
    runtime.block_on(narinfos).unwrap_or_default()
}

fn format_bytes(amount: u64) -> String {
    match binary_prefix(amount as f64) {
        Standalone(bytes) =>   format!("{} bytes", bytes),
        Prefixed(prefix, n) => format!("{:.2} {}B", n, prefix)
    }
}

fn print_statistics(narinfos: Vec<Option<NarInfo>>) {
    println!("Fetched {} .narinfos", narinfos.len());

    let total = narinfos.len();
    let mut found = 0;
    let mut file_size = 0;
    let mut nar_size = 0;
    for narinfo in narinfos {
        if let Some(narinfo) = narinfo {
            found += 1;
            file_size += narinfo.file_size;
            nar_size += narinfo.nar_size;
        }
    }

    println!("{}/{} ({:.2}%) outputs are available",
             found, total,
             100. * found as f32 / total as f32);

    println!("{} of Nix archives (compressed)", format_bytes(file_size));
    println!("{} of Nix archives (uncompressed)", format_bytes(nar_size));
}

fn main() {
    let args: Vec<_> = env::args().collect();

    let input_path = fs::canonicalize(&args[1]).expect("Unable to canonicalize path argument");
    let input_hash = StoreHash::from_path(&input_path);
    let build_time_closure = gather_closure(input_hash, &read_drv(&input_path));
    println!("done({}), gathered {} paths", args[1], build_time_closure.len());

    let build_time_outputs = build_time_closure.into_iter()
        .flat_map(|(_hash, drv)| drv.outputs)
        .map(|output| StoreHash::from_path(&output.path));
    let narinfos = fetch_narinfo(build_time_outputs);

    print_statistics(narinfos);
}
