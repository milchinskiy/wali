pub(crate) fn encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut index = 0;
    while index < input.len() {
        let first = input[index];
        let second = input.get(index + 1).copied();
        let third = input.get(index + 2).copied();

        out.push(TABLE[(first >> 2) as usize] as char);
        out.push(TABLE[((first & 0b0000_0011) << 4 | second.unwrap_or(0) >> 4) as usize] as char);

        match second {
            Some(second) => {
                out.push(TABLE[((second & 0b0000_1111) << 2 | third.unwrap_or(0) >> 6) as usize] as char);
            }
            None => out.push('='),
        }

        match third {
            Some(third) => out.push(TABLE[(third & 0b0011_1111) as usize] as char),
            None => out.push('='),
        }

        index += 3;
    }

    out
}

pub(crate) fn decode(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut clean = Vec::with_capacity(input.len());
    for byte in input.iter().copied() {
        if !byte.is_ascii_whitespace() {
            clean.push(byte);
        }
    }

    if clean.len() % 4 != 0 {
        return Err(format!("malformed base64 length {}", clean.len()));
    }

    let mut out = Vec::with_capacity((clean.len() / 4) * 3);
    let mut index = 0;
    while index < clean.len() {
        let chunk = &clean[index..index + 4];
        validate_padding(chunk, index + 4 == clean.len())?;

        let values = [
            decode_char(chunk[0])?,
            decode_char(chunk[1])?,
            decode_pad(chunk[2])?,
            decode_pad(chunk[3])?,
        ];

        out.push((values[0] << 2) | (values[1] >> 4));

        if chunk[2] != b'=' {
            out.push(((values[1] & 0b0000_1111) << 4) | (values[2] >> 2));
        }
        if chunk[3] != b'=' {
            out.push(((values[2] & 0b0000_0011) << 6) | values[3]);
        }

        index += 4;
    }

    Ok(out)
}

fn validate_padding(chunk: &[u8], is_last_chunk: bool) -> Result<(), String> {
    if chunk[0] == b'=' || chunk[1] == b'=' {
        return Err("unexpected base64 padding in required position".to_owned());
    }

    if chunk[2] == b'=' && chunk[3] != b'=' {
        return Err("invalid base64 padding".to_owned());
    }

    if (chunk[2] == b'=' || chunk[3] == b'=') && !is_last_chunk {
        return Err("base64 padding is only allowed in the final chunk".to_owned());
    }

    Ok(())
}

fn decode_char(byte: u8) -> Result<u8, String> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        b'=' => Err("unexpected base64 padding in required position".to_owned()),
        _ => Err(format!("invalid base64 character {byte:?}")),
    }
}

fn decode_pad(byte: u8) -> Result<u8, String> {
    match byte {
        b'=' => Ok(0),
        _ => decode_char(byte),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_binary_data() {
        let input = [0, b'h', b'i', 255];
        let encoded = encode(&input);
        assert_eq!(encoded, "AGhp/w==");
        assert_eq!(decode(encoded.as_bytes()).unwrap(), input.to_vec());
    }

    #[test]
    fn rejects_invalid_padding() {
        assert!(decode(b"AA=A").is_err());
        assert!(decode(b"AA==AAAA").is_err());
    }
}
