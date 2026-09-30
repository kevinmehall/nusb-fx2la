use std::{ops::Deref, path::{Path, PathBuf}};
use nusb::DeviceInfo;

use super::Error;

/// Information about a device model compatible with a specific fx2lafw firmware.
#[derive(Copy, Clone, Debug)]
pub struct Model {
    pub vid: u16,
    pub pid: u16,
    pub description: &'static str,
    pub firmware_filename: &'static str,
}

impl Model {
    /// Test the VID:PID against a [`nusb::DeviceInfo`]
    pub fn matches(&self, device: &nusb::DeviceInfo) -> bool {
        device.vendor_id() == self.vid && device.product_id() == self.pid
    }
}

macro_rules! models {
    ($(($vid:expr, $pid:expr, $desc:expr, $fw:expr)),* $(,)?) => {
        /// List of supported device models.
        ///
        /// ## Included models
        $(
            #[doc = concat!(" - ", $desc, " (", stringify!($vid), ":", stringify!($pid), ")")]
        )*
        pub const MODELS: &[Model] = &[
            $(Model {
                vid: $vid,
                pid: $pid,
                description: $desc,
                firmware_filename: $fw,
            }),*
        ];

        /// `nusb` selectors for the devices in [`MODELS`].
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

/// Trait for choosing and obtaining firmware to load onto a device.
pub trait FirmwareProvider {
    type Bytes<'a>: Deref<Target = [u8]> where Self: 'a;

    /// Get the firmware for `device`
    fn get_firmware<'a>(&'a self, device: &DeviceInfo) -> impl Future<Output = Result<Self::Bytes<'a>, Error>> + Send + Sync;
}

/// Dummy implementation that fails if the firmware is not already loaded
impl FirmwareProvider for () {
    type Bytes<'a> = &'static [u8];

    async fn get_firmware<'a>(&'a self, _device: &DeviceInfo) -> Result<Self::Bytes<'a>, Error> {
        Err(Error::Other("Device is not running fx2lafw firmware and firmware loading is not available".into()))
    }
}

/// Provide one firmware directly, regardless of device type
impl FirmwareProvider for [u8] {
    type Bytes<'a> = &'a [u8];

    async fn get_firmware<'a>(&'a self, _device: &DeviceInfo) -> Result<Self::Bytes<'a>, Error> {
        Ok(self)
    }
}

impl<T: FirmwareProvider> FirmwareProvider for &T {
    type Bytes<'a> = T::Bytes<'a> where Self: 'a;

    fn get_firmware<'a>(&'a self, device: &DeviceInfo) -> impl Future<Output = Result<Self::Bytes<'a>, Error>> + Send + Sync {
        (*self).get_firmware(device)
    }
}

/// Default firmware provider that finds the device in [`MODELS`] and loads firmware from a predefined list of paths.
///
/// It looks for the firmware in the following locations in this order:
/// - `$FX2LAFW_FIRMWARE_DIR`
/// - `../share/sigrok-firmware` relative to the executable
/// - `$COMPILE_TIME_FX2LAFW_FIRMWARE_DIR` resolved at compile time
/// - `/usr/local/share/sigrok-firmware/` (Unix only)
/// - `/usr/share/sigrok-firmware/` (Unix only)
pub struct DefaultFirmwareProvider;

/// Default firmware provider that finds the device in [`MODELS`] and loads firmware from a predefined list of paths.
impl FirmwareProvider for DefaultFirmwareProvider {
    type Bytes<'a> = Vec<u8>;

    async fn get_firmware<'a>(&'a self, device: &DeviceInfo) -> Result<Self::Bytes<'a>, Error> {
        let Some(model) = MODELS.iter().find(|m| m.matches(device)) else {
            return Err(Error::Other(format!("No matching firmware found for {:04X}:{:04X}", device.vendor_id(), device.product_id())));
        };

        let filename = model.firmware_filename;

        let paths: Vec<PathBuf> = [
            std::env::var_os("FX2LAFW_FIRMWARE_DIR").map(|dir| Path::new(&dir).join(filename)),
            std::env::current_exe().ok().and_then(|exe| exe.parent().map(|p| p.join("share/sigrok-firmware").join(filename))),
            option_env!("COMPILE_TIME_FX2LAFW_FIRMWARE_DIR").map(|dir| Path::new(dir).join(filename)),
            #[cfg(unix)]
            Some(Path::new("/usr/local/share/sigrok-firmware").join(filename)),
            #[cfg(unix)]
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
}
