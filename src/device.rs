use u_dev::Udev;

#[allow(non_camel_case_types)]
pub(crate) type unique_dev_t<'a> = &'a std::ffi::OsStr;

pub trait UniqueDevice: private::Sealed {
    fn idx(&self, ctx: &Udev) -> unique_dev_t;
}

mod private {
    pub trait Sealed {}
}

impl private::Sealed for u_dev::hotplug::Device<'_> {}

impl UniqueDevice for u_dev::hotplug::Device<'_> {
    fn idx(&self, ctx: &Udev) -> unique_dev_t {
        self.devpath(ctx).as_os_str()
    }
}

impl<D: u_dev::device::DevImpl, K: u_dev::device::Extra<D>> private::Sealed
    for u_dev::Device<D, K>
{
}

impl<D: u_dev::device::DevImpl, K: u_dev::device::Extra<D>> UniqueDevice for u_dev::Device<D, K> {
    fn idx(&self, ctx: &Udev) -> unique_dev_t {
        self.devpath(ctx).as_os_str()
    }
}
