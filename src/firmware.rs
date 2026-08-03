use std::path::{Path, PathBuf};
use super::Error;

pub struct Model {
    pub vid: u16,
    pub pid: u16,
    pub description: &'static str,
    pub firmware_filename: &'static str,
}

impl Model {
    pub fn matches(&self, device: &nusb::DeviceInfo) -> bool {
        device.vendor_id() == self.vid && device.product_id() == self.pid
    }
}

macro_rules! models {
    ($(($vid:expr, $pid:expr, $desc:expr, $fw:expr)),* $(,)?) => {
        pub static MODELS: &[Model] = &[
            $(Model {
                vid: $vid,
                pid: $pid,
                description: $desc,
                firmware_filename: $fw,
            }),*
        ];

        pub const SELECTORS: &[nusb::DeviceSelector] = &[
            $(nusb::DeviceSelector::all().with_vid_pid($vid, $pid)),*
        ];
    };
}

models!(
    (0x1d50, 0x608c, "FX2-based Logic Analyzer", "fx2lafw-sigrok-fx2-8ch.fw"),
    (0x1d50, 0x608d, "FX2-based Logic Analyzer", "fx2lafw-sigrok-fx2-16ch.fw"),
    (0x04b4, 0x8613, "Cypress FX2", "fx2lafw-cypress-fx2.fw"),
    (0x0925, 0x3881, "Saleae Logic", "fx2lafw-saleae-logic.fw"),
    (0x08a9, 0x0015, "CWAV USBee DX", "fx2lafw-cwav-usbeedx.fw"),
    (0x08a9, 0x0009, "CWAV USBee SX", "fx2lafw-cwav-usbeesx.fw"),
    (0x08a9, 0x0005, "CWAV USBee ZX", "fx2lafw-cwav-usbeezx.fw"),
    (0x16d0, 0x0498, "Braintechnology USB-LPS", "fx2lafw-braintechnology-usb-lps.fw"),
);

pub async fn get_firmware(filename: &str) -> Result<Vec<u8>, Error> {
    let paths: Vec<PathBuf> = [
        std::env::var_os("FX2LAFW_FIRMWARE_DIR").map(|dir| Path::new(&dir).join(filename)),
        std::env::current_exe().ok().and_then(|exe| exe.parent().map(|p| p.join("share/sigrok-firmware").join(filename))),
        option_env!("COMPILE_TIME_FX2LAFW_FIRMWARE_DIR").map(|dir| Path::new(dir).join(filename)),
        Some(Path::new("/usr/local/share/sigrok-firmware").join(filename)),
        Some(Path::new("/usr/share/sigrok-firmware").join(filename)),
    ].into_iter().flatten().collect();

    for path in &paths {
        if let Ok(data) = async_fs::read(path).await {
            log::debug!("Loaded firmware from {:?}", path);
            return Ok(data);
        }
    }

    Err(Error::Other(format!("Could not find firmware, tried {:?}", paths)))
}
