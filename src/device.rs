use crate::rstr::RStr;

#[derive(Debug)]
pub struct Device<'a> {
    pub devpath: RStr<'a>,
    pub subsystem: Option<RStr<'a>>,
    pub devname: Option<RStr<'a>>,
    pub devtype: Option<RStr<'a>>,
    pub bus_num: Option<RStr<'a>>,
    pub devnum: Option<libc::dev_t>,
    pub driver: Option<RStr<'a>>,
    pub seqnum: Option<u64>,
}

impl<'a> Device<'a> {
    pub(crate) fn new(
        devpath: RStr<'a>,
        subsystem: Option<RStr<'a>>,
        devname: Option<RStr<'a>>,
        devtype: Option<RStr<'a>>,
        bus_num: Option<RStr<'a>>,
        devnum: Option<libc::dev_t>,
        driver: Option<RStr<'a>>,
        seqnum: Option<u64>,
    ) -> Self {
        Self {
            devpath,
            subsystem,
            devname,
            devtype,
            bus_num,
            devnum,
            driver,
            seqnum,
        }
    }
}
