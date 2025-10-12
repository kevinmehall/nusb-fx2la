use std::path::{Path, PathBuf};
use super::Error;

pub struct Model {
    pub vid: u16,
    pub pid: u16,
    pub description: &'static str,
    pub firmware_filename: &'static str,
}

pub static MODELS: &[Model] = &[
    Model {
        vid: 0x1d50,
        pid: 0x608c,
        description: "fx2lafw 8ch",
        firmware_filename: "fx2lafw-sigrok-fx2-8ch.fw",
    },
    Model {
        vid: 0x1d50,
        pid: 0x608d,
        description: "fx2lafw 16ch",
        firmware_filename: "fx2lafw-sigrok-fx2-16ch.fw",
    },
    Model {
        vid: 0x04b4,
        pid: 0x8613,
        description: "Cypress FX2",
        firmware_filename: "fx2lafw-cypress-fx2.fw",
    },
    Model {
        vid: 0x0925,
        pid: 0x3881,
        description: "Saleae Logic",
        firmware_filename: "fx2lafw-saleae-logic.fw",
    },
    Model {
        vid: 0x08a9,
        pid: 0x0015,
        description: "CWAV USBee DX",
        firmware_filename: "fx2lafw-cwav-usbeedx.fw",
    },
    Model {
        vid: 0x08a9,
        pid: 0x0009,
        description: "CWAV USBee SX",
        firmware_filename: "fx2lafw-cwav-usbeesx.fw",
    },
    Model {
        vid: 0x08a9,
        pid: 0x0005,
        description: "CWAV USBee ZX",
        firmware_filename: "fx2lafw-cwav-usbeezx.fw",
    },
    Model {
        vid: 0x16d0,
        pid: 0x0498,
        description: "Braintechnology USB-LPS",
        firmware_filename: "fx2lafw-braintechnology-usb-lps.fw",
    }
];

pub async fn get_firmware(filename: &str) -> Result<Vec<u8>, Error> {
    let paths: Vec<PathBuf> = [
        std::env::var_os("SIGROK_FIRMWARE_DIR").map(|dir| Path::new(&dir).join(filename)),
        std::env::current_exe().ok().and_then(|exe| exe.parent().map(|p| p.join("share/sigrok-firmware").join(filename))),
        option_env!("COMPILE_TIME_SIGROK_FIRMWARE_DIR").map(|dir| Path::new(dir).join(filename)),
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
