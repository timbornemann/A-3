#![allow(dead_code)]

/// Public model used by the adapter fixture.
pub struct Model {
    pub value: u32,
    hidden: bool,
}

pub enum State {
    Ready,
    Failed { code: u8 },
}

pub trait Runner: Send + Sync {
    fn run(&self) -> u32;
}

impl Runner for Model {
    fn run(&self) -> u32 {
        helper(self.value)
    }
}

pub type Count = u32;
pub const LIMIT: Count = 4;
pub static LABEL: &str = "fixture";

pub mod nested {
    pub fn helper() {}
}

pub mod external;
pub use nested::helper as exported_helper;

fn helper(value: u32) -> u32 {
    value
}

#[test]
fn model_runs() {
    exported_helper();
    assert_eq!(helper(1), 1);
}

fn main() {
    let model = Model {
        value: LIMIT,
        hidden: false,
    };
    let result = model.run();
    println!("{}", result);
}
