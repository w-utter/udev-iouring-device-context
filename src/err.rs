use io_uring::squeue::PushError as DevErr;
use std::io::Error as IoErr;

#[derive(Debug)]
pub enum Error {
    Os(IoErr),
    Dev(DevErr),
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

impl From<DevErr> for Error {
    fn from(d: DevErr) -> Error {
        Error::Dev(d)
    }
}

use std::fmt::{Display, Formatter, Result};

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Self::Os(io) => io.fmt(f),
            Self::Dev(d) => d.fmt(f),
        }
    }
}
