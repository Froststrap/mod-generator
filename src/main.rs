use mod_generator::data_types::{Bootstrapper,FontDir};

pub fn main() {
    println!("Hello!");
    println!("FontDir={:?}", FontDir::get(Bootstrapper::Froststrap, Some("bumtimks".into())).unwrap())
}
