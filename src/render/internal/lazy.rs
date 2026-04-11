pub(crate) struct LazyNodeResources<R> {
    inner: Option<R>,
}

impl<R> LazyNodeResources<R> {
    #[inline]
    pub(crate) fn new() -> Self {
        Self { inner: None }
    }

    #[inline]
    pub(crate) fn initialize_with(&mut self, init: impl FnOnce() -> R) {
        if self.inner.is_none() {
            self.inner = Some(init());
        }
    }

    #[inline]
    pub(crate) fn get_or_init(&mut self, init: impl FnOnce() -> R) -> &mut R {
        self.inner.get_or_insert_with(init)
    }
}

impl<R> Default for LazyNodeResources<R> {
    fn default() -> Self {
        Self::new()
    }
}
