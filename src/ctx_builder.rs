use core::marker::PhantomData;
use io_uring::opcode::RecvMulti;
use io_uring::types::{BufRing, Fd};
use io_uring::IoUring;
use std::collections::{BTreeMap, HashMap};
use std::ffi::OsStr;
use udev::{Enumerator, MonitorBuilder};

use crate::ctx::{Ctx, MioCtx, Userdata};
use crate::err::Error;
use crate::initial_devices::InitialDevices;
use std::os::fd::{AsRawFd, RawFd};

#[allow(non_camel_case_types)]
pub(crate) type fd_t = u32;

pub struct CtxBuilder<T: AsRawFd> {
    ring: IoUring,
    hp: MonitorBuilder,
    mio: Option<(mio::Poll, usize)>,
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
            mio: None,
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

    pub fn register_mio(
        mut self,
        f: impl FnOnce(&mut mio::Poll) -> std::io::Result<()>,
        events_capacity: usize,
    ) -> std::io::Result<Self> {
        let mut poll = mio::Poll::new()?;
        f(&mut poll)?;
        self.mio = Some((poll, events_capacity));
        Ok(self)
    }

    pub fn build(self) -> Result<Ctx<T>, Error> {
        let CtxBuilder {
            mut ring,
            hp,
            enumerator,
            mio,
            ..
        } = self;

        let procs = HashMap::new();
        let devs = BTreeMap::new();

        let hp = hp.listen()?;
        let raw = hp.as_raw_fd();
        let mut buf = BufRing::new(128, 4096, 5).unwrap();

        buf.init();
        ring.submitter().register_buffer_ring(&buf).unwrap();
        buf.init_buffers();

        setup_device_listener(raw, &mut ring, &buf)?;
        let initial_devices = InitialDevices::new(&enumerator);

        let mio = setup_mio(mio, &mut ring)?;

        ring.submitter().submit()?;

        Ok(Ctx::new(
            devs,
            ring,
            hp,
            raw,
            buf,
            enumerator,
            initial_devices,
            procs,
            mio,
        ))
    }
}

pub(crate) fn setup_device_listener(
    fd: RawFd,
    ring: &mut IoUring,
    buf: &BufRing,
) -> Result<(), Error> {
    let recv_multi = RecvMulti::new(Fd(fd), buf.bgid())
        .build()
        .user_data(Userdata::DEVICE_EVENT);

    unsafe {
        ring.submission().push(&recv_multi)?;
    }

    Ok(())
}

pub(crate) fn setup_mio(
    mio: Option<(mio::Poll, usize)>,
    ring: &mut IoUring,
) -> Result<Option<MioCtx>, Error> {
    Ok(if let Some((m, cap)) = mio {
        let poll = io_uring::opcode::PollAdd::new(Fd(m.as_raw_fd()), libc::POLLIN as _)
            .multi(true)
            .build()
            .user_data(Userdata::MIO_EVENT);
        unsafe {
            ring.submission().push(&poll)?;
        }

        Some(MioCtx {
            inner: m,
            events: mio::Events::with_capacity(cap),
            ev_idx: 0,
        })
    } else {
        None
    })
}
