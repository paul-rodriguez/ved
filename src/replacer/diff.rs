#[derive(Ord, PartialOrd, Debug, Eq, PartialEq)]
pub struct Diff<'str> {
    /// The offset of the diff with the start of the file
    ///
    /// This field must be the first field of the class because it is
    /// important that the derived implementation of Ord use this field as
    /// the first one in the comparison.
    pub pos: usize,
    /// The number of characters to remove
    pub remove: usize,
    /// The string to add
    pub add: &'str str,
}
/*
impl<'str> Ord for Diff<'str> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.pos.cmp(other.pos)
    }
}

impl<'str> PartialOrd for Diff<'str> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        todo!()
    }
}*/
