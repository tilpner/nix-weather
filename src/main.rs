pub mod derivation;
pub mod narinfo;

use std::{
    env, fs,
    path::Path,
    collections::HashMap
};

use futures::{ stream, Future, Stream };
use tokio::runtime::Runtime;
use reqwest::r#async::Client;

use number_prefix::{ binary_prefix, Standalone, Prefixed };

use crate::{ derivation::*, narinfo::* };

fn read_drv(path: &str) -> Drv {
    let file_content = fs::read(path).expect("Unable to read derivation");
    let (rest, drv) = derivation::drv(&file_content).expect("Unable to parse derivation");
    assert!(rest.is_empty(), "Less than the entire drv was parsed");
    drv
}

fn gather_closure(name: String, drv: &Drv) -> HashMap<String, Drv> {
    fn add_to_closure(out: &mut HashMap<String, Drv>, name: String, drv: &Drv) {
        if out.contains_key(&name) { return; }
        out.insert(name, drv.clone());

        for InputDrv { path, .. } in &drv.input_drvs {
            // check cache to avoid unnecessary IO/parsing
            let input_drv = if out.contains_key(path) {
                out[path].clone()
            } else {
                read_drv(&path)
            };

            add_to_closure(out, path.clone(), &input_drv);
        }
    }

    let mut closure = HashMap::new();
    add_to_closure(&mut closure, name, &drv);
    closure
}

// This can parse as Path, then slice the filename,
// or it can assume /nix/store, which is faster
fn extract_hash_from_store_path(path: String) -> String {
    // use std::os::unix::ffi::OsStrExt;

    // let file_name = path.file_name().unwrap();
    
    // slice just the hash
    path[11..(11+32)].to_owned()
}

fn fetch_narinfo(closure: HashMap<String, Drv>) -> Vec<Option<NarInfo>> {
    let client = Client::new();

    let urls =
        closure.into_iter()
               .flat_map(|(path, drv)| drv.outputs)
               .map(|output| output.path)
               .map(extract_hash_from_store_path)
               .map(|hash| format!("http://cache.nixos.org/{}.narinfo", hash));

    let narinfos = stream::iter_ok(urls)
        .map(move |url| {
            client.get(&url)
                  .send()
                  .and_then(|res| res.into_body().concat2().from_err())
        })
        .buffer_unordered(128)
        .map(NarInfo::from)
        .collect();

    let mut runtime = Runtime::new().expect("Unable to initialise tokio runtime");
    runtime.block_on(narinfos).unwrap_or(Vec::new())
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

    let build_time_closure = gather_closure(args[1].clone(), &read_drv(&args[1]));
    // println!("done({}), gathered {} paths", args[1], closure.len());

    let narinfos = fetch_narinfo(closure);

    print_statistics(narinfos);

    /*
    let outputs: Vec<_> =
        closure.iter()
               .flat_map(|(path, drv)| drv.outputs.iter())
               .map(|output| &output.path)
               .collect();

    for o in outputs { println!("{:?}", o) }
    */
}
