/// HTTP response bytes whose ownership matches their lifetime.
#[derive(Debug)]
pub(crate) enum ResponseBody {
    Static(&'static [u8]),
    Owned(Vec<u8>),
}

impl ResponseBody {
    pub(crate) fn as_slice(&self) -> &[u8] {
        match self {
            Self::Static(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.as_slice().len()
    }
}
