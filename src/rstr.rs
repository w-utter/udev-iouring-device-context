use std::ffi::{CStr, OsStr};

// the strings that we get are delimited by null terminating characters
// so they can be represented as cstrs if we so choose, but its also beneficial
// to have their length on hand, so we have this mixed representation
#[derive(Debug, Clone, Copy)]
pub struct RStr<'a> {
    inner: &'a str,
}

impl<'a> RStr<'a> {
    pub(crate) fn new(inner: &'a str) -> Self {
        Self { inner }
    }

    pub fn as_os_str(&self) -> &'a OsStr {
        OsStr::new(self.inner)
    }

    pub fn as_c_str(&self) -> &'a CStr {
        unsafe { CStr::from_ptr(self.inner.as_ptr().cast()) }
    }

    pub(crate) fn as_str(&self) -> &'a str {
        self.inner
    }
}
