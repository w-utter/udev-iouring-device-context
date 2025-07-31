use io_uring::{IoUring, SubmissionQueue, Submitter};
use io_uring_buf_ring::{buf_ring_state, BufRing};
use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use udev::{Enumerator, MonitorSocket};

use crate::ctx_builder::{fd_t, register_buf_ring, setup_device_listener};
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
    hp_br: BufRing<buf_ring_state::Init>,
    _enumerator: Enumerator,
    initial_devices: InitialDevices,
}

impl<T: AsRawFd> Ctx<T> {
    pub(crate) fn new(
        devs: BTreeMap<OsString, i32>,
        ring: IoUring,
        _hp: MonitorSocket,
        hp_fd: RawFd,
        hp_br: BufRing<buf_ring_state::Init>,
        _enumerator: Enumerator,
        initial_devices: InitialDevices,
        procs: HashMap<fd_t, T>,
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
        }
    }

    pub fn step(&mut self) -> Option<Event<T>> {
        if let Some(dev) = self.initial_devices.next() {
            return Some(Event::Device(DeviceEvent::Added(dev)));
        }

        let completed = self.ring.completion().next()?;
        let udata = completed.user_data();

        if udata == u64::MAX {
            let res = self.hp_br.buffer_id_from_cqe(&completed);
            let buf_entry = match res {
                Err(ref e) => {
                    println!("erred ({e:?}) on step with: {completed:?}\nrestarting listener");
                    drop(res);
                    setup_device_listener(self.hp_fd, &mut self.ring, &self.hp_br).unwrap();
                    return None;
                }
                Ok(buf_entry) => buf_entry?,
            };

            // SAFETY: the buffer will outlive the function scope as it is
            // memory mapped.
            let buf = unsafe { &*(buf_entry.buffer() as *const _) };

            let raw_dev = RawDev::from_bytes(buf)?;

            match raw_dev.parse_into_actual_device() {
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
        } else if let Some(dev) = self.procs.get_mut(&userdata_to_idx(udata)) {
            return Some(Event::Io(IoEvent::from_cqueue(dev, completed)));
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
        unsafe { self.remove_device_with_id(unique.idx()) }
    }

    pub unsafe fn remove_device_with_id(&mut self, id: unique_dev_t) -> Result<Option<T>, Error> {
        let fd = match self.devs.remove(id) {
            Some(fd) => fd,
            _ => return Ok(None),
        };

        self.remove_process(fd)
    }

    pub fn register_buffer(
        &self,
        buf: BufRing<buf_ring_state::Uninit>,
        buf_id: &mut u16,
    ) -> Result<BufRing<buf_ring_state::Init>, Error> {
        register_buf_ring(&self.ring, buf, buf_id)
    }

    pub fn unregister_buffer(
        &self,
        buf: BufRing<buf_ring_state::Init>,
    ) -> Result<BufRing<buf_ring_state::Uninit>, (Error, BufRing<buf_ring_state::Init>)> {
        buf.unregister(&self.ring.submitter())
            .map_err(|(err, s)| (err.into(), s))
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

    pub fn with_io_ctx<U, E>(
        &mut self,
        f: impl FnOnce(&mut IoUring) -> Result<U, E>,
    ) -> Result<U, E> {
        let ring = &mut self.ring;
        f(ring)
    }

    pub fn submit_entry(&mut self, entry: &io_uring::squeue::Entry) -> Result<(), Error> {
        unsafe {
            self.ring.submission().push(&entry)?;
        }
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

fn userdata_to_idx(userdata: u64) -> fd_t {
    userdata as fd_t
}
