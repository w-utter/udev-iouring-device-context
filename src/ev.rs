use io_uring::cqueue::Entry as CQEntry;

pub enum Event<'a, T> {
    Device(DeviceEvent<'a>),
    Io(IoEvent<'a, T>),
}

pub struct IoEvent<'a, T> {
    pub dev: &'a mut T,
    pub userdata: u64,
    pub result: Result<IoEventOk, i32>,
}

pub struct IoEventOk {
    pub flags: u32,
    pub result: u32,
}

impl<'a, T> IoEvent<'a, T> {
    pub(crate) fn from_cqueue(dev: &'a mut T, cq: CQEntry) -> Self {
        let flags = cq.flags();
        let userdata = cq.user_data();
        let res = cq.result();
        let result = if res < 0 {
            Err(res)
        } else {
            Ok(IoEventOk {
                flags,
                result: res as u32,
            })
        };

        Self {
            userdata,
            result,
            dev,
        }
    }

    pub fn errored_gracefully(&self) -> bool {
        if let Err(e) = self.result {
            matches!(-e, libc::EINTR | libc::ETIME | libc::ENOBUFS)
        } else {
            false
        }
    }

    pub fn errored(&self) -> bool {
        self.result.is_err()
    }
}

use u_dev::device::{instance, origin};
use u_dev::hotplug::Device as AddedDevice;
use u_dev::Device;

#[derive(Debug)]
pub enum DeviceEvent<'a> {
    Added(AddedDevice<'a>),
    Removed(Device<instance::Borrowed<'a>, origin::Hotplug>),
}
