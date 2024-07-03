use libudev_sys::{
    udev_enumerate_get_list_entry, udev_enumerate_scan_devices, udev_list_entry,
    udev_list_entry_get_name, udev_list_entry_get_next,
};
use std::ffi::{CStr, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use udev::{AsRawWithContext, Device};

// this is what we use to enumerate currently existing devices so that we dont need to explicitly call .devices() on startup.
//
// that being said, we have to use a raw pointer here, because as soon as we add a lifetime to the
// list entry, the lifetime of the reference will conflict with the lifetime of the struct that we
// store it in.

pub(crate) struct InitialDevices {
    entry: *mut udev_list_entry,
}

impl InitialDevices {
    pub(crate) fn new(ctx: &udev::Enumerator) -> Self {
        let enumerator = ctx.as_raw();

        let entry = unsafe {
            udev_enumerate_scan_devices(enumerator);
            udev_enumerate_get_list_entry(enumerator)
        };

        Self { entry }
    }
}

impl Iterator for InitialDevices {
    type Item = Device;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.entry.is_null() {
            let syspath = unsafe {
                Path::new(OsStr::from_bytes(
                    CStr::from_ptr(udev_list_entry_get_name(self.entry)).to_bytes(),
                ))
            };

            self.entry = unsafe { udev_list_entry_get_next(self.entry) };

            match Device::from_syspath(syspath) {
                Ok(d) => return Some(d),
                _ => continue,
            }
        }
        None
    }
}
