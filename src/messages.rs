use std::{
    error::Error,
    fmt,
    io::{self, Read, Write},
};

use byteorder::{ReadBytesExt, WriteBytesExt, LE};

// opcodes
// note: incomplete; only contains opcodes that we currently use
#[derive(Debug)]
pub enum OpCode {
    ProtocolVersion = 0x00,
    CreateObject = 0x01,
    ReceiptNotifSet = 0x02,
    Crc = 0x03,
    Execute = 0x04,
    Select = 0x06,
    MtuGet = 0x07,
    Write = 0x08,
    Ping = 0x09,
    HardwareVersionGet = 0x0A,
    Response = 0x60, // marks the start of a response message
}

primitive_enum! {
    #[derive(Debug)]
    pub enum ResultCode(u8) {
        /// Invalid request opcode.
        Invalid = 0x00,
        /// Operation succeeded.
        Success = 0x01,
        /// Opcode not supported.
        OpCodeNotSupported = 0x02,
        /// Missing or invalid request parameter.
        InvalidParameter = 0x03,
        /// Not enough memory for the data object.
        InsufficientResources = 0x04,
        /// Data object does not match the firmware and hardware requirements,
        /// the signature is wrong, or parsing the command failed.
        InvalidObject = 0x05,
        /// Not a valid object type for a Create request.
        UnsupportedType = 0x07,
        /// The state of the DFU process does not allow this operation.
        OperationNotPermitted = 0x08,
        /// Operation failed.
        OperationFailed = 0x0A,
        /// Extended error. The next byte of the response contains the error code of
        /// the extended error (see `ExtError`).
        ExtError = 0x0B,
    }
}

primitive_enum! {
    #[derive(Debug)]
    pub enum ExtError(u8) {
        /// No extended error code set. This should never appear.
        NoError = 0x00,
        /// Invalid extended error code. This should never appear.
        InvalidErrorCode = 0x01,
        WrongCommandFormat = 0x02,
        UnknownCommand = 0x03,
        /// Initialization command invalid.
        InitCommandInvalid = 0x04,
        FwVersionFailure = 0x05,
        HwVersionFailure = 0x06,
        SdVersionFailure = 0x07,
        SignatureMissing = 0x08,
        WrongHashType = 0x09,
        HashFailed = 0x0A,
        WrongSignatureType = 0x0B,
        VerificationFailed = 0x0C,
        InsufficientSpace = 0x0D,
    }
}

/// An error code returned by the bootloader.
#[derive(Debug)]
pub struct DfuError {
    code: ResultCode,
    ext_error: Option<ExtError>,
}

impl fmt::Display for DfuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self.ext_error {
            Some(ExtError::NoError) => "no extended error set",
            Some(ExtError::InvalidErrorCode) => "invalid extended error code",
            Some(ExtError::WrongCommandFormat) => "incorrect command format",
            Some(ExtError::UnknownCommand) => "unknown command",
            Some(ExtError::InitCommandInvalid) => "initialization command invalid",
            Some(ExtError::FwVersionFailure) => {
                "invalid firmware version (possible downgrade attempted)"
            }
            Some(ExtError::HwVersionFailure) => "hardware version mismatch",
            Some(ExtError::SdVersionFailure) => "firmware requires unavailable SoftDevice version",
            Some(ExtError::SignatureMissing) => "missing image signature",
            Some(ExtError::WrongHashType) => "unsupported hash type used in initialization command",
            Some(ExtError::HashFailed) => "failed to compute firmware hash",
            Some(ExtError::WrongSignatureType) => "unsupported signature type",
            Some(ExtError::VerificationFailed) => "hash verification failed",
            Some(ExtError::InsufficientSpace) => "insufficient space for firmware",
            None => match self.code {
                ResultCode::Invalid => "invalid request opcode",
                ResultCode::Success => "success",
                ResultCode::OpCodeNotSupported => "opcode not supported",
                ResultCode::InvalidParameter => "missing or invalid request parameter",
                ResultCode::InsufficientResources => "not enough memory to create object",
                ResultCode::InvalidObject => "invalid data object",
                ResultCode::UnsupportedType => "invalid object type for create object request",
                ResultCode::OperationNotPermitted => "operation not permitted in the current state",
                ResultCode::OperationFailed => "operation failed",
                ResultCode::ExtError => {
                    panic!("`EXT_ERROR` result code without extended error byte")
                }
            },
        };
        f.write_str(s)
    }
}

impl Error for DfuError {}

#[repr(u8)]
#[derive(Copy, Clone)]
pub enum ObjectType {
    Command = 0x01,
    Data = 0x02,
}

pub trait Request {
    const OPCODE: OpCode;
    type Response: Response;

    fn write_payload<W: Write>(&self, writer: W) -> io::Result<()>;
}

pub trait Response: Sized {
    fn read_payload<R: Read>(reader: R) -> io::Result<Self>;
}

pub struct ProtocolVersionRequest;

impl Request for ProtocolVersionRequest {
    const OPCODE: OpCode = OpCode::ProtocolVersion;

    type Response = ProtocolVersionResponse;

    fn write_payload<W: Write>(&self, _writer: W) -> io::Result<()> {
        Ok(())
    }
}

pub struct ProtocolVersionResponse {
    pub version: u8,
}

impl Response for ProtocolVersionResponse {
    fn read_payload<R: Read>(mut reader: R) -> io::Result<Self> {
        Ok(Self {
            version: reader.read_u8()?,
        })
    }
}

pub struct HardwareVersionRequest;

impl Request for HardwareVersionRequest {
    const OPCODE: OpCode = OpCode::HardwareVersionGet;

    type Response = HardwareVersionResponse;

    fn write_payload<W: Write>(&self, _writer: W) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct HardwareVersionResponse {
    // See FICR register docs
    part: u32,
    variant: u32,
    rom_size: u32,
    ram_size: u32,
    rom_page_size: u32,
}

impl Response for HardwareVersionResponse {
    fn read_payload<R: Read>(mut response_bytes: R) -> io::Result<Self> {
        Ok(Self {
            part: response_bytes.read_u32::<LE>()?,
            variant: response_bytes.read_u32::<LE>()?,
            rom_size: response_bytes.read_u32::<LE>()?,
            ram_size: response_bytes.read_u32::<LE>()?,
            rom_page_size: response_bytes.read_u32::<LE>()?,
        })
    }
}

pub struct PingRequest(pub u8);

impl Request for PingRequest {
    const OPCODE: OpCode = OpCode::Ping;

    type Response = PingResponse;

    fn write_payload<W: Write>(&self, mut writer: W) -> io::Result<()> {
        writer.write_u8(self.0)
    }
}

#[derive(Debug)]
pub struct PingResponse(pub u8);

impl Response for PingResponse {
    fn read_payload<R: Read>(mut reader: R) -> io::Result<Self> {
        Ok(Self(reader.read_u8()?))
    }
}

pub struct SelectRequest(pub ObjectType);

impl Request for SelectRequest {
    const OPCODE: OpCode = OpCode::Select;

    type Response = SelectResponse;

    fn write_payload<W: Write>(&self, mut writer: W) -> io::Result<()> {
        // TODO should I cast this more nicely?
        writer.write_u8(self.0 as u8)
    }
}

#[derive(Debug)]
pub struct SelectResponse {
    pub max_size: u32,
    pub offset: u32,
    pub crc: u32,
}

impl Response for SelectResponse {
    fn read_payload<R: Read>(mut response_bytes: R) -> io::Result<Self> {
        // NOTE: The parameter order is *not* in accordance with to the parameter order from the docs
        // (https://infocenter.nordicsemi.com/index.jsp?topic=%2Fcom.nordic.infocenter.sdk5.v15.0.0%2Flib_dfu_transport.html)
        // but rather follows the order from the firmware implementation:
        // https://github.com/tmael/nRF5_SDK/blob/master/components/libraries/bootloader/serial_dfu/nrf_dfu_serial.c#L106
        Ok(Self {
            max_size: response_bytes.read_u32::<LE>()?,
            offset: response_bytes.read_u32::<LE>()?,
            crc: response_bytes.read_u32::<LE>()?,
        })
    }
}

pub struct CreateObjectRequest {
    pub obj_type: ObjectType,
    pub size: u32,
}

impl Request for CreateObjectRequest {
    const OPCODE: OpCode = OpCode::CreateObject;

    type Response = CreateObjectResponse;

    fn write_payload<W: Write>(&self, mut writer: W) -> io::Result<()> {
        // note:
        // while the docs suggest that `obj_type` should be a `u32`:
        // https://infocenter.nordicsemi.com/index.jsp?topic=%2Fcom.nordic.infocenter.sdk5.v15.0.0%2Flib_dfu_transport.html
        // the firmware code *for handling packets received via serial* only reads the first byte as
        // `obj_type`:
        // https://github.com/tmael/nRF5_SDK/blob/master/components/libraries/bootloader/serial_dfu/nrf_dfu_serial.c#L227
        // Which is why we're writing a `u8` here. The same applies to ble-received packets.
        // CAUTION: ANT+ packet handling expects a `u16`, so this code won't work in that case!
        writer.write_u8(self.obj_type as u8)?;
        writer.write_u32::<LE>(self.size)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct CreateObjectResponse;

impl Response for CreateObjectResponse {
    fn read_payload<R: Read>(_reader: R) -> io::Result<Self> {
        Ok(Self)
    }
}

pub struct SetPrnRequest(pub u16);

impl Request for SetPrnRequest {
    const OPCODE: OpCode = OpCode::ReceiptNotifSet;

    type Response = SetPrnResponse;

    fn write_payload<W: Write>(&self, mut writer: W) -> io::Result<()> {
        writer.write_u16::<LE>(self.0)
    }
}

pub struct SetPrnResponse;

impl Response for SetPrnResponse {
    fn read_payload<R: Read>(_reader: R) -> io::Result<Self> {
        Ok(Self)
    }
}

pub struct GetMtuRequest;

impl Request for GetMtuRequest {
    const OPCODE: OpCode = OpCode::MtuGet;

    type Response = GetMtuResponse;

    fn write_payload<W: Write>(&self, _writer: W) -> io::Result<()> {
        Ok(())
    }
}

pub struct GetMtuResponse(pub u16);

impl Response for GetMtuResponse {
    fn read_payload<R: Read>(mut reader: R) -> io::Result<Self> {
        Ok(Self(reader.read_u16::<LE>()?))
    }
}

pub struct WriteRequest<'a> {
    pub request_payload: &'a [u8],
}

impl Request for WriteRequest<'_> {
    const OPCODE: OpCode = OpCode::Write;

    type Response = WriteResponse;

    fn write_payload<W: Write>(&self, mut writer: W) -> io::Result<()> {
        writer.write_all(self.request_payload)?;
        Ok(())
    }
}

/// HACK: this is never used, write responses depend on the receipt response and are handled
/// manually.
#[derive(Debug)]
pub enum WriteResponse {}

impl Response for WriteResponse {
    fn read_payload<R: Read>(_reader: R) -> io::Result<Self> {
        unreachable!()
    }
}

pub struct CrcRequest;

impl Request for CrcRequest {
    const OPCODE: OpCode = OpCode::Crc;

    type Response = CrcResponse;

    fn write_payload<W: Write>(&self, _writer: W) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct CrcResponse {
    pub offset: u32,
    pub crc: u32,
}

impl Response for CrcResponse {
    fn read_payload<R: Read>(mut reader: R) -> io::Result<Self> {
        Ok(Self {
            offset: reader.read_u32::<LE>()?,
            crc: reader.read_u32::<LE>()?,
        })
    }
}

pub struct ExecuteRequest;

impl Request for ExecuteRequest {
    const OPCODE: OpCode = OpCode::Execute;

    type Response = ExecuteResponse;

    fn write_payload<W: Write>(&self, _writer: W) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct ExecuteResponse;

impl Response for ExecuteResponse {
    fn read_payload<R: Read>(_reader: R) -> io::Result<Self> {
        Ok(Self)
    }
}

pub fn parse_response<R: Request>(buf: &[u8]) -> crate::Result<R::Response> {
    // Response format:
    // - Fixed byte 0x60
    // - Request opcode
    // - Response result code
    // - Response payload
    if buf.len() < 3 {
        return Err(format!(
            "truncated response (expected at least 3 bytes, got {})",
            buf.len()
        )
        .into());
    }

    if buf[0] != OpCode::Response as u8 {
        return Err(format!(
            "malformed response (expected nrf DFU response preamble 0x60, got 0x{:02x})",
            buf[0]
        )
        .into());
    }

    if buf[1] != R::OPCODE as u8 {
        return Err(format!(
            "malformed response (expected echoed opcode {:?} (0x{:02x}), got 0x{:02x})",
            R::OPCODE,
            R::OPCODE as u8,
            buf[1]
        )
        .into());
    }

    let result: ResultCode = ResultCode::from_primitive(buf[2])
        .ok_or_else(|| format!("malformed response (invalid result code 0x{:02x})", buf[2]))?;

    match result {
        ResultCode::Success => {}
        ResultCode::ExtError => match buf.get(3) {
            Some(byte) => {
                let ext_error: ExtError = ExtError::from_primitive(*byte).ok_or_else(|| {
                    format!(
                        "malformed response (unknown extended error code 0x{:02x})",
                        byte
                    )
                })?;

                return Err(DfuError {
                    code: ResultCode::ExtError,
                    ext_error: Some(ext_error),
                }
                .into());
            }
            None => {
                return Err("malformed response (missing extended error byte)"
                    .to_string()
                    .into());
            }
        },
        code => {
            return Err(DfuError {
                code,
                ext_error: None,
            }
            .into())
        }
    }

    let mut response_bytes = &buf[3..];
    let response = R::Response::read_payload(&mut response_bytes)?;

    if !response_bytes.is_empty() {
        return Err("trailing bytes in response".to_string().into());
    }

    Ok(response)
}
