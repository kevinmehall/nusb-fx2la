//! Driver for [fx2lafw](https://sigrok.org/wiki/Fx2lafw) logic analyzers with [nusb](https://github.com/kevinmehall/nusb).
//!
//! See [`MODELS`] for the list of supported devices.
//!
//! ## Example
//!
//! ```rust,no_run
//! # async {
//! let dev = fx2la::Device::open().await?.ok_or("no device found")?;
//!
//! let mut capture = dev.start_capture(fx2la::SampleRate::from_hz(100_000.0)).await?;
//! capture.limit_remaining_samples(200_000);
//!
//! loop {
//!     let data = capture.read().await?;
//!
//!     if data.is_empty() {
//!         break;
//!     }
//!
//!     // handle data
//! }
//!
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! # };
//! ```
//!
//! See [`Device::from_nusb`] to customize device selection and firmware loading.
//!
//! ## Firmware
//!
//! The fx2lafw firmware is loaded to the device's RAM on first use each time the device is plugged in.
//! The [default firmware provider][`DefaultFirmwareProvider`] looks for firmware in common filesystem locations as well as the directories specified by the `$FX2LAFW_FIRMWARE_DIR` and `$COMPILE_TIME_FX2LAFW_FIRMWARE_DIR` environment variables.
//!
//! Firmware binaries can be [downloaded from the Sigrok project](https://sigrok.org/download/binary/sigrok-firmware-fx2lafw/) or via your package manager:
//!
//! ### Nix (run-time)
//!
//! ```bash
//! export FX2LAFW_FIRMWARE_DIR=$(nix-build '<nixpkgs>' -A sigrok-firmware-fx2lafw --no-out-link)/share/sigrok-firmware
//! ```
//!
//! ### Nix derivation (build-time)
//!
//! ```nix
//! env.COMPILE_TIME_FX2LAFW_FIRMWARE_DIR = "${pkgs.sigrok-firmware-fx2lafw}/share/sigrok-firmware";
//! ```
//!
//! ### Debian / Ubuntu
//!
//! ```bash
//! sudo apt install sigrok-firmware-fx2lafw
//! ```

use std::{mem, time::Duration};

use nusb::transfer::{Buffer, Bulk, ControlOut, ControlType, In, Recipient};
use thiserror::Error;

mod firmware;
mod fx2;
mod sample_rate;

pub use firmware::{Model, MODELS, SELECTORS, FirmwareProvider};
pub use sample_rate::SampleRate;

#[cfg(all(any(unix, windows), feature = "fs"))]
pub use firmware::DefaultFirmwareProvider;

#[derive(Debug, Error)]
pub enum Error {
    #[error("USB error: {0}")]
    Usb(#[from] nusb::Error),
    #[error("USB transfer error: {0}")]
    UsbTransfer(#[from] nusb::transfer::TransferError),
    #[error("{0}")]
    Other(String),
}

/// Opened fx2lafw device.
pub struct Device {
    intf: nusb::Interface,
}

const CMD_START: u8 = 0xb1;
const CMD_START_FLAGS_CLK_30MHZ: u8 = 0 << 6;
const CMD_START_FLAGS_CLK_48MHZ: u8 = 1 << 6;

impl Device {
    /// Open the first available device using the default firmware provider.
    #[cfg(all(any(target_family = "unix", target_family = "windows"), feature = "fs"))]
    pub async fn open() -> Result<Option<Device>, Error> {
        let Some(device) = nusb::request_device(SELECTORS).await? else {
            return Ok(None);
        };

        Self::from_nusb(device, DefaultFirmwareProvider).await.map(Some)
    }

    /// Open the specified device and load the firmware using the provided firmware provider.
    ///
    /// ## Example
    ///
    /// ```rust,no_run
    /// # async {
    /// let nusb_device = nusb::list_devices().await?
    ///   .find(|dev| fx2la::MODELS.iter().any(|m| m.matches(dev)))
    ///   .ok_or("no device found")?;
    /// let device = fx2la::Device::from_nusb(nusb_device, fx2la::DefaultFirmwareProvider).await?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// # };
    /// ```
    pub async fn from_nusb(device: nusb::DeviceInfo, firmware: impl FirmwareProvider) -> Result<Device, Error> {
        let device = if device.product_string() == Some("fx2lafw") {
            log::info!("Device already has fx2lafw firmware loaded");
            device
        } else {
            let fw_bytes = firmware.get_firmware(&device).await?;
            fx2::load_firmware(device, &fw_bytes).await?
        };

        let dev = device.open().await?;
        let intf = dev.claim_interface(0).await?;

        Ok(Device { intf })
    }

    /// Start capturing data at the specified sample rate.
    ///
    /// An error will be returned if another [`Capture`] currently exists for this device.
    pub async fn start_capture(&self, sample_rate: SampleRate) -> Result<Capture, Error> {
        let mut ep_in = self.intf.endpoint::<Bulk, In>(0x82)?;

        let flags = match sample_rate.base {
            sample_rate::BaseClock::Clk48Mhz => CMD_START_FLAGS_CLK_48MHZ,
            sample_rate::BaseClock::Clk30Mhz => CMD_START_FLAGS_CLK_30MHZ,
        };
        let [delay_h, delay_l] = (sample_rate.divisor - 1).to_be_bytes();

        self.intf
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Device,
                    request: CMD_START,
                    value: 0,
                    index: 0,
                    data: &[flags, delay_h, delay_l],
                },
                Duration::from_millis(100),
            )
            .await?;

        // Each buffer should be about 10ms of data.
        let transfer_size = ((sample_rate.as_hz() * 0.01) as usize).div_ceil(ep_in.max_packet_size()) * ep_in.max_packet_size();

        log::info!(
            "Started capture at {base:?} / {div} = {sample_rate}Hz, transfer size {transfer_size}",
            base = sample_rate.base,
            div = sample_rate.divisor,
            sample_rate = sample_rate.as_hz(),
        );

        let n_transfers = 10;
        while ep_in.pending() < n_transfers {
            let buf = ep_in.allocate(transfer_size);
            ep_in.submit(buf);
        }

        let buffer = ep_in.allocate(transfer_size);

        Ok(Capture {
            ep_in,
            buffer,
            sample_rate,
            remaining_transfers: None,
        })
    }
}

/// An ongoing capture.
pub struct Capture {
    sample_rate: SampleRate,
    ep_in: nusb::Endpoint<Bulk, In>,

    /// An inactive buffer for the borrowed slice from `read`.
    buffer: Buffer,

    remaining_transfers: Option<u64>,
}

impl Capture {
    /// Get the capture's sample rate.
    pub fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    /// Limit the number of remaining samples.
    ///
    /// The limit can be decreased, but cannot be increased.
    pub fn limit_remaining_samples(&mut self, samples: u64) {
        let transfers = samples.div_ceil(self.buffer.requested_len() as u64)
            .saturating_sub(self.ep_in.pending() as u64)
            .min(self.remaining_transfers.unwrap_or(u64::MAX));
        self.remaining_transfers = Some(transfers);
    }

    /// Read samples.
    ///
    /// An empty array will be returned when the capture has reached the [configured sample limit][Capture::limit_remaining_samples].
    pub async fn read(&mut self) -> Result<&[u8], Error> {
        if self.ep_in.pending() == 0 {
            return Ok(&[])
        }

        let completion = self.ep_in.next_complete().await;
        completion.status?;

        let buf = mem::replace(&mut self.buffer, completion.buffer);

        if self.remaining_transfers.is_none_or(|s| s > 0) {
            self.ep_in.submit(buf);
            if let Some(remaining) = &mut self.remaining_transfers { *remaining -= 1; }
        }

        Ok(&self.buffer[..])
    }

    /// Cancel all transfers immediately.
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown", target_env = "")))]
    pub fn cancel(&mut self) {
        self.ep_in.cancel_all();
    }
}
