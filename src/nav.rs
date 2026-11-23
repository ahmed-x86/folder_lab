use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavTarget {
    Home,
    Directory(PathBuf),
}

pub struct NavState {
    history: Vec<NavTarget>,
    index: usize,
}

impl NavState {
    pub fn new(initial: NavTarget) -> Self {
        Self {
            history: vec![initial],
            index: 0,
        }
    }

    pub fn current(&self) -> &NavTarget {
        &self.history[self.index]
    }

    pub fn push(&mut self, target: NavTarget) {
        if self.current() == &target {
            return;
        }
        self.history.truncate(self.index + 1);
        self.history.push(target);
        self.index = self.history.len() - 1;
    }

    pub fn can_back(&self) -> bool {
        self.index > 0
    }

    pub fn can_forward(&self) -> bool {
        self.index + 1 < self.history.len()
    }

    pub fn go_back(&mut self) -> Option<&NavTarget> {
        if self.can_back() {
            self.index -= 1;
            Some(&self.history[self.index])
        } else {
            None
        }
    }

    pub fn go_forward(&mut self) -> Option<&NavTarget> {
        if self.can_forward() {
            self.index += 1;
            Some(&self.history[self.index])
        } else {
            None
        }
    }
}
