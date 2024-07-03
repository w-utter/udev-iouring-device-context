use std::io::Error as IoErr;
pub enum Error {
    Os(IoErr)
}

impl Error {
    pub(crate) fn from_errno(errno: i32) -> Self {
        let io = IoErr::from_raw_os_error(-errno);
        Self::Os(io)
    }
}

impl From<IoErr> for Error {
    fn from(io: IoErr) -> Error {
        Error::Os(io)
    }
}

use std::fmt::{Display, Formatter, Result};

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<()> {
        match self {
            Self::Os(io) => io.fmt(f),
        }
    }
}
