use std::{ops::Deref, error::Error};
use nusb::DeviceInfo;

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

#[cfg(not(target_arch = "wasm32"))]
pub use std::marker::Send as NonWasmSend;

#[cfg(target_arch = "wasm32")]
pub trait NonWasmSend {}
#[cfg(target_arch = "wasm32")]
impl<T> NonWasmSend for T {}

/// Trait for choosing and obtaining firmware to load onto a device.
pub trait FirmwareProvider {
    type Bytes<'a>: Deref<Target = [u8]> where Self: 'a;

    /// Get the firmware for `device`
    fn get_firmware<'a>(&'a self, device: &DeviceInfo) -> impl Future<Output = Result<Self::Bytes<'a>, Box<dyn Error>>> + NonWasmSend;
}

/// Dummy implementation that fails if the firmware is not already loaded
impl FirmwareProvider for () {
    type Bytes<'a> = &'static [u8];

    async fn get_firmware<'a>(&'a self, _device: &DeviceInfo) -> Result<Self::Bytes<'a>, Box<dyn Error>> {
        Err("Device is not running fx2lafw firmware and firmware loading is not available".into())
    }
}

/// Provide one firmware directly, regardless of device type
impl FirmwareProvider for [u8] {
    type Bytes<'a> = &'a [u8];

    async fn get_firmware<'a>(&'a self, _device: &DeviceInfo) -> Result<Self::Bytes<'a>, Box<dyn Error>> {
        Ok(self)
    }
}

impl<T: FirmwareProvider> FirmwareProvider for &T {
    type Bytes<'a> = T::Bytes<'a> where Self: 'a;

    fn get_firmware<'a>(&'a self, device: &DeviceInfo) -> impl Future<Output = Result<Self::Bytes<'a>, Box<dyn Error>>> + NonWasmSend {
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
#[cfg(any(docsrs, all(any(unix, windows), feature = "fs")))]
pub struct DefaultFirmwareProvider;

/// Default firmware provider that finds the device in [`MODELS`] and loads firmware from a predefined list of paths.
#[cfg(any(docsrs, all(any(unix, windows), feature = "fs")))]
impl FirmwareProvider for DefaultFirmwareProvider {
    type Bytes<'a> = Vec<u8>;

    async fn get_firmware<'a>(&'a self, device: &DeviceInfo) -> Result<Self::Bytes<'a>, Box<dyn Error>> {
        use std::path::{Path, PathBuf};

        let Some(model) = MODELS.iter().find(|m| m.matches(device)) else {
            return Err(format!("No matching firmware found for {:04X}:{:04X}", device.vendor_id(), device.product_id()).into());
        };

        let filename = model.firmware_filename;

        let paths: Vec<PathBuf> = [
            std::env::var_os("FX2LAFW_FIRMWARE_DIR").map(|dir| Path::new(&dir).join(filename)),
            std::env::current_exe().ok().and_then(|exe| exe.parent().and_then(|d| d.parent()).map(|p| p.join("share/sigrok-firmware").join(filename))),
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

        Err(format!("Could not find firmware, tried {:?}", paths).into())
    }
}

/// Firmware provider for WebAssembly that loads firmware from a URL.
#[cfg(any(docsrs, all(feature = "web-fetch", target_arch = "wasm32", target_os = "unknown", target_env = "")))]
pub struct FetchFirmwareProvider {
    base_url: String,
    models: &'static [Model],
}

#[cfg(any(docsrs, all(feature = "web-fetch", target_arch = "wasm32", target_os = "unknown", target_env = "")))]
impl FetchFirmwareProvider {
    pub fn new(base_url: String) -> Self {
        Self { base_url, models: &MODELS }
    }
}

#[cfg(any(all(feature = "web-fetch", target_arch = "wasm32", target_os = "unknown", target_env = "")))]
impl FirmwareProvider for FetchFirmwareProvider {
    type Bytes<'a> = Vec<u8>;

    async fn get_firmware<'a>(&'a self, device: &DeviceInfo) -> Result<Self::Bytes<'a>, Box<dyn Error>> {
        let Some(model) = self.models.iter().find(|m| m.matches(device)) else {
            return Err(format!("No matching firmware found for {:04X}:{:04X}", device.vendor_id(), device.product_id()).into());
        };

        let slash = if self.base_url.ends_with('/') { "" } else { "/" };
        let url = format!("{}{}{}", self.base_url, slash, model.firmware_filename);

        log::debug!("Fetching firmware from {url}");

        let resp = gloo_net::http::Request::get(&url).send().await
            .map_err(|e| format!("Failed to fetch firmware from {url}: {e}"))?;

        if resp.status() != 200 {
            return Err(format!("Failed to fetch firmware from {url}: HTTP status {}", resp.status()).into());
        }

        let bytes = resp.binary().await
            .map_err(|e| format!("Failed to read firmware from {url}: {e}"))?;

        Ok(bytes)
    }
}

#[cfg(docsrs)] // dummy impl since rustdoc won't have gloo_net dependency
impl FirmwareProvider for FetchFirmwareProvider {
    type Bytes<'a> = Vec<u8>;

    async fn get_firmware<'a>(&'a self, device: &DeviceInfo) -> Result<Self::Bytes<'a>, Box<dyn Error>> {
       unimplemented!();
    }
}
