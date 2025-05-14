#[derive(Debug, Eq, PartialEq)]
pub struct Diff<'str> {
    pub pos: usize,
    pub remove: usize,
    pub add: &'str str,
}
