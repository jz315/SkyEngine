#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DrawFunctionId(usize);

impl DrawFunctionId {
    #[inline]
    pub const fn index(self) -> usize {
        self.0
    }

    #[inline]
    pub(crate) const fn from_raw(index: usize) -> Self {
        Self(index)
    }
}
