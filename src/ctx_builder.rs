use core::marker::PhantomData;
use io_uring::opcode::RecvMulti;
use io_uring::types::Fd;
use io_uring::IoUring;
use io_uring_buf_ring::{buf_ring_state, BufRing};
use std::collections::{BTreeMap, HashMap};
use std::ffi::OsStr;
use udev::{Enumerator, MonitorBuilder};

use crate::ctx::Ctx;
use crate::err::Error;
use crate::initial_devices::InitialDevices;
use std::os::fd::{AsRawFd, RawFd};

#[allow(non_camel_case_types)]
pub(crate) type fd_t = u32;

pub struct CtxBuilder<T: AsRawFd> {
    ring: IoUring,
    hp: MonitorBuilder,
    enumerator: Enumerator,
    _pd: PhantomData<T>,
}

impl<T: AsRawFd> CtxBuilder<T> {
    pub fn new(io_entries: u32) -> Result<Self, Error> {
        let io_entries = io_entries.next_power_of_two();
        let ring = IoUring::new(io_entries)?;
        let hp = MonitorBuilder::new()?;
        let enumerator = Enumerator::new()?;

        Ok(Self {
            ring,
            hp,
            enumerator,
            _pd: PhantomData,
        })
    }

    pub fn match_subsystems<P: AsRef<OsStr>>(
        mut self,
        sub_systems: impl Iterator<Item = P>,
    ) -> Result<Self, Error> {
        for subsystem in sub_systems {
            let subsystem = subsystem.as_ref();
            self.hp = self.hp.match_subsystem(subsystem)?;
            self.enumerator.match_subsystem(subsystem)?;
        }
        Ok(self)
    }

    pub fn build(self, buf_id: &mut u16) -> Result<Ctx<T>, Error> {
        let CtxBuilder {
            mut ring,
            hp,
            enumerator,
            ..
        } = self;

        let procs = HashMap::new();
        let devs = BTreeMap::new();

        let hp = hp.listen()?;
        let raw = hp.as_raw_fd();
        let buf = BufRing::new(128, 4096, 0).unwrap();

        let buf = initalize_device_listener(raw, &mut ring, buf_id, buf)?;
        let initial_devices = InitialDevices::new(&enumerator);

        Ok(Ctx::new(
            devs,
            ring,
            hp,
            raw,
            buf,
            enumerator,
            initial_devices,
            procs,
        ))
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
    fd: RawFd,
    ring: &mut IoUring,
    buf_id: &mut u16,
    buf: BufRing<buf_ring_state::Uninit>,
) -> Result<BufRing<buf_ring_state::Init>, Error> {
    let buf = register_buf_ring(ring, buf, buf_id)?;

    setup_device_listener(fd, ring, &buf)?;
    Ok(buf)
}

pub(crate) fn setup_device_listener(
    fd: RawFd,
    ring: &mut IoUring,
    buf: &BufRing<buf_ring_state::Init>,
) -> Result<(), Error> {
    let recv_multi = RecvMulti::new(Fd(fd), buf.bgid())
        .build()
        .user_data(u64::MAX);

    unsafe {
        ring.submission().push(&recv_multi)?;
    }

    ring.submitter().submit()?;
    Ok(())
}
