pub mod derivation;

use std::{
    env, fs,
    collections::HashMap
};

use crate::derivation::*;

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

fn main() {
    let args: Vec<_> = env::args().collect();

    let closure = gather_closure(args[1].clone(), &read_drv(&args[1]));
    println!("done, gathered {} paths", closure.len());
}
