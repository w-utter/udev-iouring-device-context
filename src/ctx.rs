use io_uring::types::BufRing;
use io_uring::{IoUring, SubmissionQueue, Submitter};
use std::collections::{HashMap, BTreeMap};
use udev::{Enumerator, MonitorSocket};
use std::ffi::OsString;

use crate::ctx_builder::{fd_t, setup_device_listener};
use crate::device::{unique_dev_t, UniqueDevice};
use crate::err::Error;
use crate::ev::{DeviceEvent, Event, IoEvent};
use crate::initial_devices::InitialDevices;
use crate::raw_device::RawDev;
use std::os::fd::{AsRawFd, RawFd};

pub struct Ctx<T: AsRawFd> {
    procs: HashMap<fd_t, T>,
    devs: BTreeMap<OsString, i32>,
    ring: IoUring,
    _hp: MonitorSocket,
    hp_fd: RawFd,
    hp_br: BufRing,
    mio: Option<MioCtx>,
    _enumerator: Enumerator,
    initial_devices: InitialDevices,
}

impl<T: AsRawFd> Ctx<T> {
    pub(crate) fn new(
        devs: BTreeMap<OsString, i32>,
        ring: IoUring,
        _hp: MonitorSocket,
        hp_fd: RawFd,
        hp_br: BufRing,
        _enumerator: Enumerator,
        initial_devices: InitialDevices,
        procs: HashMap<fd_t, T>,
        mio: Option<MioCtx>,
    ) -> Self {
        Self {
            devs,
            ring,
            _hp,
            hp_fd,
            hp_br,
            _enumerator,
            initial_devices,
            procs,
            mio,
        }
    }

    pub fn step(&mut self) -> Option<Event<T>> {
        if let Some(dev) = self.initial_devices.next() {
            return Some(Event::Device(DeviceEvent::Added(dev)));
        }

        match &mut self.mio {
            Some(m) if m.ev_idx > 0 => {
                if let Some(ev) = m.events.iter().nth(m.ev_idx) {
                    m.ev_idx += 1;
                    return Some(Event::Mio(ev))
                }
                m.ev_idx = 0;
                m.events.clear();
            }
            _ => (),
        }

        let completed = self.ring.completion().next()?;
        let udata = completed.user_data();

        match Userdata::from_raw(udata) {
            Userdata::Device => {
                unsafe { self.hp_br.advance(1) }
                let len = if completed.result() > 0 {
                    completed.result() as usize
                } else {
                    println!("erred on step with: {completed:?}\nrestarting listener");
                    setup_device_listener(self.hp_fd, &mut self.ring, &self.hp_br).unwrap();
                    return None;
                };

                if let Some(id) = self.hp_br.buffer_id_from_cqe_flags(completed.flags()) {
                    let read_buf = unsafe { self.hp_br.read_buffer(id) };
                    let bytes = &read_buf[..len];

                    if let Some(dev) = RawDev::from_bytes(bytes) {
                        match dev.parse_into_actual_device() {
                            Ok(dev) => {
                                return Some(Event::Device(DeviceEvent::Added(dev)));
                            }
                            Err(partial) => {
                                //whenever a device is removed we cannot preform a lookup on it to get
                                //more information, so we cant turn it into an actual udev device
                                let removed = partial?;
                                return Some(Event::Device(DeviceEvent::Removed(removed)));
                            }
                        }
                    }
                }
            }
            Userdata::Mio => {
                if let Some(m) = &mut self.mio {
                    let _ = m.inner.poll(&mut m.events, Some(std::time::Duration::ZERO));
                    
                    if let Some(ev) = m.events.iter().next() {
                        m.ev_idx = 1;
                        return Some(Event::Mio(ev))
                    }
                }
            }
            Userdata::User(u) => {
                if let Some(dev) = self.procs.get_mut(&(u as fd_t)) {
                    return Some(Event::Io(IoEvent::from_cqueue(dev, completed)));
                }
            }
        }
        None
    }

    pub fn add_device(&mut self, unique: &impl UniqueDevice, dev: T) -> Result<Option<T>, Error>
    where
        T: AsRawFd,
    {
        let idx = unique.idx().to_owned();
        let fd = dev.as_raw_fd();

        if let Some(_) = self.devs.insert(idx, fd) {
            return Ok(Some(dev));
        }

        self.add_process(dev)
    }

    pub fn remove_device(&mut self, unique: &impl UniqueDevice) -> Result<Option<T>, Error> {
        unsafe {
            self.remove_device_with_id(unique.idx())
        }
    }

    pub unsafe fn remove_device_with_id(&mut self, id: unique_dev_t) -> Result<Option<T>, Error> {
        let fd = match self.devs.remove(id) {
            Some(fd) => fd,
            _ => return Ok(None),
        };

        self.remove_process(fd)
    }

    pub fn add_process(&mut self, proc: T) -> Result<Option<T>, Error>
    where
        T: AsRawFd,
    {
        let idx = fd_to_index(proc.as_raw_fd())?;
        Ok(self.procs.insert(idx, proc))
    }

    fn remove_process(&mut self, fd: i32) -> Result<Option<T>, Error> {
        let idx = fd_to_index(fd)?;
        Ok(self.procs.remove(&idx))
    }

    pub fn submission_queue(&mut self) -> SubmissionQueue {
        self.ring.submission()
    }

    pub fn submit(&mut self) -> Result<usize, Error> {
        Ok(self.ring.submit()?)
    }

    pub fn submitter(&self) -> Submitter {
        self.ring.submitter()
    }

    pub fn register_buffer(&self, buf: &BufRing) -> Result<(), Error> {
        self.ring.submitter().register_buffer_ring(buf)?;
        Ok(())
    }

    pub fn with_io_ctx<U, E>(
        &mut self,
        f: impl FnOnce(&mut IoUring) -> Result<U, E>,
    ) -> Result<U, E> {
        let ring = &mut self.ring;
        f(ring)
    }

    pub fn unregister_buffer(&self, buf: &BufRing) -> Result<(), Error> {
        self.ring.submitter().unregister_buf_ring(buf.bgid())?;
        Ok(())
    }

    pub fn get_process(&self, idx: &fd_t) -> Option<&T> {
        self.procs.get(idx)
    }

    pub fn get_process_mut(&mut self, idx: &fd_t) -> Option<&mut T> {
        self.procs.get_mut(idx)
    }
}

fn fd_to_index(fd: i32) -> Result<fd_t, Error> {
    let min = fd_t::MIN as i32;
    if fd < min {
        Err(Error::from_errno(fd))
    } else {
        Ok(fd as fd_t)
    }
}

pub(crate) struct MioCtx {
    pub(crate) inner: mio::Poll,
    pub(crate) events: mio::Events,
    pub(crate) ev_idx: usize,
}

pub(crate) enum Userdata {
    Device,
    Mio,
    User(u64),
}

impl Userdata {
    pub(crate) const DEVICE_EVENT: u64 = u64::MAX;
    pub(crate) const MIO_EVENT: u64 = u64::MAX ^ (1 << 63);

    fn from_raw(raw: u64) -> Self {
        match raw {
            Self::DEVICE_EVENT => Self::Device,
            Self::MIO_EVENT => Self::Mio,
            u => Self::User(u),
        }
    }
}
