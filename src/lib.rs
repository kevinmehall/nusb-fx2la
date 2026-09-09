use std::{mem, time::Duration};

use nusb::transfer::{Buffer, Bulk, ControlOut, ControlType, In, Recipient};
use thiserror::Error;

mod firmware;
mod fx2;
mod sample_rate;

pub use firmware::{Model, MODELS, SELECTORS};
pub use sample_rate::SampleRate;

#[derive(Debug, Error)]
pub enum Error {
    #[error("USB error: {0}")]
    Usb(#[from] nusb::Error),
    #[error("USB transfer error: {0}")]
    UsbTransfer(#[from] nusb::transfer::TransferError),
    #[error("{0}")]
    Other(String),
}


pub struct Device {
    intf: nusb::Interface,
}

const CMD_START: u8 = 0xb1;
const CMD_START_FLAGS_CLK_30MHZ: u8 = 0 << 6;
const CMD_START_FLAGS_CLK_48MHZ: u8 = 1 << 6;

impl Device {
    pub async fn open() -> Result<Option<Device>, Error> {
        let Some((device, model)) = nusb::list_devices()
            .await?
            .find_map(|d| {
                let model = firmware::MODELS
                    .iter()
                    .find(|m| m.matches(&d))?;
                Some((d, model))
            }) else {
                return Ok(None);
            };

        log::info!("Found {}", model.description);

        Self::from_nusb(device, model).await.map(Some)
    }

    pub async fn from_nusb(device: nusb::DeviceInfo, model: &Model) -> Result<Device, Error> {
        let device = if device.product_string() == Some("fx2lafw") {
            log::info!("Device already has fx2lafw firmware loaded");
            device
        } else {
            let fw_bytes = firmware::get_firmware(model.firmware_filename).await?;
            fx2::load_firmware(device, &fw_bytes).await?
        };

        let dev = device.open().await?;
        let intf = dev.claim_interface(0).await?;

        Ok(Device { intf })
    }

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
        let n_transfers = 8;

        log::info!(
            "Started capture at {base:?} / {div} = {sample_rate}Hz, transfer size {transfer_size}",
            base = sample_rate.base,
            div = sample_rate.divisor,
            sample_rate = sample_rate.as_hz(),
        );

        while ep_in.pending() < n_transfers {
            let buf = ep_in.allocate(transfer_size);
            ep_in.submit(buf);
        }

        let buffer = ep_in.allocate(transfer_size);

        Ok(Capture {
            ep_in,
            buffer,
            sample_rate,
        })
    }
}

pub struct Capture {
    sample_rate: SampleRate,
    ep_in: nusb::Endpoint<Bulk, In>,

    /// An inactive buffer for the borrowed slice from `read`.
    buffer: Buffer,
}

impl Capture {
    pub fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    pub async fn read(&mut self) -> Result<&[u8], Error> {
        let completion = self.ep_in.next_complete().await;
        completion.status?;

        let buf = mem::replace(&mut self.buffer, completion.buffer);
        self.ep_in.submit(buf);

        Ok(&self.buffer[..])
    }

    pub fn cancel(&mut self) {
        self.ep_in.cancel_all();
    }
}
