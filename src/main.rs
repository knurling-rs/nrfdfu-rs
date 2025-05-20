use clap::Parser;
use log::LevelFilter;
use serialport::{available_ports, SerialPort};
use std::convert::TryInto;
use std::hash::Hasher;
use std::time::Duration;
use std::{error::Error, fs};

#[macro_use]
mod macros;
mod elf;
mod init_packet;
mod messages;
mod slip;

use messages::*;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const USB_VID: u16 = 0x1915;
const USB_PID: u16 = 0x521f;

/// Bootloader protocol version we support.
const PROTOCOL_VERSION: u8 = 1;

fn main() {
    match run() {
        Ok(()) => {}
        Err(e) => {
            if e.downcast_ref::<PreviousErrors>().is_none() {
                eprintln!("error: {}", e);
            }
            std::process::exit(1);
        }
    }
}
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Prints the bootloader and application version, along with their memory areas.
    #[arg(long)]
    get_images: bool,

    /// Selects the USB device with the given serial number
    #[arg(long)]
    serial: Option<String>,

    /// File to flash
    elf_path: Option<std::path::PathBuf>,

    /// Reboots into the application even if no other application was performed.
    #[arg(long)]
    abort: bool,

    /// Runs the command on *all* recognized devices after printing the port's details.
    ///
    /// Errors are accumulated and reported after all ports have been tried.
    #[arg(long)]
    all: bool,
}

/// Previous errors occurred and were printed.
///
/// This error is explicitly *not* shown in main (because the errors were printed already).
#[derive(Debug)]
struct PreviousErrors;

impl std::fmt::Display for PreviousErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Previous errors occurred")
    }
}

impl std::error::Error for PreviousErrors {}

fn run() -> Result<()> {
    // We show info and higher levels by default, but allow overriding this via `RUST_LOG`.
    env_logger::builder()
        .filter_level(LevelFilter::Info)
        .parse_default_env()
        .init();

    let args = Args::parse();

    let image = if let Some(elf_path) = &args.elf_path {
        let elf = fs::read(elf_path)
            .map_err(|e| format!("couldn't read `{}`: {}", elf_path.to_string_lossy(), e))?;

        let mut image = elf::read_elf_image(&elf)?;

        // The firmware image must be padded with 0xFF to be a multiple of 4 Bytes. To our knowledge,
        // this is undocumented.
        while image.len() % 4 != 0 {
            image.push(0xff);
        }

        Some(image)
    } else {
        None
    };

    let matching_ports: Vec<_> = available_ports()?
        .into_iter()
        .filter(|port| match &port.port_type {
            serialport::SerialPortType::UsbPort(usb) => {
                // macOS presents two serial devices - /dev/tty* for receiving
                // incoming data and /dev/cu for dialling out. We can use
                // either, but we only want one of them, so hide the /dev/tty
                // devices.
                #[cfg(target_os = "macos")]
                if port.port_name.starts_with("/dev/tty") {
                    return false;
                }

                usb.vid == USB_VID
                    && usb.pid == USB_PID
                    && args
                        .serial
                        .as_ref()
                        .is_none_or(|s| Some(s) == usb.serial_number.as_ref())
            }
            _ => false,
        })
        .collect();

    match matching_ports.len() {
        0 => {
            return Err(
                "no matching USB serial device found.\n       Remember to put the \
                                 device in bootloader mode by pressing the reset button!"
                    .into(),
            )
        }
        1 => (),
        _ => {
            if !args.all {
                return Err("multiple matching USB serial devices found".into());
            }
        }
    };

    let mut errors_in_all = false;

    for port in matching_ports {
        let result = run_on_port(&port, &args, image.as_deref());
        if let Err(e) = result {
            if args.all {
                // The current port is printed in run_on_port anyway, but it doesn't hurt to be
                // explicit, especially since stdout and stderr might not be sorted properly.
                eprintln!("error processing {}: {e}", &port.port_name);
                errors_in_all = true;
            } else {
                return Err(e);
            }
        }
    }

    if errors_in_all {
        return Err(PreviousErrors.into());
    }

    if image.is_none() && !args.get_images && !args.abort {
        // This is done at the end so that errors from working on an --all still show up, to
        // increase the usefulness of RUST_LOG=debug or as a kind of readiness check.
        return Err("No actions performed; provide an .elf file on the command line to flash, or set querying options.".into());
    }

    Ok(())
}

fn run_on_port(port: &serialport::SerialPortInfo, args: &Args, image: Option<&[u8]>) -> Result<()> {
    log::debug!("opening {} (type {:?})", port.port_name, port.port_type);
    if args.all {
        let serial = match &port.port_type {
            serialport::SerialPortType::UsbPort(s) => s.serial_number.as_ref(),
            // Those don't get through filtering anyway
            _ => None,
        };
        println!(
            "Found port {} (serial: {})",
            port.port_name,
            serial
                .map(|s| format!("{:?}", s))
                .unwrap_or("unknown".into())
        );
    }

    let mut port = serialport::new(&port.port_name, 115200)
        .timeout(Duration::from_millis(1000))
        .open()?;

    // On Windows, this is required, otherwise communication fails with timeouts
    // (or just hangs forever).
    port.write_data_terminal_ready(true)?;

    let mut conn = BootloaderConnection::new(port)?;

    // Disable receipt notification. USB is a reliable transport.
    conn.set_receipt_notification(0)?;

    let obj_select = conn.select_object_command();
    log::debug!("select object response: {:?}", obj_select);

    let version = conn.fetch_protocol_version()?;
    log::debug!("protocol version: {}", version);

    let hw_version = conn.fetch_hardware_version()?;
    log::debug!("hardware version: {:?}", hw_version);

    if args.get_images {
        let bootloader_version = conn.fetch_firmware_version(0)?;
        println!("* image 0: {}", bootloader_version);

        let primary_version = conn.fetch_firmware_version(1)?;
        println!("* image 1: {}", primary_version);

        if primary_version.type_ == Some(FirmwareType::Softdevice) {
            let secondary_version = conn.fetch_firmware_version(2)?;
            println!("* image 2: {:#?}", secondary_version);
        }
    }

    if let Some(image) = image.as_ref() {
        let init_packet = init_packet::build_init_packet(image);
        conn.send_init_packet(&init_packet)?;
        conn.send_firmware(image)?;
    }

    if args.abort {
        let abort_result = conn.abort();

        if abort_result.is_ok() {
            log::warn!("Response received to Abort command (expected USB disconnect)");
        }
    }

    Ok(())
}

struct BootloaderConnection {
    serial: Box<dyn SerialPort>,
    buf: Vec<u8>,
    mtu: u16,
}

impl BootloaderConnection {
    fn new(serial: Box<dyn SerialPort>) -> Result<Self> {
        let mut this = Self {
            serial,
            buf: Vec::new(),
            mtu: 0,
        };

        // We must check the protocol version before doing anything else, since any other command
        // might change if the version changes.
        let proto_version = this.fetch_protocol_version()?;
        if proto_version != PROTOCOL_VERSION {
            return Err(format!(
                "device reports protocol version {}, we only support {}",
                proto_version, PROTOCOL_VERSION
            )
            .into());
        }

        let mtu = this.fetch_mtu()?;
        log::debug!("MTU = {} Bytes", mtu);
        this.mtu = mtu;
        Ok(this)
    }

    /// send `req` and do not fetch any response
    fn request<R: Request>(&mut self, req: R) -> Result<()> {
        let mut buf = vec![R::OPCODE as u8];
        req.write_payload(&mut buf)?;
        log::trace!("--> {:?}", buf);

        // Go through an intermediate buffer to avoid writing every byte individually.
        self.buf.clear();
        slip::encode_frame(&buf, &mut self.buf)?;
        self.serial
            .write_all(&self.buf)
            .map_err(|e| format!("error while writing to serial port: {}", e))?;
        self.serial.flush()?;

        Ok(())
    }

    /// send `req` and expect a response.
    /// aborts if no response is received within timeout window.
    fn request_response<R: Request>(&mut self, req: R) -> Result<R::Response> {
        self.request(req)?;

        self.buf.clear();
        slip::decode_frame(&mut self.serial, &mut self.buf)
            .map_err(|e| format!("error while reading from serial port: {}", e))?;
        log::trace!("<-- {:?}", self.buf);

        messages::parse_response::<R>(&self.buf)
    }

    fn fetch_protocol_version(&mut self) -> Result<u8> {
        let response = self.request_response(ProtocolVersionRequest);
        match response {
            Ok(version_response) => Ok(version_response.version),
            Err(e) => Err(e),
        }
    }

    fn fetch_hardware_version(&mut self) -> Result<HardwareVersionResponse> {
        self.request_response(HardwareVersionRequest)
    }

    fn fetch_firmware_version(&mut self, image: u8) -> Result<FirmwareVersionResponse> {
        self.request_response(FirmwareVersionRequest(image))
    }

    /// Sends the `.dat` file that's zipped into our firmware DFU .zip(?)
    /// modeled after `pc-nrfutil`s `dfu_transport_serial::send_init_packet()`
    fn send_init_packet(&mut self, data: &[u8]) -> Result<()> {
        log::info!("Sending init packet...");
        let select_response = self.select_object_command()?;
        log::debug!("Object selected: {:?}", select_response);

        let data_size = data.len() as u32;

        log::debug!("Creating Command...");
        self.create_command_object(data_size)?;
        log::debug!("Command created");

        log::debug!("Streaming Data: len: {}", data_size);
        self.stream_object_data(data)?;

        let received_crc = self.get_crc()?.crc;
        self.check_crc(data, received_crc, 0)?;

        self.execute()?;

        Ok(())
    }

    /// Sends the firmware image at `bin_path`.
    /// This is done in chunks to avoid exceeding our MTU  and involves periodic CRC checks.
    fn send_firmware(&mut self, image: &[u8]) -> Result<()> {
        log::info!("Sending firmware image of size {}...", image.len());

        log::debug!("Selecting Object: type Data");
        let select_response = self.select_object_data()?;
        log::debug!("Object selected: {:?}", select_response);

        let max_size = select_response.max_size;
        let mut prev_chunk_crc: u32 = 0;

        for chunk in image.chunks(max_size.try_into().unwrap()) {
            let curr_chunk_sz: u32 = chunk.len().try_into().unwrap();
            self.create_data_object(curr_chunk_sz)?;
            log::debug!("Streaming Data: len: {}", curr_chunk_sz);

            self.stream_object_data(chunk)?;

            let received_crc = self.get_crc()?;
            log::debug!("crc response: {:?}", received_crc);
            prev_chunk_crc = self.check_crc(chunk, received_crc.crc, prev_chunk_crc)?;

            self.execute()?;
        }

        log::info!("Done.");
        Ok(())
    }

    fn check_crc(&self, data: &[u8], received_crc: u32, initial: u32) -> Result<u32> {
        let mut digest = crc32fast::Hasher::new_with_initial(initial);
        digest.write(data);
        let expected_crc = digest.finalize();

        if expected_crc == received_crc {
            log::debug!("crc passed.");
            Ok(expected_crc)
        } else {
            let err_msg = format!(
                "crc failed: expected {} - received {}",
                expected_crc, received_crc
            );
            log::debug!("{}", err_msg);
            Err(err_msg.into())
        }
    }

    /// Sends a
    /// Request Type: `Select`
    /// Parameters:   `Object type = Command`
    fn select_object_command(&mut self) -> Result<SelectResponse> {
        self.request_response(SelectRequest(ObjectType::Command))
    }

    /// Sends a
    /// Request Type: `Select`
    /// Parameters:   `Object type = Data`
    fn select_object_data(&mut self) -> Result<SelectResponse> {
        self.request_response(SelectRequest(ObjectType::Data))
    }

    /// Sends a
    /// Request Type: `Create`
    /// Parameters:   `Object type = Command`
    ///               `size`
    fn create_command_object(&mut self, size: u32) -> Result<()> {
        self.request_response(CreateObjectRequest {
            obj_type: ObjectType::Command,
            size,
        })?;
        Ok(())
    }

    /// Sends a
    /// Request Type: `Create`
    /// Parameters:   `Object type = Data`
    ///               `size`
    fn create_data_object(&mut self, size: u32) -> Result<()> {
        // Note: Data objects cannot be created if no init packet has been sent. This results in an
        // `OperationNotPermitted` error.
        self.request_response(CreateObjectRequest {
            obj_type: ObjectType::Data,
            size,
        })?;
        Ok(())
    }

    fn set_receipt_notification(&mut self, every_n_packets: u16) -> Result<()> {
        self.request_response(SetPrnRequest(every_n_packets))?;
        Ok(())
    }

    fn fetch_mtu(&mut self) -> Result<u16> {
        Ok(self.request_response(GetMtuRequest)?.0)
    }

    fn stream_object_data(&mut self, data: &[u8]) -> Result<()> {
        // On the wire, the write request contains the opcode byte, and is then SLIP-encoded,
        // potentially doubling the size, and adding a frame terminator, so the chunk size has
        // to be smaller than the MTU.
        let max_chunk_size = usize::from((self.mtu - 1) / 2 - 1);

        for chunk in data.chunks(max_chunk_size) {
            // TODO: this also needs to take into account the receipt response. In our case we turn
            // it off, so there's nothing to do here.
            self.request(WriteRequest {
                request_payload: chunk,
            })?;
        }

        Ok(())
    }

    fn get_crc(&mut self) -> Result<CrcResponse> {
        self.request_response(CrcRequest)
    }

    // tell the target to execute whatever request setup we sent them before
    fn execute(&mut self) -> Result<ExecuteResponse> {
        self.request_response(ExecuteRequest)
    }

    /// Sends the Abort command, causing a reboot without reflashing.
    fn abort(&mut self) -> Result<AbortResponse> {
        self.request_response(AbortRequest)
    }
}
