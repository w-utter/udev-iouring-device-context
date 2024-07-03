use io_uring::types::BufRing;
use io_uring::{IoUring, SubmissionQueue, Submitter};
use std::collections::HashMap;
use udev::{Enumerator, MonitorSocket};

use crate::ctx_builder::{fd_t, setup_device_listener};
use crate::err::Error;
use crate::ev::{DeviceEvent, Event, IoEvent};
use crate::initial_devices::InitialDevices;
use crate::raw_device::RawDev;
use std::os::fd::{AsRawFd, RawFd};

pub struct Ctx<T: AsRawFd> {
    devs: HashMap<fd_t, T>,
    ring: IoUring,
    _hp: MonitorSocket,
    hp_fd: RawFd,
    hp_br: BufRing,
    _enumerator: Enumerator,
    initial_devices: InitialDevices,
}

impl<T: AsRawFd> Ctx<T> {
    pub(crate) fn new(
        devs: HashMap<fd_t, T>,
        ring: IoUring,
        _hp: MonitorSocket,
        hp_fd: RawFd,
        hp_br: BufRing,
        _enumerator: Enumerator,
        initial_devices: InitialDevices,
    ) -> Self {
        Self {
            devs,
            ring,
            _hp,
            hp_fd,
            hp_br,
            _enumerator,
            initial_devices,
        }
    }

    pub fn step(&mut self) -> Option<Event<T>> {
        if let Some(dev) = self.initial_devices.next() {
            return Some(Event::Device(DeviceEvent::Added(dev)));
        }

        let completed = self.ring.completion().next()?;
        let udata = completed.user_data();

        if udata == u64::MAX {
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
        } else if let Some(dev) = self.devs.get_mut(&userdata_to_idx(udata)) {
            return Some(Event::Io(IoEvent::from_cqueue(dev, completed)));
        }
        None
    }

    pub fn add_device(&mut self, dev: T) -> Result<Option<T>, Error>
    where
        T: AsRawFd,
    {
        let idx = fd_to_index(dev.as_raw_fd())?;
        Ok(self.devs.insert(idx, dev))
    }

    pub fn submission_queue(&mut self) -> SubmissionQueue {
        self.ring.submission()
    }

    pub fn submit(&mut self) -> Result<usize, Error> {
        Ok(self.ring.submit()?)
    }

    pub fn submitter(&mut self) -> Submitter {
        self.ring.submitter()
    }

    pub fn register_buffer(&self, buf: &BufRing) -> Result<(), Error> {
        self.ring.submitter().register_buffer_ring(buf)?;
        Ok(())
    }

    pub fn unregister_buffer(&self, buf: &BufRing) -> Result<(), Error> {
        self.ring.submitter().unregister_buf_ring(buf.bgid())?;
        Ok(())
    }

    pub fn remove_device(&mut self, fd: i32) -> Result<Option<T>, Error> {
        let idx = fd_to_index(fd)?;
        Ok(self.devs.remove(&idx))
    }

    pub fn get_device(&self, idx: &fd_t) -> Option<&T> {
        self.devs.get(idx)
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
