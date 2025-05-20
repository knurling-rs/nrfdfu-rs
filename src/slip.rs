use std::io::{self, Read, Write};

const END: u8 = 0xC0;
const ESC: u8 = 0xDB;
const ESC_END: u8 = 0xDC;
const ESC_ESC: u8 = 0xDD;

pub fn encode_frame(buf: &[u8], mut writer: impl Write) -> io::Result<()> {
    for &byte in buf {
        match byte {
            END => writer.write_all(&[ESC, ESC_END])?,
            ESC => writer.write_all(&[ESC, ESC_ESC])?,
            _ => writer.write_all(&[byte])?,
        }
    }

    writer.write_all(&[END])?;

    Ok(())
}

pub fn decode_frame(reader: impl Read, buf: &mut Vec<u8>) -> io::Result<()> {
    let bufreader = std::io::BufReader::new(reader);
    let mut bytes = bufreader.bytes();
    loop {
        let encoded_byte = match bytes.next() {
            Some(byte) => byte,
            None => return Err(io::ErrorKind::UnexpectedEof.into()),
        };

        let decoded_byte = match encoded_byte? {
            ESC => match bytes.next() {
                None => return Err(io::ErrorKind::UnexpectedEof.into()),
                Some(Ok(ESC_ESC)) => ESC,
                Some(Ok(ESC_END)) => END,
                Some(Ok(invalid)) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid byte following ESC: 0x{:02x}", invalid),
                    ))
                }
                Some(Err(e)) => return Err(e),
            },
            END => return Ok(()),
            other => other,
        };

        buf.push(decoded_byte);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(buf: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        encode_frame(buf, &mut out).unwrap();
        out
    }

    fn decode(mut buf: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        decode_frame(&mut buf, &mut out).unwrap();
        out
    }

    #[test]
    fn test_encode() {
        assert_eq!(encode(&[0]), vec![0, END]);
        assert_eq!(encode(&[END, 9]), vec![ESC, ESC_END, 9, END]);
        assert_eq!(
            encode(&[0, END, ESC, 1]),
            vec![0, ESC, ESC_END, ESC, ESC_ESC, 1, END]
        );
    }

    #[test]
    fn test_decode() {
        assert_eq!(decode(&[0, END]), vec![0]);
        assert_eq!(decode(&[ESC, ESC_END, 9, END]), vec![END, 9]);
        assert_eq!(
            decode(&[0, ESC, ESC_END, ESC, ESC_ESC, 1, END]),
            vec![0, END, ESC, 1]
        );
    }
}
