use crate::device::Device;
use crate::rstr::RStr;
use std::path::Path;
use udev::Device as UDev;

macro_rules! utf8_unchecked {
    ($e:expr) => {
        unsafe { core::str::from_utf8_unchecked($e) }
    };
}

#[derive(Debug)]
pub(crate) struct RawDev<'a> {
    action: Option<Action>,
    devpath: Option<RStr<'a>>,
    subsystem: Option<RStr<'a>>,
    devname: Option<RStr<'a>>,
    devtype: Option<RStr<'a>>,
    bus_num: Option<RStr<'a>>,
    devnum: Option<libc::dev_t>,
    driver: Option<RStr<'a>>,
    seqnum: Option<u64>,
}

impl<'a> RawDev<'a> {
    pub(crate) fn from_bytes(bytes: &'a [u8]) -> Option<Self> {
        //we go from &[u8] -> str -> &[u8] -> str
        //here because we cant use .find() on bytes,
        //but we need to be able to index into the array
        //(which str's dont support)
        //technically the magic of the netlink packet is not valid utf8
        //so we get ub, but we throw it away anyway so who cares
        let str = utf8_unchecked!(bytes);
        let begin = str.find("ACTION")?;

        let bytes = &bytes[begin..];
        let trunc = utf8_unchecked!(bytes);

        let pairs = trunc.split('\0');

        let mut action = None;
        let mut dev_path = None;
        let mut subsystem = None;
        let mut dev_name = None;
        let mut dev_type = None;
        let mut bus_num = None;
        let mut dev_num = None;
        let mut driver = None;
        let mut seqnum = None;

        for kv_pair in pairs {
            let mut kv = kv_pair.split('=');
            let (k, v) = match (kv.next(), kv.next()) {
                (Some(key), Some(value)) => (key, value),
                _ => continue,
            };

            let r_val = RStr::new(v);

            match k {
                "ACTION" => action = Some(Action::from_str(v)?),
                "DEVPATH" => dev_path = Some(r_val),
                "SUBSYSTEM" => subsystem = Some(r_val),
                "DEVNAME" => dev_name = Some(r_val),
                "DEVTYPE" => dev_type = Some(r_val),
                "BUSNUM" => bus_num = Some(r_val),
                "DEVNUM" => dev_num = Some(v.parse().ok()?),
                "DRIVER" => driver = Some(r_val),
                "SEQNUM" => seqnum = Some(v.parse().ok()?),
                _ => (),
            }
        }
        Some(Self {
            action,
            devpath: dev_path,
            subsystem,
            devname: dev_name,
            devtype: dev_type,
            bus_num,
            devnum: dev_num,
            driver,
            seqnum,
        })
    }

    pub(crate) fn parse_into_actual_device(self) -> Result<UDev, Option<Device<'a>>> {
        if !matches!(self.action.ok_or(None)?, Action::Remove | Action::Add) {
            //all of the changed and binding events we dont really care abt
            return Err(None);
        }

        let path = self.devpath.ok_or(None)?;
        let proper = path.as_str();

        let p = format!("/sys{proper}");
        let syspath = Path::new(&p);
        UDev::from_syspath(&syspath).map_err(|_| self.into_dev())
    }

    pub(crate) fn into_dev(self) -> Option<Device<'a>> {
        let Self {
            devpath,
            subsystem,
            devname,
            devtype,
            bus_num,
            devnum,
            driver,
            seqnum,
            ..
        } = self;

        let devpath = unsafe { devpath.unwrap_unchecked() };

        Some(Device::new(
            devpath, subsystem, devname, devtype, bus_num, devnum, driver, seqnum,
        ))
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Action {
    Add,
    Remove,
    //anything below this comment we dont really care about
    Change,
    Bind,
    Unbind,
}

impl Action {
    fn from_str(str: &str) -> Option<Self> {
        Some(match str {
            "remove" => Self::Remove,
            "add" => Self::Add,
            "change" => Self::Change,
            "bind" => Self::Bind,
            "unbind" => Self::Unbind,
            _ => return None,
        })
    }
}
