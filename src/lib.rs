mod firmware;
mod fx2;

use std::{mem, time::Duration};

use nusb::transfer::{Buffer, Bulk, ControlOut, ControlType, In, Recipient};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("USB error: {0}")]
    Usb(#[from] nusb::Error),
    #[error("USB transfer error: {0}")]
    UsbTransfer(#[from] nusb::transfer::TransferError),
    #[error("{0}")]
    Other(String),
    #[error("No device found")]
    NotFound,
}

pub enum SampleRate {}

pub struct Device {
    intf: nusb::Interface,
}

const CMD_START: u8 = 0xb1;

const MAX_SAMPLE_DELAY: u32 = 6 * 256;
const CMD_START_FLAGS_CLK_30MHZ: u8 = 0 << 6;
const CMD_START_FLAGS_CLK_48MHZ: u8 = 1 << 6;

impl Device {
    pub async fn open() -> Result<Device, Error> {
        let (device, model) = nusb::list_devices()
            .await?
            .find_map(|d| {
                let model = firmware::MODELS
                    .iter()
                    .find(|m| d.vendor_id() == m.vid && d.product_id() == m.pid)?;
                Some((d, model))
            })
            .ok_or(Error::NotFound)?;

        log::info!("Found {}", model.description);

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

    pub async fn start_capture(&self, sample_rate: u32) -> Result<Capture, Error> {
        let mut ep_in = self.intf.endpoint::<Bulk, In>(0x82)?;

        let (base, div, clock_flag) = if 48_000_000 % sample_rate == 0
            && 48_000_000 / sample_rate <= MAX_SAMPLE_DELAY
        {
            (48, 48_000_000 / sample_rate, CMD_START_FLAGS_CLK_48MHZ)
        } else if 30_000_000 % sample_rate == 0 && 30_000_000 / sample_rate <= MAX_SAMPLE_DELAY {
            (30, 30_000_000 / sample_rate, CMD_START_FLAGS_CLK_30MHZ)
        } else {
            return Err(Error::Other("Unsupported sample rate".to_string()));
        };

        let flags = clock_flag;
        let [delay_h, delay_l] = ((div - 1) as u16).to_be_bytes();

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
        let transfer_size = ((sample_rate / 100) as usize).div_ceil(ep_in.max_packet_size())
            * ep_in.max_packet_size();
        let n_transfers = 8;

        log::info!(
            "Started capture at {base}MHz / {div} = {sample_rate}Hz, transfer size {transfer_size}"
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
    sample_rate: u32,
    ep_in: nusb::Endpoint<Bulk, In>,

    /// An inactive buffer for the borrowed slice from `read`.
    buffer: Buffer,
}

impl Capture {
    pub fn sample_rate(&self) -> u32 {
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
