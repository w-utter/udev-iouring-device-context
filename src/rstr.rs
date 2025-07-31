use std::ffi::{CStr, OsStr};

// the strings that we get are delimited by null terminating characters
// so they can be represented as cstrs if we so choose, but its also beneficial
// to have their length on hand, so we have this mixed representation
#[derive(Clone, Copy)]
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

use core::fmt;
impl<'a> fmt::Debug for RStr<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}
