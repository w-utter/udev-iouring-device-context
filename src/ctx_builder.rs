use core::marker::PhantomData;
use io_uring::opcode::RecvMulti;
use io_uring::types::Fd;
use io_uring::IoUring;
use io_uring_buf_ring::{buf_ring_state, BufRing};
use std::collections::{BTreeMap, HashMap};
use std::ffi::OsStr;

use crate::ctx::Ctx;
use crate::err::Error;
use std::os::fd::AsRawFd;

use u_dev::{socket_state, EventSource, Monitor, Udev};

#[allow(non_camel_case_types)]
pub(crate) type fd_t = u32;

pub struct CtxBuilder<'a, 'b, T: AsRawFd> {
    ring: IoUring,
    _pd: PhantomData<T>,
    monitor: Monitor<'a, 'b, socket_state::Initalizing>,
    udev: &'b Udev,
}

impl<'a, 'b, T: AsRawFd> CtxBuilder<'a, 'b, T> {
    pub fn new(io_entries: u32, udev: &'b Udev) -> Result<Self, Error> {
        let io_entries = io_entries.next_power_of_two();
        let ring = IoUring::new(io_entries)?;
        let mut monitor = Monitor::new()?;
        monitor.enumerate(udev);

        Ok(Self {
            ring,
            _pd: PhantomData,
            monitor,
            udev,
        })
    }

    pub fn match_subsystem<P: Into<std::borrow::Cow<'a, OsStr>>>(
        mut self,
        subsystem: P,
    ) -> Result<Self, Error> {
        let subsystem = subsystem.into();
        self.monitor.match_subsystem(subsystem)?;
        Ok(self)
    }

    pub fn match_subsystems<P: Into<std::borrow::Cow<'a, OsStr>>>(
        mut self,
        subsystems: impl Iterator<Item = P>,
    ) -> Result<Self, Error> {
        for subsystem in subsystems {
            let subsystem = subsystem.into();
            self.monitor.match_subsystem(subsystem)?;
        }
        Ok(self)
    }

    pub fn build(self, buf_id: &mut u16) -> Result<Ctx<'a, 'b, T>, Error> {
        let CtxBuilder {
            mut ring,
            monitor,
            udev,
            ..
        } = self;

        let procs = HashMap::new();
        let devs = BTreeMap::new();

        let monitor = monitor.listen(Some(EventSource::Kernel))?;

        let buf = BufRing::new(128, 4096, 0).unwrap();
        let buf = initalize_device_listener(&monitor, &mut ring, buf_id, buf)?;

        Ok(Ctx::new(devs, ring, buf, procs, monitor, udev))
    }
}

pub(crate) fn register_buf_ring(
    ring: &IoUring,
    buf: BufRing<buf_ring_state::Uninit>,
    buf_id: &mut u16,
) -> Result<BufRing<buf_ring_state::Init>, Error> {
    let mut res = buf.register(&ring.submitter());

    while let Err((e, mut buf)) = res {
        if let std::io::ErrorKind::AlreadyExists = e.kind() {
            *buf_id += 1;
            buf.set_bgid(*buf_id);
            res = buf.register(&ring.submitter());
        } else {
            return Err(e.into());
        }
    }
    let buf = unsafe { res.unwrap_unchecked() }.init();

    Ok(buf)
}

fn initalize_device_listener(
    monitor: &Monitor<'_, '_, socket_state::Listening>,
    ring: &mut IoUring,
    buf_id: &mut u16,
    buf: BufRing<buf_ring_state::Uninit>,
) -> Result<BufRing<buf_ring_state::Init>, Error> {
    let buf = register_buf_ring(ring, buf, buf_id)?;

    setup_device_listener(monitor, ring, &buf)?;
    Ok(buf)
}

pub(crate) fn setup_device_listener(
    monitor: &Monitor<'_, '_, socket_state::Listening>,
    ring: &mut IoUring,
    buf: &BufRing<buf_ring_state::Init>,
) -> Result<(), Error> {
    let fd = monitor.as_raw_fd();
    let recv_multi = RecvMulti::new(Fd(fd), buf.bgid())
        .build()
        .user_data(u64::MAX);

    unsafe {
        ring.submission().push(&recv_multi)?;
    }

    ring.submitter().submit()?;
    Ok(())
}
