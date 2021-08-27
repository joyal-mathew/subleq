mod assembling;
mod subleq;

use std::fs;

fn main() {
    assembling::assemble(fs::read_to_string("main.sla").unwrap(), std::env::args().len() > 1).unwrap().run();
}
