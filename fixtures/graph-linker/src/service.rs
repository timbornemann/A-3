pub struct Service;

impl Service {
    pub fn run(&self) -> usize {
        helper()
    }
}

pub fn start() -> usize {
    helper()
}

fn helper() -> usize {
    1
}

#[cfg(test)]
mod tests {
    #[test]
    fn service_runs() {
        assert_eq!(super::helper(), 1);
    }
}
